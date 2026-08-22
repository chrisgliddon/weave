use std::collections::BTreeMap;

use weave_character::{
    ALIGNMENT_EXTENSION_NAMESPACE, ALL_HEXACO_FACETS, AlignmentReceipt, Attributed,
    CHARACTER_AUTHORING_REVISION_FORMAT_VERSION, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION, CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION,
    CHARACTER_TEMPLATE_FORMAT_VERSION, CharacterAuthoringChange, CharacterAuthoringInputMode,
    CharacterAuthoringPreview, CharacterAuthoringRevision, CharacterAuthoringWorkspace,
    CharacterFinalReviewDecision, CharacterOperation, CharacterOperationAction, CharacterOverlay,
    CharacterQuestionnaireAnswers, CharacterQuestionnaireComparison,
    CharacterQuestionnaireConflictDecision, CharacterQuestionnaireConflictPredicate,
    CharacterQuestionnaireConflictRule, CharacterQuestionnaireFacetDecision,
    CharacterQuestionnaireItem, CharacterQuestionnairePack, CharacterQuestionnairePackRef,
    CharacterReviewedEnrichment, CharacterTemplate, CharacterTemplateRef, Confidence, Freshness,
    HexacoTrait, LockState, ReviewState, TraitBand, TraitMeasurement, ValueState,
    apply_authoring_revision, apply_character_questionnaire_review,
    character_authoring_workspace_schema, clone_authoring_draft, create_authoring_draft,
    create_character_questionnaire_review, effective_authoring_profile,
    enrichment_authoring_revision, export_authoring_profile, inspect_authoring_fields,
    list_authoring_drafts, new_authoring_workspace, preview_authoring_revision,
    propose_character_questionnaire, questionnaire_authoring_revision,
    questionnaire_pack_fingerprint, review_authoring_draft, template_fingerprint,
    validate_authoring_revision,
};
use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

const PROFILE: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.json");
const ALIGNMENT_INPUT: &str =
    include_str!("../../../examples/domain-modules/weave-character/alignment/input.character.json");
const ALIGNMENT_RECEIPT: &str = include_str!(
    "../../../examples/domain-modules/weave-character/alignment/receipt.alignment-receipt.json"
);

fn source(id: &str, attribution: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "guided-authoring-v1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            attribution: attribution.to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([("artifact".to_owned(), vec![id.to_owned()])]),
    }
}

fn authored_text(value: &str, lineage: &str) -> Attributed<String> {
    Attributed {
        value: value.to_owned(),
        state: ValueState::Authored,
        confidence: Confidence::High,
        review: ReviewState::NotRequired,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: vec![lineage.to_owned()],
        rationale: None,
    }
}

fn blank_overlay(id: &str, name: &str) -> CharacterOverlay {
    CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: format!("{id}.overlay"),
        character_id: id.to_owned(),
        template: None,
        operations: vec![CharacterOperation {
            id: "set_display_name".to_owned(),
            expected_prior_sha256: None,
            rationale: "Create a stable fictional identity.".to_owned(),
            action: CharacterOperationAction::SetDisplayName {
                value: authored_text(name, "authoring_original"),
            },
        }],
        provenance: source(
            "authoring_original",
            "Original synthetic guided-authoring test profile.",
        ),
    }
}

