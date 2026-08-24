use std::error::Error;
use std::io;

use bevy::prelude::*;
use weave_core::ir::{DomainValueIr, StoryIr};

const TRACER_STORY: &str = include_str!("../../domain-modules/contract/tracer.story.ron");
const WORLD_STORIES: [&str; 4] = [
    include_str!(
        "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.ron"
    ),
    include_str!("../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.ron"),
    include_str!("../../domain-modules/weave-world/corpus/stories/maldives.story.ron"),
    include_str!("../../domain-modules/weave-world/reference-place.story.ron"),
];
const COMPOSED_WORLD_STORY: &str =
    include_str!("../../domain-modules/weave-world/composed-setting.story.ron");
const CHARACTER_STORY: &str =
    include_str!("../../domain-modules/weave-character/ari-vale.story.ron");
const TEMPORAL_CHARACTER_STORY: &str = include_str!(
    "../../domain-modules/weave-character/context/runtime/ari-vale-temporal.story.ron"
);

#[derive(Resource, Debug, Clone, PartialEq)]
struct ConstellationReading {
    label: String,
    phase: String,
    intensity: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct WorldReading {
    name: String,
    primary_biome: String,
    water_setting: String,
    minimum_annual_mean_temperature_c: f64,
    maximum_annual_mean_temperature_c: f64,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct WorldReadings(Vec<WorldReading>);

#[derive(Resource, Debug, Clone, PartialEq)]
struct ComposedWorldReading {
    beacons_answer_storms: bool,
    harbor_name: String,
    harbor_parent_id: String,
    road_name: String,
    primary_biome: String,
    climate_band: String,
    harbor_coastal: bool,
    travel_behavior: String,
    presentation_palette: String,
    authored_value_count: usize,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct CharacterReading {
    id: String,
    display_name: String,
    factor_scores: [f64; 6],
    creativity: f64,
    ocean_openness: f64,
    ocean_is_lossy: bool,
    pronoun_subject: String,
    palette_accent: String,
    avatar_path: String,
    catalog_id: String,
    catalog_sha256: String,
    visual_tone: String,
    presentation_personality_write_back: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct AlignmentValueReading {
    id: String,
    label_id: String,
    label: String,
    decision: String,
    score_micros: Option<f64>,
    coverage_micros: f64,
    input_paths: Vec<String>,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct AlignmentCharacterReading {
    view_id: String,
    pack_id: String,
    pack_version: String,
    pack_sha256: String,
    review_sha256: String,
    applied_sha256: String,
    canonical_personality_write_back: bool,
    values: Vec<AlignmentValueReading>,
}

#[derive(Debug, Clone, PartialEq)]
struct TemporalCueReading {
    record_id: String,
    kind: String,
    decision: String,
    fact_source_ids: Vec<String>,
    cue_source_ids: Vec<String>,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct TemporalCharacterReading {
    canonical_personality_write_back: bool,
    accepted_record_ids: Vec<String>,
    cues: Vec<TemporalCueReading>,
}

#[derive(Debug, Clone, PartialEq)]
struct RelationshipEdgeReading {
    id: String,
    source_character_id: String,
    target_character_id: String,
    kind: String,
    origin: String,
    review: String,
    lock: String,
    evidence_count: usize,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct RelationshipGraphReading {
    canonical_personality_write_back: bool,
    kind_pack_id: String,
    kind_pack_version: String,
    kind_pack_sha256: String,
    edges: Vec<RelationshipEdgeReading>,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct ExpressionReading {
    term_id: String,
    term_surface: String,
    term_normalized: String,
    term_origin: String,
    preference_target: String,
    preference_polarity: String,
    template_id: String,
    scenario_id: String,
    pack_id: String,
    pack_version: String,
    pack_sha256: String,
    canonical_personality_write_back: bool,
}

fn reading_from_story(story: &StoryIr) -> Result<ConstellationReading, io::Error> {
    let module = story.modules.get("constellation").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "constellation module is absent")
    })?;
    let label = match module.value(&["observation", "label"]) {
        Some(DomainValueIr::String(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation observation label is invalid",
            ));
        }
    };
    let phase = match module.value(&["phase"]) {
        Some(DomainValueIr::Symbol(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation phase is invalid",
            ));
        }
    };
    let intensity = match module.value(&["observation", "intensity"]) {
        Some(DomainValueIr::Number(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation observation intensity is invalid",
            ));
        }
    };
    Ok(ConstellationReading {
        label,
        phase,
        intensity,
    })
}

fn world_reading_from_story(story: &StoryIr, alias: &str) -> Result<WorldReading, io::Error> {
    let module = story
        .modules
        .get(alias)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "world module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let symbol = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    Ok(WorldReading {
        name: string(
            &["seed", "identity", "display_name"],
            "world display name is invalid",
        )?,
        primary_biome: symbol(&["seed", "primary_biome"], "world primary biome is invalid")?,
        water_setting: symbol(
            &["seed", "water", "setting"],
            "world water setting is invalid",
        )?,
        minimum_annual_mean_temperature_c: number(
            &["seed", "climate", "annual_mean_temperature_c", "minimum"],
            "world minimum annual mean temperature is invalid",
        )?,
        maximum_annual_mean_temperature_c: number(
            &["seed", "climate", "annual_mean_temperature_c", "maximum"],
            "world maximum annual mean temperature is invalid",
        )?,
    })
}

