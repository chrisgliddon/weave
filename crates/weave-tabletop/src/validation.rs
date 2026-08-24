use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;
use weave_domain::{FieldDeclaration, TypeExpression, validate_typed_value};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_SELECTION_FORMAT_VERSION,
    ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition, AdapterExtensionSurface,
    AdapterManifest, AdapterProvenance, AdapterSelection, AdapterSourceClass,
    CHARACTER_PROJECTION_FORMAT_VERSION, CanonicalSuggestion, CreationFieldAuthority,
    ExcludedMaterial, RESOLVER_CONTRACT_VERSION, RESOLVER_FORMAT_VERSION, ResolutionReceipt,
    ResolutionRequest, ResolvedAdapter, SuggestionDecision, TabletopCapability,
    TabletopCharacterProjection, TabletopError, TabletopState,
};

const REQUIRED_EXCLUSIONS: [ExcludedMaterial; 6] = [
    ExcludedMaterial::CommunitySupplements,
    ExcludedMaterial::Logos,
    ExcludedMaterial::TradeDress,
    ExcludedMaterial::Artwork,
    ExcludedMaterial::Layout,
    ExcludedMaterial::UnverifiedAssets,
];

/// Canonical SHA-256 of any stable serialized contract value.
pub fn canonical_fingerprint(value: &impl Serialize) -> Result<String, TabletopError> {
    let bytes = serde_json::to_vec(value).map_err(|_| TabletopError::Artifact)?;
    Ok(sha256_bytes(&bytes))
}

/// SHA-256 for explicitly supplied source bytes.
#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Produce the exact coordinate embedded by selection, projection, state, and events.
pub fn resolved_adapter(manifest: &AdapterManifest) -> Result<ResolvedAdapter, TabletopError> {
    validate_adapter_manifest(manifest)?;
    Ok(ResolvedAdapter {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        schema_version: manifest.schema_version,
        content_sha256: canonical_fingerprint(manifest)?,
    })
}

/// Validate the complete adapter contract and public-source gate.
pub fn validate_adapter_manifest(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    if manifest.manifest_format_version != ADAPTER_MANIFEST_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "manifest_format_version",
        });
    }
    validate_global_id("id", &manifest.id)?;
    validate_semver("version", &manifest.version)?;
    if manifest.schema_version == 0 {
        return Err(invalid(
            "schema_version",
            "expected a positive schema version",
        ));
    }
    validate_local_id("namespace", &manifest.namespace)?;
    validate_text("title", &manifest.title, 1, 160)?;
    validate_text("compatibility_label", &manifest.compatibility_label, 1, 160)?;
    validate_text("summary", &manifest.summary, 1, 1_024)?;
    parse_requirement("weave_version", &manifest.weave_version)?;
    parse_requirement(
        "character_contract_version",
        &manifest.character_contract_version,
    )?;
    match manifest.extension_surface {
        AdapterExtensionSurface::DeclarativeDataWithRegisteredResolver {
            resolver_contract_version,
        } if resolver_contract_version == RESOLVER_CONTRACT_VERSION => {}
        _ => {
            return Err(TabletopError::UnsupportedVersion {
                path: "extension_surface.resolver_contract_version",
            });
        }
    }
    validate_capabilities(manifest)?;
    validate_type_contract(manifest)?;
    validate_creation_steps(manifest)?;
    validate_operations(manifest)?;
    if manifest.visibility != Default::default() {
        return Err(invalid(
            "visibility",
            "expected the conservative v1 public/authoring/authority-host matrix",
        ));
    }
    validate_migrations(manifest)?;
    validate_adapter_provenance(&manifest.provenance)
}

/// Enforce the local allowlist and verify the exact source and license bytes.
pub fn verify_adapter_source(
    manifest: &AdapterManifest,
    source_bytes: &[u8],
    license_text_bytes: &[u8],
) -> Result<(), TabletopError> {
    validate_adapter_manifest(manifest)?;
    if sha256_bytes(source_bytes) != manifest.provenance.sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "provenance.sha256",
        });
    }
    if sha256_bytes(license_text_bytes) != manifest.provenance.required_license_text.sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "provenance.required_license_text.sha256",
        });
    }
    Ok(())
}

