use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use url::Url;

use crate::DomainError;
use crate::model::{
    CapabilityDeclaration, DOMAIN_CONTRACT_VERSION, DOMAIN_PACK_FORMAT_VERSION, DomainOverride,
    DomainPack, DomainValue, EntityCollectionDeclaration, ExportDeclaration, FieldDeclaration,
    ModuleManifest, Provenance, ProvenanceKind, TypeExpression,
};

const MAX_TEXT_LENGTH: usize = 65_536;
const MAX_LIST_ITEMS: usize = 65_536;
const MAX_VALUE_DEPTH: usize = 32;
const MAX_VALUE_NODES: usize = 262_144;

const CAPABILITIES: &[(&str, u32)] = &[
    ("data", 1),
    ("editor_schema", 1),
    ("operations", 1),
    ("state", 1),
];

const RESERVED_NAMESPACES: &[&str] = &[
    "grammar", "module", "pattern", "runtime", "state", "story", "weave",
];

/// Validate one manifest against the current Weave release.
pub fn validate_manifest(
    manifest: &ModuleManifest,
    current_weave: &Version,
) -> Result<(), DomainError> {
    if manifest.contract_version != DOMAIN_CONTRACT_VERSION {
        return Err(DomainError::UnsupportedContract {
            found: manifest.contract_version,
            expected: DOMAIN_CONTRACT_VERSION,
        });
    }
    if manifest.pack_format_version != DOMAIN_PACK_FORMAT_VERSION {
        return Err(DomainError::UnsupportedPackFormat {
            found: manifest.pack_format_version,
            expected: DOMAIN_PACK_FORMAT_VERSION,
        });
    }
    validate_module_id("id", &manifest.id)?;
    parse_version("version", &manifest.version)?;
    validate_namespace("namespace", &manifest.namespace)?;
    if RESERVED_NAMESPACES.contains(&manifest.namespace.as_str()) {
        return Err(invalid("namespace", "namespace is reserved by Weave"));
    }
    validate_text("title", &manifest.title, 1, 160)?;
    validate_text("summary", &manifest.summary, 1, 1_024)?;
    if manifest.authors.is_empty() || manifest.authors.len() > 64 {
        return Err(invalid("authors", "expected between 1 and 64 authors"));
    }
    for (index, author) in manifest.authors.iter().enumerate() {
        validate_text(&format!("authors[{index}].name"), &author.name, 1, 160)?;
        if let Some(url) = &author.url {
            validate_https(&format!("authors[{index}].url"), url)?;
        }
    }
    validate_spdx("license", &manifest.license)?;
    validate_https("license_url", &manifest.license_url)?;
    let requirement = parse_requirement("weave_version", &manifest.weave_version)?;
    if !requirement.matches(current_weave) {
        return Err(DomainError::IncompatibleWeave {
            module_id: manifest.id.clone(),
            current: current_weave.clone(),
        });
    }
    validate_capabilities(&manifest.capabilities)?;
    validate_dependencies(manifest)?;
    validate_types_and_exports(&manifest.types, &manifest.exports)?;
    validate_authoring(manifest)?;
    validate_provenance("provenance", &manifest.provenance)?;
    Ok(())
}

/// Validate one data pack against its owning manifest and the current Weave release.
pub fn validate_pack(
    pack: &DomainPack,
    manifest: &ModuleManifest,
    current_weave: &Version,
) -> Result<(), DomainError> {
    validate_manifest(manifest, current_weave)?;
    if pack.pack_format_version != DOMAIN_PACK_FORMAT_VERSION {
        return Err(DomainError::UnsupportedPackFormat {
            found: pack.pack_format_version,
            expected: DOMAIN_PACK_FORMAT_VERSION,
        });
    }
    validate_namespace("id", &pack.id)?;
    parse_version("version", &pack.version)?;
    validate_text("title", &pack.title, 1, 160)?;
    validate_module_id("module.id", &pack.module.id)?;
    if pack.module.id != manifest.id {
        return Err(invalid("module.id", "pack belongs to a different module"));
    }
    let module_requirement = parse_requirement("module.version", &pack.module.version)?;
    let module_version = parse_version("manifest.version", &manifest.version)?;
    if !module_requirement.matches(&module_version) {
        return Err(DomainError::IncompatibleModule {
            pack_id: pack.id.clone(),
            module_id: manifest.id.clone(),
        });
    }

    let mut previous = None;
    let mut dependency_keys = BTreeSet::new();
    for (index, dependency) in pack.dependencies.iter().enumerate() {
        let path = format!("dependencies[{index}]");
        validate_module_id(&format!("{path}.module_id"), &dependency.module_id)?;
        validate_namespace(&format!("{path}.pack_id"), &dependency.pack_id)?;
        parse_requirement(&format!("{path}.version"), &dependency.version)?;
        let key = (&dependency.module_id, &dependency.pack_id);
        if previous.is_some_and(|prior| prior >= key) {
            return Err(invalid(
                "dependencies",
                "pack dependencies must be unique and sorted by module_id then pack_id",
            ));
        }
        previous = Some(key);
        if !dependency_keys.insert((dependency.module_id.clone(), dependency.pack_id.clone())) {
            return Err(invalid("dependencies", "duplicate pack dependency"));
        }
    }

    validate_effective_values(manifest, &pack.values)?;
    validate_provenance("provenance", &pack.provenance)?;
    for name in pack.values.keys() {
        let claim = format!("values.{name}");
        if !pack.provenance.claims.contains_key(&claim) {
            return Err(invalid(
                format!("provenance.claims.{claim}"),
                "every exported value requires a provenance claim",
            ));
        }
    }
    Ok(())
}

