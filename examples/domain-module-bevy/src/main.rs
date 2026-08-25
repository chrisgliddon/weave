use std::error::Error;
use std::io;

use bevy::prelude::*;
use weave_core::ir::{DomainValueIr, StoryIr};
use weave_domain::DomainValue;
use weave_tabletop::{
    EventVisibility, FreehackAuthorityReceipt, FreehackPublicReceipt, ResolutionReceipt,
    dungeonpunk_manifest, plug_and_play_manifest, validate_freehack_authority_receipt,
    validate_freehack_public_receipt, validate_resolution_receipt,
};

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
const TABLETOP_STORY: &str =
    include_str!("../../tabletop-adapters/plug-and-play/runtime/ember-vale.story.ron");
const TABLETOP_RECEIPT: &str =
    include_str!("../../tabletop-adapters/plug-and-play/runtime.tabletop-receipt.json");
const DUNGEONPUNK_STORY: &str =
    include_str!("../../tabletop-adapters/dungeonpunk/runtime/vesper-ash.story.ron");
const DUNGEONPUNK_RECEIPT: &str =
    include_str!("../../tabletop-adapters/dungeonpunk/runtime.tabletop-receipt.json");
const FREEHACK_STORY: &str =
    include_str!("../../tabletop-adapters/freehack/runtime/tavi-quill.story.ron");
const FREEHACK_PUBLIC_RECEIPT: &str =
    include_str!("../../tabletop-adapters/freehack/public-receipt.freehack-public-receipt.json");
