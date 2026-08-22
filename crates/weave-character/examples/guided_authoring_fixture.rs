use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use weave_character::{
    ALL_HEXACO_FACETS, AlignmentReceipt, Attributed, AuthoredNote, BirthDate,
    CHARACTER_AUTHORING_REVISION_FORMAT_VERSION, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION, CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION,
    CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar, CharacterAuthoringChange,
    CharacterAuthoringInputMode, CharacterAuthoringRevision, CharacterFinalReviewDecision,
    CharacterOperation, CharacterOperationAction, CharacterOverlay, CharacterProfile,
    CharacterQuestionnaireAnswers, CharacterQuestionnaireComparison,
    CharacterQuestionnaireConflictDecision, CharacterQuestionnaireConflictPredicate,
    CharacterQuestionnaireConflictRule, CharacterQuestionnaireFacetDecision,
    CharacterQuestionnaireItem, CharacterQuestionnairePack, CharacterQuestionnairePackRef,
    CharacterReviewedEnrichment, CharacterTemplate, CharacterTemplateRef, Confidence, Freshness,
    HexacoTrait, InnerLifeCategory, LockState, ReviewState, TraitBand, TraitMeasurement,
    ValueState, VoiceCategory, VoiceDirection, apply_authoring_revision,
    apply_character_questionnaire_review, character_authoring_preview_schema,
    character_authoring_revision_schema, character_authoring_workspace_schema,
    character_final_review_schema, character_questionnaire_answers_schema,
    character_questionnaire_pack_schema, character_questionnaire_proposal_schema,
    character_questionnaire_receipt_schema, character_questionnaire_review_schema,
    create_authoring_draft, create_character_questionnaire_review, effective_authoring_profile,
    enrichment_authoring_revision, export_authoring_profile, new_authoring_workspace,
    preview_authoring_revision, propose_character_questionnaire, questionnaire_authoring_revision,
    questionnaire_pack_fingerprint, review_authoring_draft, template_fingerprint,
};
use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--check".to_owned());
    let write = match mode.as_str() {
        "--write" => true,
        "--check" => false,
        _ => return Err("expected --write or --check".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root")?
        .to_path_buf();
    let fixture = root.join("examples/domain-modules/weave-character/authoring");
    let schemas = root.join("schemas");

    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.lumen_reed",
        original_provenance(
            "workspace_original",
            "workspace",
            "Original synthetic guided-authoring workspace.",
        ),
    )?;
    let overlay = blank_overlay();
    let created = create_authoring_draft(&workspace, None, overlay.clone())?;
    let input = effective_authoring_profile(&created.drafts["org.weave.character.lumen_reed"])?;
    let pack = questionnaire_pack();
    let answers = questionnaire_answers(&pack)?;
    let proposal = propose_character_questionnaire(&input, &pack, &answers, 20_260_822)?;
    let facet_decisions = ALL_HEXACO_FACETS
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
        .collect::<BTreeMap<_, _>>();
    let conflict_decisions = BTreeMap::from([(
        "bold_plan_tension".to_owned(),
        CharacterQuestionnaireConflictDecision::DeliberateException {
            rationale: "Lumen speaks readily while still preparing careful routes.".to_owned(),
        },
    )]);
    let review = create_character_questionnaire_review(
        &proposal,
        "org.weave.reviewer.fixture",
        "Review every proposed facet and retain one intentional narrative tension.",
        facet_decisions.clone(),
        conflict_decisions.clone(),
    )?;
    let receipt = apply_character_questionnaire_review(&proposal, &review)?;
    let revision = questionnaire_authoring_revision(
        &created.drafts["org.weave.character.lumen_reed"],
        "org.weave.character.revision.lumen_questionnaire",
        "Apply the fully reviewed original narrative questionnaire.",
        receipt.clone(),
    )?;
    let preview = preview_authoring_revision(&created, &revision)?;
    let revised = apply_authoring_revision(&created, &revision)?;
    let reviewed = review_authoring_draft(
        &revised,
        "org.weave.character.lumen_reed",
        "org.weave.reviewer.final",
        "Canonical fields, derived views, context, provenance, and diagnostics are ready.",
        CharacterFinalReviewDecision::Accepted,
    )?;
    let exported = export_authoring_profile(&reviewed, "org.weave.character.lumen_reed", true)?;

    write_pair(
        &fixture,
        "workspace.empty.authoring-workspace",
        &workspace,
        write,
    )?;
    write_pair(&fixture, "blank.authoring-overlay", &overlay, write)?;
    write_pair(
        &fixture,
        "workspace.created.authoring-workspace",
        &created,
        write,
    )?;
    write_pair(&fixture, "input.character", &input, write)?;
    write_pair(&fixture, "lantern_choices.questionnaire-pack", &pack, write)?;
    write_pair(&fixture, "answers.questionnaire-answers", &answers, write)?;
    write_pair(
        &fixture,
        "proposal.questionnaire-proposal",
        &proposal,
        write,
    )?;
    write_pair(
        &fixture,
        "facet-decisions.questionnaire-review",
        &facet_decisions,
        write,
    )?;
    write_pair(
        &fixture,
        "conflict-decisions.questionnaire-review",
        &conflict_decisions,
        write,
    )?;
    write_pair(&fixture, "review.questionnaire-review", &review, write)?;
    write_pair(&fixture, "receipt.questionnaire-receipt", &receipt, write)?;
    write_pair(
        &fixture,
        "questionnaire.authoring-revision",
        &revision,
        write,
    )?;
    write_pair(&fixture, "questionnaire.authoring-preview", &preview, write)?;
    write_pair(
        &fixture,
        "workspace.revised.authoring-workspace",
        &revised,
        write,
    )?;
    write_pair(
        &fixture,
        "workspace.reviewed.authoring-workspace",
        &reviewed,
        write,
    )?;
    write_pair(&fixture, "exported.character", &exported, write)?;

    write_invalidation_fixture(&root, &fixture, write)?;
    write_migration_fixture(&root, &fixture, write)?;
    write_context_review_fixture(&root, &fixture, write)?;
    write_invalid_fixtures(&created, &revision, &fixture, write)?;

    write_schema(
        &schemas.join("weave-character-authoring-workspace-v1.schema.json"),
        character_authoring_workspace_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-authoring-revision-v1.schema.json"),
        character_authoring_revision_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-authoring-preview-v1.schema.json"),
        character_authoring_preview_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-questionnaire-pack-v1.schema.json"),
        character_questionnaire_pack_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-questionnaire-answers-v1.schema.json"),
        character_questionnaire_answers_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-questionnaire-proposal-v1.schema.json"),
        character_questionnaire_proposal_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-questionnaire-review-v1.schema.json"),
        character_questionnaire_review_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-questionnaire-receipt-v1.schema.json"),
        character_questionnaire_receipt_schema()?,
        write,
    )?;
    write_schema(
        &schemas.join("weave-character-final-review-v1.schema.json"),
        character_final_review_schema()?,
        write,
    )
}

