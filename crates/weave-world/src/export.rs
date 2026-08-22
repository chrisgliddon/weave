use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use weave_domain::{
    DomainError, ResolvedDomainModule, parse_strict_json, to_pretty_json, to_pretty_ron,
};

use crate::composition::validate_receipt;
use crate::model::{
    CompactWorldExport, CompactWorldPlace, FullWorldExport, WORLD_COMPOSITION_FORMAT_VERSION,
    WORLD_EXPORT_FORMAT_VERSION, WorldCompositionPlan, WorldCompositionReceipt,
};
use crate::{ResolvedWorld, WORLD_MODULE_ID, WorldError, WorldValueOrigin, resolve_world};

const COMPOSITION_SCHEMA_ID: &str = "urn:weave:schema:world-composition:1";
const FULL_EXPORT_SCHEMA_ID: &str = "urn:weave:schema:world-full:1";
const COMPACT_EXPORT_SCHEMA_ID: &str = "urn:weave:schema:world-compact:1";

/// Redaction-safe failure at the portable World export boundary.
#[derive(Debug, thiserror::Error)]
pub enum WorldExportError {
    /// World resolution failed.
    #[error(transparent)]
    World(#[from] WorldError),
    /// Serialization or strict JSON parsing failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Composition and selected module coordinates do not describe the same build.
    #[error("portable World export metadata is inconsistent")]
    InconsistentMetadata,
    /// Serialized format version is unsupported.
    #[error("unsupported portable World export version")]
    UnsupportedVersion,
}

impl FullWorldExport {
    /// Parse strict full-export JSON.
    pub fn from_json(source: &str) -> Result<Self, WorldExportError> {
        let export = parse_strict_json(source)?;
        validate_full(&export)?;
        Ok(export)
    }

    /// Parse strict full-export RON.
    pub fn from_ron(source: &str) -> Result<Self, WorldExportError> {
        let export: Self = ron::from_str(source).map_err(|_| DomainError::InvalidRon)?;
        validate_full(&export)?;
        Ok(export)
    }

    /// Serialize stable full-export JSON.
    pub fn to_json(&self) -> Result<String, WorldExportError> {
        validate_full(self)?;
        Ok(to_pretty_json(self)?)
    }

    /// Serialize stable full-export RON.
    pub fn to_ron(&self) -> Result<String, WorldExportError> {
        validate_full(self)?;
        Ok(to_pretty_ron(self)?)
    }
}

impl CompactWorldExport {
    /// Parse strict compact-export JSON.
    pub fn from_json(source: &str) -> Result<Self, WorldExportError> {
        let export = parse_strict_json(source)?;
        validate_compact(&export)?;
        Ok(export)
    }

    /// Parse strict compact-export RON.
    pub fn from_ron(source: &str) -> Result<Self, WorldExportError> {
        let export: Self = ron::from_str(source).map_err(|_| DomainError::InvalidRon)?;
        validate_compact(&export)?;
        Ok(export)
    }

    /// Serialize stable compact-export JSON.
    pub fn to_json(&self) -> Result<String, WorldExportError> {
        validate_compact(self)?;
        Ok(to_pretty_json(self)?)
    }