/// Verify the primary source, every declared companion artifact, and the retained license text.
///
/// Map keys are the exact project-relative names recorded in
/// [`AdapterProvenance::additional_artifacts`](crate::AdapterProvenance::additional_artifacts).
/// Callers are expected to acquire those bytes through an explicit review workflow; this function
/// performs no network access.
pub fn verify_adapter_source_bundle(
    manifest: &AdapterManifest,
    source_bytes: &[u8],
    additional_source_bytes: &BTreeMap<String, Vec<u8>>,
    license_text_bytes: &[u8],
) -> Result<(), TabletopError> {
    verify_adapter_source(manifest, source_bytes, license_text_bytes)?;
    if additional_source_bytes.len() != manifest.provenance.additional_artifacts.len() {
        return Err(invalid(
            "provenance.additional_artifacts",
            "companion source artifact set is incomplete",
        ));
    }
    for artifact in &manifest.provenance.additional_artifacts {
        let Some(bytes) = additional_source_bytes.get(&artifact.exact_artifact) else {
            return Err(invalid(
                "provenance.additional_artifacts",
                "companion source artifact set is incomplete",
            ));
        };
        if sha256_bytes(bytes) != artifact.sha256 {
            return Err(TabletopError::ContentHashMismatch {
                path: "provenance.additional_artifacts.sha256",
            });
        }
    }
    Ok(())
}

/// Validate an installed set and its zero-or-one primary selection against exact manifests.
pub fn validate_adapter_selection(
    selection: &AdapterSelection,
    manifests: &[AdapterManifest],
) -> Result<(), TabletopError> {
    if selection.selection_format_version != ADAPTER_SELECTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "selection_format_version",
        });
    }
    if selection.installed.len() > 1_024 {
        return Err(invalid("installed", "too many installed adapters"));
    }
    if selection.primary.len() > 1 {
        return Err(TabletopError::PrimaryConflict);
    }
    let mut expected = BTreeMap::new();
    for manifest in manifests {
        let coordinate = resolved_adapter(manifest)?;
        if expected.insert(coordinate.id.clone(), coordinate).is_some() {
            return Err(invalid("manifests", "duplicate adapter identity"));
        }
    }
    let mut previous = None;
    let mut installed = BTreeSet::new();
    for (index, coordinate) in selection.installed.iter().enumerate() {
        validate_coordinate(&format!("installed[{index}]"), coordinate)?;
        if previous
            .as_ref()
            .is_some_and(|value| value >= &coordinate.id)
        {
            return Err(invalid("installed", "expected unique adapter-id order"));
        }
        previous = Some(coordinate.id.clone());
        let Some(exact) = expected.get(&coordinate.id) else {
            return Err(TabletopError::AdapterNotInstalled);
        };
        if exact != coordinate {
            return Err(TabletopError::ContentHashMismatch { path: "installed" });
        }
        installed.insert(coordinate.clone());
    }
    if let Some(primary) = selection.primary.first() {
        validate_coordinate("primary[0]", primary)?;
        if !installed.contains(primary) {
            return Err(TabletopError::AdapterNotInstalled);
        }
    }
    Ok(())
}

/// Validate one immutable Character-to-adapter projection.
pub fn validate_character_projection(
    projection: &TabletopCharacterProjection,
    manifests: &[AdapterManifest],
) -> Result<(), TabletopError> {
    if projection.projection_format_version != CHARACTER_PROJECTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "projection_format_version",
        });
    }
    validate_global_id("character_id", &projection.character_id)?;
    validate_sha256(
        "canonical_profile_sha256",
        &projection.canonical_profile_sha256,
    )?;
    if projection.canonical_character_write_back {
        return Err(TabletopError::CanonicalWriteBack);
    }
    let manifests = manifest_map(manifests)?;
    if let Some(active) = &projection.active {
        validate_character_definition("active", active, &manifests)?;
    }
    for (key, archived) in &projection.inactive {
        validate_local_id("inactive key", key)?;
        if key != &archived.adapter.namespace_key() {
            return Err(invalid(
                "inactive key",
                "archive key must match the exact adapter coordinate",
            ));
        }
        if projection
            .active
            .as_ref()
            .is_some_and(|active| active.adapter == archived.adapter)
        {
            return Err(invalid(
                "inactive",
                "active adapter cannot also be archived",
            ));
        }
        validate_character_definition(
            &format!("inactive.{key}"),
            &AdapterCharacterDefinition {
                adapter: archived.adapter.clone(),
                definition: archived.definition.clone(),
                definition_sha256: archived.definition_sha256.clone(),
                completed_creation_steps: Vec::new(),
            },
            &manifests,
        )?;
        validate_text(format!("inactive.{key}.reason"), &archived.reason, 1, 2_048)?;
    }
    let mut suggestion_ids = BTreeSet::new();
    for (index, suggestion) in projection.suggestions.iter().enumerate() {
        validate_suggestion(index, suggestion)?;
        if !suggestion_ids.insert(&suggestion.id) {
            return Err(invalid("suggestions", "duplicate suggestion identity"));
        }
    }
    Ok(())
}

