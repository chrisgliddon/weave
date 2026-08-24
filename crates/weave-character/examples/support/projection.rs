use std::collections::BTreeMap;

use weave_character::{
    CHARACTER_PROFILE_FORMAT_VERSION, HexacoTrait, PROJECTION_PACK_FORMAT_VERSION,
    ProjectionCalibrationFixture, ProjectionEntry, ProjectionInputField, ProjectionKind,
    ProjectionPack, ProjectionTaxonomy, projection_calibration_rankings,
};
use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

pub const SOURCE_ID: &str = "weave_glasswind_lenses_original";
pub const PACK_ID: &str = "org.weave.projection.glasswind_lenses";

const PERSONALITY: &str = "org.weave.projection.glasswind_lenses.personality";
const VOCATION: &str = "org.weave.projection.glasswind_lenses.vocation";
const SOCIAL_ROLE: &str = "org.weave.projection.glasswind_lenses.social_role";
const NARRATIVE_ROLE: &str = "org.weave.projection.glasswind_lenses.narrative_role";

pub fn reference_projection_pack() -> ProjectionPack {
    let mut pack = ProjectionPack {
        pack_format_version: PROJECTION_PACK_FORMAT_VERSION,
        id: PACK_ID.to_owned(),
        version: "1.0.0".to_owned(),
        title: "Glasswind Lenses".to_owned(),
        description: "Original categorical displays and fictional role prompts for the deterministic Character projection workflow."
            .to_owned(),
        compatible_profile_versions: vec![CHARACTER_PROFILE_FORMAT_VERSION],
        independently_authored: true,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        methodology: "Each candidate declares signed weights over canonical HEXACO factors or facets. Available values are converted to millionths, centered around 0.5, multiplied by signed integer weights, summed, and divided by covered absolute weight. Missing values reduce coverage and contribute no value. Eligible candidates are ranked by score, then a pinned SHA-256 seed trace, then stable identifiers."
            .to_owned(),
        limitations: "These labels are optional fictional writing prompts. They do not measure aptitude, diagnose a person, recommend employment, predict conduct, establish a normative distribution, or replace authored character canon. Capacity values demonstrate project allocation policy only."
            .to_owned(),
        inputs: inputs(),
        taxonomies: taxonomies(),
        calibrations: BTreeMap::new(),
        provenance: provenance(),
    };
    for (id, description, inputs_micros) in [
        (
            "bright_route",
            "High exploration, organization, social momentum, patience, and sincerity.",
            BTreeMap::from([
                ("agreeableness".to_owned(), 690_000),
                ("conscientiousness".to_owned(), 810_000),
                ("creativity".to_owned(), 880_000),
                ("diligence".to_owned(), 790_000),
                ("extraversion".to_owned(), 720_000),
                ("inquisitiveness".to_owned(), 910_000),
                ("openness".to_owned(), 860_000),
                ("organization".to_owned(), 830_000),
                ("patience".to_owned(), 740_000),
                ("sincerity".to_owned(), 760_000),
                ("social_boldness".to_owned(), 680_000),
                ("sociability".to_owned(), 710_000),
            ]),
        ),
        (
            "quiet_harbor",
            "Reflective exploration with low outward momentum and strong patience and care.",
            BTreeMap::from([
                ("agreeableness".to_owned(), 820_000),
                ("conscientiousness".to_owned(), 700_000),
                ("creativity".to_owned(), 780_000),
                ("diligence".to_owned(), 680_000),
                ("extraversion".to_owned(), 250_000),
                ("inquisitiveness".to_owned(), 820_000),
                ("openness".to_owned(), 790_000),
                ("organization".to_owned(), 720_000),
                ("patience".to_owned(), 900_000),
                ("sincerity".to_owned(), 840_000),
                ("social_boldness".to_owned(), 230_000),
                ("sociability".to_owned(), 300_000),
            ]),
        ),
    ] {
        let expected_rankings = projection_calibration_rankings(&pack, &inputs_micros)
            .expect("reference projection calibration is structurally complete");
        pack.calibrations.insert(
            id.to_owned(),
            ProjectionCalibrationFixture {
                id: id.to_owned(),
                description: description.to_owned(),
                inputs_micros,
                expected_rankings,
            },
        );
    }
    pack
}

pub fn taxonomy_ids() -> Vec<String> {
    vec![
        NARRATIVE_ROLE.to_owned(),
        PERSONALITY.to_owned(),
        SOCIAL_ROLE.to_owned(),
        VOCATION.to_owned(),
    ]
}