    /// Serialize stable compact-export RON.
    pub fn to_ron(&self) -> Result<String, WorldExportError> {
        validate_compact(self)?;
        Ok(to_pretty_ron(self)?)
    }
}

/// Resolve one compiled World activation into full and compact portable projections.
pub fn export_world(
    module: &ResolvedDomainModule,
    composition: &WorldCompositionReceipt,
) -> Result<(FullWorldExport, CompactWorldExport), WorldExportError> {
    if module.manifest.id != WORLD_MODULE_ID
        || composition.format_version != WORLD_COMPOSITION_FORMAT_VERSION
        || module.pack.id != composition.output.id
        || module.pack.version != composition.output.version
    {
        return Err(WorldExportError::InconsistentMetadata);
    }
    let world = resolve_world(module)?;
    let full = FullWorldExport {
        format_version: WORLD_EXPORT_FORMAT_VERSION,
        module_id: module.manifest.id.clone(),
        module_version: module.manifest.version.clone(),
        pack_id: module.pack.id.clone(),
        pack_version: module.pack.version.clone(),
        composition: composition.clone(),
        authored_override_paths: module
            .authored_overrides
            .iter()
            .map(|authored| authored.path.clone())
            .collect(),
        world: world.clone(),
    };
    let compact = CompactWorldExport {
        format_version: WORLD_EXPORT_FORMAT_VERSION,
        module_id: module.manifest.id.clone(),
        module_version: module.manifest.version.clone(),
        pack_id: module.pack.id.clone(),
        pack_version: module.pack.version.clone(),
        rules: world.rules.clone(),
        roots: world.roots.clone(),
        places: world
            .places
            .into_iter()
            .map(|(id, place)| {
                (
                    id,
                    CompactWorldPlace {
                        id: place.id,
                        name: place.name,
                        kind: place.kind,
                        order: place.order,
                        parent_id: place.parent_id,
                        related_place_ids: place.related_place_ids,
                        environment: place.environment.values,
                        climate: place.climate.values,
                        attributes: place.attributes,
                    },
                )
            })
            .collect(),
    };
    validate_full(&full)?;
    validate_compact(&compact)?;
    Ok((full, compact))
}

/// Generate the canonical World composition-plan JSON Schema.
pub fn world_composition_schema() -> Result<String, WorldExportError> {
    schema::<WorldCompositionPlan>(
        COMPOSITION_SCHEMA_ID,
        "Weave World Composition v1",
        "format_version",
        WORLD_COMPOSITION_FORMAT_VERSION,
    )
}

/// Generate the canonical full World export JSON Schema.
pub fn full_world_schema() -> Result<String, WorldExportError> {
    schema::<FullWorldExport>(
        FULL_EXPORT_SCHEMA_ID,
        "Weave World Full Export v1",
        "format_version",
        WORLD_EXPORT_FORMAT_VERSION,
    )
}

/// Generate the canonical compact World export JSON Schema.
pub fn compact_world_schema() -> Result<String, WorldExportError> {
    schema::<CompactWorldExport>(
        COMPACT_EXPORT_SCHEMA_ID,
        "Weave World Compact Export v1",
        "format_version",
        WORLD_EXPORT_FORMAT_VERSION,
    )
}

fn validate_full(export: &FullWorldExport) -> Result<(), WorldExportError> {
    if export.format_version != WORLD_EXPORT_FORMAT_VERSION {
        return Err(WorldExportError::UnsupportedVersion);
    }
    validate_receipt(&export.composition).map_err(|_| WorldExportError::InconsistentMetadata)?;
    if export.module_id != WORLD_MODULE_ID
        || export.pack_id != export.composition.output.id
        || export.pack_version != export.composition.output.version
        || export.composition.format_version != WORLD_COMPOSITION_FORMAT_VERSION
        || !strict_paths(&export.authored_override_paths)
        || !valid_resolved_world(&export.world)
    {
        return Err(WorldExportError::InconsistentMetadata);
    }
    Ok(())
}

fn validate_compact(export: &CompactWorldExport) -> Result<(), WorldExportError> {
    if export.format_version != WORLD_EXPORT_FORMAT_VERSION {
        return Err(WorldExportError::UnsupportedVersion);
    }
    if export.module_id != WORLD_MODULE_ID
        || Version::parse(&export.module_version).is_err()
        || Version::parse(&export.pack_version).is_err()
        || export.pack_id.is_empty()
        || !valid_place_map(&export.places, &export.roots)
        || export.places.values().any(|place| {
            !valid_environment_fields(place.environment.keys().map(String::as_str))
                || !valid_climate_fields(place.climate.keys().map(String::as_str))
        })
    {
        return Err(WorldExportError::InconsistentMetadata);
    }
    Ok(())
}

fn strict_paths(paths: &[Vec<String>]) -> bool {
    paths
        .iter()
        .all(|path| !path.is_empty() && path.iter().all(|segment| valid_identifier(segment)))
        && paths.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_resolved_world(world: &ResolvedWorld) -> bool {
    let mut expected_roots = world
        .places
        .values()
        .filter(|place| place.parent_id.is_none())
        .map(|place| (place.order, place.id.as_str()))
        .collect::<Vec<_>>();
    expected_roots.sort_unstable();
    if expected_roots
        .iter()
        .map(|(_, id)| *id)
        .ne(world.roots.iter().map(String::as_str))
    {
        return false;
    }
    world.places.iter().all(|(id, place)| {
        place.id == *id
            && valid_resolved_parent_chain(&world.places, id)
            && place
                .related_place_ids
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && place.related_place_ids.iter().all(|related| {
                world.places.get(related).is_some_and(|other| {
                    related != id && other.related_place_ids.binary_search(id).is_ok()
                })
            })
            && place
                .environment
                .values
                .keys()
                .eq(place.environment.origins.keys())
            && place.climate.values.keys().eq(place.climate.origins.keys())
            && valid_environment_fields(place.environment.values.keys().map(String::as_str))
            && valid_climate_fields(place.climate.values.keys().map(String::as_str))
            && place
                .environment
                .origins
                .values()
                .all(|origin| valid_origin(origin, id, &world.places, world.places.len() + 1))
            && place
                .climate
                .origins
                .values()
                .all(|origin| valid_origin(origin, id, &world.places, world.places.len() + 1))
    })
}

fn valid_resolved_parent_chain(
    places: &BTreeMap<String, crate::ResolvedWorldPlace>,
    start: &str,
) -> bool {
    let mut current = Some(start);
    let mut remaining = places.len().saturating_add(1);
    while let Some(id) = current {
        if remaining == 0 {
            return false;
        }
        let Some(place) = places.get(id) else {
            return false;
        };
        current = place.parent_id.as_deref();
        remaining -= 1;
    }
    true
}

fn valid_origin(
    origin: &WorldValueOrigin,
    owner: &str,
    places: &BTreeMap<String, crate::ResolvedWorldPlace>,
    remaining: usize,
) -> bool {
    if remaining == 0 {
        return false;
    }
    match origin {
        WorldValueOrigin::Generated { source_path }
        | WorldValueOrigin::Authored { source_path } => valid_export_path(source_path),
        WorldValueOrigin::Inherited {
            from_place_id,
            upstream,
        } => {
            from_place_id != owner
                && places.contains_key(from_place_id)
                && valid_origin(upstream, from_place_id, places, remaining - 1)
        }
    }
}

fn valid_export_path(path: &str) -> bool {
    !path.is_empty() && path.split('.').all(valid_identifier)
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

fn valid_environment_fields<'a>(fields: impl Iterator<Item = &'a str>) -> bool {
    fields.collect::<BTreeSet<_>>()
        == BTreeSet::from([
            "coastal",
            "island_system",
            "primary_biome",
            "terrain",
            "water_setting",
            "weather_tendencies",
        ])
}

fn valid_climate_fields<'a>(fields: impl Iterator<Item = &'a str>) -> bool {
    fields.collect::<BTreeSet<_>>()
        == BTreeSet::from([
            "annual_mean_temperature_c",
            "annual_precipitation_mm",
            "band",
        ])
}

fn valid_place_map(places: &BTreeMap<String, CompactWorldPlace>, roots: &[String]) -> bool {
    let mut expected_roots = places
        .values()
        .filter(|place| place.parent_id.is_none())
        .map(|place| (place.order, place.id.as_str()))
        .collect::<Vec<_>>();
    expected_roots.sort_unstable();
    if expected_roots
        .iter()
        .map(|(_, id)| *id)
        .ne(roots.iter().map(String::as_str))
    {
        return false;
    }
    places.iter().all(|(id, place)| {
        place.id == *id
            && valid_parent_chain(places, id)
            && place
                .related_place_ids
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && place.related_place_ids.iter().all(|related| {
                places.get(related).is_some_and(|other| {
                    related != id && other.related_place_ids.binary_search(id).is_ok()
                })
            })
    })
}

fn valid_parent_chain(places: &BTreeMap<String, CompactWorldPlace>, start: &str) -> bool {
    let mut current = Some(start);
    let mut remaining = places.len().saturating_add(1);
    while let Some(id) = current {
        if remaining == 0 {
            return false;
        }
        let Some(place) = places.get(id) else {
            return false;
        };
        current = place.parent_id.as_deref();
        remaining -= 1;
    }
    true
}

fn schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version_property: &str,
    version: u32,
) -> Result<String, WorldExportError> {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| DomainError::Serialize)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-world-format-version".to_owned(),
            serde_json::Value::from(version),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut(version_property))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    Ok(to_pretty_json(&value)?)
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                sort_json_keys(value);
            }
            object.sort_keys();
        }
        _ => {}
    }
}