/// Validate mutable state against the selected adapter's exact schema and hash.
pub fn validate_tabletop_state(
    state: &TabletopState,
    manifest: &AdapterManifest,
) -> Result<(), TabletopError> {
    validate_adapter_manifest(manifest)?;
    if state.state_format_version != ADAPTER_STATE_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "state_format_version",
        });
    }
    let expected = resolved_adapter(manifest)?;
    if state.adapter != expected {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    validate_global_id("owner_id", &state.owner_id)?;
    validate_sha256("definition_sha256", &state.definition_sha256)?;
    if state.entropy.algorithm != "sha256_counter_v1" {
        return Err(TabletopError::UnsupportedVersion {
            path: "entropy.algorithm",
        });
    }
    validate_typed_value("value", &state.value, &manifest.state_type, &manifest.types)
        .map_err(|_| TabletopError::SchemaMismatch { path: "value" })
}

/// Validate a request before it is handed to trusted resolver code.
pub fn validate_resolution_request(
    request: &ResolutionRequest,
    state: &TabletopState,
    manifest: &AdapterManifest,
) -> Result<(), TabletopError> {
    if request.resolver_format_version != RESOLVER_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "resolver_format_version",
        });
    }
    validate_local_id("request_id", &request.request_id)?;
    if request.adapter != resolved_adapter(manifest)? || request.adapter != state.adapter {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    let Some(operation) = manifest.operations.get(&request.operation) else {
        return Err(TabletopError::UndeclaredCapability {
            capability: request.capability,
        });
    };
    if operation.required_capability != request.capability
        || !has_capability(manifest, request.capability)
    {
        return Err(TabletopError::UndeclaredCapability {
            capability: request.capability,
        });
    }
    if request.expected_state_sha256 != canonical_fingerprint(state)? {
        return Err(TabletopError::StaleState);
    }
    if request.definition_sha256 != canonical_fingerprint(&request.definition)? {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    if request.definition_sha256 != state.definition_sha256 {
        return Err(TabletopError::StaleState);
    }
    validate_typed_value(
        "definition",
        &request.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    validate_typed_value(
        "input",
        &request.input,
        &operation.request_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "input" })
}

/// Validate a completed receipt, including payload hashes and state lineage.
pub fn validate_resolution_receipt(
    receipt: &ResolutionReceipt,
    manifest: &AdapterManifest,
) -> Result<(), TabletopError> {
    if receipt.resolver_format_version != RESOLVER_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "resolver_format_version",
        });
    }
    if receipt.adapter != resolved_adapter(manifest)? {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    let Some(operation) = manifest.operations.get(&receipt.operation) else {
        return Err(TabletopError::UndeclaredCapability {
            capability: receipt.capability,
        });
    };
    if operation.required_capability != receipt.capability
        || !has_capability(manifest, receipt.capability)
    {
        return Err(TabletopError::UndeclaredCapability {
            capability: receipt.capability,
        });
    }
    validate_sha256("request_sha256", &receipt.request_sha256)?;
    validate_sha256("before_state_sha256", &receipt.before_state_sha256)?;
    if receipt.before_revision == u64::MAX
        || receipt.after_state.revision != receipt.before_revision + 1
        || receipt.entropy_before.algorithm != "sha256_counter_v1"
        || receipt.entropy_before.seed != receipt.after_state.entropy.seed
        || receipt.after_state.entropy.cursor < receipt.entropy_before.cursor
        || receipt.entropy_consumed
            != receipt.after_state.entropy.cursor - receipt.entropy_before.cursor
    {
        return Err(invalid(
            "receipt lineage",
            "revision or entropy transition is inconsistent",
        ));
    }
    if receipt.after_state_sha256 != canonical_fingerprint(&receipt.after_state)? {
        return Err(TabletopError::ContentHashMismatch {
            path: "after_state_sha256",
        });
    }
    validate_tabletop_state(&receipt.after_state, manifest)?;
    for (index, event) in receipt.events.iter().enumerate() {
        if event.sequence != index as u64 {
            return Err(invalid("events", "expected contiguous event sequence"));
        }
        validate_local_id("events.kind", &event.kind)?;
        if event.capability != receipt.capability || !operation.event_kinds.contains(&event.kind) {
            return Err(TabletopError::UndeclaredEvent);
        }
        let Some(value_type) = manifest.event_types.get(&event.kind) else {
            return Err(TabletopError::UndeclaredEvent);
        };
        validate_sha256("events.payload_sha256", &event.payload_sha256)?;
        if let Some(payload) = &event.payload {
            if event.payload_sha256 != canonical_fingerprint(payload)? {
                return Err(TabletopError::ContentHashMismatch {
                    path: "events.payload_sha256",
                });
            }
            validate_typed_value("events.payload", payload, value_type, &manifest.types).map_err(
                |_| TabletopError::SchemaMismatch {
                    path: "events.payload",
                },
            )?;
        }
    }
    Ok(())
}