/// Validate one effective export set after source-authored replacements are applied.
///
/// Unlike [`validate_pack`], this deliberately does not require pack provenance claims for
/// source-authored values. The compiler carries those replacements separately from the immutable
/// selected pack so a host can distinguish public defaults from fictional decisions.
pub fn validate_effective_values(
    manifest: &ModuleManifest,
    values: &BTreeMap<String, DomainValue>,
) -> Result<(), DomainError> {
    for name in values.keys() {
        if !manifest.exports.contains_key(name) {
            return Err(DomainError::UnknownExport { name: name.clone() });
        }
    }
    for (name, export) in &manifest.exports {
        match values.get(name) {
            Some(value) => {
                let mut nodes = 0;
                validate_value(
                    &format!("values.{name}"),
                    value,
                    &export.value_type,
                    &manifest.types,
                    0,
                    &mut nodes,
                )?;
            }
            None if export.required => {
                return Err(DomainError::MissingExport { name: name.clone() });
            }
            None => {}
        }
    }
    validate_entity_collections(manifest, values)
}

/// Apply compile-time constant replacements to immutable pack defaults.
///
/// Replacement order is canonicalized by path and duplicate paths fail closed. Intermediate
/// object and map containers may be created, but the complete effective value set must satisfy the
/// manifest and its declarative entity constraints before it is returned.
pub fn apply_authored_overrides(
    manifest: &ModuleManifest,
    base_values: &BTreeMap<String, DomainValue>,
    overrides: &[DomainOverride],
) -> Result<BTreeMap<String, DomainValue>, DomainError> {
    if overrides.len() > 16_384 {
        return Err(invalid(
            "overrides",
            "too many source-authored replacements",
        ));
    }
    let mut ordered = overrides.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    let mut previous: Option<&[String]> = None;
    let mut values = base_values.clone();
    for authored in ordered {
        if authored.path.is_empty() {
            return Err(invalid("overrides", "replacement path must not be empty"));
        }
        if previous.is_some_and(|path| path == authored.path) {
            return Err(invalid("overrides", "duplicate replacement path"));
        }
        previous = Some(&authored.path);
        for (index, segment) in authored.path.iter().enumerate() {
            validate_namespace(&format!("overrides.path[{index}]"), segment)?;
        }
        apply_override(manifest, &mut values, authored)?;
    }
    validate_effective_values(manifest, &values)?;
    Ok(values)
}

fn apply_override(
    manifest: &ModuleManifest,
    values: &mut BTreeMap<String, DomainValue>,
    authored: &DomainOverride,
) -> Result<(), DomainError> {
    let Some((export_name, fields)) = authored.path.split_first() else {
        return Err(invalid("overrides", "replacement path must not be empty"));
    };
    let export = manifest
        .exports
        .get(export_name)
        .ok_or_else(|| DomainError::UnknownExport {
            name: export_name.clone(),
        })?;
    let display_path = format!("values.{}", authored.path.join("."));
    if fields.is_empty() {
        validate_one_value(
            &display_path,
            &authored.value,
            &export.value_type,
            &manifest.types,
        )?;
        values.insert(export_name.clone(), authored.value.clone());
        return Ok(());
    }
    let root = match values.entry(export_name.clone()) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => entry.insert(empty_container(
            &export.value_type,
            &manifest.types,
            &format!("values.{export_name}"),
        )?),
    };
    set_nested_value(
        root,
        &export.value_type,
        fields,
        &authored.value,
        &manifest.types,
        &format!("values.{export_name}"),
    )
}

fn set_nested_value(
    current: &mut DomainValue,
    value_type: &TypeExpression,
    path: &[String],
    replacement: &DomainValue,
    types: &BTreeMap<String, TypeExpression>,
    display_path: &str,
) -> Result<(), DomainError> {
    let Some((segment, remaining)) = path.split_first() else {
        return Err(invalid(display_path, "replacement path must not be empty"));
    };
    let resolved = resolve_named_type(value_type, types)?;
    let child_type = match resolved {
        TypeExpression::Object { fields } => fields
            .get(segment)
            .map(|field| &field.value_type)
            .ok_or_else(|| invalid(format!("{display_path}.{segment}"), "unknown object field"))?,
        TypeExpression::Map { values, .. } => {
            validate_namespace(&format!("{display_path}.{segment}"), segment)?;
            values
        }
        _ => {
            return Err(invalid(
                display_path,
                "replacement path cannot descend into a scalar or list",
            ));
        }
    };
    let DomainValue::Object(entries) = current else {
        return Err(invalid(
            display_path,
            "replacement path encountered a non-object value",
        ));
    };
    let child_path = format!("{display_path}.{segment}");
    if remaining.is_empty() {
        validate_one_value(&child_path, replacement, child_type, types)?;
        entries.insert(segment.clone(), replacement.clone());
        return Ok(());
    }
    let empty = empty_container(child_type, types, &child_path)?;
    let child = entries.entry(segment.clone()).or_insert(empty);
    set_nested_value(
        child,
        child_type,
        remaining,
        replacement,
        types,
        &child_path,
    )
}

