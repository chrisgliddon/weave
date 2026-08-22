//! Resolution of stable fictional places layered over a validated Weave World reference seed.
//!
//! This crate is host-independent and data-only: it never parses source, performs I/O, accesses a
//! network, or depends on an editor or game engine. The shared compiler keeps immutable pack
//! defaults separate from authored replacements; this crate resolves the World-specific
//! environment and climate inheritance declared by those values.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, ResolvedDomainModule};

mod composition;
mod export;
mod model;

pub use composition::{ComposedWorldPack, WorldCompositionError, compose_world_pack};
pub use export::{
    WorldExportError, compact_world_schema, export_world, full_world_schema,
    world_composition_schema,
};
pub use model::{
    AppliedWorldLayer, CompactWorldExport, CompactWorldPlace, FullWorldExport,
    WORLD_COMPOSITION_FORMAT_VERSION, WORLD_EXPORT_FORMAT_VERSION, WorldCompositionPlan,
    WorldCompositionReceipt, WorldConflictPolicy, WorldLayerDifference, WorldLayerResolution,
    WorldLayerRole, WorldLayerSelection, WorldLayerSpec, WorldOutputPack, WorldPackSelector,
};

/// Stable identity of the reference Weave World module.
pub const WORLD_MODULE_ID: &str = "org.weave.world";

/// One effective field's authorship and inheritance chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WorldValueOrigin {
    /// Deterministically normalized from the selected public reference pack.
    Generated {
        /// Stable pack-value path.
        source_path: String,
    },
    /// Explicit fictional decision written in source.
    Authored {
        /// Stable authored override path.
        source_path: String,
    },
    /// Value inherited from another stable fictional place.
    Inherited {
        /// Immediate place from which the value was inherited.
        from_place_id: String,
        /// Ultimate generated or authored source, including deeper inheritance when present.
        upstream: Box<WorldValueOrigin>,
    },
}

/// Resolved local environment with one origin record per field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedEnvironment {
    /// Effective portable fields.
    pub values: BTreeMap<String, DomainValue>,
    /// Field names mapped to exact ownership.
    pub origins: BTreeMap<String, WorldValueOrigin>,
}

/// Resolved local climate with one origin record per field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedClimate {
    /// Effective portable fields.
    pub values: BTreeMap<String, DomainValue>,
    /// Field names mapped to exact ownership.
    pub origins: BTreeMap<String, WorldValueOrigin>,
}

/// One stable place after environment and climate inheritance are resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedWorldPlace {
    /// Stable source-facing id.
    pub id: String,
    /// Human-editable fictional display name.
    pub name: String,
    /// Broad place kind.
    pub kind: String,
    /// Deterministic sibling order.
    pub order: i64,
    /// Optional stable parent id.
    pub parent_id: Option<String>,
    /// Symmetric related place ids.
    pub related_place_ids: Vec<String>,
    /// Effective local environment.
    pub environment: ResolvedEnvironment,
    /// Effective local climate.
    pub climate: ResolvedClimate,
    /// Optional typed authored attributes.
    pub attributes: Option<DomainValue>,
}

/// Complete authored world projection suitable for an editor or runtime host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedWorld {
    /// Optional typed world-rule maps.
    pub rules: Option<DomainValue>,
    /// Root place ids in deterministic order.
    pub roots: Vec<String>,
    /// Resolved places keyed by stable id.
    pub places: BTreeMap<String, ResolvedWorldPlace>,
}

/// Redaction-safe failure while interpreting already validated World values.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorldError {
    /// A different module was passed to the resolver.
    #[error("selected domain module is not Weave World")]
    WrongModule,
    /// A required field is absent or has an unexpected shape.
    #[error("invalid Weave World value at `{path}`: {reason}")]
    InvalidValue {
        /// Stable schema path; values are never echoed.
        path: String,
        /// Redaction-safe explanation.
        reason: &'static str,
    },
    /// An inheritance cycle survived upstream validation.
    #[error("Weave World inheritance contains a cycle")]
    InheritanceCycle,
}

#[derive(Debug, Clone)]
struct RawPlace {
    id: String,
    name: String,
    kind: String,
    order: i64,
    parent_id: Option<String>,
    related_place_ids: Vec<String>,
    environment_source: Option<String>,
    climate_source: Option<String>,
    environment_override: BTreeMap<String, DomainValue>,
    climate_override: BTreeMap<String, DomainValue>,
    attributes: Option<DomainValue>,
}

