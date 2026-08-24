use std::collections::BTreeMap;

use weave_character::{
    EXPRESSION_PACK_FORMAT_VERSION, ExpressionApplicability, ExpressionConstraintEffect,
    ExpressionContextPredicate, ExpressionDialogueTemplate, ExpressionDialogueVariant,
    ExpressionMedium, ExpressionPack, ExpressionPackCoverageRequirements,
    ExpressionPackEligibility, ExpressionPackEntry, ExpressionPackValue,
    ExpressionPackVocabularyPool, ExpressionPlaceholderDeclaration, ExpressionPlaceholderValue,
    ExpressionSourceLocation, ExpressionSpeakerRequirement, ExpressionTermKind,
    ExpressionTextAuthority, HexacoTrait, PreferencePolarity, ReviewState, TraitBand,
};
use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

pub const SOURCE_ID: &str = "weave_expression_glasswind_original";
pub const PACK_ID: &str = "org.weave.expression.glasswind";

pub fn reference_expression_pack() -> ExpressionPack {
    let source_ids = vec![SOURCE_ID.to_owned()];
    let limitations = vec![
        "Expression records are authored fictional guidance, not generated prose, diagnosis, identity inference, or objective personality evidence."
            .to_owned(),
    ];
    ExpressionPack {
        pack_format_version: EXPRESSION_PACK_FORMAT_VERSION,
        id: PACK_ID.to_owned(),
        version: "1.0.0".to_owned(),
        title: "Glasswind Expression Reference Pack".to_owned(),
        description: "Original neutral lexicon, preference, behavior, voice, and dialogue examples for deterministic offline Character tooling."
            .to_owned(),
        independently_authored: true,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        eligibility: ExpressionPackEligibility {
            compatible_profile_versions: vec![1],
            required_extension_namespaces: Vec::new(),
        },
        entries: BTreeMap::from([
            (
                "avoid_crowded_metaphors".to_owned(),
                ExpressionPackEntry {
                    id: "avoid_crowded_metaphors".to_owned(),
                    label: "Avoid crowded metaphors".to_owned(),
                    description: "Keeps a single concrete image in one short passage."
                        .to_owned(),
                    value: ExpressionPackValue::VoiceConstraint {
                        category: "org.weave.expression.clarity".to_owned(),
                        medium: ExpressionMedium::Both,
                        effect: ExpressionConstraintEffect::Avoid,
                        target: "crowded metaphors".to_owned(),
                        instruction: "Use at most one concrete image in a short passage."
                            .to_owned(),
                    },
                    strength: 0.8,
                    applicability: ExpressionApplicability::default(),
                    source_ids: source_ids.clone(),
                    limitations: limitations.clone(),
                },
            ),
            (
                "pauses_to_map".to_owned(),
                ExpressionPackEntry {
                    id: "pauses_to_map".to_owned(),
                    label: "Pauses to map".to_owned(),
                    description: "A small planning behavior for a wholly fictional character."
                        .to_owned(),
                    value: ExpressionPackValue::BehavioralSignature {
                        category: "org.weave.expression.behavior".to_owned(),
                        cue: "Pauses to sketch a route before choosing a direction.".to_owned(),
                    },
                    strength: 0.75,
                    applicability: ExpressionApplicability::default(),
                    source_ids: source_ids.clone(),
                    limitations: limitations.clone(),
                },
            ),
            (
                "prefers_shared_landmarks".to_owned(),
                ExpressionPackEntry {
                    id: "prefers_shared_landmarks".to_owned(),
                    label: "Prefers shared landmarks".to_owned(),
                    description: "A categorized communication preference."
                        .to_owned(),
                    value: ExpressionPackValue::Preference {
                        category: "org.weave.preference.communication".to_owned(),
                        target: "shared landmarks".to_owned(),
                        polarity: PreferencePolarity::Prefer,
                    },
                    strength: 0.7,
                    applicability: ExpressionApplicability::default(),
                    source_ids: source_ids.clone(),
                    limitations: limitations.clone(),
                },
            ),
            (
                "steady_question".to_owned(),
                ExpressionPackEntry {
                    id: "steady_question".to_owned(),
                    label: "Steady question".to_owned(),
                    description: "A short reusable phrase for calm invitations."
                        .to_owned(),
                    value: ExpressionPackValue::Term {
                        category: "org.weave.expression.navigation".to_owned(),
                        term_kind: ExpressionTermKind::Phrase,
                        surface: "steady question".to_owned(),
                    },
                    strength: 0.65,
                    applicability: ExpressionApplicability::default(),
                    source_ids: source_ids.clone(),
                    limitations: limitations.clone(),
                },
            ),
            (
                "trailmark".to_owned(),
                ExpressionPackEntry {
                    id: "trailmark".to_owned(),
                    label: "Trailmark".to_owned(),
                    description: "A synthetic navigation term for template substitution."
                        .to_owned(),
                    value: ExpressionPackValue::Term {
                        category: "org.weave.expression.navigation".to_owned(),
                        term_kind: ExpressionTermKind::Term,
                        surface: "trailmark".to_owned(),
                    },
                    strength: 0.9,
                    applicability: ExpressionApplicability::default(),
                    source_ids: source_ids.clone(),
                    limitations: limitations.clone(),
                },
            ),
        ]),
        vocabulary_pools: BTreeMap::from([(
            "wayfinding_terms".to_owned(),
            ExpressionPackVocabularyPool {
                id: "wayfinding_terms".to_owned(),
                category: "org.weave.expression.navigation".to_owned(),
                entry_ids: vec!["steady_question".to_owned(), "trailmark".to_owned()],
                applicability: ExpressionApplicability::default(),
                source_ids: source_ids.clone(),
            },
        )]),
        templates: BTreeMap::from([(
            "arrival_greeting".to_owned(),
            arrival_template(&source_ids, &limitations),
        )]),
        coverage: ExpressionPackCoverageRequirements {
            minimum_terms: 2,
            minimum_preferences: 1,
            minimum_behavioral_signatures: 1,
            minimum_voice_constraints: 1,
            required_categories: vec![
                "org.weave.expression.behavior".to_owned(),
                "org.weave.expression.clarity".to_owned(),
                "org.weave.expression.navigation".to_owned(),
                "org.weave.preference.communication".to_owned(),
            ],
            required_scenario_ids: vec!["arrival".to_owned()],
        },
        provenance: Provenance {
            sources: vec![ProvenanceSource {
                id: SOURCE_ID.to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "expression-dialogue-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
                attribution: "Original neutral expression vocabulary, editorial constraints, templates, lines, scenarios, and expected behavior."
                    .to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                ("entries".to_owned(), vec![SOURCE_ID.to_owned()]),
                ("templates".to_owned(), vec![SOURCE_ID.to_owned()]),
            ]),
        },
    }
}