const FREEHACK_AUTHORITY_RECEIPT: &str = include_str!(
    "../../tabletop-adapters/freehack/authority-receipt.freehack-authority-receipt.json"
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
struct ProjectionValueReading {
    id: String,
    kind: String,
    label: String,
    lossy: bool,
    independent_evidence: bool,
    decision: String,
    explanation: String,
    rationale: String,
    input_paths: Vec<String>,
    pack_id: String,
    pack_version: String,
    pack_sha256: String,
    proposal_sha256: String,
    review_sha256: String,
    lock: String,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct ProjectionCharacterReading {
    values: Vec<ProjectionValueReading>,
    write_back: std::collections::BTreeMap<String, bool>,
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

#[derive(Resource, Debug, Clone, PartialEq)]
struct TabletopReading {
    adapter_id: String,
    adapter_version: String,
    adapter_sha256: String,
    character_name: String,
    attributes: [f64; 4],
    fortune: f64,
    survivability: f64,
    wounded: bool,
    operation: String,
    outcome: String,
    total: f64,
    rerolled: bool,
    request_sha256: String,
    hidden_entropy_sha256: String,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct DungeonpunkReading {
    adapter_id: String,
    adapter_version: String,
    adapter_sha256: String,
    character_name: String,
    attributes: [f64; 6],
    fate: f64,
    hp: f64,
    stress: f64,
    xp: f64,
    encumbered: bool,
    operation: String,
    outcome: String,
    selected: f64,
    dice_pool: f64,
    helped: bool,
    pushed: bool,
    request_sha256: String,
    hidden_entropy_sha256: String,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct FreehackReading {
    adapter_id: String,
    adapter_version: String,
    adapter_sha256: String,
    character_name: String,
    archetype_id: String,
    focus: f64,
    fatigue_progress: f64,
    gantry_status: String,
    public_memory_count: f64,
    operation: String,
    outcome: String,
    magnitude: f64,
    support_total: f64,
    public_state_sha256: String,
    authority_private_fields_present: bool,
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

fn projection_character_reading(story: &StoryIr) -> Result<ProjectionCharacterReading, io::Error> {
    let module = story
        .modules
        .get("character")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Character module is absent"))?;
    let Some(DomainValueIr::Object(projections)) = module.value(&["profile", "projections"]) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "reviewed Character projections are invalid",
        ));
    };
    let Some(DomainValueIr::Object(raw_values)) = projections.get("values") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "approved projection value map is invalid",
        ));
    };
    let values = raw_values
        .iter()
        .map(|(projection_id, value)| {
            let DomainValueIr::Object(fields) = value else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection value is invalid",
                ));
            };
            let string = |name: &str| match fields.get(name) {
                Some(DomainValueIr::String(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection string is invalid",
                )),
            };
            let symbol = |name: &str| match fields.get(name) {
                Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection symbol is invalid",
                )),
            };
            let boolean = |name: &str| match fields.get(name) {
                Some(DomainValueIr::Bool(value)) => Ok(*value),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection marker is invalid",
                )),
            };
            let id = string("id")?;
            if id != *projection_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection id does not match its map key",
                ));
            }
            let Some(DomainValueIr::Object(pack)) = fields.get("pack") else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection pack coordinate is invalid",
                ));
            };
            let pack_string = |name: &str| match pack.get(name) {
                Some(DomainValueIr::String(value)) => Ok(value.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection pack field is invalid",
                )),
            };
            let pack_sha256 = pack_string("sha256")?;
            let proposal_sha256 = string("proposal_sha256")?;
            let review_sha256 = string("review_sha256")?;
            if !is_sha256(&pack_sha256)
                || !is_sha256(&proposal_sha256)
                || !is_sha256(&review_sha256)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "approved projection fingerprint is invalid",
                ));
            }
            Ok(ProjectionValueReading {
                id,
                kind: symbol("kind")?,
                label: string("label")?,
                lossy: boolean("lossy")?,
                independent_evidence: boolean("independent_evidence")?,
                decision: symbol("decision")?,
                explanation: string("explanation")?,
                rationale: string("rationale")?,
                input_paths: string_list(
                    fields.get("input_paths"),
                    "approved projection input paths are invalid",
                )?,
                pack_id: pack_string("id")?,
                pack_version: pack_string("version")?,
                pack_sha256,
                proposal_sha256,
                review_sha256,
                lock: symbol("lock")?,
            })
        })
        .collect::<Result<Vec<_>, io::Error>>()?;
    let Some(DomainValueIr::Object(raw_write_back)) = projections.get("write_back") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "projection write-back contract is invalid",
        ));
    };
    let write_back = raw_write_back
        .iter()
        .map(|(target, value)| match value {
            DomainValueIr::Bool(value) => Ok((target.clone(), *value)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "projection write-back marker is invalid",
            )),
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, io::Error>>()?;
    let expected_targets = [
        "alignment",
        "birth",
        "hexaco",
        "identity",
        "ocean",
        "relationships",
        "ruleset",
    ];
    if write_back.keys().map(String::as_str).ne(expected_targets)
        || write_back.values().any(|value| *value)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "projection write-back authority is invalid",
        ));
    }
    Ok(ProjectionCharacterReading { values, write_back })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
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