/// Validate a receipt against the exact request whose hash and public operation it records.
pub fn validate_resolution_receipt_for_request(
    receipt: &ResolutionReceipt,
    manifest: &AdapterManifest,
    request: &ResolutionRequest,
) -> Result<(), TabletopError> {
    validate_resolution_receipt(receipt, manifest)?;
    if receipt.request_sha256 != canonical_fingerprint(request)?
        || receipt.adapter != request.adapter
        || receipt.operation != request.operation
        || receipt.capability != request.capability
        || receipt.before_state_sha256 != request.expected_state_sha256
        || receipt.after_state.definition_sha256 != request.definition_sha256
    {
        return Err(TabletopError::ContentHashMismatch {
            path: "request_sha256",
        });
    }
    Ok(())
}

/// True when an exact capability declaration is present.
#[must_use]
pub fn has_capability(manifest: &AdapterManifest, capability: TabletopCapability) -> bool {
    manifest
        .capabilities
        .iter()
        .any(|declaration| declaration.capability == capability && declaration.version == 1)
}

fn validate_capabilities(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    if manifest.capabilities.is_empty() {
        return Err(invalid(
            "capabilities",
            "at least one capability is required",
        ));
    }
    let mut previous = None;
    for declaration in &manifest.capabilities {
        if declaration.version != 1 {
            return Err(TabletopError::UnsupportedVersion {
                path: "capabilities.version",
            });
        }
        if previous.is_some_and(|value| value >= declaration.capability) {
            return Err(invalid("capabilities", "expected unique capability order"));
        }
        previous = Some(declaration.capability);
    }
    Ok(())
}

fn validate_type_contract(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    for name in manifest.types.keys() {
        validate_type_name("types", name)?;
    }
    let mut stack = BTreeSet::new();
    validate_type_expression(
        "definition_type",
        &manifest.definition_type,
        &manifest.types,
        &mut stack,
        0,
    )?;
    validate_type_expression(
        "state_type",
        &manifest.state_type,
        &manifest.types,
        &mut stack,
        0,
    )?;
    for (name, value_type) in &manifest.types {
        validate_type_expression(
            &format!("types.{name}"),
            value_type,
            &manifest.types,
            &mut stack,
            0,
        )?;
    }
    Ok(())
}

fn validate_type_expression(
    path: &str,
    value_type: &TypeExpression,
    types: &BTreeMap<String, TypeExpression>,
    stack: &mut BTreeSet<String>,
    depth: usize,
) -> Result<(), TabletopError> {
    if depth > 32 {
        return Err(invalid(path, "type exceeds nesting limit"));
    }
    match value_type {
        TypeExpression::String {
            min_length,
            max_length,
        } if min_length > max_length || *max_length > 65_536 => {
            Err(invalid(path, "invalid string bounds"))
        }
        TypeExpression::Symbol { values } => {
            if values.is_empty() || values.len() > 4_096 || !strictly_ordered(values) {
                return Err(invalid(path, "expected unique ordered symbol values"));
            }
            for value in values {
                validate_local_id(path, value)?;
            }
            Ok(())
        }
        TypeExpression::List {
            items,
            min_items,
            max_items,
        } if min_items > max_items || *max_items > 65_536 => {
            let _ = items;
            Err(invalid(path, "invalid list bounds"))
        }
        TypeExpression::List { items, .. } => {
            validate_type_expression(path, items, types, stack, depth + 1)
        }
        TypeExpression::Map {
            values,
            min_entries,
            max_entries,
        } if min_entries > max_entries || *max_entries > 65_536 => {
            let _ = values;
            Err(invalid(path, "invalid map bounds"))
        }
        TypeExpression::Map { values, .. } => {
            validate_type_expression(path, values, types, stack, depth + 1)
        }
        TypeExpression::Object { fields } => {
            if fields.len() > 4_096 {
                return Err(invalid(path, "too many object fields"));
            }
            for (name, field) in fields {
                validate_local_id(path, name)?;
                validate_field(path, field, types, stack, depth + 1)?;
            }
            Ok(())
        }
        TypeExpression::Named { name } => {
            let Some(value_type) = types.get(name) else {
                return Err(invalid(path, "unknown named type"));
            };
            if !stack.insert(name.clone()) {
                return Err(invalid(path, "recursive named types are forbidden"));
            }
            let result = validate_type_expression(path, value_type, types, stack, depth + 1);
            stack.remove(name);
            result
        }
        TypeExpression::Number {
            minimum, maximum, ..
        } if minimum.is_some_and(|value| !value.is_finite())
            || maximum.is_some_and(|value| !value.is_finite())
            || minimum.zip(*maximum).is_some_and(|(min, max)| min > max) =>
        {
            Err(invalid(path, "invalid numeric bounds"))
        }
        _ => Ok(()),
    }
}