fn composed_world_reading(story: &StoryIr) -> Result<ComposedWorldReading, io::Error> {
    let module = story
        .modules
        .get("world")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "world module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let symbol = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let boolean = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Bool(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let beacons_answer_storms = match module.value(&["rules", "booleans", "beacons_answer_storms"])
    {
        Some(DomainValueIr::Bool(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "authored world rule is invalid",
            ));
        }
    };
    let road_name = string(
        &["places", "lantern_road", "name"],
        "authored road name is invalid",
    )?;
    let primary_biome = symbol(
        &["seed", "primary_biome"],
        "composed primary biome is invalid",
    )?;
    let climate_band = symbol(
        &["seed", "climate", "band"],
        "composed climate band is invalid",
    )?;
    let harbor_coastal = boolean(
        &[
            "places",
            "emberwake_harbor",
            "environment_override",
            "coastal",
        ],
        "authored harbor environment is invalid",
    )?;
    let travel_mode = string(
        &["rules", "symbols", "travel_mode"],
        "authored travel mode is invalid",
    )?;
    let travel_behavior = if beacons_answer_storms && harbor_coastal && travel_mode == "tidebound" {
        format!("beacon escort via {road_name}")
    } else {
        "ordinary overland travel".to_owned()
    };
    let presentation_palette = match (primary_biome.as_str(), climate_band.as_str()) {
        ("temperate_conifer_forest", "humid_continental") => "cedar-snow",
        ("temperate_conifer_forest", _) => "cedar-rain",
        _ => "neutral-world",
    }
    .to_owned();
    Ok(ComposedWorldReading {
        beacons_answer_storms,
        harbor_name: string(
            &["places", "emberwake_harbor", "name"],
            "authored harbor name is invalid",
        )?,
        harbor_parent_id: string(
            &["places", "emberwake_harbor", "parent_id"],
            "authored harbor parent is invalid",
        )?,
        road_name,
        primary_biome,
        climate_band,
        harbor_coastal,
        travel_behavior,
        presentation_palette,
        authored_value_count: module.authored_overrides.len(),
    })
}

