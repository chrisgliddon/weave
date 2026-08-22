use std::collections::{BTreeMap, BTreeSet};

use semver::Version;
use sha2::{Digest, Sha256};
use weave_domain::{
    DOMAIN_PACK_FORMAT_VERSION, DomainError, DomainPack, DomainValue, ModuleManifest,
    ModuleRequirement, PackDependency, Provenance, ProvenanceSource, ProvenanceTransformation,
    parse_strict_json, to_pretty_json, to_pretty_ron, validate_manifest, validate_pack,
    validate_provenance,
};

use crate::WORLD_MODULE_ID;
use crate::model::{
    AppliedWorldLayer, WORLD_COMPOSITION_FORMAT_VERSION, WorldCompositionPlan,
    WorldCompositionReceipt, WorldConflictPolicy, WorldLayerDifference, WorldLayerResolution,
    WorldLayerSelection, WorldPackSelector,
};

/// One composed pack plus the exact decisions that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedWorldPack {
    /// Ordinary validated domain pack suitable for a catalog, registry, and lockfile.
    pub pack: DomainPack,
    /// Portable deterministic composition proof.
    pub receipt: WorldCompositionReceipt,
}

/// Redaction-safe failure while validating or applying a World composition plan.
#[derive(Debug, thiserror::Error)]
pub enum WorldCompositionError {
    /// The plan or one selected input violates the domain contract.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// A stable plan path violates the closed composition contract.
    #[error("invalid World composition at `{path}`: {reason}")]
    InvalidPlan {
        /// Stable schema or export path; rejected values are never included.
        path: String,
        /// Redaction-safe explanation.
        reason: &'static str,
    },
    /// An explicit reject policy encountered an earlier value.
    #[error("World composition conflict at `{path}`")]
    Conflict {
        /// Stable export path; values are never included.
        path: String,
    },
}

impl WorldCompositionPlan {
    /// Parse strict JSON without duplicate object keys, then validate the closed plan shape.
    pub fn from_json(source: &str) -> Result<Self, WorldCompositionError> {
        let plan = parse_strict_json(source)?;
        validate_plan(&plan)?;
        Ok(plan)
    }

    /// Parse strict RON, then validate the closed plan shape.
    pub fn from_ron(source: &str) -> Result<Self, WorldCompositionError> {
        let plan: Self = ron::from_str(source).map_err(|_| DomainError::InvalidRon)?;
        validate_plan(&plan)?;
        Ok(plan)
    }

    /// Serialize canonical human-readable JSON after validation.
    pub fn to_json(&self) -> Result<String, WorldCompositionError> {
        validate_plan(self)?;
        Ok(to_pretty_json(self)?)
    }

    /// Serialize canonical human-readable RON after validation.
    pub fn to_ron(&self) -> Result<String, WorldCompositionError> {
        validate_plan(self)?;
        Ok(to_pretty_ron(self)?)
    }
}

impl WorldCompositionReceipt {
    /// Parse one strict JSON receipt.
    pub fn from_json(source: &str) -> Result<Self, WorldCompositionError> {
        let receipt = parse_strict_json(source)?;
        validate_receipt(&receipt)?;
        Ok(receipt)
    }

    /// Parse one strict RON receipt.
    pub fn from_ron(source: &str) -> Result<Self, WorldCompositionError> {
        let receipt: Self = ron::from_str(source).map_err(|_| DomainError::InvalidRon)?;
        validate_receipt(&receipt)?;
        Ok(receipt)
    }

    /// Serialize canonical human-readable JSON.
    pub fn to_json(&self) -> Result<String, WorldCompositionError> {
        validate_receipt(self)?;
        Ok(to_pretty_json(self)?)
    }

    /// Serialize canonical human-readable RON.
    pub fn to_ron(&self) -> Result<String, WorldCompositionError> {
        validate_receipt(self)?;
        Ok(to_pretty_ron(self)?)
    }
}