/// Resolve stable World places and their environment/climate lineage.
pub fn resolve_world(module: &ResolvedDomainModule) -> Result<ResolvedWorld, WorldError> {
    if module.manifest.id != WORLD_MODULE_ID {
        return Err(WorldError::WrongModule);
    }
    let seed = object_at(&module.effective_values, &["seed"])?;
    let seed_environment = seed_environment(seed)?;
    let seed_climate = seed_climate(seed)?;
    let raw_places = parse_places(module.effective_values.get("places"))?;
    let mut memo = BTreeMap::new();
    for id in raw_places.keys() {
        resolve_place(
            id,
            &raw_places,
            &seed_environment,
            &seed_climate,
            &mut memo,
            &mut BTreeSet::new(),
        )?;
    }
    let mut roots = memo
        .values()
        .filter(|place| place.parent_id.is_none())
        .map(|place| (place.order, place.id.clone()))
        .collect::<Vec<_>>();
    roots.sort();
    Ok(ResolvedWorld {
        rules: module.effective_values.get("rules").cloned(),
        roots: roots.into_iter().map(|(_, id)| id).collect(),
        places: memo,
    })
}

fn seed_environment(
    seed: &BTreeMap<String, DomainValue>,
) -> Result<ResolvedEnvironment, WorldError> {
    let water = object_field("seed", seed, "water")?;
    let fields = [
        (
            "primary_biome",
            required_field("seed", seed, "primary_biome")?.clone(),
            "seed.primary_biome",
        ),
        (
            "terrain",
            required_field("seed", seed, "terrain")?.clone(),
            "seed.terrain",
        ),
        (
            "coastal",
            required_field("seed.water", water, "coastal")?.clone(),
            "seed.water.coastal",
        ),
        (
            "island_system",
            required_field("seed.water", water, "island_system")?.clone(),
            "seed.water.island_system",
        ),
        (
            "water_setting",
            required_field("seed.water", water, "setting")?.clone(),
            "seed.water.setting",
        ),
        (
            "weather_tendencies",
            required_field("seed", seed, "weather_tendencies")?.clone(),
            "seed.weather_tendencies",
        ),
    ];
    Ok(resolved_environment(fields))
}

fn resolved_environment<const N: usize>(
    fields: [(&str, DomainValue, &str); N],
) -> ResolvedEnvironment {
    let mut values = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for (name, value, source_path) in fields {
        values.insert(name.to_owned(), value);
        origins.insert(
            name.to_owned(),
            WorldValueOrigin::Generated {
                source_path: source_path.to_owned(),
            },
        );
    }
    ResolvedEnvironment { values, origins }
}

fn seed_climate(seed: &BTreeMap<String, DomainValue>) -> Result<ResolvedClimate, WorldError> {
    let climate = object_field("seed", seed, "climate")?;
    let fields = [
        "annual_mean_temperature_c",
        "annual_precipitation_mm",
        "band",
    ];
    let mut values = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for name in fields {
        values.insert(
            name.to_owned(),
            required_field("seed.climate", climate, name)?.clone(),
        );
        origins.insert(
            name.to_owned(),
            WorldValueOrigin::Generated {
                source_path: format!("seed.climate.{name}"),
            },
        );
    }
    Ok(ResolvedClimate { values, origins })
}

fn parse_places(value: Option<&DomainValue>) -> Result<BTreeMap<String, RawPlace>, WorldError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let DomainValue::Object(places) = value else {
        return Err(invalid("places", "expected an entity map"));
    };
    places
        .iter()
        .map(|(id, value)| parse_place(id, value).map(|place| (id.clone(), place)))
        .collect()
}