fn character_reading(story: &StoryIr) -> Result<CharacterReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let boolean = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Bool(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let factor = |name: &str| {
        number(
            &["profile", "hexaco", name, "summary", "projection_score"],
            "Character factor summary is invalid",
        )
    };
    Ok(CharacterReading {
        id: string(&["profile", "identity", "id"], "Character id is invalid")?,
        display_name: string(
            &["profile", "identity", "display_name", "value"],
            "Character display name is invalid",
        )?,
        factor_scores: [
            factor("honesty_humility")?,
            factor("emotionality")?,
            factor("extraversion")?,
            factor("agreeableness")?,
            factor("conscientiousness")?,
            factor("openness")?,
        ],
        creativity: number(
            &[
                "profile",
                "hexaco",
                "openness",
                "creativity",
                "projection_score",
            ],
            "Character creativity facet is invalid",
        )?,
        ocean_openness: number(
            &["profile", "ocean", "openness", "score"],
            "Character OCEAN openness is invalid",
        )?,
        ocean_is_lossy: boolean(
            &["profile", "ocean", "lossy"],
            "Character OCEAN lossiness marker is invalid",
        )?,
        pronoun_subject: string(
            &["profile", "presentation", "pronouns", "subject"],
            "Character presentation pronoun subject is invalid",
        )?,
        palette_accent: string(
            &["profile", "presentation", "palette", "colors", "accent"],
            "Character presentation accent is invalid",
        )?,
        avatar_path: safe_relative_asset_path(string(
            &[
                "profile",
                "presentation",
                "assets",
                "authored_avatar",
                "path",
            ],
            "Character presentation avatar path is invalid",
        )?)?,
        catalog_id: string(
            &[
                "profile",
                "presentation",
                "catalog_assignments",
                "avatar",
                "catalog",
                "id",
            ],
            "Character presentation catalog id is invalid",
        )?,
        catalog_sha256: string(
            &[
                "profile",
                "presentation",
                "catalog_assignments",
                "avatar",
                "catalog",
                "sha256",
            ],
            "Character presentation catalog hash is invalid",
        )?,
        visual_tone: string(
            &[
                "profile",
                "presentation",
                "catalog_assignments",
                "visual_tone",
                "value",
                "tag",
            ],
            "Character presentation visual tone is invalid",
        )?,
        presentation_personality_write_back: boolean(
            &[
                "profile",
                "presentation",
                "canonical_personality_write_back",
            ],
            "Character presentation write-back marker is invalid",
        )?,
    })
}

fn safe_relative_asset_path(value: String) -> Result<String, io::Error> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Character presentation avatar path is invalid",
        ));
    }
    Ok(value)
}

fn temporal_character_reading(story: &StoryIr) -> Result<TemporalCharacterReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let write_back = match module.value(&[
        "profile",
        "date_context",
        "canonical_personality_write_back",
    ]) {
        Some(DomainValueIr::Bool(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "temporal write-back marker is invalid",
            ));
        }
    };
    let accepted_record_ids = string_list(
        module.value(&["profile", "date_context", "accepted_record_ids"]),
        "accepted temporal record list is invalid",
    )?;
    let Some(DomainValueIr::List(values)) = module.value(&["profile", "date_context", "cues"])
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "reviewed temporal cue list is invalid",
        ));
    };
    let cues = values
        .iter()
        .map(|value| {
            let DomainValueIr::Object(fields) = value else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "reviewed temporal cue is invalid",
                ));
            };
            let string = |name: &str| match fields.get(name) {
                Some(DomainValueIr::String(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "reviewed temporal cue string is invalid",
                )),
            };
            let symbol = |name: &str| match fields.get(name) {
                Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "reviewed temporal cue symbol is invalid",
                )),
            };
            Ok(TemporalCueReading {
                record_id: string("record_id")?,
                kind: symbol("kind")?,
                decision: symbol("decision")?,
                fact_source_ids: string_list(
                    fields.get("fact_source_ids"),
                    "temporal fact lineage is invalid",
                )?,
                cue_source_ids: string_list(
                    fields.get("cue_source_ids"),
                    "temporal cue lineage is invalid",
                )?,
            })
        })
        .collect::<Result<Vec<_>, io::Error>>()?;
    Ok(TemporalCharacterReading {
        canonical_personality_write_back: write_back,
        accepted_record_ids,
        cues,
    })
}