fn tabletop_reading(
    story: &StoryIr,
    receipt: &ResolutionReceipt,
) -> Result<TabletopReading, io::Error> {
    validate_resolution_receipt(receipt, &plug_and_play_manifest()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Plug-And-Play receipt contract is invalid",
        )
    })?;
    let module = story
        .modules
        .get("rules")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "rules module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let boolean = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Bool(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let adapter_id = string(&["adapter", "id"], "tabletop adapter id is invalid")?;
    let adapter_version = string(
        &["adapter", "version"],
        "tabletop adapter version is invalid",
    )?;
    let adapter_sha256 = string(
        &["adapter", "content_sha256"],
        "tabletop adapter fingerprint is invalid",
    )?;
    if receipt.adapter.id != adapter_id
        || receipt.adapter.version != adapter_version
        || receipt.adapter.content_sha256 != adapter_sha256
        || !is_sha256(&adapter_sha256)
        || !is_sha256(&receipt.request_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "tabletop story and receipt coordinates disagree",
        ));
    }
    let check = receipt
        .events
        .iter()
        .find(|event| event.kind == "check_resolved")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "check event is absent"))?;
    let DomainValue::Object(check) = check.payload.as_ref().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "public check payload is absent")
    })?
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "public check payload is invalid",
        ));
    };
    let outcome = match check.get("outcome") {
        Some(DomainValue::Symbol(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "public check outcome is invalid",
            ));
        }
    };
    let total = match check.get("total") {
        Some(DomainValue::Number(value)) if value.is_finite() => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "public check total is invalid",
            ));
        }
    };
    let rerolled = match check.get("rerolled") {
        Some(DomainValue::Bool(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "public Fortune-reroll flag is invalid",
            ));
        }
    };
    let entropy = receipt
        .events
        .iter()
        .find(|event| event.kind == "entropy_trace")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "entropy audit is absent"))?;
    if entropy.visibility != EventVisibility::HostOnly
        || entropy.payload.is_some()
        || !is_sha256(&entropy.payload_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime entropy audit visibility is invalid",
        ));
    }
    Ok(TabletopReading {
        adapter_id,
        adapter_version,
        adapter_sha256,
        character_name: string(
            &["definition", "name"],
            "tabletop character name is invalid",
        )?,
        attributes: [
            number(
                &["definition", "effective_attributes", "agility"],
                "Agility is invalid",
            )?,
            number(
                &["definition", "effective_attributes", "brains"],
                "Brains is invalid",
            )?,
            number(
                &["definition", "effective_attributes", "brawn"],
                "Brawn is invalid",
            )?,
            number(
                &["definition", "effective_attributes", "wits"],
                "Wits is invalid",
            )?,
        ],
        fortune: number(&["state", "fortune_remaining"], "Fortune is invalid")?,
        survivability: number(
            &["state", "survivability_current"],
            "Survivability is invalid",
        )?,
        wounded: boolean(&["state", "wounded"], "wound state is invalid")?,
        operation: receipt.operation.clone(),
        outcome,
        total,
        rerolled,
        request_sha256: receipt.request_sha256.clone(),
        hidden_entropy_sha256: entropy.payload_sha256.clone(),
    })
}

fn dungeonpunk_reading(
    story: &StoryIr,
    receipt: &ResolutionReceipt,
) -> Result<DungeonpunkReading, io::Error> {
    validate_resolution_receipt(receipt, &dungeonpunk_manifest()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Dungeonpunk receipt contract is invalid",
        )
    })?;
    let module = story
        .modules
        .get("rules")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "rules module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let boolean = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Bool(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let adapter_id = string(&["adapter", "id"], "Dungeonpunk adapter id is invalid")?;
    let adapter_version = string(
        &["adapter", "version"],
        "Dungeonpunk adapter version is invalid",
    )?;
    let adapter_sha256 = string(
        &["adapter", "content_sha256"],
        "Dungeonpunk adapter fingerprint is invalid",
    )?;
    if receipt.adapter.id != adapter_id
        || receipt.adapter.version != adapter_version
        || receipt.adapter.content_sha256 != adapter_sha256
        || receipt.operation != "struggle"
        || !is_sha256(&adapter_sha256)
        || !is_sha256(&receipt.request_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Dungeonpunk story and receipt coordinates disagree",
        ));
    }
    let roll_event = receipt
        .events
        .iter()
        .find(|event| event.kind == "roll_resolved")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Struggle roll is absent"))?;
    if roll_event.visibility != EventVisibility::Public {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Struggle roll visibility is invalid",
        ));
    }
    let DomainValue::Object(roll) = roll_event.payload.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "public Struggle payload is absent",
        )
    })?
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "public Struggle payload is invalid",
        ));
    };
    let event_number = |field: &str| match roll.get(field) {
        Some(DomainValue::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "public Struggle number is invalid",
        )),
    };
    let event_bool = |field: &str| match roll.get(field) {
        Some(DomainValue::Bool(value)) => Ok(*value),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "public Struggle marker is invalid",
        )),
    };
    let outcome = match roll.get("outcome") {
        Some(DomainValue::Symbol(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "public Struggle outcome is invalid",
            ));
        }
    };
    let entropy = receipt
        .events
        .iter()
        .find(|event| event.kind == "entropy_trace")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "entropy audit is absent"))?;
    if entropy.visibility != EventVisibility::HostOnly
        || entropy.payload.is_some()
        || !is_sha256(&entropy.payload_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime entropy audit visibility is invalid",
        ));
    }
    Ok(DungeonpunkReading {
        adapter_id,
        adapter_version,
        adapter_sha256,
        character_name: string(&["definition", "name"], "Dungeonpunk name is invalid")?,
        attributes: [
            number(
                &["definition", "attributes", "strength"],
                "Strength is invalid",
            )?,
            number(
                &["definition", "attributes", "dexterity"],
                "Dexterity is invalid",
            )?,
            number(
                &["definition", "attributes", "constitution"],
                "Constitution is invalid",
            )?,
            number(
                &["definition", "attributes", "intelligence"],
                "Intelligence is invalid",
            )?,
            number(
                &["definition", "attributes", "charisma"],
                "Charisma is invalid",
            )?,
            number(&["definition", "attributes", "wisdom"], "Wisdom is invalid")?,
        ],
        fate: number(&["definition", "constants", "fate"], "FATE is invalid")?,
        hp: number(&["state", "hp_current"], "HP is invalid")?,
        stress: number(&["state", "stress"], "Stress is invalid")?,
        xp: number(&["state", "xp"], "XP is invalid")?,
        encumbered: boolean(&["state", "encumbered"], "encumbrance is invalid")?,
        operation: receipt.operation.clone(),
        outcome,
        selected: event_number("selected")?,
        dice_pool: event_number("dice_pool")?,
        helped: event_bool("helped")?,
        pushed: event_bool("push")?,
        request_sha256: receipt.request_sha256.clone(),
        hidden_entropy_sha256: entropy.payload_sha256.clone(),
    })
}