fn questionnaire_pack() -> CharacterQuestionnairePack {
    let items = ALL_HEXACO_FACETS
        .iter()
        .enumerate()
        .map(|(index, trait_id)| {
            let id = format!("behavior_{index:02}");
            (
                id.clone(),
                CharacterQuestionnaireItem {
                    id,
                    prompt: format!(
                        "In fictional scene {index}, how consistently does the character choose this described behavior?"
                    ),
                    facet_weights_micros: BTreeMap::from([(*trait_id, 1_000_000)]),
                },
            )
        })
        .collect();
    CharacterQuestionnairePack {
        pack_format_version: CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION,
        id: "org.weave.character.questionnaire.lantern_choices".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Lantern Choices".to_owned(),
        methodology: "Each original fictional behavior response contributes through one declared signed fixed-point facet weight.".to_owned(),
        limitations: vec![
            "It is a fictional authoring aid, not an assessment of a person.".to_owned(),
            "Scores are reviewable prompts and are not independent personality evidence.".to_owned(),
        ],
        narrative_authoring_only: true,
        independently_authored_prompts: true,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        items,
        conflict_rules: BTreeMap::from([(
            "bold_plan_tension".to_owned(),
            CharacterQuestionnaireConflictRule {
                id: "bold_plan_tension".to_owned(),
                label: "Bold plan tension".to_owned(),
                explanation: "High social boldness and high prudence can be retained as an intentional fictional tension.".to_owned(),
                predicates: vec![
                    CharacterQuestionnaireConflictPredicate {
                        trait_id: HexacoTrait::SocialBoldness,
                        comparison: CharacterQuestionnaireComparison::AtLeast,
                        threshold_micros: 700_000,
                    },
                    CharacterQuestionnaireConflictPredicate {
                        trait_id: HexacoTrait::Prudence,
                        comparison: CharacterQuestionnaireComparison::AtLeast,
                        threshold_micros: 700_000,
                    },
                ],
                rejected_traits: vec![
                    HexacoTrait::SocialBoldness,
                    HexacoTrait::Prudence,
                ],
                rationale_required: true,
            },
        )]),
        provenance: source(
            "questionnaire_original",
            "Original synthetic narrative-questionnaire test pack.",
        ),
    }
}

fn answers(pack: &CharacterQuestionnairePack) -> CharacterQuestionnaireAnswers {
    CharacterQuestionnaireAnswers {
        answers_format_version: CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION,
        id: "org.weave.character.answers.lumen_reed".to_owned(),
        pack: CharacterQuestionnairePackRef {
            id: pack.id.clone(),
            version: pack.version.clone(),
            sha256: questionnaire_pack_fingerprint(pack).unwrap(),
        },
        responses: pack.items.keys().cloned().map(|id| (id, 2)).collect(),
    }
}

fn accepted_review(
    proposal: &weave_character::CharacterQuestionnaireProposal,
) -> weave_character::CharacterQuestionnaireReview {
    create_character_questionnaire_review(
        proposal,
        "org.weave.reviewer.author",
        "Review every generated facet and retain one deliberate fictional tension.",
        ALL_HEXACO_FACETS
            .into_iter()
            .map(|trait_id| {
                (
                    trait_id,
                    CharacterQuestionnaireFacetDecision::Accept {
                        confidence: Confidence::Moderate,
                        lock: LockState::Unlocked,
                    },
                )
            })
            .collect(),
        BTreeMap::from([(
            "bold_plan_tension".to_owned(),
            CharacterQuestionnaireConflictDecision::DeliberateException {
                rationale: "The character speaks readily but still prepares careful routes."
                    .to_owned(),
            },
        )]),
    )
    .unwrap()
}

fn assert_semantic_pair<T>(root: &std::path::Path, stem: &str)
where
    T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = std::fs::read_to_string(root.join(format!("{stem}.json"))).unwrap();
    let ron = std::fs::read_to_string(root.join(format!("{stem}.ron"))).unwrap();
    let json_value: T = weave_domain::parse_strict_json(&json).unwrap();
    let ron_value: T = ron::from_str(&ron).unwrap();
    assert_eq!(json_value, ron_value, "semantic JSON/RON drift for {stem}");
}