fn validate_field(
    path: &str,
    field: &FieldDeclaration,
    types: &BTreeMap<String, TypeExpression>,
    stack: &mut BTreeSet<String>,
    depth: usize,
) -> Result<(), TabletopError> {
    validate_type_expression(path, &field.value_type, types, stack, depth)
}

fn validate_creation_steps(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    let mut ids = BTreeSet::new();
    for (index, step) in manifest.creation_steps.iter().enumerate() {
        validate_local_id(&format!("creation_steps[{index}].id"), &step.id)?;
        validate_text(
            format!("creation_steps[{index}].title"),
            &step.title,
            1,
            160,
        )?;
        validate_text(
            format!("creation_steps[{index}].description"),
            &step.description,
            1,
            2_048,
        )?;
        if !ids.insert(&step.id) {
            return Err(invalid("creation_steps", "duplicate step identity"));
        }
        if !has_capability(manifest, step.required_capability) {
            return Err(TabletopError::UndeclaredCapability {
                capability: step.required_capability,
            });
        }
        let mut field_ids = BTreeSet::new();
        for field in &step.fields {
            validate_local_id("creation_steps.fields.id", &field.id)?;
            validate_text("creation_steps.fields.label", &field.label, 1, 160)?;
            validate_text(
                "creation_steps.fields.description",
                &field.description,
                1,
                2_048,
            )?;
            if !field_ids.insert(&field.id) {
                return Err(invalid("creation_steps.fields", "duplicate field identity"));
            }
            validate_type_expression(
                "creation_steps.fields.value_type",
                &field.value_type,
                &manifest.types,
                &mut BTreeSet::new(),
                0,
            )?;
            if field.authority == CreationFieldAuthority::CanonicalSuggestion && field.required {
                return Err(invalid(
                    "creation_steps.fields.authority",
                    "canonical suggestions cannot be required adapter input",
                ));
            }
        }
    }
    if has_capability(manifest, TabletopCapability::CharacterCreation)
        && manifest.creation_steps.is_empty()
    {
        return Err(invalid(
            "creation_steps",
            "character creation capability requires at least one step",
        ));
    }
    Ok(())
}

fn validate_operations(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    for (kind, value_type) in &manifest.event_types {
        validate_local_id("event_types key", kind)?;
        validate_type_expression(
            "event_types",
            value_type,
            &manifest.types,
            &mut BTreeSet::new(),
            0,
        )?;
    }
    for (id, operation) in &manifest.operations {
        validate_local_id("operations key", id)?;
        if &operation.id != id {
            return Err(invalid(
                "operations.id",
                "operation id must match its map key",
            ));
        }
        validate_text("operations.title", &operation.title, 1, 160)?;
        validate_text("operations.description", &operation.description, 1, 2_048)?;
        if !has_capability(manifest, operation.required_capability) {
            return Err(TabletopError::UndeclaredCapability {
                capability: operation.required_capability,
            });
        }
        validate_type_expression(
            "operations.request_type",
            &operation.request_type,
            &manifest.types,
            &mut BTreeSet::new(),
            0,
        )?;
        if operation.event_kinds.is_empty() || !strictly_ordered(&operation.event_kinds) {
            return Err(invalid(
                "operations.event_kinds",
                "expected unique ordered event kinds",
            ));
        }
        for kind in &operation.event_kinds {
            validate_local_id("operations.event_kinds", kind)?;
            if !manifest.event_types.contains_key(kind) {
                return Err(invalid(
                    "operations.event_kinds",
                    "event kind has no declared payload type",
                ));
            }
        }
    }
    Ok(())
}