fn blank_overlay() -> CharacterOverlay {
    CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.lumen_reed".to_owned(),
        character_id: "org.weave.character.lumen_reed".to_owned(),
        template: None,
        operations: vec![
            CharacterOperation {
                id: "set_birth_date".to_owned(),
                expected_prior_sha256: None,
                rationale: "Record only the explicitly authored date precision.".to_owned(),
                action: CharacterOperationAction::SetBirthDate {
                    value: authored(BirthDate::Full {
                        calendar: Calendar::ProlepticGregorian,
                        year: 1001,
                        month: 4,
                        day: 7,
                    }),
                },
            },
            CharacterOperation {
                id: "set_display_name".to_owned(),
                expected_prior_sha256: None,
                rationale: "Create one stable fictional identity.".to_owned(),
                action: CharacterOperationAction::SetDisplayName {
                    value: authored("Lumen Reed".to_owned()),
                },
            },
            CharacterOperation {
                id: "set_quiet_vow".to_owned(),
                expected_prior_sha256: None,
                rationale: "Record one authored inner-life value.".to_owned(),
                action: CharacterOperationAction::UpsertInnerLife {
                    record: AuthoredNote {
                        id: "quiet_vow".to_owned(),
                        category: InnerLifeCategory::Value,
                        content: authored(
                            "Leave a clear trail for anyone who must follow after the storm."
                                .to_owned(),
                        ),
                    },
                },
            },
            CharacterOperation {
                id: "set_measured_cadence".to_owned(),
                expected_prior_sha256: None,
                rationale: "Record one authored voice direction.".to_owned(),
                action: CharacterOperationAction::UpsertVoice {
                    record: VoiceDirection {
                        id: "measured_cadence".to_owned(),
                        category: VoiceCategory::Cadence,
                        content: authored(
                            "Uses short clauses, then leaves a deliberate pause before a decision."
                                .to_owned(),
                        ),
                    },
                },
            },
        ],
        provenance: original_provenance(
            "authoring_original",
            "overlay",
            "Original synthetic blank Character overlay.",
        ),
    }
}