fn alignment_character_reading(story: &StoryIr) -> Result<AlignmentCharacterReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let Some(DomainValueIr::Object(alignment)) = module.value(&["profile", "alignment"]) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "reviewed Character alignment is invalid",
        ));
    };
    let string = |fields: &std::collections::BTreeMap<String, DomainValueIr>,
                  name: &str,
                  message: &'static str| {
        match fields.get(name) {
            Some(DomainValueIr::String(value)) => Ok(value.clone()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
        }
    };
    let view_id = string(alignment, "view_id", "alignment view id is invalid")?;
    let review_sha256 = string(
        alignment,
        "review_sha256",
        "alignment review fingerprint is invalid",
    )?;
    let applied_sha256 = string(
        alignment,
        "applied_sha256",
        "alignment application fingerprint is invalid",
    )?;
    let canonical_personality_write_back = match alignment.get("canonical_personality_write_back") {
        Some(DomainValueIr::Bool(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "alignment write-back marker is invalid",
            ));
        }
    };
    let Some(DomainValueIr::Object(pack)) = alignment.get("pack") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "alignment pack coordinate is invalid",
        ));
    };
    let Some(DomainValueIr::Object(raw_values)) = alignment.get("values") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "approved alignment value map is invalid",
        ));
    };
    let values = raw_values
        .iter()
        .map(|(axis_id, value)| {
            let DomainValueIr::Object(fields) = value else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved alignment value is invalid",
                ));
            };
            let id = string(fields, "id", "approved alignment id is invalid")?;
            if id != *axis_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved alignment id does not match its map key",
                ));
            }
            let decision = match fields.get("decision") {
                Some(DomainValueIr::Symbol(value))
                    if matches!(value.as_str(), "reviewed" | "edited" | "overridden") =>
                {
                    value.clone()
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "approved alignment decision is invalid",
                    ));
                }
            };
            let score_micros = match fields.get("score_micros") {
                Some(DomainValueIr::Number(value)) => Some(*value),
                None => None,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "approved alignment score is invalid",
                    ));
                }
            };
            let coverage_micros = match fields.get("coverage_micros") {
                Some(DomainValueIr::Number(value)) => *value,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "approved alignment coverage is invalid",
                    ));
                }
            };
            Ok(AlignmentValueReading {
                id,
                label_id: string(fields, "label_id", "approved alignment label id is invalid")?,
                label: string(fields, "label", "approved alignment label is invalid")?,
                decision,
                score_micros,
                coverage_micros,
                input_paths: string_list(
                    fields.get("input_paths"),
                    "approved alignment input paths are invalid",
                )?,
            })
        })
        .collect::<Result<Vec<_>, io::Error>>()?;
    Ok(AlignmentCharacterReading {
        view_id,
        pack_id: string(pack, "id", "alignment pack id is invalid")?,
        pack_version: string(pack, "version", "alignment pack version is invalid")?,
        pack_sha256: string(pack, "sha256", "alignment pack fingerprint is invalid")?,
        review_sha256,
        applied_sha256,
        canonical_personality_write_back,
        values,
    })
}

fn relationship_graph_reading(story: &StoryIr) -> Result<RelationshipGraphReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let Some(DomainValueIr::Object(graph)) = module.value(&["profile", "relationships"]) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Character relationship graph is invalid",
        ));
    };
    let canonical_personality_write_back = match graph.get("canonical_personality_write_back") {
        Some(DomainValueIr::Bool(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relationship write-back marker is invalid",
            ));
        }
    };
    let Some(DomainValueIr::Object(kind_pack)) = graph.get("kind_pack") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relationship kind-pack coordinate is invalid",
        ));
    };
    let kind_pack_string = |name: &str| match kind_pack.get(name) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relationship kind-pack field is invalid",
        )),
    };
    let kind_pack_id = kind_pack_string("id")?;
    let kind_pack_version = kind_pack_string("version")?;
    let kind_pack_sha256 = kind_pack_string("sha256")?;
    if kind_pack_sha256.len() != 64
        || !kind_pack_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relationship kind-pack fingerprint is invalid",
        ));
    }
    let Some(DomainValueIr::Object(raw_edges)) = graph.get("edges") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "relationship edge map is invalid",
        ));
    };
    let edges = raw_edges
        .iter()
        .map(|(edge_id, value)| {
            let DomainValueIr::Object(fields) = value else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "relationship edge is invalid",
                ));
            };
            let string = |name: &str| match fields.get(name) {
                Some(DomainValueIr::String(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "relationship edge string is invalid",
                )),
            };
            let symbol = |name: &str| match fields.get(name) {
                Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "relationship edge symbol is invalid",
                )),
            };
            let id = string("id")?;
            if id != *edge_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "relationship edge id does not match its map key",
                ));
            }
            let evidence_count = match fields.get("evidence") {
                Some(DomainValueIr::List(values)) => values.len(),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "relationship evidence list is invalid",
                    ));
                }
            };
            Ok(RelationshipEdgeReading {
                id,
                source_character_id: string("source_character_id")?,
                target_character_id: string("target_character_id")?,
                kind: string("kind")?,
                origin: symbol("origin")?,
                review: symbol("review")?,
                lock: symbol("lock")?,
                evidence_count,
            })
        })
        .collect::<Result<Vec<_>, io::Error>>()?;
    Ok(RelationshipGraphReading {
        canonical_personality_write_back,
        kind_pack_id,
        kind_pack_version,
        kind_pack_sha256,
        edges,
    })
}