fn validate_migrations(manifest: &AdapterManifest) -> Result<(), TabletopError> {
    let mut ids = BTreeSet::new();
    for migration in &manifest.migrations {
        validate_local_id("migrations.id", &migration.id)?;
        validate_global_id("migrations.from_adapter_id", &migration.from_adapter_id)?;
        parse_requirement("migrations.from_version", &migration.from_version)?;
        if migration.from_schema_version == 0 || migration.to_schema_version == 0 {
            return Err(invalid("migrations", "schema versions must be positive"));
        }
        if !migration.reviewed || !migration.preserves_canonical_character {
            return Err(invalid(
                "migrations",
                "migration must be reviewed and preserve canonical Character data",
            ));
        }
        validate_text("migrations.description", &migration.description, 1, 2_048)?;
        if !ids.insert(&migration.id) {
            return Err(invalid("migrations", "duplicate migration identity"));
        }
    }
    Ok(())
}

fn validate_adapter_provenance(provenance: &AdapterProvenance) -> Result<(), TabletopError> {
    validate_https("provenance.source_url", &provenance.source_url)?;
    validate_artifact_path("provenance.exact_artifact", &provenance.exact_artifact)?;
    validate_text("provenance.revision", &provenance.revision, 1, 256)?;
    validate_date("provenance.retrieved_on", &provenance.retrieved_on)?;
    validate_sha256("provenance.sha256", &provenance.sha256)?;
    let eligible = matches!(
        (provenance.source_class, provenance.license.as_str()),
        (AdapterSourceClass::Original, "MIT")
            | (AdapterSourceClass::VerifiedCc0, "CC0-1.0")
            | (AdapterSourceClass::SeparatelyLicensedApache2, "Apache-2.0")
    );
    if !eligible {
        return Err(TabletopError::LicenseRejected);
    }
    validate_https("provenance.license_url", &provenance.license_url)?;
    if provenance.covered_files_or_sections.is_empty()
        || provenance.covered_files_or_sections.len() > 1_024
    {
        return Err(invalid(
            "provenance.covered_files_or_sections",
            "expected explicit covered material",
        ));
    }
    for value in &provenance.covered_files_or_sections {
        validate_text("provenance.covered_files_or_sections", value, 1, 512)?;
    }
    if provenance.exclusions.as_slice() != REQUIRED_EXCLUSIONS {
        return Err(invalid(
            "provenance.exclusions",
            "required excluded-material boundary is incomplete",
        ));
    }
    validate_text("provenance.attribution", &provenance.attribution, 1, 2_048)?;
    validate_artifact_path(
        "provenance.required_license_text.artifact",
        &provenance.required_license_text.artifact,
    )?;
    validate_sha256(
        "provenance.required_license_text.sha256",
        &provenance.required_license_text.sha256,
    )?;
    let mut previous_artifact = None;
    for artifact in &provenance.additional_artifacts {
        validate_https(
            "provenance.additional_artifacts.source_url",
            &artifact.source_url,
        )?;
        validate_artifact_path(
            "provenance.additional_artifacts.exact_artifact",
            &artifact.exact_artifact,
        )?;
        if artifact.exact_artifact == provenance.exact_artifact
            || previous_artifact
                .as_ref()
                .is_some_and(|previous| previous >= &artifact.exact_artifact)
        {
            return Err(invalid(
                "provenance.additional_artifacts",
                "expected unique companion artifacts ordered by exact name",
            ));
        }
        previous_artifact = Some(artifact.exact_artifact.clone());
        validate_text(
            "provenance.additional_artifacts.revision",
            &artifact.revision,
            1,
            256,
        )?;
        validate_date(
            "provenance.additional_artifacts.retrieved_on",
            &artifact.retrieved_on,
        )?;
        validate_sha256("provenance.additional_artifacts.sha256", &artifact.sha256)?;
        validate_media_type(
            "provenance.additional_artifacts.media_type",
            &artifact.media_type,
        )?;
        validate_text(
            "provenance.additional_artifacts.purpose",
            &artifact.purpose,
            1,
            512,
        )?;
    }
    if provenance.notices.len() > 128 {
        return Err(invalid("provenance.notices", "too many notices"));
    }
    for notice in &provenance.notices {
        validate_text("provenance.notices", notice, 1, 2_048)?;
    }
    if provenance.source_class == AdapterSourceClass::SeparatelyLicensedApache2
        && provenance.notices.is_empty()
    {
        return Err(invalid(
            "provenance.notices",
            "Apache-2.0 material requires an explicit notice record",
        ));
    }
    validate_text(
        "provenance.compatibility_statement",
        &provenance.compatibility_statement,
        1,
        1_024,
    )
}