/// Compose exact selected fields in declared broad-to-specific order.
///
/// Input packs remain immutable. The output is an ordinary pack whose exact selected inputs are
/// pack dependencies, so existing project locks pin the complete composition closure.
pub fn compose_world_pack(
    plan: &WorldCompositionPlan,
    manifest: &ModuleManifest,
    available_packs: impl IntoIterator<Item = DomainPack>,
    current_weave: &Version,
) -> Result<ComposedWorldPack, WorldCompositionError> {
    validate_plan(plan)?;
    if manifest.id != WORLD_MODULE_ID {
        return Err(invalid("manifest.id", "expected the Weave World module"));
    }
    validate_manifest(manifest, current_weave)?;

    let mut packs = BTreeMap::new();
    for pack in available_packs {
        let coordinate = (pack.id.clone(), pack.version.clone());
        if packs.insert(coordinate, pack).is_some() {
            return Err(invalid("packs", "duplicate exact pack coordinate"));
        }
    }

    let mut leaves = BTreeMap::<String, DomainValue>::new();
    let mut origins = BTreeMap::<String, String>::new();
    let mut leaf_claims = BTreeMap::<String, Vec<String>>::new();
    let mut sources = vec![composition_source(&plan.provenance)];
    let mut transformations = Vec::new();
    let mut dependencies = BTreeMap::<(String, String), PackDependency>::new();
    let mut applied_layers = Vec::with_capacity(plan.layers.len());
    let mut differences = Vec::new();

    for (layer_index, layer) in plan.layers.iter().enumerate() {
        let selected_index = select_candidate(plan.random_seed, layer_index, layer);
        let selected = &layer.candidates[selected_index];
        let pack = packs
            .get(&(selected.pack_id.clone(), selected.version.clone()))
            .ok_or_else(|| {
                invalid(
                    format!("layers[{layer_index}].candidates"),
                    "exact candidate pack is unavailable",
                )
            })?;
        validate_pack(pack, manifest, current_weave)?;
        if pack.id == plan.output.id {
            return Err(invalid(
                format!("layers[{layer_index}].candidates"),
                "output pack cannot depend on its own identity",
            ));
        }
        for (candidate_index, candidate) in layer.candidates.iter().enumerate() {
            let candidate_pack = packs
                .get(&(candidate.pack_id.clone(), candidate.version.clone()))
                .ok_or_else(|| {
                    invalid(
                        format!("layers[{layer_index}].candidates[{candidate_index}]"),
                        "exact candidate pack is unavailable",
                    )
                })?;
            validate_pack(candidate_pack, manifest, current_weave)?;
            validate_included_paths(candidate_pack, &layer.include, layer_index)?;
        }

        let dependency_key = (pack.module.id.clone(), pack.id.clone());
        let dependency = PackDependency {
            module_id: pack.module.id.clone(),
            pack_id: pack.id.clone(),
            version: format!("={}", pack.version),
        };
        if dependencies
            .get(&dependency_key)
            .is_some_and(|existing| existing.version != dependency.version)
        {
            return Err(invalid(
                format!("layers[{layer_index}].candidates"),
                "selected layers require different versions of one pack identity",
            ));
        }
        dependencies.insert(dependency_key, dependency);
        let copied = copy_provenance(layer_index, &pack.provenance)?;
        sources.extend(copied.sources);
        transformations.extend(copied.transformations);
        let selected_leaves = selected_leaves(pack, &layer.include)?;
        for (path, incoming) in selected_leaves {
            let previous_layer_id = origins.get(&path).cloned();
            let resolution = match (leaves.get(&path), layer.conflict) {
                (None, _) => {
                    leaves.insert(path.clone(), incoming);
                    origins.insert(path.clone(), layer.id.clone());
                    leaf_claims.insert(path.clone(), mapped_claim(pack, &path, &copied.ids)?);
                    WorldLayerResolution::Added
                }
                (Some(_), WorldConflictPolicy::Reject) => {
                    return Err(WorldCompositionError::Conflict { path });
                }
                (Some(_), WorldConflictPolicy::Keep) => WorldLayerResolution::Kept,
                (Some(previous), WorldConflictPolicy::Replace) => {
                    let resolution = if previous == &incoming {
                        WorldLayerResolution::Confirmed
                    } else {
                        WorldLayerResolution::Replaced
                    };
                    leaves.insert(path.clone(), incoming);
                    origins.insert(path.clone(), layer.id.clone());
                    leaf_claims.insert(path.clone(), mapped_claim(pack, &path, &copied.ids)?);
                    resolution
                }
            };
            differences.push(WorldLayerDifference {
                path,
                incoming_layer_id: layer.id.clone(),
                previous_layer_id,
                resolution,
            });
        }
        applied_layers.push(AppliedWorldLayer {
            id: layer.id.clone(),
            role: layer.role,
            selection: layer.selection,
            candidates: layer.candidates.clone(),
            selected_candidate: selected_index,
            pack: selected.clone(),
            include: layer.include.clone(),
            conflict: layer.conflict,
        });
    }

    let values = inflate_leaves(leaves)?;
    let mut root_inputs = leaf_claims
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>();
    root_inputs.insert("composition_plan".to_owned());
    transformations.push(ProvenanceTransformation {
        id: "world_composition".to_owned(),
        inputs: root_inputs.into_iter().collect(),
        description: "Composed selected World fields in declared broad-to-regional-to-ecosystem order; explicit conflict policies resolved every overlap, and seeded candidate layers used the stable SHA-256 chooser.".to_owned(),
    });
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    transformations.sort_by(|left, right| left.id.cmp(&right.id));

    let mut claims = leaf_claims
        .into_iter()
        .map(|(path, references)| (format!("values.{path}"), references))
        .collect::<BTreeMap<_, _>>();
    for export in values.keys() {
        claims.insert(
            format!("values.{export}"),
            vec!["world_composition".to_owned()],
        );
    }
    let pack = DomainPack {
        pack_format_version: DOMAIN_PACK_FORMAT_VERSION,
        id: plan.output.id.clone(),
        version: plan.output.version.clone(),
        title: plan.output.title.clone(),
        module: ModuleRequirement {
            id: manifest.id.clone(),
            version: format!("={}", manifest.version),
        },
        dependencies: dependencies.into_values().collect(),
        values,
        provenance: Provenance {
            sources,
            transformations,
            claims,
        },
    };
    validate_pack(&pack, manifest, current_weave)?;
    let receipt = WorldCompositionReceipt {
        format_version: WORLD_COMPOSITION_FORMAT_VERSION,
        output: plan.output.clone(),
        random_seed: plan.random_seed,
        layers: applied_layers,
        differences,
    };
    validate_receipt(&receipt)?;
    Ok(ComposedWorldPack { pack, receipt })
}