fn authored<T>(value: T) -> Attributed<T> {
    Attributed {
        value,
        state: ValueState::Authored,
        confidence: Confidence::High,
        review: ReviewState::NotRequired,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: vec!["authoring_original".to_owned()],
        rationale: None,
    }
}

fn questionnaire_pack() -> CharacterQuestionnairePack {
    let prompts = [
        "When a promise becomes inconvenient, the character states their real intent plainly.",
        "When dividing scarce supplies, the character applies the same rule to every companion.",
        "When offered status for its own sake, the character can leave the offer behind.",
        "When praised in a crowded room, the character shares credit without erasing their work.",
        "When a route becomes dangerous, the character notices the risk before stepping forward.",
        "When plans remain uncertain, the character revisits possible outcomes before resting.",
        "When overwhelmed, the character asks a trusted companion to stay nearby.",
        "When an old keepsake resurfaces, the character allows the memory to affect the moment.",
        "When challenged publicly, the character retains a stable sense of their own worth.",
        "When a room falls silent, the character is willing to speak first.",
        "When work pauses, the character seeks company rather than solitude.",
        "When the group is tired, the character brings visible energy back to the task.",
        "When someone repairs a past harm, the character can loosen an old grievance.",
        "When correcting a mistake, the character chooses language that preserves dignity.",
        "When a plan changes, the character can adapt without treating revision as defeat.",
        "When provoked, the character leaves space before answering.",
        "When tools scatter, the character restores an order others can follow.",
        "When a long task becomes repetitive, the character continues the necessary work.",
        "When a craft is nearly complete, the character still notices small inconsistencies.",
        "Before a consequential choice, the character checks likely effects and escape routes.",
        "When entering an unfamiliar place, the character pauses to notice its form and texture.",
        "When a mechanism behaves strangely, the character keeps asking how it works.",
        "When a familiar method fails, the character invents another way to frame the problem.",
        "When custom blocks a useful idea, the character is willing to question the custom.",
    ];
    let items = ALL_HEXACO_FACETS
        .iter()
        .zip(prompts)
        .enumerate()
        .map(|(index, (trait_id, prompt))| {
            let id = format!("behavior_{index:02}");
            (
                id.clone(),
                CharacterQuestionnaireItem {
                    id,
                    prompt: prompt.to_owned(),
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
        methodology: "Each independently written fictional behavior response contributes through one declared signed fixed-point facet weight; authors review every result before canon changes.".to_owned(),
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
                explanation: "High social boldness and high prudence can be retained as an intentional fictional tension rather than silently averaged.".to_owned(),
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
        provenance: original_provenance(
            "questionnaire_original",
            "pack",
            "Original synthetic Lantern Choices questionnaire pack.",
        ),
    }
}

fn questionnaire_answers(
    pack: &CharacterQuestionnairePack,
) -> Result<CharacterQuestionnaireAnswers, weave_character::CharacterError> {
    Ok(CharacterQuestionnaireAnswers {
        answers_format_version: CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION,
        id: "org.weave.character.answers.lumen_reed".to_owned(),
        pack: CharacterQuestionnairePackRef {
            id: pack.id.clone(),
            version: pack.version.clone(),
            sha256: questionnaire_pack_fingerprint(pack)?,
        },
        responses: pack.items.keys().cloned().map(|id| (id, 2)).collect(),
    })
}

fn write_invalidation_fixture(
    root: &Path,
    fixture: &Path,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = CharacterProfile::from_json(&fs::read_to_string(
        root.join("examples/domain-modules/weave-character/alignment/input.character.json"),
    )?)?;
    let receipt = AlignmentReceipt::from_json(&fs::read_to_string(root.join(
        "examples/domain-modules/weave-character/alignment/receipt.alignment-receipt.json",
    ))?)?;
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.alignment_authoring".to_owned(),
        version: "1.0.0".to_owned(),
        profile: input.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.alignment_authoring".to_owned(),
        character_id: input.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template)?,
        }),
        operations: Vec::new(),
        provenance: input.provenance.clone(),
    };
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.invalidation",
        original_provenance(
            "workspace_original",
            "workspace",
            "Original synthetic invalidation workspace.",
        ),
    )?;
    let workspace = create_authoring_draft(&workspace, Some(template), overlay)?;
    let enrichment = enrichment_authoring_revision(
        &workspace.drafts[&input.id],
        "org.weave.character.revision.adopt_alignment",
        "Adopt the independently reproduced alignment receipt.",
        CharacterReviewedEnrichment::Alignment(Box::new(receipt)),
    )?;
    let enriched = apply_authoring_revision(&workspace, &enrichment)?;
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: "org.weave.character.revision.invalidate_alignment".to_owned(),
        draft_id: input.id.clone(),
        expected_draft_revision: enriched.drafts[&input.id].revision,
        input_mode: CharacterAuthoringInputMode::DirectFacets,
        rationale: "Revise one canonical facet after inspecting downstream invalidation."
            .to_owned(),
        changes: vec![CharacterAuthoringChange {
            id: "revise_social_boldness".to_owned(),
            rationale: "The author chose a more reserved scene behavior.".to_owned(),
            action: CharacterOperationAction::SetHexacoTrait {
                trait_id: HexacoTrait::SocialBoldness,
                value: Attributed {
                    value: TraitMeasurement::Band {
                        band: TraitBand::Low,
                    },
                    state: ValueState::Authored,
                    confidence: Confidence::High,
                    review: ReviewState::NotRequired,
                    lock: LockState::Unlocked,
                    freshness: Freshness::Current,
                    lineage: vec!["revision_original".to_owned()],
                    rationale: None,
                },
            },
        }],
        questionnaire_receipt: None,
        enrichment: None,
        template_migration: None,
        provenance: original_provenance(
            "revision_original",
            "revision",
            "Original synthetic facet revision.",
        ),
    };
    let preview = preview_authoring_revision(&enriched, &revision)?;
    let invalidated = apply_authoring_revision(&enriched, &revision)?;
    let needs_changes = review_authoring_draft(
        &invalidated,
        &input.id,
        "org.weave.reviewer.final",
        "Alignment must be refreshed before this draft is ready to export.",
        CharacterFinalReviewDecision::NeedsChanges,
    )?;
    write_pair(
        fixture,
        "invalidation.enriched.authoring-workspace",
        &enriched,
        write,
    )?;
    write_pair(fixture, "invalidation.authoring-revision", &revision, write)?;
    write_pair(fixture, "invalidation.authoring-preview", &preview, write)?;
    write_pair(
        fixture,
        "invalidation.needs-changes.authoring-workspace",
        &needs_changes,
        write,
    )
}