fn empty_container(
    value_type: &TypeExpression,
    types: &BTreeMap<String, TypeExpression>,
    path: &str,
) -> Result<DomainValue, DomainError> {
    match resolve_named_type(value_type, types)? {
        TypeExpression::Map { .. } | TypeExpression::Object { .. } => {
            Ok(DomainValue::Object(BTreeMap::new()))
        }
        _ => Err(invalid(
            path,
            "replacement path cannot create a scalar or list container",
        )),
    }
}

fn validate_one_value(
    path: &str,
    value: &DomainValue,
    value_type: &TypeExpression,
    types: &BTreeMap<String, TypeExpression>,
) -> Result<(), DomainError> {
    let mut nodes = 0;
    validate_value(path, value, value_type, types, 0, &mut nodes)
}

/// Validate and deterministically order one active module set.
pub fn resolve_module_order(
    manifests: &[ModuleManifest],
    current_weave: &Version,
) -> Result<Vec<String>, DomainError> {
    let mut modules = BTreeMap::new();
    let mut namespaces: BTreeMap<&str, &str> = BTreeMap::new();
    for manifest in manifests {
        validate_manifest(manifest, current_weave)?;
        if modules.insert(manifest.id.as_str(), manifest).is_some() {
            return Err(DomainError::DuplicateModule {
                module_id: manifest.id.clone(),
            });
        }
        if let Some(existing) = namespaces.insert(&manifest.namespace, &manifest.id) {
            return Err(DomainError::NamespaceCollision {
                namespace: manifest.namespace.clone(),
                first: existing.to_owned(),
                second: manifest.id.clone(),
            });
        }
    }

    let mut dependencies: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for manifest in manifests {
        let entry = dependencies.entry(&manifest.id).or_default();
        for dependency in &manifest.dependencies {
            let Some(installed) = modules.get(dependency.id.as_str()) else {
                return Err(DomainError::MissingDependency {
                    module_id: manifest.id.clone(),
                    dependency_id: dependency.id.clone(),
                });
            };
            let requirement = parse_requirement("dependencies.version", &dependency.version)?;
            let installed_version = parse_version("dependency.version", &installed.version)?;
            if !requirement.matches(&installed_version) {
                return Err(DomainError::IncompatibleDependency {
                    module_id: manifest.id.clone(),
                    dependency_id: dependency.id.clone(),
                });
            }
            entry.insert(&dependency.id);
            dependents
                .entry(&dependency.id)
                .or_default()
                .insert(&manifest.id);
        }
    }

    let mut ready = dependencies
        .iter()
        .filter_map(|(id, requirements)| requirements.is_empty().then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(manifests.len());
    while let Some(id) = ready.pop_first() {
        ordered.push(id.to_owned());
        if let Some(children) = dependents.get(id) {
            for child in children {
                let requirements = dependencies
                    .get_mut(child)
                    .expect("every dependent has a dependency entry");
                requirements.remove(id);
                if requirements.is_empty() {
                    ready.insert(child);
                }
            }
        }
    }
    if ordered.len() != manifests.len() {
        return Err(DomainError::DependencyCycle);
    }
    Ok(ordered)
}

fn validate_capabilities(capabilities: &[CapabilityDeclaration]) -> Result<(), DomainError> {
    let mut previous = None;
    for (index, capability) in capabilities.iter().enumerate() {
        validate_namespace(&format!("capabilities[{index}].id"), &capability.id)?;
        if previous.is_some_and(|prior: &str| prior >= capability.id.as_str()) {
            return Err(invalid(
                "capabilities",
                "capabilities must be unique and sorted by id",
            ));
        }
        previous = Some(&capability.id);
        let supported = CAPABILITIES
            .iter()
            .find_map(|(id, version)| (*id == capability.id).then_some(*version));
        if supported != Some(capability.version) {
            return Err(DomainError::UnsupportedCapability {
                id: capability.id.clone(),
                version: capability.version,
            });
        }
    }
    Ok(())
}

fn validate_dependencies(manifest: &ModuleManifest) -> Result<(), DomainError> {
    let mut previous = None;
    for (index, dependency) in manifest.dependencies.iter().enumerate() {
        validate_module_id(&format!("dependencies[{index}].id"), &dependency.id)?;
        if dependency.id == manifest.id {
            return Err(invalid("dependencies", "module cannot depend on itself"));
        }
        if previous.is_some_and(|prior: &str| prior >= dependency.id.as_str()) {
            return Err(invalid(
                "dependencies",
                "module dependencies must be unique and sorted by id",
            ));
        }
        previous = Some(&dependency.id);
        parse_requirement(
            &format!("dependencies[{index}].version"),
            &dependency.version,
        )?;
    }
    Ok(())
}

fn validate_types_and_exports(
    types: &BTreeMap<String, TypeExpression>,
    exports: &BTreeMap<String, ExportDeclaration>,
) -> Result<(), DomainError> {
    for (name, value_type) in types {
        validate_type_name(&format!("types.{name}"), name)?;
        validate_type_expression(&format!("types.{name}"), value_type, types)?;
        let mut active = BTreeSet::new();
        validate_named_cycles(name, types, &mut active)?;
    }
    if exports.is_empty() {
        return Err(invalid("exports", "module must expose at least one value"));
    }
    for (name, export) in exports {
        validate_namespace(&format!("exports.{name}"), name)?;
        validate_text(
            &format!("exports.{name}.description"),
            &export.description,
            1,
            1_024,
        )?;
        validate_type_expression(&format!("exports.{name}.type"), &export.value_type, types)?;
    }
    Ok(())
}

fn validate_authoring(manifest: &ModuleManifest) -> Result<(), DomainError> {
    if manifest.authoring.entity_collections.len() > 256 {
        return Err(invalid(
            "authoring.entity_collections",
            "too many entity collections",
        ));
    }
    let mut previous = None;
    for (index, collection) in manifest.authoring.entity_collections.iter().enumerate() {
        let path = format!("authoring.entity_collections[{index}]");
        validate_namespace(&format!("{path}.export"), &collection.export)?;
        if previous.is_some_and(|prior: &str| prior >= collection.export.as_str()) {
            return Err(invalid(
                "authoring.entity_collections",
                "entity collections must be unique and sorted by export",
            ));
        }
        previous = Some(&collection.export);
        for (field_path, field) in [
            ("id_field", &collection.id_field),
            ("label_field", &collection.label_field),
            ("order_field", &collection.order_field),
            ("parent_field", &collection.parent_field),
            ("relation_field", &collection.relation_field),
        ] {
            validate_namespace(&format!("{path}.{field_path}"), field)?;
        }
        validate_sorted_identifiers(
            &format!("{path}.inheritance_fields"),
            &collection.inheritance_fields,
        )?;
        let mut structural_fields = BTreeSet::new();
        for field in [
            &collection.id_field,
            &collection.label_field,
            &collection.order_field,
            &collection.parent_field,
            &collection.relation_field,
        ]
        .into_iter()
        .chain(collection.inheritance_fields.iter())
        {
            if !structural_fields.insert(field) {
                return Err(invalid(
                    path.clone(),
                    "entity authoring fields must be distinct",
                ));
            }
        }
        let export =
            manifest
                .exports
                .get(&collection.export)
                .ok_or_else(|| DomainError::UnknownExport {
                    name: collection.export.clone(),
                })?;
        let TypeExpression::Map { values, .. } =
            resolve_named_type(&export.value_type, &manifest.types)?
        else {
            return Err(invalid(
                format!("{path}.export"),
                "entity collection export must be a map",
            ));
        };
        let TypeExpression::Object { fields } = resolve_named_type(values, &manifest.types)? else {
            return Err(invalid(
                format!("{path}.export"),
                "entity collection values must be objects",
            ));
        };
        require_string_field(&path, fields, &collection.id_field, &manifest.types)?;
        require_string_field(&path, fields, &collection.label_field, &manifest.types)?;
        require_integer_field(&path, fields, &collection.order_field, &manifest.types)?;
        require_string_field(&path, fields, &collection.parent_field, &manifest.types)?;
        require_string_list_field(&path, fields, &collection.relation_field, &manifest.types)?;
        for field in &collection.inheritance_fields {
            require_string_field(&path, fields, field, &manifest.types)?;
        }
    }
    Ok(())
}

fn require_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, FieldDeclaration>,
    name: &str,
) -> Result<&'a FieldDeclaration, DomainError> {
    fields
        .get(name)
        .ok_or_else(|| invalid(format!("{path}.{name}"), "authoring field is not declared"))
}