fn validate_plan(plan: &WorldCompositionPlan) -> Result<(), WorldCompositionError> {
    if plan.format_version != WORLD_COMPOSITION_FORMAT_VERSION {
        return Err(invalid("format_version", "unsupported composition format"));
    }
    if plan.layers.is_empty() || plan.layers.len() > 64 {
        return Err(invalid("layers", "expected between 1 and 64 layers"));
    }
    if plan.provenance.id != "composition_plan"
        || plan.provenance.kind != weave_domain::ProvenanceKind::Original
    {
        return Err(invalid(
            "provenance",
            "composition decisions require the original `composition_plan` source",
        ));
    }
    validate_provenance(&Provenance {
        sources: vec![plan.provenance.clone()],
        transformations: Vec::new(),
        claims: BTreeMap::from([(
            "composition_plan".to_owned(),
            vec!["composition_plan".to_owned()],
        )]),
    })?;
    Version::parse(&plan.output.version)
        .map_err(|_| invalid("output.version", "expected a semantic version"))?;
    if !valid_identifier(&plan.output.id) {
        return Err(invalid(
            "output.id",
            "expected a lowercase ASCII identifier",
        ));
    }
    if plan.output.title.is_empty() || plan.output.title.chars().count() > 160 {
        return Err(invalid(
            "output.title",
            "title length is outside the allowed bounds",
        ));
    }

    let mut previous: Option<(crate::model::WorldLayerRole, &str)> = None;
    for (layer_index, layer) in plan.layers.iter().enumerate() {
        let path = format!("layers[{layer_index}]");
        if !valid_identifier(&layer.id) {
            return Err(invalid(
                format!("{path}.id"),
                "expected a lowercase ASCII identifier",
            ));
        }
        if previous.is_some_and(|prior| prior >= (layer.role, layer.id.as_str())) {
            return Err(invalid(
                "layers",
                "layers must be unique and ordered by role then identifier",
            ));
        }
        previous = Some((layer.role, &layer.id));
        if layer.candidates.is_empty() || layer.candidates.len() > 64 {
            return Err(invalid(
                format!("{path}.candidates"),
                "expected between 1 and 64 candidates",
            ));
        }
        if layer.selection == WorldLayerSelection::Exact && layer.candidates.len() != 1 {
            return Err(invalid(
                format!("{path}.selection"),
                "exact selection requires one candidate",
            ));
        }
        let mut prior_candidate = None;
        for (candidate_index, candidate) in layer.candidates.iter().enumerate() {
            if !valid_identifier(&candidate.pack_id) {
                return Err(invalid(
                    format!("{path}.candidates[{candidate_index}].pack_id"),
                    "expected a lowercase ASCII identifier",
                ));
            }
            Version::parse(&candidate.version).map_err(|_| {
                invalid(
                    format!("{path}.candidates[{candidate_index}].version"),
                    "expected an exact semantic version",
                )
            })?;
            if prior_candidate.is_some_and(|prior: &WorldPackSelector| prior >= candidate) {
                return Err(invalid(
                    format!("{path}.candidates"),
                    "candidates must be unique and sorted",
                ));
            }
            prior_candidate = Some(candidate);
        }
        if layer.include.is_empty() || layer.include.len() > 256 {
            return Err(invalid(
                format!("{path}.include"),
                "expected between 1 and 256 included paths",
            ));
        }
        let mut prior_include: Option<&str> = None;
        for (include_index, include) in layer.include.iter().enumerate() {
            let segments = parse_path(include).ok_or_else(|| {
                invalid(
                    format!("{path}.include[{include_index}]"),
                    "expected a dot-separated lowercase export path",
                )
            })?;
            if prior_include.is_some_and(|prior| prior >= include.as_str()) {
                return Err(invalid(
                    format!("{path}.include"),
                    "included paths must be unique and sorted",
                ));
            }
            if layer
                .include
                .iter()
                .enumerate()
                .any(|(other_index, other)| {
                    other_index != include_index
                        && path_is_prefix(&segments, &other.split('.').collect::<Vec<_>>())
                })
            {
                return Err(invalid(
                    format!("{path}.include"),
                    "included paths must not overlap",
                ));
            }
            prior_include = Some(include);
        }
    }
    Ok(())
}