fn expression_reading(story: &StoryIr) -> Result<ExpressionReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let symbol = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let canonical_personality_write_back =
        match module.value(&["profile", "expression", "canonical_personality_write_back"]) {
            Some(DomainValueIr::Bool(value)) => *value,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expression write-back marker is invalid",
                ));
            }
        };
    let pack_sha256 = string(
        &[
            "profile",
            "expression",
            "template_assignments",
            "arrival_greeting",
            "pack",
            "sha256",
        ],
        "expression pack fingerprint is invalid",
    )?;
    if pack_sha256.len() != 64
        || !pack_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expression pack fingerprint is invalid",
        ));
    }
    Ok(ExpressionReading {
        term_id: string(
            &["profile", "expression", "lexicon", "trailmark", "id"],
            "expression term id is invalid",
        )?,
        term_surface: string(
            &["profile", "expression", "lexicon", "trailmark", "surface"],
            "expression term surface is invalid",
        )?,
        term_normalized: string(
            &[
                "profile",
                "expression",
                "lexicon",
                "trailmark",
                "normalized",
            ],
            "expression normalized term is invalid",
        )?,
        term_origin: symbol(
            &["profile", "expression", "lexicon", "trailmark", "origin"],
            "expression term origin is invalid",
        )?,
        preference_target: string(
            &[
                "profile",
                "expression",
                "preferences",
                "clear_questions",
                "target",
            ],
            "expression preference target is invalid",
        )?,
        preference_polarity: symbol(
            &[
                "profile",
                "expression",
                "preferences",
                "clear_questions",
                "polarity",
            ],
            "expression preference polarity is invalid",
        )?,
        template_id: string(
            &[
                "profile",
                "expression",
                "template_assignments",
                "arrival_greeting",
                "template_id",
            ],
            "expression template id is invalid",
        )?,
        scenario_id: string(
            &[
                "profile",
                "expression",
                "template_assignments",
                "arrival_greeting",
                "scenario_id",
            ],
            "expression scenario id is invalid",
        )?,
        pack_id: string(
            &[
                "profile",
                "expression",
                "template_assignments",
                "arrival_greeting",
                "pack",
                "id",
            ],
            "expression pack id is invalid",
        )?,
        pack_version: string(
            &[
                "profile",
                "expression",
                "template_assignments",
                "arrival_greeting",
                "pack",
                "version",
            ],
            "expression pack version is invalid",
        )?,
        pack_sha256,
        canonical_personality_write_back,
    })
}

fn string_list(
    value: Option<&DomainValueIr>,
    message: &'static str,
) -> Result<Vec<String>, io::Error> {
    let Some(DomainValueIr::List(values)) = value else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, message));
    };
    values
        .iter()
        .map(|value| match value {
            DomainValueIr::String(value) => Ok(value.clone()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
        })
        .collect()
}

fn report_reading(reading: Res<ConstellationReading>) {
    println!(
        "Bevy read {}: {} at {} intensity",
        reading.label, reading.phase, reading.intensity
    );
}

fn report_world(readings: Res<WorldReadings>) {
    for reading in &readings.0 {
        println!(
            "Bevy read {}: {} in an {} setting ({:.1}–{:.1} °C annual means)",
            reading.name,
            reading.primary_biome,
            reading.water_setting,
            reading.minimum_annual_mean_temperature_c,
            reading.maximum_annual_mean_temperature_c,
        );
    }
}