fn validate_media_type(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() > 127
        || !value.contains('/')
        || value.bytes().any(|byte| {
            !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'/' | b'.' | b'+' | b'-'))
        })
    {
        return Err(invalid(path, "expected a lowercase media type"));
    }
    Ok(())
}

fn validate_character_definition(
    path: &str,
    definition: &AdapterCharacterDefinition,
    manifests: &BTreeMap<String, &AdapterManifest>,
) -> Result<(), TabletopError> {
    let Some(manifest) = manifests.get(&definition.adapter.id) else {
        return Err(TabletopError::AdapterNotInstalled);
    };
    if definition.adapter != resolved_adapter(manifest)? {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    if definition.definition_sha256 != canonical_fingerprint(&definition.definition)? {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    validate_typed_value(
        &format!("{path}.definition"),
        &definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    let declared = manifest
        .creation_steps
        .iter()
        .map(|step| step.id.as_str())
        .collect::<BTreeSet<_>>();
    if !strictly_ordered(&definition.completed_creation_steps)
        || definition
            .completed_creation_steps
            .iter()
            .any(|step| !declared.contains(step.as_str()))
    {
        return Err(invalid(
            "completed_creation_steps",
            "unknown, duplicate, or unordered creation step",
        ));
    }
    Ok(())
}

fn validate_suggestion(
    index: usize,
    suggestion: &CanonicalSuggestion,
) -> Result<(), TabletopError> {
    let path = format!("suggestions[{index}]");
    validate_local_id(&format!("{path}.id"), &suggestion.id)?;
    if suggestion.target_path.is_empty() || suggestion.target_path.len() > 32 {
        return Err(invalid(
            format!("{path}.target_path"),
            "expected a bounded canonical path",
        ));
    }
    for segment in &suggestion.target_path {
        validate_local_id(&format!("{path}.target_path"), segment)?;
    }
    let mut nodes = 0;
    validate_untyped_value(
        &format!("{path}.proposed_value"),
        &suggestion.proposed_value,
        0,
        &mut nodes,
    )?;
    validate_text(
        format!("{path}.explanation"),
        &suggestion.explanation,
        1,
        4_096,
    )?;
    if suggestion.source_paths.is_empty() || !strictly_ordered(&suggestion.source_paths) {
        return Err(invalid(
            format!("{path}.source_paths"),
            "expected unique ordered explanation inputs",
        ));
    }
    for source_path in &suggestion.source_paths {
        validate_text(format!("{path}.source_paths"), source_path, 1, 512)?;
    }
    let rationale_required = matches!(
        suggestion.decision,
        SuggestionDecision::Rejected
            | SuggestionDecision::Overridden
            | SuggestionDecision::Withheld
    );
    match &suggestion.rationale {
        Some(rationale) => validate_text(format!("{path}.rationale"), rationale, 1, 2_048),
        None if rationale_required => Err(invalid(
            format!("{path}.rationale"),
            "decision requires a rationale",
        )),
        None => Ok(()),
    }
}

fn validate_untyped_value(
    path: &str,
    value: &weave_domain::DomainValue,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), TabletopError> {
    *nodes += 1;
    if depth > 32 || *nodes > 262_144 {
        return Err(invalid(path, "value exceeds structural limits"));
    }
    match value {
        weave_domain::DomainValue::Number(value) if !value.is_finite() => {
            Err(invalid(path, "number must be finite"))
        }
        weave_domain::DomainValue::String(value) | weave_domain::DomainValue::Symbol(value) => {
            validate_text(path, value, 0, 65_536)
        }
        weave_domain::DomainValue::List(values) => {
            if values.len() > 65_536 {
                return Err(invalid(path, "list exceeds structural limits"));
            }
            for (index, value) in values.iter().enumerate() {
                validate_untyped_value(&format!("{path}[{index}]"), value, depth + 1, nodes)?;
            }
            Ok(())
        }
        weave_domain::DomainValue::Object(values) => {
            if values.len() > 65_536 {
                return Err(invalid(path, "object exceeds structural limits"));
            }
            for (name, value) in values {
                validate_local_id(path, name)?;
                validate_untyped_value(&format!("{path}.{name}"), value, depth + 1, nodes)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn manifest_map(
    manifests: &[AdapterManifest],
) -> Result<BTreeMap<String, &AdapterManifest>, TabletopError> {
    let mut result = BTreeMap::new();
    for manifest in manifests {
        validate_adapter_manifest(manifest)?;
        if result.insert(manifest.id.clone(), manifest).is_some() {
            return Err(invalid("manifests", "duplicate adapter identity"));
        }
    }
    Ok(result)
}

fn validate_coordinate(path: &str, coordinate: &ResolvedAdapter) -> Result<(), TabletopError> {
    validate_global_id(&format!("{path}.id"), &coordinate.id)?;
    validate_semver(&format!("{path}.version"), &coordinate.version)?;
    if coordinate.schema_version == 0 {
        return Err(invalid(
            format!("{path}.schema_version"),
            "expected a positive schema version",
        ));
    }
    validate_sha256(
        &format!("{path}.content_sha256"),
        &coordinate.content_sha256,
    )
}

fn validate_global_id(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() > 255
        || value.split('.').count() < 3
        || value.split('.').any(|segment| !valid_local_id(segment))
    {
        return Err(invalid(path, "expected a namespaced lowercase identifier"));
    }
    Ok(())
}

fn validate_local_id(path: &str, value: &str) -> Result<(), TabletopError> {
    if !valid_local_id(value) {
        return Err(invalid(path, "expected a lowercase identifier"));
    }
    Ok(())
}

fn valid_local_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && value.len() <= 128
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_type_name(path: &str, value: &str) -> Result<(), TabletopError> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        || !chars.all(|character| character.is_ascii_alphanumeric())
    {
        return Err(invalid(path, "expected an ASCII UpperCamelCase type name"));
    }
    Ok(())
}

fn validate_text(
    path: impl Into<String>,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), TabletopError> {
    let path = path.into();
    if !(minimum..=maximum).contains(&value.chars().count()) {
        return Err(invalid(path, "text length is outside allowed bounds"));
    }
    if contains_sensitive(value) {
        return Err(invalid(path, "value resembles a secret or credential"));
    }
    Ok(())
}

fn contains_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if (lower.contains("-----begin") && lower.contains("private key-----"))
        || lower.contains("github_pat_")
        || lower.contains("ghp_")
    {
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

fn validate_semver(path: &str, value: &str) -> Result<(), TabletopError> {
    Version::parse(value)
        .map(|_| ())
        .map_err(|_| invalid(path, "expected a semantic version"))
}

fn parse_requirement(path: &str, value: &str) -> Result<(), TabletopError> {
    VersionReq::parse(value)
        .map(|_| ())
        .map_err(|_| invalid(path, "expected a semantic-version requirement"))
}

fn validate_sha256(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid(path, "expected a SHA-256 digest"))
    }
}

fn validate_https(path: &str, value: &str) -> Result<(), TabletopError> {
    let parsed = Url::parse(value).map_err(|_| invalid(path, "expected a public HTTPS URL"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(invalid(
            path,
            "expected a public HTTPS URL without credentials",
        ));
    }
    Ok(())
}

fn validate_artifact_path(path: &str, value: &str) -> Result<(), TabletopError> {
    validate_text(path, value, 1, 512)?;
    if value.starts_with('/')
        || value.starts_with('~')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid(
            path,
            "expected a safe project-relative artifact path",
        ));
    }
    Ok(())
}

fn validate_date(path: &str, value: &str) -> Result<(), TabletopError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(invalid(path, "expected an ISO 8601 calendar date"));
    }
    let year = value[0..4]
        .parse::<u32>()
        .map_err(|_| invalid(path, "expected an ISO 8601 calendar date"))?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| invalid(path, "expected an ISO 8601 calendar date"))?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|_| invalid(path, "expected an ISO 8601 calendar date"))?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days {
        return Err(invalid(path, "expected an ISO 8601 calendar date"));
    }
    Ok(())
}

fn strictly_ordered<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn invalid(path: impl Into<String>, reason: &'static str) -> TabletopError {
    TabletopError::InvalidField {
        path: path.into(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventVisibility;
    use weave_domain::DomainValue;

    #[test]
    fn secret_detection_is_fail_closed() {
        assert!(contains_sensitive(
            "credential ghp_<synthetic-not-a-secret>"
        ));
        let private_key_marker = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
        assert!(contains_sensitive(&private_key_marker));
        assert!(!contains_sensitive("public synthetic fixture"));
    }

    #[test]
    fn every_visibility_is_orderable_for_canonical_policies() {
        assert!(strictly_ordered(&[
            EventVisibility::Public,
            EventVisibility::Authoring,
            EventVisibility::HostOnly,
        ]));
    }

    #[test]
    fn domain_values_remain_host_independent() {
        let value = DomainValue::Object(BTreeMap::from([(
            "score".to_owned(),
            DomainValue::Number(2.0),
        )]));
        assert_eq!(canonical_fingerprint(&value).unwrap().len(), 64);
    }
}