fn require_string_field(
    path: &str,
    fields: &BTreeMap<String, FieldDeclaration>,
    name: &str,
    types: &BTreeMap<String, TypeExpression>,
) -> Result<(), DomainError> {
    let field = require_field(path, fields, name)?;
    if !matches!(
        resolve_named_type(&field.value_type, types)?,
        TypeExpression::String { .. }
    ) {
        return Err(invalid(
            format!("{path}.{name}"),
            "authoring field must be a string",
        ));
    }
    Ok(())
}

fn require_integer_field(
    path: &str,
    fields: &BTreeMap<String, FieldDeclaration>,
    name: &str,
    types: &BTreeMap<String, TypeExpression>,
) -> Result<(), DomainError> {
    let field = require_field(path, fields, name)?;
    if !matches!(
        resolve_named_type(&field.value_type, types)?,
        TypeExpression::Number { integer: true, .. }
    ) {
        return Err(invalid(
            format!("{path}.{name}"),
            "authoring order field must be an integer",
        ));
    }
    Ok(())
}

fn require_string_list_field(
    path: &str,
    fields: &BTreeMap<String, FieldDeclaration>,
    name: &str,
    types: &BTreeMap<String, TypeExpression>,
) -> Result<(), DomainError> {
    let field = require_field(path, fields, name)?;
    let TypeExpression::List { items, .. } = resolve_named_type(&field.value_type, types)? else {
        return Err(invalid(
            format!("{path}.{name}"),
            "authoring relation field must be a list",
        ));
    };
    if !matches!(
        resolve_named_type(items, types)?,
        TypeExpression::String { .. }
    ) {
        return Err(invalid(
            format!("{path}.{name}"),
            "authoring relation entries must be strings",
        ));
    }
    Ok(())
}

fn resolve_named_type<'a>(
    mut value_type: &'a TypeExpression,
    types: &'a BTreeMap<String, TypeExpression>,
) -> Result<&'a TypeExpression, DomainError> {
    let mut remaining = types.len().saturating_add(1);
    while let TypeExpression::Named { name } = value_type {
        if remaining == 0 {
            return Err(DomainError::RecursiveType { name: name.clone() });
        }
        value_type = types
            .get(name)
            .ok_or_else(|| DomainError::UnknownType { name: name.clone() })?;
        remaining -= 1;
    }
    Ok(value_type)
}