#[test]
fn questionnaire_workspace_flow_round_trips_and_exports_reviewed_canon() {
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.demo",
        source(
            "workspace_original",
            "Original synthetic guided-authoring workspace.",
        ),
    )
    .unwrap();
    let workspace = create_authoring_draft(
        &workspace,
        None,
        blank_overlay("org.weave.character.lumen_reed", "Lumen Reed"),
    )
    .unwrap();
    let draft = &workspace.drafts["org.weave.character.lumen_reed"];
    let input = effective_authoring_profile(draft).unwrap();
    let pack = questionnaire_pack();
    let proposal =
        propose_character_questionnaire(&input, &pack, &answers(&pack), 20_260_822).unwrap();
    assert_eq!(proposal.facets.len(), 24);
    assert_eq!(proposal.conflicts.len(), 1);
    assert!(proposal.facets.values().all(|facet| facet.trace.len() == 1));

    let review = accepted_review(&proposal);
    let receipt = apply_character_questionnaire_review(&proposal, &review).unwrap();
    assert_eq!(receipt.applied_traits.len(), 24);
    let revision = questionnaire_authoring_revision(
        draft,
        "org.weave.character.revision.questionnaire",
        "Apply the fully reviewed original questionnaire proposal.",
        receipt,
    )
    .unwrap();
    let preview = preview_authoring_revision(&workspace, &revision).unwrap();
    assert!(preview.derived_ocean.visually_distinct_from_canon);
    assert!(preview.derived_ocean.recomputed_in_candidate);
    assert!(
        preview
            .invalidations
            .iter()
            .any(|item| item.id == "derived_ocean" && item.resolved_in_candidate)
    );
    let workspace = apply_authoring_revision(&workspace, &revision).unwrap();
    let workspace = review_authoring_draft(
        &workspace,
        "org.weave.character.lumen_reed",
        "org.weave.reviewer.final",
        "The canonical, derived, contextual, and provenance summaries are ready.",
        CharacterFinalReviewDecision::Accepted,
    )
    .unwrap();
    let exported =
        export_authoring_profile(&workspace, "org.weave.character.lumen_reed", true).unwrap();
    assert_eq!(
        exported
            .canon
            .personality
            .conscientiousness
            .prudence
            .as_ref()
            .unwrap()
            .state,
        ValueState::Reviewed
    );

    let json = workspace.to_json().unwrap();
    let ron = workspace.to_ron().unwrap();
    assert_eq!(
        weave_character::CharacterAuthoringWorkspace::from_json(&json).unwrap(),
        workspace
    );
    assert_eq!(
        weave_character::CharacterAuthoringWorkspace::from_ron(&ron).unwrap(),
        workspace
    );
    assert!(
        character_authoring_workspace_schema()
            .unwrap()
            .contains("workspace_format_version")
    );
}

#[test]
fn direct_picker_and_protected_edits_share_one_fail_closed_revision_path() {
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.direct",
        source("workspace_original", "Original synthetic workspace."),
    )
    .unwrap();
    let workspace = create_authoring_draft(
        &workspace,
        None,
        blank_overlay("org.weave.character.ember_song", "Ember Song"),
    )
    .unwrap();
    let draft = &workspace.drafts["org.weave.character.ember_song"];
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: "org.weave.character.revision.picker".to_owned(),
        draft_id: draft.id.clone(),
        expected_draft_revision: draft.revision,
        input_mode: CharacterAuthoringInputMode::ConcisePicker,
        rationale: "Choose one concise authored facet band.".to_owned(),
        changes: vec![CharacterAuthoringChange {
            id: "set_prudence".to_owned(),
            rationale: "The author selected a concise band.".to_owned(),
            action: CharacterOperationAction::SetHexacoTrait {
                trait_id: HexacoTrait::Prudence,
                value: Attributed {
                    value: TraitMeasurement::Band {
                        band: TraitBand::High,
                    },
                    state: ValueState::Authored,
                    confidence: Confidence::High,
                    review: ReviewState::NotRequired,
                    lock: LockState::Unlocked,
                    freshness: Freshness::Current,
                    lineage: vec!["authoring_original".to_owned()],
                    rationale: None,
                },
            },
        }],
        questionnaire_receipt: None,
        enrichment: None,
        template_migration: None,
        provenance: draft.overlay.provenance.clone(),
    };
    let workspace = apply_authoring_revision(&workspace, &revision).unwrap();
    let fields = inspect_authoring_fields(&workspace.drafts[&draft.id]).unwrap();
    assert!(fields.iter().any(|field| {
        field.path.ends_with(".prudence") && field.overridden && !field.inherited
    }));

    let mut protected = revision.clone();
    protected.id = "org.weave.character.revision.protected".to_owned();
    protected.expected_draft_revision = 1;
    protected.input_mode = CharacterAuthoringInputMode::General;
    protected.changes[0].action = CharacterOperationAction::RemoveExtension {
        namespace: ALIGNMENT_EXTENSION_NAMESPACE.to_owned(),
    };
    assert!(validate_authoring_revision(&protected).is_err());

    let mut placeholder = protected;
    placeholder.id = "org.weave.character.revision.placeholder".to_owned();
    placeholder.changes[0].action = CharacterOperationAction::SetDisplayName {
        value: authored_text("<unknown>", "authoring_original"),
    };
    assert!(validate_authoring_revision(&placeholder).is_err());
}