fn write_migration_fixture(
    root: &Path,
    fixture: &Path,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let template = CharacterTemplate::from_json(&fs::read_to_string(
        root.join("examples/domain-modules/weave-character/template.character.json"),
    )?)?;
    let overlay = CharacterOverlay::from_json(&fs::read_to_string(
        root.join("examples/domain-modules/weave-character/overlay.character.json"),
    )?)?;
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.migration",
        original_provenance(
            "workspace_original",
            "workspace",
            "Original synthetic template-migration workspace.",
        ),
    )?;
    let workspace = create_authoring_draft(&workspace, Some(template.clone()), overlay)?;
    let mut updated = template.clone();
    updated.version = "1.1.0".to_owned();
    updated.profile.canon.identity.display_name.value = "Ari Vale, Wayfinder".to_owned();
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: "org.weave.character.revision.template_migration".to_owned(),
        draft_id: workspace
            .drafts
            .keys()
            .next()
            .cloned()
            .ok_or("missing draft")?,
        expected_draft_revision: 0,
        input_mode: CharacterAuthoringInputMode::TemplateMigration,
        rationale: "Preview and accept one explicit reviewed template release migration."
            .to_owned(),
        changes: Vec::new(),
        questionnaire_receipt: None,
        enrichment: None,
        template_migration: Some(Box::new(
            weave_character::ReviewedCharacterTemplateMigration {
                prior_template_sha256: template_fingerprint(&template)?,
                template: updated,
                reviewer: "org.weave.reviewer.fixture".to_owned(),
                rationale:
                    "The effective authored override remains intact after reviewing base changes."
                        .to_owned(),
            },
        )),
        provenance: original_provenance(
            "migration_original",
            "migration",
            "Original synthetic template migration review.",
        ),
    };
    let preview = preview_authoring_revision(&workspace, &revision)?;
    let migrated = apply_authoring_revision(&workspace, &revision)?;
    write_pair(
        fixture,
        "migration.before.authoring-workspace",
        &workspace,
        write,
    )?;
    write_pair(fixture, "migration.authoring-revision", &revision, write)?;
    write_pair(fixture, "migration.authoring-preview", &preview, write)?;
    write_pair(
        fixture,
        "migration.after.authoring-workspace",
        &migrated,
        write,
    )
}