fn validate_type_expression(
    path: &str,
    value_type: &TypeExpression,
    types: &BTreeMap<String, TypeExpression>,
) -> Result<(), DomainError> {
    match value_type {
        TypeExpression::Null | TypeExpression::Bool => {}
        TypeExpression::Number {
            minimum, maximum, ..
        } => {
            if minimum.is_some_and(|value| !value.is_finite())
                || maximum.is_some_and(|value| !value.is_finite())
                || minimum.zip(*maximum).is_some_and(|(min, max)| min > max)
            {
                return Err(invalid(path, "invalid finite numeric bounds"));
            }
        }
        TypeExpression::String {
            min_length,
            max_length,
        } => {
            if min_length > max_length || *max_length > MAX_TEXT_LENGTH {
                return Err(invalid(path, "invalid string length bounds"));
            }
        }
        TypeExpression::Symbol { values } => {
            if values.is_empty() || values.len() > 4_096 {
                return Err(invalid(
                    path,
                    "symbol type requires a bounded non-empty set",
                ));
            }
            validate_sorted_identifiers(path, values)?;
        }
        TypeExpression::List {
            items,
            min_items,
            max_items,
        } => {
            if min_items > max_items || *max_items > MAX_LIST_ITEMS {
                return Err(invalid(path, "invalid list length bounds"));
            }
            validate_type_expression(&format!("{path}.items"), items, types)?;
        }
        TypeExpression::Map {
            values,
            min_entries,
            max_entries,
        } => {
            if min_entries > max_entries || *max_entries > MAX_LIST_ITEMS {
                return Err(invalid(path, "invalid map entry bounds"));
            }
            validate_type_expression(&format!("{path}.values"), values, types)?;
        }
        TypeExpression::Object { fields } => {
            if fields.len() > 4_096 {
                return Err(invalid(path, "object declares too many fields"));
            }
            for (name, field) in fields {
                validate_namespace(&format!("{path}.fields.{name}"), name)?;
                validate_text(
                    &format!("{path}.fields.{name}.description"),
                    &field.description,
                    1,
                    1_024,
                )?;
                validate_type_expression(
                    &format!("{path}.fields.{name}.type"),
                    &field.value_type,
                    types,
                )?;
            }
        }
        TypeExpression::Named { name } => {
            validate_type_name(path, name)?;
            if !types.contains_key(name) {
                return Err(DomainError::UnknownType { name: name.clone() });
            }
        }
    }
    Ok(())
}

fn validate_named_cycles<'a>(
    name: &'a str,
    types: &'a BTreeMap<String, TypeExpression>,
    active: &mut BTreeSet<&'a str>,
) -> Result<(), DomainError> {
    if !active.insert(name) {
        return Err(DomainError::RecursiveType {
            name: name.to_owned(),
        });
    }
    let value_type = types
        .get(name)
        .ok_or_else(|| DomainError::UnknownType { name: name.into() })?;
    walk_named(value_type, types, active)?;
    active.remove(name);
    Ok(())
}