fn inputs() -> BTreeMap<String, ProjectionInputField> {
    BTreeMap::from([
        input(
            "agreeableness",
            HexacoTrait::Agreeableness,
            "canon.personality.agreeableness.factor",
            "Agreeableness",
        ),
        input(
            "conscientiousness",
            HexacoTrait::Conscientiousness,
            "canon.personality.conscientiousness.factor",
            "Conscientiousness",
        ),
        input(
            "creativity",
            HexacoTrait::Creativity,
            "canon.personality.openness.creativity",
            "Creativity",
        ),
        input(
            "diligence",
            HexacoTrait::Diligence,
            "canon.personality.conscientiousness.diligence",
            "Diligence",
        ),
        input(
            "extraversion",
            HexacoTrait::Extraversion,
            "canon.personality.extraversion.factor",
            "Extraversion",
        ),
        input(
            "inquisitiveness",
            HexacoTrait::Inquisitiveness,
            "canon.personality.openness.inquisitiveness",
            "Inquisitiveness",
        ),
        input(
            "openness",
            HexacoTrait::Openness,
            "canon.personality.openness.factor",
            "Openness",
        ),
        input(
            "organization",
            HexacoTrait::Organization,
            "canon.personality.conscientiousness.organization",
            "Organization",
        ),
        input(
            "patience",
            HexacoTrait::Patience,
            "canon.personality.agreeableness.patience",
            "Patience",
        ),
        input(
            "sincerity",
            HexacoTrait::Sincerity,
            "canon.personality.honesty_humility.sincerity",
            "Sincerity",
        ),
        input(
            "social_boldness",
            HexacoTrait::SocialBoldness,
            "canon.personality.extraversion.social_boldness",
            "Social boldness",
        ),
        input(
            "sociability",
            HexacoTrait::Sociability,
            "canon.personality.extraversion.sociability",
            "Sociability",
        ),
    ])
}

fn input(
    id: &str,
    trait_id: HexacoTrait,
    profile_path: &str,
    label: &str,
) -> (String, ProjectionInputField) {
    (
        id.to_owned(),
        ProjectionInputField {
            id: id.to_owned(),
            trait_id,
            profile_path: profile_path.to_owned(),
            label: label.to_owned(),
            description: format!(
                "Canonical {label} evidence used only by the declared projection formula."
            ),
        },
    )
}