fn arrival_template(source_ids: &[String], limitations: &[String]) -> ExpressionDialogueTemplate {
    let placeholders = BTreeMap::from([
        (
            "listener_name".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "listener_name".to_owned(),
                value: ExpressionPlaceholderValue::ListenerDisplayName,
                required: false,
            },
        ),
        (
            "date_label".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "date_label".to_owned(),
                value: ExpressionPlaceholderValue::DateLabel,
                required: false,
            },
        ),
        (
            "relationship_kind".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "relationship_kind".to_owned(),
                value: ExpressionPlaceholderValue::RelationshipKind,
                required: false,
            },
        ),
        (
            "speaker_name".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "speaker_name".to_owned(),
                value: ExpressionPlaceholderValue::SpeakerDisplayName,
                required: true,
            },
        ),
        (
            "waymark".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "waymark".to_owned(),
                value: ExpressionPlaceholderValue::LexiconTerm {
                    term_id: "trailmark".to_owned(),
                },
                required: false,
            },
        ),
        (
            "world_place".to_owned(),
            ExpressionPlaceholderDeclaration {
                id: "world_place".to_owned(),
                value: ExpressionPlaceholderValue::WorldPlaceName,
                required: false,
            },
        ),
    ]);
    let location = |line| ExpressionSourceLocation {
        source_id: "expression/glasswind-arrival.txt".to_owned(),
        line,
        column: 1,
    };
    ExpressionDialogueTemplate {
        id: "arrival_greeting".to_owned(),
        scenario_id: "arrival".to_owned(),
        speaker_requirement: ExpressionSpeakerRequirement::AssignedCharacter,
        placeholders,
        variants: BTreeMap::from([
            (
                "fallback".to_owned(),
                ExpressionDialogueVariant {
                    id: "fallback".to_owned(),
                    content: "{{speaker_name}} checks the route.".to_owned(),
                    priority: 0,
                    authority: ExpressionTextAuthority::Authored,
                    review: ReviewState::NotRequired,
                    applicability: ExpressionApplicability::default(),
                    source: location(1),
                    source_ids: source_ids.to_vec(),
                },
            ),
            (
                "festival_arrival".to_owned(),
                ExpressionDialogueVariant {
                    id: "festival_arrival".to_owned(),
                    content: "{{speaker_name}} marks {{date_label}} with a {{waymark}}."
                        .to_owned(),
                    priority: 15,
                    authority: ExpressionTextAuthority::Reviewed,
                    review: ReviewState::Accepted,
                    applicability: ExpressionApplicability {
                        scenario_ids: vec!["arrival".to_owned()],
                        predicates: vec![ExpressionContextPredicate::DateContext {
                            cue_id: "harbor_festival".to_owned(),
                        }],
                    },
                    source: location(2),
                    source_ids: source_ids.to_vec(),
                },
            ),
            (
                "friend_at_stormwatch".to_owned(),
                ExpressionDialogueVariant {
                    id: "friend_at_stormwatch".to_owned(),
                    content: "{{speaker_name}} greets {{listener_name}} at {{world_place}} and names their {{relationship_kind}} a {{waymark}}."
                        .to_owned(),
                    priority: 30,
                    authority: ExpressionTextAuthority::Authored,
                    review: ReviewState::NotRequired,
                    applicability: ExpressionApplicability {
                        scenario_ids: vec!["arrival".to_owned()],
                        predicates: vec![ExpressionContextPredicate::Relationship {
                            relationship_kind_id: "org.weave.relationship.friend".to_owned(),
                            other_character_id: Some(
                                "org.weave.character.tavi_quill".to_owned(),
                            ),
                        }],
                    },
                    source: location(3),
                    source_ids: source_ids.to_vec(),
                },
            ),
            (
                "open_arrival".to_owned(),
                ExpressionDialogueVariant {
                    id: "open_arrival".to_owned(),
                    content: "{{speaker_name}} offers a {{waymark}} and leaves the first turn open."
                        .to_owned(),
                    priority: 20,
                    authority: ExpressionTextAuthority::Reviewed,
                    review: ReviewState::Accepted,
                    applicability: ExpressionApplicability {
                        scenario_ids: vec!["arrival".to_owned()],
                        predicates: vec![ExpressionContextPredicate::PersonalityBand {
                            trait_id: HexacoTrait::Openness,
                            bands: vec![TraitBand::High, TraitBand::VeryHigh],
                        }],
                    },
                    source: location(4),
                    source_ids: source_ids.to_vec(),
                },
            ),
            (
                "stormwatch_arrival".to_owned(),
                ExpressionDialogueVariant {
                    id: "stormwatch_arrival".to_owned(),
                    content: "{{speaker_name}} calls {{world_place}} a {{waymark}} before stepping in."
                        .to_owned(),
                    priority: 10,
                    authority: ExpressionTextAuthority::Reviewed,
                    review: ReviewState::Accepted,
                    applicability: ExpressionApplicability {
                        scenario_ids: vec!["arrival".to_owned()],
                        predicates: vec![ExpressionContextPredicate::WorldContext {
                            tag: "stormwatch".to_owned(),
                        }],
                    },
                    source: location(5),
                    source_ids: source_ids.to_vec(),
                },
            ),
        ]),
        fallback_variant_id: "fallback".to_owned(),
        source_ids: source_ids.to_vec(),
        limitations: limitations.to_vec(),
    }
}