fn walk_named<'a>(
    value_type: &'a TypeExpression,
    types: &'a BTreeMap<String, TypeExpression>,
    active: &mut BTreeSet<&'a str>,
) -> Result<(), DomainError> {
    match value_type {
        TypeExpression::Named { name } => validate_named_cycles(name, types, active),
        TypeExpression::List { items, .. } => walk_named(items, types, active),
        TypeExpression::Map { values, .. } => walk_named(values, types, active),
        TypeExpression::Object { fields } => {
            for field in fields.values() {
                walk_named(&field.value_type, types, active)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_value(
    path: &str,
    value: &DomainValue,
    value_type: &TypeExpression,
    types: &BTreeMap<String, TypeExpression>,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), DomainError> {
    *nodes += 1;
    if depth > MAX_VALUE_DEPTH || *nodes > MAX_VALUE_NODES {
        return Err(invalid(path, "value exceeds structural limits"));
    }
    match (value, value_type) {
        (DomainValue::Null, TypeExpression::Null)
        | (DomainValue::Bool(_), TypeExpression::Bool) => Ok(()),
        (
            DomainValue::Number(value),
            TypeExpression::Number {
                integer,
                minimum,
                maximum,
            },
        ) if value.is_finite()
            && (!integer || value.fract() == 0.0)
            && minimum.is_none_or(|minimum| *value >= minimum)
            && maximum.is_none_or(|maximum| *value <= maximum) =>
        {
            Ok(())
        }
        (
            DomainValue::String(value),
            TypeExpression::String {
                min_length,
                max_length,
            },
        ) if (*min_length..=*max_length).contains(&value.chars().count())
            && !contains_sensitive(value) =>
        {
            Ok(())
        }
        (DomainValue::Symbol(value), TypeExpression::Symbol { values })
            if values.binary_search(value).is_ok() =>
        {
            Ok(())
        }
        (
            DomainValue::List(values),
            TypeExpression::List {
                items,
                min_items,
                max_items,
            },
        ) if (*min_items..=*max_items).contains(&values.len()) => {
            for (index, value) in values.iter().enumerate() {
                validate_value(
                    &format!("{path}[{index}]"),
                    value,
                    items,
                    types,
                    depth + 1,
                    nodes,
                )?;
            }
            Ok(())
        }
        (
            DomainValue::Object(entries),
            TypeExpression::Map {
                values,
                min_entries,
                max_entries,
            },
        ) if (*min_entries..=*max_entries).contains(&entries.len()) => {
            for (name, value) in entries {
                validate_namespace(&format!("{path}.{name}"), name)?;
                validate_value(
                    &format!("{path}.{name}"),
                    value,
                    values,
                    types,
                    depth + 1,
                    nodes,
                )?;
            }
            Ok(())
        }
        (DomainValue::Object(values), TypeExpression::Object { fields }) => {
            for name in values.keys() {
                if !fields.contains_key(name) {
                    return Err(invalid(
                        format!("{path}.{name}"),
                        "object contains an undeclared field",
                    ));
                }
            }
            for (name, field) in fields {
                match values.get(name) {
                    Some(value) => validate_value(
                        &format!("{path}.{name}"),
                        value,
                        &field.value_type,
                        types,
                        depth + 1,
                        nodes,
                    )?,
                    None if field.required => {
                        return Err(invalid(
                            format!("{path}.{name}"),
                            "required object field is missing",
                        ));
                    }
                    None => {}
                }
            }
            Ok(())
        }
        (_, TypeExpression::Named { name }) => {
            let resolved = types
                .get(name)
                .ok_or_else(|| DomainError::UnknownType { name: name.clone() })?;
            validate_value(path, value, resolved, types, depth + 1, nodes)
        }
        _ => Err(DomainError::TypeMismatch {
            path: path.to_owned(),
            expected: type_label(value_type),
            found: value_label(value),
        }),
    }
}

fn validate_entity_collections(
    manifest: &ModuleManifest,
    values: &BTreeMap<String, DomainValue>,
) -> Result<(), DomainError> {
    for collection in &manifest.authoring.entity_collections {
        let Some(value) = values.get(&collection.export) else {
            continue;
        };
        let DomainValue::Object(entities) = value else {
            return Err(invalid(
                format!("values.{}", collection.export),
                "entity collection must be an object map",
            ));
        };
        for (id, entity) in entities {
            let path = format!("values.{}.{id}", collection.export);
            let fields = object_fields(&path, entity)?;
            if string_field(&path, fields, &collection.id_field)? != id {
                return Err(invalid(
                    format!("{path}.{}", collection.id_field),
                    "stable entity id must equal its map key",
                ));
            }
            let parent = string_field(&path, fields, &collection.parent_field)?;
            validate_entity_reference(&path, id, parent, entities, &collection.parent_field)?;
            for field in &collection.inheritance_fields {
                let reference = string_field(&path, fields, field)?;
                validate_entity_reference(&path, id, reference, entities, field)?;
            }
            let related = string_list_field(&path, fields, &collection.relation_field)?;
            let mut seen = BTreeSet::new();
            for relation in related {
                if relation == id {
                    return Err(invalid(
                        format!("{path}.{}", collection.relation_field),
                        "entity cannot relate to itself",
                    ));
                }
                if !seen.insert(relation) {
                    return Err(invalid(
                        format!("{path}.{}", collection.relation_field),
                        "entity relations must be unique",
                    ));
                }
                if !entities.contains_key(relation) {
                    return Err(invalid(
                        format!("{path}.{}", collection.relation_field),
                        "entity relation references an unknown stable id",
                    ));
                }
            }
        }
        validate_reference_chain(entities, collection, &collection.parent_field)?;
        for field in &collection.inheritance_fields {
            validate_reference_chain(entities, collection, field)?;
        }
        validate_symmetric_relations(entities, collection)?;
    }
    Ok(())
}

fn object_fields<'a>(
    path: &str,
    value: &'a DomainValue,
) -> Result<&'a BTreeMap<String, DomainValue>, DomainError> {
    match value {
        DomainValue::Object(fields) => Ok(fields),
        _ => Err(invalid(path, "entity must be an object")),
    }
}

fn string_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, DomainError> {
    match fields.get(field) {
        Some(DomainValue::String(value)) => Ok(value),
        _ => Err(invalid(
            format!("{path}.{field}"),
            "entity field must be a string",
        )),
    }
}

fn string_list_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Vec<&'a str>, DomainError> {
    let Some(DomainValue::List(values)) = fields.get(field) else {
        return Err(invalid(
            format!("{path}.{field}"),
            "entity field must be a list",
        ));
    };
    values
        .iter()
        .map(|value| match value {
            DomainValue::String(value) => Ok(value.as_str()),
            _ => Err(invalid(
                format!("{path}.{field}"),
                "entity relation entries must be strings",
            )),
        })
        .collect()
}

fn validate_entity_reference(
    path: &str,
    id: &str,
    reference: &str,
    entities: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<(), DomainError> {
    if reference.is_empty() {
        return Ok(());
    }
    if reference == id {
        return Err(invalid(
            format!("{path}.{field}"),
            "entity cannot reference itself",
        ));
    }
    if !entities.contains_key(reference) {
        return Err(invalid(
            format!("{path}.{field}"),
            "entity references an unknown stable id",
        ));
    }
    Ok(())
}

fn validate_reference_chain(
    entities: &BTreeMap<String, DomainValue>,
    collection: &EntityCollectionDeclaration,
    field: &str,
) -> Result<(), DomainError> {
    let mut complete = BTreeSet::new();
    for start in entities.keys() {
        if complete.contains(start) {
            continue;
        }
        let mut chain = Vec::new();
        let mut positions = BTreeMap::new();
        let mut current = start.as_str();
        loop {
            if current.is_empty() || complete.contains(current) {
                break;
            }
            if positions.insert(current, chain.len()).is_some() {
                return Err(invalid(
                    format!("values.{}.{start}.{field}", collection.export),
                    "entity reference chain contains a cycle",
                ));
            }
            chain.push(current);
            let entity = entities.get(current).ok_or_else(|| {
                invalid(
                    format!("values.{}.{start}.{field}", collection.export),
                    "entity references an unknown stable id",
                )
            })?;
            let fields = object_fields(&format!("values.{}.{current}", collection.export), entity)?;
            current = string_field(
                &format!("values.{}.{current}", collection.export),
                fields,
                field,
            )?;
        }
        complete.extend(chain.into_iter().map(str::to_owned));
    }
    Ok(())
}

fn validate_symmetric_relations(
    entities: &BTreeMap<String, DomainValue>,
    collection: &EntityCollectionDeclaration,
) -> Result<(), DomainError> {
    for (id, entity) in entities {
        let path = format!("values.{}.{id}", collection.export);
        let fields = object_fields(&path, entity)?;
        for related in string_list_field(&path, fields, &collection.relation_field)? {
            let related_path = format!("values.{}.{related}", collection.export);
            let related_entity = entities.get(related).ok_or_else(|| {
                invalid(
                    format!("{path}.{}", collection.relation_field),
                    "entity relation references an unknown stable id",
                )
            })?;
            let related_fields = object_fields(&related_path, related_entity)?;
            if !string_list_field(&related_path, related_fields, &collection.relation_field)?
                .contains(&id.as_str())
            {
                return Err(invalid(
                    format!("{path}.{}", collection.relation_field),
                    "entity relations must be symmetric",
                ));
            }
        }
    }
    Ok(())
}

fn validate_provenance(path: &str, provenance: &Provenance) -> Result<(), DomainError> {
    if provenance.sources.is_empty() || provenance.sources.len() > 4_096 {
        return Err(invalid(
            format!("{path}.sources"),
            "expected between 1 and 4096 provenance sources",
        ));
    }
    let mut identifiers = BTreeSet::new();
    let mut previous = None;
    for (index, source) in provenance.sources.iter().enumerate() {
        let source_path = format!("{path}.sources[{index}]");
        validate_namespace(&format!("{source_path}.id"), &source.id)?;
        if previous.is_some_and(|prior: &str| prior >= source.id.as_str()) {
            return Err(invalid(
                format!("{path}.sources"),
                "sources must be unique and sorted by id",
            ));
        }
        previous = Some(&source.id);
        identifiers.insert(source.id.as_str());
        validate_https(&format!("{source_path}.url"), &source.url)?;
        validate_text(&format!("{source_path}.revision"), &source.revision, 1, 256)?;
        validate_spdx(&format!("{source_path}.license"), &source.license)?;
        validate_https(&format!("{source_path}.license_url"), &source.license_url)?;
        validate_text(
            &format!("{source_path}.attribution"),
            &source.attribution,
            1,
            2_048,
        )?;
        match (&source.kind, &source.sha256) {
            (ProvenanceKind::Original, None) => {}
            (_, Some(sha256)) if valid_sha256(sha256) => {}
            _ => {
                return Err(invalid(
                    format!("{source_path}.sha256"),
                    "public and derived sources require a SHA-256",
                ));
            }
        }
    }
    let mut previous = None;
    for (index, transformation) in provenance.transformations.iter().enumerate() {
        let transform_path = format!("{path}.transformations[{index}]");
        validate_namespace(&format!("{transform_path}.id"), &transformation.id)?;
        if previous.is_some_and(|prior: &str| prior >= transformation.id.as_str())
            || !identifiers.insert(&transformation.id)
        {
            return Err(invalid(
                format!("{path}.transformations"),
                "transformations must be unique and sorted by id",
            ));
        }
        previous = Some(&transformation.id);
        validate_text(
            &format!("{transform_path}.description"),
            &transformation.description,
            1,
            2_048,
        )?;
        if transformation.inputs.is_empty() {
            return Err(invalid(
                format!("{transform_path}.inputs"),
                "transformation requires at least one input",
            ));
        }
        validate_sorted_references(
            &format!("{transform_path}.inputs"),
            &transformation.inputs,
            &identifiers,
        )?;
    }
    if provenance.claims.is_empty() {
        return Err(invalid(
            format!("{path}.claims"),
            "at least one provenance claim is required",
        ));
    }
    for (claim, references) in &provenance.claims {
        validate_text(&format!("{path}.claims"), claim, 1, 512)?;
        if references.is_empty() {
            return Err(invalid(
                format!("{path}.claims.{claim}"),
                "claim requires at least one source or transformation",
            ));
        }
        validate_sorted_references(&format!("{path}.claims.{claim}"), references, &identifiers)?;
    }
    Ok(())
}

fn validate_sorted_references(
    path: &str,
    references: &[String],
    identifiers: &BTreeSet<&str>,
) -> Result<(), DomainError> {
    let mut previous = None;
    for reference in references {
        if previous.is_some_and(|prior: &str| prior >= reference.as_str()) {
            return Err(invalid(path, "references must be unique and sorted"));
        }
        previous = Some(reference);
        if !identifiers.contains(reference.as_str()) {
            return Err(invalid(path, "reference is not declared by provenance"));
        }
    }
    Ok(())
}

fn validate_sorted_identifiers(path: &str, values: &[String]) -> Result<(), DomainError> {
    let mut previous = None;
    for value in values {
        validate_namespace(path, value)?;
        if previous.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid(path, "values must be unique and sorted"));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_module_id(path: &str, value: &str) -> Result<(), DomainError> {
    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 2 || segments.iter().any(|segment| !valid_namespace(segment)) {
        return Err(invalid(
            path,
            "expected a dot-separated lowercase module identifier",
        ));
    }
    Ok(())
}

fn validate_namespace(path: &str, value: &str) -> Result<(), DomainError> {
    if !valid_namespace(value) {
        return Err(invalid(path, "expected a lowercase ASCII identifier"));
    }
    Ok(())
}

fn valid_namespace(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_type_name(path: &str, value: &str) -> Result<(), DomainError> {
    let mut characters = value.chars();
    if !characters
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        || !characters.all(|character| character.is_ascii_alphanumeric())
    {
        return Err(invalid(path, "expected an ASCII UpperCamelCase type name"));
    }
    Ok(())
}

fn validate_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), DomainError> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(invalid(path, "text length is outside the allowed bounds"));
    }
    if contains_sensitive(value) {
        return Err(invalid(path, "value resembles a secret or credential"));
    }
    Ok(())
}

fn validate_https(path: &str, value: &str) -> Result<(), DomainError> {
    let parsed = Url::parse(value).map_err(|_| invalid(path, "expected a valid HTTPS URL"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() || !parsed.username().is_empty() {
        return Err(invalid(
            path,
            "expected a public HTTPS URL without credentials",
        ));
    }
    if parsed.password().is_some() {
        return Err(invalid(
            path,
            "expected a public HTTPS URL without credentials",
        ));
    }
    Ok(())
}

fn parse_version(path: &str, value: &str) -> Result<Version, DomainError> {
    Version::parse(value).map_err(|_| invalid(path, "expected a semantic version"))
}

fn parse_requirement(path: &str, value: &str) -> Result<VersionReq, DomainError> {
    VersionReq::parse(value).map_err(|_| invalid(path, "expected a semantic-version requirement"))
}

fn validate_spdx(path: &str, value: &str) -> Result<(), DomainError> {
    validate_text(path, value, 1, 256)?;
    let mut tokens = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'(' => {
                tokens.push(LicenseToken::Left);
                index += 1;
            }
            b')' => {
                tokens.push(LicenseToken::Right);
                index += 1;
            }
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'(' | b')')
                {
                    index += 1;
                }
                let raw = &value[start..index];
                let token = match raw {
                    "AND" => LicenseToken::And,
                    "OR" => LicenseToken::Or,
                    "WITH" => LicenseToken::With,
                    _ if raw.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
                    }) =>
                    {
                        LicenseToken::Atom
                    }
                    _ => return Err(invalid(path, "expected SPDX expression syntax")),
                };
                tokens.push(token);
            }
        }
    }
    let mut parser = LicenseParser {
        tokens: &tokens,
        position: 0,
    };
    if !parser.parse_or() || parser.position != tokens.len() {
        return Err(invalid(path, "expected SPDX expression syntax"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LicenseToken {
    Atom,
    And,
    Or,
    With,
    Left,
    Right,
}

struct LicenseParser<'a> {
    tokens: &'a [LicenseToken],
    position: usize,
}

impl LicenseParser<'_> {
    fn parse_or(&mut self) -> bool {
        if !self.parse_and() {
            return false;
        }
        while self.take(LicenseToken::Or) {
            if !self.parse_and() {
                return false;
            }
        }
        true
    }

    fn parse_and(&mut self) -> bool {
        if !self.parse_primary() {
            return false;
        }
        while self.take(LicenseToken::And) {
            if !self.parse_primary() {
                return false;
            }
        }
        true
    }

    fn parse_primary(&mut self) -> bool {
        if self.take(LicenseToken::Left) {
            return self.parse_or() && self.take(LicenseToken::Right);
        }
        if !self.take(LicenseToken::Atom) {
            return false;
        }
        !self.take(LicenseToken::With) || self.take(LicenseToken::Atom)
    }

    fn take(&mut self, token: LicenseToken) -> bool {
        if self.tokens.get(self.position) == Some(&token) {
            self.position += 1;
            true
        } else {
            false
        }
    }
}

fn contains_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    if lower.contains("github_pat_") || lower.contains("ghp_") {
        return true;
    }
    value
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
        .any(|token| {
            (token.starts_with("sk-") && token.len() >= 19)
                || (token.starts_with("AKIA")
                    && token.len() == 20
                    && token
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric()))
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn type_label(value_type: &TypeExpression) -> &'static str {
    match value_type {
        TypeExpression::Null => "null",
        TypeExpression::Bool => "bool",
        TypeExpression::Number { .. } => "number",
        TypeExpression::String { .. } => "string",
        TypeExpression::Symbol { .. } => "symbol",
        TypeExpression::List { .. } => "list",
        TypeExpression::Map { .. } => "map",
        TypeExpression::Object { .. } => "object",
        TypeExpression::Named { .. } => "named type",
    }
}

fn value_label(value: &DomainValue) -> &'static str {
    match value {
        DomainValue::Null => "null",
        DomainValue::Bool(_) => "bool",
        DomainValue::Number(_) => "number",
        DomainValue::String(_) => "string",
        DomainValue::Symbol(_) => "symbol",
        DomainValue::List(_) => "list",
        DomainValue::Object(_) => "object",
    }
}

fn invalid(path: impl Into<String>, reason: &'static str) -> DomainError {
    DomainError::InvalidField {
        path: path.into(),
        reason,
    }
}