fn freehack_reading(
    story: &StoryIr,
    public: &FreehackPublicReceipt,
    authority: &FreehackAuthorityReceipt,
) -> Result<FreehackReading, io::Error> {
    validate_freehack_public_receipt(public).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public receipt contract is invalid",
        )
    })?;
    validate_freehack_authority_receipt(authority).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack authority receipt contract is invalid",
        )
    })?;
    let module = story.modules.get("freehack").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public module is absent",
        )
    })?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let symbol = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let adapter_id = string(&["adapter", "id"], "Freehack adapter id is invalid")?;
    let adapter_version = string(
        &["adapter", "version"],
        "Freehack adapter version is invalid",
    )?;
    let adapter_sha256 = string(
        &["adapter", "content_sha256"],
        "Freehack adapter fingerprint is invalid",
    )?;
    if public.adapter != authority.receipt.adapter
        || public.adapter.id != adapter_id
        || public.adapter.version != adapter_version
        || public.adapter.content_sha256 != adapter_sha256
        || public.operation != "resolve_check"
        || authority.receipt.operation != public.operation
        || !is_sha256(&adapter_sha256)
        || !is_sha256(&public.public_state_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack story, public receipt, and authority receipt disagree",
        ));
    }

    let public_json = public.to_json().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public receipt cannot be encoded",
        )
    })?;
    let forbidden = [
        "request_sha256",
        "entropy",
        "opposition_total",
        "draw_index",
        "signed_result",
        "_authority",
        "Sealed Current",
        "sealed_current",
    ];
    if forbidden.iter().any(|marker| public_json.contains(marker))
        || public
            .events
            .iter()
            .any(|event| event.kind.ends_with("_authority"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public transport crosses the authority boundary",
        ));
    }
    let public_event = public
        .events
        .iter()
        .find(|event| event.kind == "check_resolved")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Freehack public check is absent",
            )
        })?;
    let DomainValue::Object(public_check) = &public_event.payload else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public check payload is invalid",
        ));
    };
    let public_number = |field: &str| match public_check.get(field) {
        Some(DomainValue::Number(value)) if value.is_finite() => Ok(*value),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack public check number is invalid",
        )),
    };
    let outcome = match public_check.get("outcome") {
        Some(DomainValue::Symbol(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Freehack public check outcome is invalid",
            ));
        }
    };

    let private_check = authority
        .receipt
        .events
        .iter()
        .find(|event| {
            event.kind == "check_resolved_authority"
                && event.visibility == EventVisibility::HostOnly
        })
        .and_then(|event| event.payload.as_ref());
    let private_fields_present = matches!(
        private_check,
        Some(DomainValue::Object(fields))
            if ["draw_index", "opposition", "opposition_total", "signed_result"]
                .iter()
                .all(|field| fields.contains_key(*field))
    );
    let entropy_present = authority.receipt.events.iter().any(|event| {
        event.kind == "entropy_trace"
            && event.visibility == EventVisibility::HostOnly
            && event.payload.is_some()
    });
    if !private_fields_present || !entropy_present {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Freehack authority host lacks its private resolution audit",
        ));
    }

    Ok(FreehackReading {
        adapter_id,
        adapter_version,
        adapter_sha256,
        character_name: string(&["character", "name"], "Freehack character name is invalid")?,
        archetype_id: string(
            &["character", "archetype_id"],
            "Freehack archetype is invalid",
        )?,
        focus: number(
            &["character", "modifiers", "focus"],
            "Freehack Focus is invalid",
        )?,
        fatigue_progress: number(
            &["tracks", "fatigue", "progress"],
            "Freehack fatigue is invalid",
        )?,
        gantry_status: symbol(
            &["snapshot", "gantry_status"],
            "Freehack gantry status is invalid",
        )?,
        public_memory_count: number(
            &["snapshot", "memory_count"],
            "Freehack public memory count is invalid",
        )?,
        operation: public.operation.clone(),
        outcome,
        magnitude: public_number("magnitude")?,
        support_total: public_number("support_total")?,
        public_state_sha256: public.public_state_sha256.clone(),
        authority_private_fields_present: true,
    })
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