fn taxonomies() -> BTreeMap<String, ProjectionTaxonomy> {
    BTreeMap::from([
        (
            NARRATIVE_ROLE.to_owned(),
            taxonomy(
                NARRATIVE_ROLE,
                "narrative_role",
                ProjectionKind::NarrativeRole,
                "Narrative role",
                "Optional story-function prompts for scene planning.",
                false,
                [
                    entry(
                        "signal_keeper",
                        "Signal Keeper",
                        "Maintains continuity and makes important changes legible to others.",
                        [
                            ("diligence", 900),
                            ("organization", 850),
                            ("sincerity", 500),
                        ],
                    ),
                    entry(
                        "trail_opener",
                        "Trail Opener",
                        "Moves first when a story needs a new route or possibility.",
                        [
                            ("creativity", 850),
                            ("social_boldness", 700),
                            ("openness", 750),
                        ],
                    ),
                    entry(
                        "turning_witness",
                        "Turning Witness",
                        "Notices transformation and gives it narrative weight.",
                        [
                            ("inquisitiveness", 700),
                            ("patience", 800),
                            ("sincerity", 650),
                        ],
                    ),
                ],
            ),
        ),
        (
            PERSONALITY.to_owned(),
            taxonomy(
                PERSONALITY,
                "personality_lens",
                ProjectionKind::CategoricalPersonality,
                "Personality display",
                "A deliberately lossy categorical lens over selected canonical evidence.",
                true,
                [
                    entry(
                        "measured_observer",
                        "Measured Observer",
                        "Favors patient attention and deliberate organization.",
                        [
                            ("patience", 900),
                            ("organization", 750),
                            ("extraversion", -450),
                        ],
                    ),
                    entry(
                        "open_explorer",
                        "Open Explorer",
                        "Favors inquisitive and creative engagement with unfamiliar paths.",
                        [
                            ("inquisitiveness", 900),
                            ("creativity", 850),
                            ("openness", 700),
                        ],
                    ),
                    entry(
                        "steady_coordinator",
                        "Steady Coordinator",
                        "Favors organized follow-through with outward collaboration.",
                        [
                            ("organization", 850),
                            ("diligence", 800),
                            ("sociability", 550),
                        ],
                    ),
                ],
            ),
        ),
        (
            SOCIAL_ROLE.to_owned(),
            taxonomy(
                SOCIAL_ROLE,
                "social_role",
                ProjectionKind::SocialRole,
                "Social role",
                "Optional prompts for how a fictional character might contribute within a group.",
                false,
                [
                    entry(
                        "boundary_guide",
                        "Boundary Guide",
                        "Makes limits explicit while helping a group move safely.",
                        [
                            ("sincerity", 800),
                            ("social_boldness", 650),
                            ("organization", 500),
                        ],
                    ),
                    entry(
                        "circle_anchor",
                        "Circle Anchor",
                        "Offers patient continuity when a group is changing direction.",
                        [
                            ("patience", 900),
                            ("agreeableness", 700),
                            ("diligence", 500),
                        ],
                    ),
                    entry(
                        "question_host",
                        "Question Host",
                        "Invites shared inquiry and keeps space open for multiple answers.",
                        [
                            ("inquisitiveness", 850),
                            ("sociability", 650),
                            ("sincerity", 500),
                        ],
                    ),
                ],
            ),
        ),
        (
            VOCATION.to_owned(),
            taxonomy(
                VOCATION,
                "vocation",
                ProjectionKind::Vocation,
                "Vocation",
                "Optional fictional vocation prompts, never aptitude or employment recommendations.",
                false,
                [
                    entry(
                        "field_liaison",
                        "Field Liaison",
                        "Connects practical field observations with the people who need them.",
                        [("sociability", 800), ("sincerity", 650), ("diligence", 550)],
                    ),
                    entry(
                        "route_archivist",
                        "Route Archivist",
                        "Maintains navigable records while continuing to investigate new routes.",
                        [
                            ("organization", 850),
                            ("inquisitiveness", 800),
                            ("diligence", 700),
                        ],
                    ),
                    entry(
                        "signal_crafter",
                        "Signal Crafter",
                        "Builds expressive systems that make complex information easier to follow.",
                        [
                            ("creativity", 900),
                            ("conscientiousness", 650),
                            ("social_boldness", 450),
                        ],
                    ),
                ],
            ),
        ),
    ])
}

fn taxonomy<const N: usize>(
    id: &str,
    output_id: &str,
    kind: ProjectionKind,
    label: &str,
    description: &str,
    lossy: bool,
    entries: [(String, ProjectionEntry); N],
) -> ProjectionTaxonomy {
    ProjectionTaxonomy {
        id: id.to_owned(),
        output_id: output_id.to_owned(),
        kind,
        label: label.to_owned(),
        description: description.to_owned(),
        lossy,
        independent_evidence: false,
        requires_review: true,
        minimum_coverage_micros: 750_000,
        limitations: "A selected label is a transparent fictional prompt, remains outside canon, and cannot be reused as personality evidence."
            .to_owned(),
        entries: BTreeMap::from(entries),
    }
}

fn entry<const N: usize>(
    id: &str,
    label: &str,
    description: &str,
    evidence: [(&str, i16); N],
) -> (String, ProjectionEntry) {
    (
        id.to_owned(),
        ProjectionEntry {
            id: id.to_owned(),
            label: label.to_owned(),
            description: description.to_owned(),
            evidence: evidence
                .into_iter()
                .map(|(input, weight)| (input.to_owned(), weight))
                .collect(),
            minimum_score_micros: -1_000_000,
            eligible_character_ids: Vec::new(),
            eligible_id_prefixes: vec!["org.weave.character".to_owned()],
            excluded_character_ids: Vec::new(),
            default_capacity: Some(2),
        },
    )
}

fn provenance() -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: SOURCE_ID.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "glasswind-lenses-v1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            attribution: "Original Glasswind Lenses inputs, labels, scoring method, limitations, and calibration vectors."
                .to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([
            ("calibrations".to_owned(), vec![SOURCE_ID.to_owned()]),
            ("entries".to_owned(), vec![SOURCE_ID.to_owned()]),
            ("inputs".to_owned(), vec![SOURCE_ID.to_owned()]),
            ("methodology".to_owned(), vec![SOURCE_ID.to_owned()]),
            ("taxonomies".to_owned(), vec![SOURCE_ID.to_owned()]),
        ]),
    }
}