fn write_context_review_fixture(
    root: &Path,
    fixture: &Path,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let profile = CharacterProfile::from_json(&fs::read_to_string(
        root.join("examples/domain-modules/weave-character/context/enriched.character.json"),
    )?)?;
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.reviewed_context".to_owned(),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.reviewed_context".to_owned(),
        character_id: profile.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template)?,
        }),
        operations: Vec::new(),
        provenance: profile.provenance.clone(),
    };
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.reviewed_context",
        original_provenance(
            "workspace_original",
            "workspace",
            "Original synthetic final-context review workspace.",
        ),
    )?;
    let workspace = create_authoring_draft(&workspace, Some(template), overlay)?;
    let reviewed = review_authoring_draft(
        &workspace,
        &profile.id,
        "org.weave.reviewer.final",
        "Review canonical traits, derived display, approved alignment, accepted date context, provenance, and diagnostics.",
        CharacterFinalReviewDecision::Accepted,
    )?;
    write_pair(
        fixture,
        "context.workspace.reviewed.authoring-workspace",
        &reviewed,
        write,
    )
}

fn write_invalid_fixtures(
    created: &weave_character::CharacterAuthoringWorkspace,
    revision: &CharacterAuthoringRevision,
    fixture: &Path,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let invalid = fixture.join("invalid");
    let mut stale = revision.clone();
    stale.expected_draft_revision = 99;
    write_raw_json(
        &invalid.join("stale.authoring-revision.json"),
        &stale,
        write,
    )?;

    let mut placeholder = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: "org.weave.character.revision.placeholder".to_owned(),
        draft_id: "org.weave.character.lumen_reed".to_owned(),
        expected_draft_revision: 0,
        input_mode: CharacterAuthoringInputMode::General,
        rationale: "Invalid placeholder fixture.".to_owned(),
        changes: vec![CharacterAuthoringChange {
            id: "placeholder_name".to_owned(),
            rationale: "Invalid placeholder fixture.".to_owned(),
            action: CharacterOperationAction::SetDisplayName {
                value: Attributed {
                    value: "<unknown>".to_owned(),
                    state: ValueState::Authored,
                    confidence: Confidence::Unknown,
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
        provenance: created.drafts["org.weave.character.lumen_reed"]
            .overlay
            .provenance
            .clone(),
    };
    write_raw_json(
        &invalid.join("unknown-placeholder.authoring-revision.json"),
        &placeholder,
        write,
    )?;
    placeholder.id = "org.weave.character.revision.protected".to_owned();
    placeholder.changes[0].id = "remove_alignment".to_owned();
    placeholder.changes[0].action = CharacterOperationAction::RemoveExtension {
        namespace: "org.weave.character.alignment".to_owned(),
    };
    write_raw_json(
        &invalid.join("protected-extension.authoring-revision.json"),
        &placeholder,
        write,
    )?;

    let mut broken = created.clone();
    broken
        .drafts
        .get_mut("org.weave.character.lumen_reed")
        .expect("draft")
        .overlay
        .template = Some(CharacterTemplateRef {
        id: "org.weave.character.template.missing".to_owned(),
        version: "1.0.0".to_owned(),
        sha256: "0".repeat(64),
    });
    write_raw_json(
        &invalid.join("broken-template.authoring-workspace.json"),
        &broken,
        write,
    )?;

    let mut unknown = serde_json::to_value(revision)?;
    unknown
        .as_object_mut()
        .ok_or("revision is not an object")?
        .insert(
            "account_tier".to_owned(),
            serde_json::Value::String("unsupported".to_owned()),
        );
    write_raw_json(
        &invalid.join("unknown-field.authoring-revision.json"),
        &unknown,
        write,
    )
}

fn original_provenance(source_id: &str, claim: &str, attribution: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: source_id.to_owned(),
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
        claims: BTreeMap::from([(claim.to_owned(), vec![source_id.to_owned()])]),
    }
}

fn write_pair<T: serde::Serialize>(
    directory: &Path,
    stem: &str,
    value: &T,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_or_check(
        &directory.join(format!("{stem}.json")),
        weave_domain::to_pretty_json(value)?.as_bytes(),
        write,
    )?;
    write_or_check(
        &directory.join(format!("{stem}.ron")),
        weave_domain::to_pretty_ron(value)?.as_bytes(),
        write,
    )
}

fn write_raw_json(
    path: &Path,
    value: &impl serde::Serialize,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_or_check(path, weave_domain::to_pretty_json(value)?.as_bytes(), write)
}

fn write_schema(
    path: &Path,
    schema: String,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_or_check(path, schema.as_bytes(), write)
}

fn write_or_check(
    path: &Path,
    expected: &[u8],
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, expected)?;
        return Ok(());
    }
    let actual = fs::read(path)?;
    if actual != expected {
        return Err(format!("{} is stale", path.display()).into());
    }
    Ok(())
}