fn report_projection_character(reading: Res<ProjectionCharacterReading>) {
    println!(
        "Bevy read {} explainable classification and role values from {}; protected write-back targets={}",
        reading.values.len(),
        reading
            .values
            .first()
            .map_or("no pack", |value| value.pack_id.as_str()),
        reading.write_back.len(),
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

fn report_tabletop(reading: Res<TabletopReading>) {
    println!(
        "Bevy read {} in {}@{} ({}): A/Bn/Bw/W {:?}, Fortune {}, Survivability {}, wounded={}; {} resolved {} at total {} (rerolled={}, request={}, hidden entropy={})",
        reading.character_name,
        reading.adapter_id,
        reading.adapter_version,
        reading.adapter_sha256,
        reading.attributes,
        reading.fortune,
        reading.survivability,
        reading.wounded,
        reading.operation,
        reading.outcome,
        reading.total,
        reading.rerolled,
        reading.request_sha256,
        reading.hidden_entropy_sha256,
    );
}

fn report_dungeonpunk(reading: Res<DungeonpunkReading>) {
    println!(
        "Bevy read {} in {}@{} ({}): STR/DEX/CON/INT/CHA/WIS {:?}, FATE {}, HP {}, Stress {}, XP {}, encumbered={}; {} resolved {} from pool {} at {} (helped={}, pushed={}, request={}, hidden entropy={})",
        reading.character_name,
        reading.adapter_id,
        reading.adapter_version,
        reading.adapter_sha256,
        reading.attributes,
        reading.fate,
        reading.hp,
        reading.stress,
        reading.xp,
        reading.encumbered,
        reading.operation,
        reading.outcome,
        reading.dice_pool,
        reading.selected,
        reading.helped,
        reading.pushed,
        reading.request_sha256,
        reading.hidden_entropy_sha256,
    );
}

fn report_freehack(reading: Res<FreehackReading>) {
    println!(
        "Bevy read {} ({}) in {}@{} ({}): Focus {}, Fatigue {}, gantry {}, {} public memories; {} resolved {} at magnitude {} with public support {} (public state {}, authority audit validated={})",
        reading.character_name,
        reading.archetype_id,
        reading.adapter_id,
        reading.adapter_version,
        reading.adapter_sha256,
        reading.focus,
        reading.fatigue_progress,
        reading.gantry_status,
        reading.public_memory_count,
        reading.operation,
        reading.outcome,
        reading.magnitude,
        reading.support_total,
        reading.public_state_sha256,
        reading.authority_private_fields_present,
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
    let projection_character =
        projection_character_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let relationship_graph =
        relationship_graph_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let expression = expression_reading(&ron::from_str::<StoryIr>(CHARACTER_STORY)?)?;
    let temporal_character =
        temporal_character_reading(&ron::from_str::<StoryIr>(TEMPORAL_CHARACTER_STORY)?)?;
    let tabletop_receipt = ResolutionReceipt::from_json(TABLETOP_RECEIPT)?;
    let tabletop = tabletop_reading(
        &ron::from_str::<StoryIr>(TABLETOP_STORY)?,
        &tabletop_receipt,
    )?;
    let dungeonpunk_receipt = ResolutionReceipt::from_json(DUNGEONPUNK_RECEIPT)?;
    let dungeonpunk = dungeonpunk_reading(
        &ron::from_str::<StoryIr>(DUNGEONPUNK_STORY)?,
        &dungeonpunk_receipt,
    )?;
    let freehack_public = FreehackPublicReceipt::from_json(FREEHACK_PUBLIC_RECEIPT)?;
    let freehack_authority = FreehackAuthorityReceipt::from_json(FREEHACK_AUTHORITY_RECEIPT)?;
    let freehack = freehack_reading(
        &ron::from_str::<StoryIr>(FREEHACK_STORY)?,
        &freehack_public,
        &freehack_authority,
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .insert_resource(world_readings)
        .insert_resource(composed_world)
        .insert_resource(character)
        .insert_resource(alignment_character)
        .insert_resource(projection_character)
        .insert_resource(relationship_graph)
        .insert_resource(expression)
        .insert_resource(temporal_character)
        .insert_resource(tabletop)
        .insert_resource(dungeonpunk)
        .insert_resource(freehack)
        .add_systems(
            Startup,
            (
                report_reading,
                report_world,
                report_composed_world,
                report_character,
                report_alignment_character,
                report_projection_character,
                report_relationship_graph,
                report_expression,
                report_temporal_character,
                report_tabletop,
                report_dungeonpunk,
                report_freehack,
            )
                .chain(),
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
    fn reads_explainable_projection_values_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(CHARACTER_STORY).expect("checked Character RON");
        let reading =
            projection_character_reading(&story).expect("read approved Character projections");
        assert_eq!(
            reading
                .values
                .iter()
                .map(|value| (
                    value.id.as_str(),
                    value.kind.as_str(),
                    value.label.as_str(),
                    value.decision.as_str(),
                    value.lossy,
                    value.lock.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "narrative_role",
                    "narrative_role",
                    "Signal Keeper",
                    "reviewed",
                    false,
                    "locked",
                ),
                (
                    "personality_lens",
                    "categorical_personality",
                    "Open Explorer",
                    "derived",
                    true,
                    "unlocked",
                ),
                (
                    "social_role",
                    "social_role",
                    "Question Host",
                    "reviewed",
                    false,
                    "unlocked",
                ),
                (
                    "vocation",
                    "vocation",
                    "Route Archivist",
                    "reviewed",
                    false,
                    "unlocked",
                ),
            ]
        );
        assert!(reading.values.iter().all(|value| {
            !value.independent_evidence
                && !value.explanation.is_empty()
                && !value.rationale.is_empty()
                && !value.input_paths.is_empty()
                && value.pack_id == "org.weave.projection.glasswind_lenses"
                && value.pack_version == "1.0.0"
                && is_sha256(&value.pack_sha256)
                && is_sha256(&value.proposal_sha256)
                && is_sha256(&value.review_sha256)
        }));
        assert_eq!(
            reading
                .write_back
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec![
                "alignment",
                "birth",
                "hexaco",
                "identity",
                "ocean",
                "relationships",
                "ruleset",
            ]
        );
        assert!(reading.write_back.values().all(|value| !value));
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

    #[test]
    fn reads_plug_and_play_story_and_redacted_receipt_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(TABLETOP_STORY).expect("checked tabletop story RON");
        let receipt =
            ResolutionReceipt::from_json(TABLETOP_RECEIPT).expect("checked tabletop receipt JSON");
        let reading = tabletop_reading(&story, &receipt).expect("read tabletop presentation");
        assert_eq!(reading.adapter_id, "org.weave.tabletop.plug_and_play");
        assert_eq!(reading.adapter_version, "1.0.0");
        assert_eq!(reading.character_name, "Ember Vale");
        assert!(reading.attributes.iter().all(|value| value.is_finite()));
        assert!(reading.fortune >= 0.0);
        assert!(reading.survivability >= 0.0);
        assert_eq!(reading.operation, "check");
        assert!(is_sha256(&reading.request_sha256));
        assert!(is_sha256(&reading.hidden_entropy_sha256));
    }

    #[test]
    fn reads_dungeonpunk_story_and_redacted_receipt_without_editor_dependencies() {
        let story =
            ron::from_str::<StoryIr>(DUNGEONPUNK_STORY).expect("checked Dungeonpunk story RON");
        let receipt = ResolutionReceipt::from_json(DUNGEONPUNK_RECEIPT)
            .expect("checked Dungeonpunk receipt JSON");
        let reading = dungeonpunk_reading(&story, &receipt).expect("read Dungeonpunk presentation");
        assert_eq!(reading.adapter_id, "org.weave.tabletop.dungeonpunk");
        assert_eq!(reading.adapter_version, "1.0.0");
        assert_eq!(reading.character_name, "Vesper Ash");
        assert_eq!(reading.attributes, [2.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
        assert_eq!(reading.fate, 1.0);
        assert_eq!(reading.operation, "struggle");
        assert_eq!(reading.outcome, "failure");
        assert!(reading.helped);
        assert!(reading.pushed);
        assert!(is_sha256(&reading.request_sha256));
        assert!(is_sha256(&reading.hidden_entropy_sha256));
    }

    #[test]
    fn reads_freehack_public_story_while_authority_host_retains_private_audit() {
        let story = ron::from_str::<StoryIr>(FREEHACK_STORY).expect("checked Freehack story RON");
        let public = FreehackPublicReceipt::from_json(FREEHACK_PUBLIC_RECEIPT)
            .expect("checked Freehack public receipt JSON");
        let authority = FreehackAuthorityReceipt::from_json(FREEHACK_AUTHORITY_RECEIPT)
            .expect("checked Freehack authority receipt JSON");
        let reading = freehack_reading(&story, &public, &authority)
            .expect("read split Freehack presentation");
        assert_eq!(reading.adapter_id, "org.weave.tabletop.freehack");
        assert_eq!(reading.adapter_version, "1.0.0");
        assert_eq!(reading.character_name, "Tavi Quill");
        assert_eq!(reading.archetype_id, "courier");
        assert_eq!(reading.focus, 4.0);
        assert_eq!(reading.fatigue_progress, 0.0);
        assert_eq!(reading.gantry_status, "resolved");
        assert_eq!(reading.public_memory_count, 2.0);
        assert_eq!(reading.operation, "resolve_check");
        assert_eq!(reading.outcome, "success");
        assert_eq!(reading.magnitude, 1.0);
        assert_eq!(reading.support_total, 4.0);
        assert!(is_sha256(&reading.public_state_sha256));
        assert!(reading.authority_private_fields_present);
    }

    #[test]
    fn rejects_freehack_player_transport_with_an_authority_event() {
        let story = ron::from_str::<StoryIr>(FREEHACK_STORY).expect("checked Freehack story RON");
        let mut public = FreehackPublicReceipt::from_json(FREEHACK_PUBLIC_RECEIPT)
            .expect("checked Freehack public receipt JSON");
        public.events[0].kind = "check_resolved_authority".to_owned();
        let authority = FreehackAuthorityReceipt::from_json(FREEHACK_AUTHORITY_RECEIPT)
            .expect("checked Freehack authority receipt JSON");
        assert!(freehack_reading(&story, &public, &authority).is_err());
    }
}