pub(crate) fn validate_receipt(
    receipt: &WorldCompositionReceipt,
) -> Result<(), WorldCompositionError> {
    if receipt.format_version != WORLD_COMPOSITION_FORMAT_VERSION {
        return Err(invalid("format_version", "unsupported composition receipt"));
    }
    if receipt.layers.is_empty() || receipt.layers.len() > 64 {
        return Err(invalid("layers", "receipt has an invalid layer count"));
    }
    Version::parse(&receipt.output.version)
        .map_err(|_| invalid("output.version", "expected a semantic version"))?;
    if !valid_identifier(&receipt.output.id) {
        return Err(invalid(
            "output.id",
            "expected a lowercase ASCII identifier",
        ));
    }
    if receipt.output.title.is_empty() || receipt.output.title.chars().count() > 160 {
        return Err(invalid(
            "output.title",
            "title length is outside the allowed bounds",
        ));
    }

    let mut layer_indices = BTreeMap::new();
    let mut previous_layer: Option<(crate::model::WorldLayerRole, &str)> = None;
    for (index, layer) in receipt.layers.iter().enumerate() {
        if !valid_identifier(&layer.id)
            || previous_layer.is_some_and(|previous| previous >= (layer.role, layer.id.as_str()))
        {
            return Err(invalid(
                "layers",
                "receipt layers must be unique and ordered by role then identifier",
            ));
        }
        if layer.candidates.is_empty()
            || layer.candidates.len() > 64
            || layer.candidates.iter().any(|candidate| {
                !valid_identifier(&candidate.pack_id) || Version::parse(&candidate.version).is_err()
            })
            || layer.candidates.windows(2).any(|pair| pair[0] >= pair[1])
            || (layer.selection == WorldLayerSelection::Exact && layer.candidates.len() != 1)
            || layer.selected_candidate >= layer.candidates.len()
            || layer.pack != layer.candidates[layer.selected_candidate]
            || layer.selected_candidate
                != selected_candidate_index(
                    receipt.random_seed,
                    index,
                    &layer.id,
                    layer.selection,
                    layer.candidates.len(),
                )
        {
            return Err(invalid(
                format!("layers[{index}]"),
                "receipt contains an invalid selected pack",
            ));
        }
        validate_includes(&layer.include, &format!("layers[{index}].include"))?;
        layer_indices.insert(layer.id.as_str(), index);
        previous_layer = Some((layer.role, &layer.id));
    }

    let mut previous_difference: Option<(usize, &str)> = None;
    for (index, difference) in receipt.differences.iter().enumerate() {
        if parse_path(&difference.path).is_none() {
            return Err(invalid(
                format!("differences[{index}].path"),
                "receipt contains an invalid export path",
            ));
        }
        let Some(&incoming_index) = layer_indices.get(difference.incoming_layer_id.as_str()) else {
            return Err(invalid(
                format!("differences[{index}].incoming_layer_id"),
                "receipt difference refers to an unknown layer",
            ));
        };
        if previous_difference
            .is_some_and(|previous| previous >= (incoming_index, difference.path.as_str()))
        {
            return Err(invalid(
                "differences",
                "receipt differences must follow layer and path order",
            ));
        }
        match (&difference.previous_layer_id, difference.resolution) {
            (None, WorldLayerResolution::Added) => {}
            (Some(previous_id), resolution) if resolution != WorldLayerResolution::Added => {
                let Some(&previous_index) = layer_indices.get(previous_id.as_str()) else {
                    return Err(invalid(
                        format!("differences[{index}].previous_layer_id"),
                        "receipt difference refers to an unknown prior layer",
                    ));
                };
                if previous_index >= incoming_index {
                    return Err(invalid(
                        format!("differences[{index}].previous_layer_id"),
                        "receipt difference must refer to an earlier layer",
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    format!("differences[{index}].resolution"),
                    "receipt resolution and prior layer are inconsistent",
                ));
            }
        }
        previous_difference = Some((incoming_index, &difference.path));
    }
    Ok(())
}

fn validate_includes(includes: &[String], path: &str) -> Result<(), WorldCompositionError> {
    if includes.is_empty() || includes.len() > 256 {
        return Err(invalid(path, "expected between 1 and 256 included paths"));
    }
    let parsed = includes
        .iter()
        .enumerate()
        .map(|(index, include)| {
            parse_path(include).ok_or_else(|| {
                invalid(
                    format!("{path}[{index}]"),
                    "expected a dot-separated lowercase export path",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if includes.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(path, "included paths must be unique and sorted"));
    }
    for (index, candidate) in parsed.iter().enumerate() {
        if parsed
            .iter()
            .enumerate()
            .any(|(other_index, other)| other_index != index && path_is_prefix(candidate, other))
        {
            return Err(invalid(path, "included paths must not overlap"));
        }
    }
    Ok(())
}

fn validate_included_paths(
    pack: &DomainPack,
    includes: &[String],
    layer_index: usize,
) -> Result<(), WorldCompositionError> {
    for (include_index, include) in includes.iter().enumerate() {
        if value_at(&pack.values, &include.split('.').collect::<Vec<_>>()).is_none() {
            return Err(invalid(
                format!("layers[{layer_index}].include[{include_index}]"),
                "included path is absent from a candidate pack",
            ));
        }
    }
    Ok(())
}

fn select_candidate(seed: u64, layer_index: usize, layer: &crate::model::WorldLayerSpec) -> usize {
    selected_candidate_index(
        seed,
        layer_index,
        &layer.id,
        layer.selection,
        layer.candidates.len(),
    )
}

fn selected_candidate_index(
    seed: u64,
    layer_index: usize,
    layer_id: &str,
    selection: WorldLayerSelection,
    candidate_count: usize,
) -> usize {
    if selection == WorldLayerSelection::Exact || candidate_count == 1 {
        return 0;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"weave-world-layer-selection-v1\0");
    hasher.update(seed.to_le_bytes());
    hasher.update((layer_index as u64).to_le_bytes());
    hasher.update(layer_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_le_bytes(bytes) % candidate_count as u64) as usize
}

fn selected_leaves(
    pack: &DomainPack,
    includes: &[String],
) -> Result<BTreeMap<String, DomainValue>, WorldCompositionError> {
    let mut selected = BTreeMap::new();
    for include in includes {
        let segments = include.split('.').collect::<Vec<_>>();
        let value = value_at(&pack.values, &segments)
            .ok_or_else(|| invalid("layers.include", "included path is absent"))?;
        let mut path = segments
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect();
        flatten_value(value, &mut path, &mut selected);
    }
    Ok(selected)
}

fn flatten_value(
    value: &DomainValue,
    path: &mut Vec<String>,
    output: &mut BTreeMap<String, DomainValue>,
) {
    match value {
        DomainValue::Object(fields) if !fields.is_empty() => {
            for (field, value) in fields {
                path.push(field.clone());
                flatten_value(value, path, output);
                path.pop();
            }
        }
        _ => {
            output.insert(path.join("."), value.clone());
        }
    }
}

fn inflate_leaves(
    leaves: BTreeMap<String, DomainValue>,
) -> Result<BTreeMap<String, DomainValue>, WorldCompositionError> {
    let mut values = BTreeMap::new();
    for (path, value) in leaves {
        let segments = path.split('.').collect::<Vec<_>>();
        insert_path(&mut values, &segments, value, &path)?;
    }
    Ok(values)
}

fn insert_path(
    object: &mut BTreeMap<String, DomainValue>,
    path: &[&str],
    value: DomainValue,
    display_path: &str,
) -> Result<(), WorldCompositionError> {
    let Some((segment, remaining)) = path.split_first() else {
        return Err(invalid(display_path, "path must not be empty"));
    };
    if remaining.is_empty() {
        if object.insert((*segment).to_owned(), value).is_some() {
            return Err(invalid(display_path, "path was produced more than once"));
        }
        return Ok(());
    }
    let entry = object
        .entry((*segment).to_owned())
        .or_insert_with(|| DomainValue::Object(BTreeMap::new()));
    let DomainValue::Object(child) = entry else {
        return Err(invalid(display_path, "path overlaps a scalar value"));
    };
    insert_path(child, remaining, value, display_path)
}

struct CopiedProvenance {
    sources: Vec<ProvenanceSource>,
    transformations: Vec<ProvenanceTransformation>,
    ids: BTreeMap<String, String>,
}

fn copy_provenance(
    layer_index: usize,
    provenance: &Provenance,
) -> Result<CopiedProvenance, WorldCompositionError> {
    let prefix = format!("layer{layer_index}_");
    let mut ids = BTreeMap::new();
    for source in &provenance.sources {
        ids.insert(source.id.clone(), format!("{prefix}{}", source.id));
    }
    for transformation in &provenance.transformations {
        ids.insert(
            transformation.id.clone(),
            format!("{prefix}{}", transformation.id),
        );
    }
    let sources = provenance
        .sources
        .iter()
        .map(|source| {
            let mut source = source.clone();
            source.id = ids[&source.id].clone();
            source
        })
        .collect();
    let transformations = provenance
        .transformations
        .iter()
        .map(|transformation| {
            Ok(ProvenanceTransformation {
                id: ids[&transformation.id].clone(),
                inputs: transformation
                    .inputs
                    .iter()
                    .map(|input| {
                        ids.get(input).cloned().ok_or_else(|| {
                            invalid(
                                "provenance.transformations.inputs",
                                "input provenance reference is unavailable",
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                description: transformation.description.clone(),
            })
        })
        .collect::<Result<Vec<_>, WorldCompositionError>>()?;
    Ok(CopiedProvenance {
        sources,
        transformations,
        ids,
    })
}

fn mapped_claim(
    pack: &DomainPack,
    leaf_path: &str,
    ids: &BTreeMap<String, String>,
) -> Result<Vec<String>, WorldCompositionError> {
    let mut segments = format!("values.{leaf_path}")
        .split('.')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    while !segments.is_empty() {
        if let Some(references) = pack.provenance.claims.get(&segments.join(".")) {
            return references
                .iter()
                .map(|reference| {
                    ids.get(reference).cloned().ok_or_else(|| {
                        invalid(
                            "provenance.claims",
                            "claim provenance reference is unavailable",
                        )
                    })
                })
                .collect();
        }
        segments.pop();
    }
    Err(invalid(
        "provenance.claims",
        "selected value has no provenance claim",
    ))
}

fn composition_source(source: &ProvenanceSource) -> ProvenanceSource {
    source.clone()
}

fn value_at<'a>(
    values: &'a BTreeMap<String, DomainValue>,
    path: &[&str],
) -> Option<&'a DomainValue> {
    let (export, fields) = path.split_first()?;
    let mut value = values.get(*export)?;
    for field in fields {
        let DomainValue::Object(object) = value else {
            return None;
        };
        value = object.get(*field)?;
    }
    Some(value)
}

fn parse_path(path: &str) -> Option<Vec<&str>> {
    if path.is_empty() || path.len() > 512 {
        return None;
    }
    let segments = path.split('.').collect::<Vec<_>>();
    segments
        .iter()
        .all(|segment| valid_identifier(segment))
        .then_some(segments)
}

fn path_is_prefix(left: &[&str], right: &[&str]) -> bool {
    left.len() < right.len() && right.starts_with(left)
}

fn valid_identifier(value: &str) -> bool {
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

fn invalid(path: impl Into<String>, reason: &'static str) -> WorldCompositionError {
    WorldCompositionError::InvalidPlan {
        path: path.into(),
        reason,
    }
}