fn report_composed_world(reading: Res<ComposedWorldReading>) {
    println!(
        "Bevy read {} under {}: {}, {} palette, {}/{} environment, coastal={}, beacons={}, road={} ({} authored values)",
        reading.harbor_name,
        reading.harbor_parent_id,
        reading.travel_behavior,
        reading.presentation_palette,
        reading.primary_biome,
        reading.climate_band,
        reading.harbor_coastal,
        reading.beacons_answer_storms,
        reading.road_name,
        reading.authored_value_count,
    );
}

fn report_character(reading: Res<CharacterReading>) {
    println!(
        "Bevy read {} ({}): creativity {:.2}, derived OCEAN openness {:.2}, lossy={}, {} pronouns, {} accent, avatar {}, {} tone from {} ({}), presentation write-back={}",
        reading.display_name,
        reading.id,
        reading.creativity,
        reading.ocean_openness,
        reading.ocean_is_lossy,
        reading.pronoun_subject,
        reading.palette_accent,
        reading.avatar_path,
        reading.visual_tone,
        reading.catalog_id,
        reading.catalog_sha256,
        reading.presentation_personality_write_back,
    );
}

fn report_alignment_character(reading: Res<AlignmentCharacterReading>) {
    println!(
        "Bevy read {} approved alignment values from {}@{}; personality write-back={}",
        reading.values.len(),
        reading.pack_id,
        reading.pack_version,
        reading.canonical_personality_write_back,
    );
}

fn report_temporal_character(reading: Res<TemporalCharacterReading>) {
    println!(
        "Bevy read {} reviewed temporal cues across {} accepted records; personality write-back={}",
        reading.cues.len(),
        reading.accepted_record_ids.len(),
        reading.canonical_personality_write_back,
    );
}

fn report_relationship_graph(reading: Res<RelationshipGraphReading>) {
    println!(
        "Bevy read {} layered Character relationship edges from {}@{}; personality write-back={}",
        reading.edges.len(),
        reading.kind_pack_id,
        reading.kind_pack_version,
        reading.canonical_personality_write_back,
    );
}