#[test]
fn reviewed_alignment_enters_only_through_its_reproducible_receipt() {
    let input = weave_character::CharacterProfile::from_json(ALIGNMENT_INPUT).unwrap();
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.alignment_input".to_owned(),
        version: "1.0.0".to_owned(),
        profile: input.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.alignment_input".to_owned(),
        character_id: input.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template).unwrap(),
        }),
        operations: Vec::new(),
        provenance: input.provenance.clone(),
    };
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.alignment",
        source("workspace_original", "Original synthetic workspace."),
    )
    .unwrap();
    let workspace = create_authoring_draft(&workspace, Some(template), overlay).unwrap();
    let receipt = AlignmentReceipt::from_json(ALIGNMENT_RECEIPT).unwrap();
    let draft = &workspace.drafts[&input.id];
    let revision = enrichment_authoring_revision(
        draft,
        "org.weave.character.revision.alignment",
        "Adopt the independently reproduced alignment review.",
        CharacterReviewedEnrichment::Alignment(Box::new(receipt.clone())),
    )
    .unwrap();
    let preview = preview_authoring_revision(&workspace, &revision).unwrap();
    assert_eq!(
        preview.resulting_profile_sha256,
        receipt.output_profile_sha256
    );
    let workspace = apply_authoring_revision(&workspace, &revision).unwrap();
    let profile = effective_authoring_profile(&workspace.drafts[&input.id]).unwrap();
    assert_eq!(profile, receipt.output_profile);
}

#[test]
fn template_clone_retains_base_and_effective_inspection() {
    let mut profile = weave_character::CharacterProfile::from_json(PROFILE).unwrap();
    profile
        .extensions
        .remove("org.weave.character.relationships");
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.public_safe".to_owned(),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.template_source".to_owned(),
        character_id: profile.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template).unwrap(),
        }),
        operations: Vec::new(),
        provenance: profile.provenance.clone(),
    };
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.clone",
        source("workspace_original", "Original synthetic workspace."),
    )
    .unwrap();
    let workspace = create_authoring_draft(&workspace, Some(template), overlay).unwrap();
    let workspace = clone_authoring_draft(
        &workspace,
        &profile.id,
        "org.weave.character.ari_vale_clone",
        "org.weave.character.overlay.ari_vale_clone",
    )
    .unwrap();
    assert_eq!(list_authoring_drafts(&workspace).unwrap().len(), 2);
    let fields =
        inspect_authoring_fields(&workspace.drafts["org.weave.character.ari_vale_clone"]).unwrap();
    assert!(fields.iter().all(|field| !field.overridden));
    assert!(fields.iter().any(|field| field.inherited));
}