fn parse_place(id: &str, value: &DomainValue) -> Result<RawPlace, WorldError> {
    let path = format!("places.{id}");
    let DomainValue::Object(fields) = value else {
        return Err(invalid(&path, "expected an object"));
    };
    Ok(RawPlace {
        id: string_field(&path, fields, "id")?.to_owned(),
        name: string_field(&path, fields, "name")?.to_owned(),
        kind: symbol_field(&path, fields, "kind")?.to_owned(),
        order: number_field(&path, fields, "order")? as i64,
        parent_id: optional_reference(string_field(&path, fields, "parent_id")?),
        related_place_ids: string_list_field(&path, fields, "related_place_ids")?,
        environment_source: optional_reference(string_field(&path, fields, "environment_source")?),
        climate_source: optional_reference(string_field(&path, fields, "climate_source")?),
        environment_override: optional_object(
            fields.get("environment_override"),
            &format!("{path}.environment_override"),
        )?,
        climate_override: optional_object(
            fields.get("climate_override"),
            &format!("{path}.climate_override"),
        )?,
        attributes: fields.get("attributes").cloned(),
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_place(
    id: &str,
    raw_places: &BTreeMap<String, RawPlace>,
    seed_environment: &ResolvedEnvironment,
    seed_climate: &ResolvedClimate,
    memo: &mut BTreeMap<String, ResolvedWorldPlace>,
    visiting: &mut BTreeSet<String>,
) -> Result<ResolvedWorldPlace, WorldError> {
    if let Some(place) = memo.get(id) {
        return Ok(place.clone());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(WorldError::InheritanceCycle);
    }
    let raw = raw_places
        .get(id)
        .ok_or_else(|| invalid("places", "inheritance references an unknown place"))?;

    let mut environment = if let Some(source) = raw.environment_source.as_deref() {
        let source_place = resolve_place(
            source,
            raw_places,
            seed_environment,
            seed_climate,
            memo,
            visiting,
        )?;
        inherited_environment(source, &source_place.environment)
    } else {
        seed_environment.clone()
    };
    apply_environment_override(id, &raw.environment_override, &mut environment);

    let mut climate = if let Some(source) = raw.climate_source.as_deref() {
        let source_place = resolve_place(
            source,
            raw_places,
            seed_environment,
            seed_climate,
            memo,
            visiting,
        )?;
        inherited_climate(source, &source_place.climate)
    } else {
        seed_climate.clone()
    };
    apply_climate_override(id, &raw.climate_override, &mut climate);

    visiting.remove(id);
    let resolved = ResolvedWorldPlace {
        id: raw.id.clone(),
        name: raw.name.clone(),
        kind: raw.kind.clone(),
        order: raw.order,
        parent_id: raw.parent_id.clone(),
        related_place_ids: raw.related_place_ids.clone(),
        environment,
        climate,
        attributes: raw.attributes.clone(),
    };
    memo.insert(id.to_owned(), resolved.clone());
    Ok(resolved)
}

fn inherited_environment(from: &str, source: &ResolvedEnvironment) -> ResolvedEnvironment {
    ResolvedEnvironment {
        values: source.values.clone(),
        origins: source
            .origins
            .iter()
            .map(|(field, origin)| {
                (
                    field.clone(),
                    WorldValueOrigin::Inherited {
                        from_place_id: from.to_owned(),
                        upstream: Box::new(origin.clone()),
                    },
                )
            })
            .collect(),
    }
}

fn inherited_climate(from: &str, source: &ResolvedClimate) -> ResolvedClimate {
    ResolvedClimate {
        values: source.values.clone(),
        origins: source
            .origins
            .iter()
            .map(|(field, origin)| {
                (
                    field.clone(),
                    WorldValueOrigin::Inherited {
                        from_place_id: from.to_owned(),
                        upstream: Box::new(origin.clone()),
                    },
                )
            })
            .collect(),
    }
}

fn apply_environment_override(
    id: &str,
    authored: &BTreeMap<String, DomainValue>,
    resolved: &mut ResolvedEnvironment,
) {
    for (field, value) in authored {
        resolved.values.insert(field.clone(), value.clone());
        resolved.origins.insert(
            field.clone(),
            WorldValueOrigin::Authored {
                source_path: format!("places.{id}.environment_override.{field}"),
            },
        );
    }
}

fn apply_climate_override(
    id: &str,
    authored: &BTreeMap<String, DomainValue>,
    resolved: &mut ResolvedClimate,
) {
    for (field, value) in authored {
        resolved.values.insert(field.clone(), value.clone());
        resolved.origins.insert(
            field.clone(),
            WorldValueOrigin::Authored {
                source_path: format!("places.{id}.climate_override.{field}"),
            },
        );
    }
}

fn object_at<'a>(
    values: &'a BTreeMap<String, DomainValue>,
    path: &[&str],
) -> Result<&'a BTreeMap<String, DomainValue>, WorldError> {
    let (root, fields) = path
        .split_first()
        .ok_or_else(|| invalid("values", "path must not be empty"))?;
    let mut value = values
        .get(*root)
        .ok_or_else(|| invalid(root, "required export is missing"))?;
    for field in fields {
        let DomainValue::Object(object) = value else {
            return Err(invalid(&path.join("."), "expected an object"));
        };
        value = object
            .get(*field)
            .ok_or_else(|| invalid(&path.join("."), "required field is missing"))?;
    }
    match value {
        DomainValue::Object(object) => Ok(object),
        _ => Err(invalid(&path.join("."), "expected an object")),
    }
}

fn object_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a BTreeMap<String, DomainValue>, WorldError> {
    match fields.get(field) {
        Some(DomainValue::Object(value)) => Ok(value),
        _ => Err(invalid(&format!("{path}.{field}"), "expected an object")),
    }
}

fn required_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a DomainValue, WorldError> {
    fields
        .get(field)
        .ok_or_else(|| invalid(&format!("{path}.{field}"), "required field is missing"))
}

fn string_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, WorldError> {
    match fields.get(field) {
        Some(DomainValue::String(value)) => Ok(value),
        _ => Err(invalid(&format!("{path}.{field}"), "expected a string")),
    }
}

fn symbol_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, WorldError> {
    match fields.get(field) {
        Some(DomainValue::Symbol(value)) => Ok(value),
        _ => Err(invalid(&format!("{path}.{field}"), "expected a symbol")),
    }
}

fn number_field(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<f64, WorldError> {
    match fields.get(field) {
        Some(DomainValue::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(invalid(
            &format!("{path}.{field}"),
            "expected a finite number",
        )),
    }
}

fn string_list_field(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Vec<String>, WorldError> {
    let Some(DomainValue::List(values)) = fields.get(field) else {
        return Err(invalid(&format!("{path}.{field}"), "expected a list"));
    };
    values
        .iter()
        .map(|value| match value {
            DomainValue::String(value) => Ok(value.clone()),
            _ => Err(invalid(
                &format!("{path}.{field}"),
                "expected string list entries",
            )),
        })
        .collect()
}

fn optional_object(
    value: Option<&DomainValue>,
    path: &str,
) -> Result<BTreeMap<String, DomainValue>, WorldError> {
    match value {
        None => Ok(BTreeMap::new()),
        Some(DomainValue::Object(fields)) => Ok(fields.clone()),
        Some(_) => Err(invalid(path, "expected an object")),
    }
}

fn optional_reference(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn invalid(path: &str, reason: &'static str) -> WorldError {
    WorldError::InvalidValue {
        path: path.to_owned(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use weave_compiler::{CompileOptions, compile_with_modules};
    use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};

    use super::*;

    #[test]
    fn resolves_generated_inherited_and_authored_place_values() {
        let manifest = ModuleManifest::from_json(include_str!(
            "../../../examples/domain-modules/weave-world/module.weave-module.json"
        ))
        .expect("world manifest");
        let pack = DomainPack::from_json(include_str!(
            "../../../examples/domain-modules/weave-world/pack.weave-domain.json"
        ))
        .expect("world pack");
        let catalog = DomainCatalog::from_artifacts([manifest], [pack]).expect("world catalog");
        let compiled = compile_with_modules(
            include_str!("../../../examples/domain-modules/weave-world/authored-setting.weave"),
            &CompileOptions::default(),
            &catalog,
        )
        .expect("authored setting compiles");
        let world = resolve_world(&compiled.domain_modules["world"]).expect("resolve world");

        assert_eq!(world.roots, ["glasswind_reach"]);
        assert_eq!(world.places.len(), 4);
        assert!(matches!(
            world.places["glasswind_reach"].environment.origins["primary_biome"],
            WorldValueOrigin::Generated { .. }
        ));
        assert!(matches!(
            world.places["emberwake_harbor"].environment.origins["coastal"],
            WorldValueOrigin::Authored { .. }
        ));
        assert!(matches!(
            world.places["lantern_road"].environment.origins["primary_biome"],
            WorldValueOrigin::Inherited { .. }
        ));
        let WorldValueOrigin::Inherited { upstream, .. } =
            &world.places["saltglass_point"].environment.origins["coastal"]
        else {
            panic!("saltglass coastal value should inherit from harbor");
        };
        assert!(matches!(**upstream, WorldValueOrigin::Authored { .. }));
        assert!(matches!(
            world.places["saltglass_point"].climate.origins["annual_mean_temperature_c"],
            WorldValueOrigin::Authored { .. }
        ));
        assert_eq!(
            world.rules.as_ref().and_then(|rules| match rules {
                DomainValue::Object(fields) => fields.get("booleans"),
                _ => None,
            }),
            Some(&DomainValue::Object(BTreeMap::from([(
                "beacons_answer_storms".to_owned(),
                DomainValue::Bool(true),
            )])))
        );
    }
}