fn report_expression(reading: Res<ExpressionReading>) {
    println!(
        "Bevy read {} expression term `{}` and {} preference `{}` with template {} for {} from {}@{}; personality write-back={}",
        reading.term_origin,
        reading.term_surface,
        reading.preference_polarity,
        reading.preference_target,
        reading.template_id,
        reading.scenario_id,
        reading.pack_id,
        reading.pack_version,
        reading.canonical_personality_write_back,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let story = ron::from_str::<StoryIr>(TRACER_STORY)?;
    let reading = reading_from_story(&story)?;
    let world_readings = WorldReadings(
        WORLD_STORIES
            .iter()
            .map(|source| -> Result<WorldReading, Box<dyn Error>> {
                let story = ron::from_str::<StoryIr>(source)?;
                Ok(world_reading_from_story(&story, "world")?)
            })
            .collect::<Result<_, _>>()?,
    );
    let composed_world = composed_world_reading(&ron::from_str::<StoryIr>(COMPOSED_WORLD_STORY)?)?;
    let character = character_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let alignment_character =
        alignment_character_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let relationship_graph =
        relationship_graph_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let expression = expression_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let temporal_character =
        temporal_character_reading(&ron::from_str::<StoryIr>(TEMPORAL_CHARACTER_STORY)?)?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .insert_resource(world_readings)
        .insert_resource(composed_world)
        .insert_resource(character)
        .insert_resource(alignment_character)
        .insert_resource(relationship_graph)
        .insert_resource(expression)
        .insert_resource(temporal_character)
        .add_systems(
            Startup,
            (
                report_reading,
                report_world,
                report_composed_world,
                report_character,
                report_alignment_character,
                report_relationship_graph,
                report_expression,
                report_temporal_character,
            ),
        );
    app.update();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_portable_tracer_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(TRACER_STORY).expect("checked tracer RON");
        assert_eq!(
            reading_from_story(&story).expect("read module export"),
            ConstellationReading {
                label: "Glasswing constellation".to_owned(),
                phase: "twilight".to_owned(),
                intensity: 0.625,
            }
        );
    }

    #[test]
    fn reads_four_portable_world_seeds_without_editor_dependencies() {
        assert_eq!(
            WORLD_STORIES
                .iter()
                .map(|source| {
                    let story = ron::from_str::<StoryIr>(source).expect("checked world RON");
                    world_reading_from_story(&story, "world").expect("read world export")
                })
                .collect::<Vec<_>>(),
            vec![
                WorldReading {
                    name: "British Columbia Temperate Forests".to_owned(),
                    primary_biome: "temperate_conifer_forest".to_owned(),
                    water_setting: "open_ocean_coast".to_owned(),
                    minimum_annual_mean_temperature_c: 7.0,
                    maximum_annual_mean_temperature_c: 7.0,
                },
                WorldReading {
                    name: "Hokkaido, Japan".to_owned(),
                    primary_biome: "temperate_broadleaf_and_mixed_forest".to_owned(),
                    water_setting: "semi_enclosed_sea_coast".to_owned(),
                    minimum_annual_mean_temperature_c: 6.13,
                    maximum_annual_mean_temperature_c: 6.13,
                },
                WorldReading {
                    name: "Maldives".to_owned(),
                    primary_biome: "tropical_and_subtropical_moist_broadleaf_forest".to_owned(),
                    water_setting: "oceanic_atolls".to_owned(),
                    minimum_annual_mean_temperature_c: 27.71,
                    maximum_annual_mean_temperature_c: 27.71,
                },
                WorldReading {
                    name: "Aotearoa New Zealand".to_owned(),
                    primary_biome: "temperate_broadleaf_and_mixed_forest".to_owned(),
                    water_setting: "oceanic_islands".to_owned(),
                    minimum_annual_mean_temperature_c: 8.8,
                    maximum_annual_mean_temperature_c: 16.0,
                },
            ]
        );
    }

    #[test]
    fn composed_rules_places_and_environment_change_host_behavior_and_presentation() {
        let story = ron::from_str::<StoryIr>(COMPOSED_WORLD_STORY).expect("composed World RON");
        assert_eq!(
            composed_world_reading(&story).expect("read composed World values"),
            ComposedWorldReading {
                beacons_answer_storms: true,
                harbor_name: "Emberwake Harbor".to_owned(),
                harbor_parent_id: "glasswind_reach".to_owned(),
                road_name: "Lantern Road".to_owned(),
                primary_biome: "temperate_conifer_forest".to_owned(),
                climate_band: "humid_continental".to_owned(),
                harbor_coastal: true,
                travel_behavior: "beacon escort via Lantern Road".to_owned(),
                presentation_palette: "cedar-snow".to_owned(),
                authored_value_count: 46,
            }
        );
    }

    #[test]
    fn reads_the_complete_character_profile_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(CHARACTER_STORY).expect("checked Character RON");
        assert_eq!(
            character_reading(&story).expect("read Character values"),
            CharacterReading {
                id: "org.weave.character.ari_vale".to_owned(),
                display_name: "Ari Vale, Wayfinder".to_owned(),
                factor_scores: [0.72, 0.57, 0.68, 0.63, 0.78, 0.83],
                creativity: 0.86,
                ocean_openness: 0.83,
                ocean_is_lossy: true,
                pronoun_subject: "they".to_owned(),
                palette_accent: "#D6A24A".to_owned(),
                avatar_path: "presentation/assets/ari-vale-avatar.svg".to_owned(),
                catalog_id: "org.weave.character.presentation.glasswind".to_owned(),
                catalog_sha256: "b1fdbb422b359dea37ebe00ee6ac68485410b13f4ff92e7c146012e4a6cb0cfa"
                    .to_owned(),
                visual_tone: "river_glass".to_owned(),
                presentation_personality_write_back: false,
            }
        );
    }

    #[test]
    fn rejects_unsafe_character_asset_paths_without_disclosing_them() {
        let rejected = "../private-avatar.svg";
        let error = safe_relative_asset_path(rejected.to_owned()).expect_err("unsafe path");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!error.to_string().contains(rejected));
    }

    #[test]
    fn reads_only_approved_alignment_values_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(CHARACTER_STORY).expect("checked Character RON");
        let reading =
            alignment_character_reading(&story).expect("read approved Character alignment");
        assert_eq!(reading.view_id, "org.weave.alignment.wayfinder_compass");
        assert_eq!(reading.pack_id, "org.weave.alignment.wayfinder_compass");
        assert_eq!(reading.pack_version, "1.0.0");
        assert_eq!(reading.pack_sha256.len(), 64);
        assert_eq!(reading.review_sha256.len(), 64);
        assert_eq!(reading.applied_sha256.len(), 64);
        assert!(!reading.canonical_personality_write_back);
        assert_eq!(reading.values.len(), 3);
        assert_eq!(
            reading
                .values
                .iter()
                .map(|value| (
                    value.id.as_str(),
                    value.label_id.as_str(),
                    value.label.as_str(),
                    value.decision.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                ("horizon", "seeking", "Seeking", "reviewed"),
                ("reciprocity", "mutual", "Mutual", "edited"),
                ("structure", "adapting", "Adapting", "overridden"),
            ]
        );
        assert!(reading.values.iter().all(|value| {
            value.score_micros.is_some()
                && value.coverage_micros == 1_000_000.0
                && !value.input_paths.is_empty()
                && value.id != "signal"
                && value.id != "tempo"
        }));
    }

    #[test]
    fn queries_layered_relationship_edges_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(CHARACTER_STORY).expect("checked Character RON");
        let reading = relationship_graph_reading(&story).expect("read relationship graph");
        assert!(!reading.canonical_personality_write_back);
        assert_eq!(reading.kind_pack_id, "org.weave.relationship.reference");
        assert_eq!(reading.kind_pack_version, "1.0.0");
        assert_eq!(reading.kind_pack_sha256.len(), 64);
        assert_eq!(
            reading.edges,
            vec![RelationshipEdgeReading {
                id: "mentor_sable".to_owned(),
                source_character_id: "org.weave.character.ari_vale".to_owned(),
                target_character_id: "org.weave.character.sable_reed".to_owned(),
                kind: "org.weave.relationship.mentor".to_owned(),
                origin: "authored".to_owned(),
                review: "not_required".to_owned(),
                lock: "unlocked".to_owned(),
                evidence_count: 0,
            }]
        );
    }

    #[test]
    fn reads_observable_expression_and_template_coordinates_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(CHARACTER_STORY).expect("checked Character RON");
        let reading = expression_reading(&story).expect("read Character expression");
        assert_eq!(
            reading,
            ExpressionReading {
                term_id: "trailmark".to_owned(),
                term_surface: "trailmark".to_owned(),
                term_normalized: "trailmark".to_owned(),
                term_origin: "pack_assigned".to_owned(),
                preference_target: "clear questions".to_owned(),
                preference_polarity: "prefer".to_owned(),
                template_id: "arrival_greeting".to_owned(),
                scenario_id: "arrival".to_owned(),
                pack_id: "org.weave.expression.glasswind".to_owned(),
                pack_version: "1.0.0".to_owned(),
                pack_sha256: reading.pack_sha256.clone(),
                canonical_personality_write_back: false,
            }
        );
        assert_eq!(reading.pack_sha256.len(), 64);
    }

    #[test]
    fn reads_reviewed_temporal_cues_with_separate_lineage_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(TEMPORAL_CHARACTER_STORY)
            .expect("checked temporal Character RON");
        let reading = temporal_character_reading(&story).expect("read temporal Character values");
        assert!(!reading.canonical_personality_write_back);
        assert_eq!(
            reading.accepted_record_ids,
            [
                "apollo_11_lunar_landing",
                "calendar_midsummer_period",
                "world_coastal_fog_cycle",
            ]
            .map(str::to_owned)
        );
        assert_eq!(reading.cues.len(), 3);
        assert!(reading.cues.iter().all(|cue| {
            !cue.fact_source_ids.is_empty()
                && !cue.cue_source_ids.is_empty()
                && matches!(cue.decision.as_str(), "accepted" | "edited" | "overridden")
        }));
        assert!(reading.cues.iter().any(|cue| {
            cue.record_id == "apollo_11_lunar_landing"
                && cue.kind == "value"
                && cue.fact_source_ids == ["apollo_11_wikidata"]
                && cue.cue_source_ids == ["weave_historical_cues"]
        }));
    }
}