#[test]
fn checked_golden_projects_cover_migration_invalidation_review_export_and_reopen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/domain-modules/weave-character/authoring");
    for stem in [
        "workspace.empty.authoring-workspace",
        "workspace.created.authoring-workspace",
        "workspace.revised.authoring-workspace",
        "workspace.reviewed.authoring-workspace",
    ] {
        assert_semantic_pair::<CharacterAuthoringWorkspace>(&root, stem);
    }
    assert_semantic_pair::<CharacterAuthoringRevision>(&root, "questionnaire.authoring-revision");
    for stem in [
        "questionnaire.authoring-preview",
        "migration.authoring-preview",
        "invalidation.authoring-preview",
    ] {
        assert_semantic_pair::<CharacterAuthoringPreview>(&root, stem);
    }
    assert_semantic_pair::<weave_character::CharacterQuestionnaireProposal>(
        &root,
        "proposal.questionnaire-proposal",
    );
    assert_semantic_pair::<weave_character::CharacterQuestionnaireReview>(
        &root,
        "review.questionnaire-review",
    );
    assert_semantic_pair::<weave_character::CharacterQuestionnaireReceipt>(
        &root,
        "receipt.questionnaire-receipt",
    );

    let reviewed = CharacterAuthoringWorkspace::from_json(
        &std::fs::read_to_string(root.join("workspace.reviewed.authoring-workspace.json")).unwrap(),
    )
    .unwrap();
    let exported =
        export_authoring_profile(&reviewed, "org.weave.character.lumen_reed", true).unwrap();
    assert_eq!(
        exported,
        weave_character::CharacterProfile::from_json(
            &std::fs::read_to_string(root.join("exported.character.json")).unwrap()
        )
        .unwrap()
    );

    let migration: CharacterAuthoringPreview = weave_domain::parse_strict_json(
        &std::fs::read_to_string(root.join("migration.authoring-preview.json")).unwrap(),
    )
    .unwrap();
    assert!(!migration.migration_effects.is_empty());
    let invalidation: CharacterAuthoringPreview = weave_domain::parse_strict_json(
        &std::fs::read_to_string(root.join("invalidation.authoring-preview.json")).unwrap(),
    )
    .unwrap();
    assert!(invalidation.invalidations.iter().any(|item| {
        item.id == "alignment_review" && item.blocking_final_review && !item.resolved_in_candidate
    }));

    let context = CharacterAuthoringWorkspace::from_json(
        &std::fs::read_to_string(root.join("context.workspace.reviewed.authoring-workspace.json"))
            .unwrap(),
    )
    .unwrap();
    let final_summary = &context
        .drafts
        .values()
        .next()
        .unwrap()
        .final_review
        .as_ref()
        .unwrap()
        .summary;
    assert!(final_summary.approved_alignment.is_some());
    assert!(final_summary.accepted_date_context.is_some());

    for file in [
        "unknown-placeholder.authoring-revision.json",
        "protected-extension.authoring-revision.json",
        "unknown-field.authoring-revision.json",
    ] {
        assert!(
            CharacterAuthoringRevision::from_json(
                &std::fs::read_to_string(root.join("invalid").join(file)).unwrap()
            )
            .is_err()
        );
    }
    assert!(CharacterAuthoringWorkspace::from_json(
        &std::fs::read_to_string(
            root.join("invalid/broken-template.authoring-workspace.json"),
        )
        .unwrap(),
    )
    .is_err());
    let created = CharacterAuthoringWorkspace::from_json(
        &std::fs::read_to_string(root.join("workspace.created.authoring-workspace.json")).unwrap(),
    )
    .unwrap();
    let stale = CharacterAuthoringRevision::from_json(
        &std::fs::read_to_string(root.join("invalid/stale.authoring-revision.json")).unwrap(),
    )
    .unwrap();
    assert!(preview_authoring_revision(&created, &stale).is_err());
}
