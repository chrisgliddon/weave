use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
#[path = "support/expression.rs"]
mod expression_support;

use weave_character::{
    ALIGNMENT_CONFIG_FORMAT_VERSION, ALIGNMENT_PACK_FORMAT_VERSION, Agreeableness, AlignmentAxis,
    AlignmentCalibrationExpected, AlignmentCalibrationFixture, AlignmentConfig,
    AlignmentInputField, AlignmentPack, AlignmentPackProvider, AlignmentProposal, AlignmentReceipt,
    AlignmentReview, AlignmentReviewAction, AlignmentReviewDecision, AlignmentThreshold,
    AppearanceDescriptor, Attributed, AuthoredNote, BehavioralSignature, BehavioralSignatures,
    BirthDate, CHARACTER_COLLECTION_FORMAT_VERSION, CHARACTER_OPERATION_REQUEST_FORMAT_VERSION,
    CHARACTER_OVERLAY_FORMAT_VERSION, CHARACTER_PROFILE_FORMAT_VERSION,
    CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar, CharacterCanon, CharacterCollection,
    CharacterCorpusAction, CharacterDerivedViews, CharacterExtension, CharacterIdentity,
    CharacterOperation, CharacterOperationAction, CharacterOperationRequest, CharacterOverlay,
    CharacterProfile, CharacterReviewDecision, CharacterScope, CharacterSuggestion,
    CharacterTemplate, CharacterTemplateRef, Confidence, Conscientiousness, DateContext,
    DateContextCueKind, DateContextSensitivity, DateContextUncertainty, Emotionality,
    ExpressionApplicability, ExpressionConstraintEffect, ExpressionData, ExpressionMedium,
    ExpressionPackRef, ExpressionRecordOrigin, ExpressionTemplateAssignment, ExpressionTermKind,
    ExpressionVocabularyPool, ExpressionVoiceConstraint, ExtensionHeader, ExtensionWriteBack,
    Extraversion, Freshness, HexacoProfile, HexacoTrait, HonestyHumility, IdentityContextKind,
    IdentityContextNote, IdentityPresentation, InnerLifeCategory, LockState,
    NormalizedExpressionTerm, NormalizedPreference, OpaqueExtensionData, OpaqueInterpretation,
    Openness, PRESENTATION_ALLOCATION_REQUEST_FORMAT_VERSION, PRESENTATION_CATALOG_FORMAT_VERSION,
    PRESENTATION_LOCK_REVISION_FORMAT_VERSION, PreferencePolarity, PresentationAllocationMode,
    PresentationAllocationRequest, PresentationAssetKind, PresentationAssetReference,
    PresentationCatalog, PresentationCatalogEntry, PresentationCatalogSlot,
    PresentationCatalogValue, PresentationCatalogValueKind, PresentationLockRevision,
    PresentationLockTarget, PresentationPalette, PresentationProposal, PresentationReceipt,
    PresentationReview, PresentationReviewDecision, PronounSet, RELATIONSHIP_CONFIG_FORMAT_VERSION,
    RELATIONSHIP_KIND_PACK_FORMAT_VERSION, RELATIONSHIP_POLICY_FORMAT_VERSION,
    RELATIONSHIP_REVISION_FORMAT_VERSION, RelationshipConsent, RelationshipConsentRecord,
    RelationshipConsentState, RelationshipDate, RelationshipDiagnosticCode,
    RelationshipDirectionality, RelationshipEdge, RelationshipEdgeOrigin, RelationshipEdges,
    RelationshipEvidenceRule, RelationshipGraphPolicy, RelationshipGraphRevision,
    RelationshipKindDefinition, RelationshipKindFamily, RelationshipKindPack,
    RelationshipKindPackRef, RelationshipKinshipSemantics, RelationshipMetadataRequirement,
    RelationshipNote, RelationshipProposal, RelationshipProposalConfig, RelationshipProposalTarget,
    RelationshipReceipt, RelationshipReconciliationReport, RelationshipReview,
    RelationshipReviewDecision, RelationshipSafeguards, RelationshipValidityPeriod, ReviewState,
    RoleProjection, RoleProjections, TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION,
    TEMPORAL_CONTEXT_PACK_FORMAT_VERSION, TemporalAuthoringCue, TemporalAutoApprovePolicy,
    TemporalContextConfig, TemporalContextPack, TemporalContextProposal, TemporalContextProvider,
    TemporalContextReceipt, TemporalContextRecord, TemporalContextReview, TemporalDate,
    TemporalEvidenceClass, TemporalExtent, TemporalPlaceScope, TemporalRecordKind,
    TemporalReferencePeriod, TemporalResolution, TemporalReviewAction, TemporalReviewDecision,
    TemporalSensitivity, TemporalTimeZone, TemporalUncertainty, TraitMeasurement, ValueState,
    VersionedExtension, VoiceCategory, VoiceDirection, alignment_config_schema,
    alignment_pack_schema, alignment_proposal_schema, alignment_provider_content_fingerprint,
    alignment_receipt_schema, alignment_review_schema, apply_presentation_lock_revision,
    apply_presentation_review, apply_relationship_graph_revision, apply_reviewed_alignment,
    apply_reviewed_character_proposal, apply_reviewed_relationships,
    apply_reviewed_temporal_context, character_collection_schema, character_diagnostic_schema,
    character_domain_pack, character_module_manifest, character_operation_request_schema,
    character_overlay_schema, character_profile_schema, character_progress_schema,
    character_proposal_schema, character_review_schema, character_synthesis_schema,
    character_template_schema, collection_fingerprint, create_alignment_review,
    create_relationship_review, create_temporal_context_review,
    presentation_allocation_request_schema, presentation_catalog_ref, presentation_catalog_schema,
    presentation_lock_revision_schema, presentation_proposal_schema, presentation_receipt_schema,
    presentation_review_schema, propose_alignment, propose_character_operation,
    propose_presentation_allocations, propose_relationships, propose_temporal_context,
    recompute_derived, reconcile_relationship_graph, relationship_config_schema,
    relationship_dense_matrix_review_csv, relationship_edge_review_csv, relationship_kind_pack_ref,
    relationship_kind_pack_schema, relationship_policy_schema, relationship_proposal_schema,
    relationship_receipt_schema, relationship_reconciliation_schema, relationship_review_schema,
    relationship_revision_schema, resume_character_operation, review_character_proposal,
    review_presentation_proposal, synthesize_character, template_fingerprint,
    temporal_context_config_schema, temporal_context_pack_schema, temporal_context_proposal_schema,
    temporal_context_receipt_schema, temporal_context_review_schema,
    temporal_provider_content_fingerprint,
};
use weave_domain::{DomainValue, Provenance, ProvenanceKind, ProvenanceSource, to_pretty_json};

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
    let fixture = root.join("examples/domain-modules/weave-character");
    let schemas = root.join("schemas");

    let alignment = alignment_fixture(&complete_profile())?;
    let presentation = presentation_fixture(&alignment.receipt.output_profile)?;
    let profile = presentation
        .receipt
        .output_collection
        .characters
        .get("org.weave.character.ari_vale")
        .cloned()
        .ok_or("presentation fixture omitted Ari Vale")?;
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.glasswind_wayfinder".to_owned(),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = overlay(&template)?;
    let synthesis = synthesize_character(Some(&template), &overlay)?;
    let mut omitted = profile.clone();
    omitted.extensions.clear();
    omitted.suggestions.clear();
    recompute_derived(&mut omitted);
    let unknown_extension = synthesis.effective_profile.clone();
    let module_manifest = character_module_manifest()?;
    let domain_pack = character_domain_pack(
        &profile,
        "ari_vale",
        "1.0.0",
        "Ari Vale Synthetic Character",
    )?;
    let collection = presentation.receipt.output_collection.clone();
    let rename_request = rename_request(&collection)?;
    let progress = resume_character_operation(&collection, &rename_request, None, 1)?.progress;
    let rename_proposal = propose_character_operation(&collection, &rename_request)?;
    let rename_review = review_character_proposal(
        &rename_proposal,
        CharacterReviewDecision::Accepted,
        "org.weave.reviewer.fixture",
        "Approve the complete synthetic reference-safe rename.",
    )?;
    let renamed_collection =
        apply_reviewed_character_proposal(&collection, &rename_proposal, &rename_review)?;
    let temporal = temporal_fixture(&profile)?;
    let temporal_domain_pack = character_domain_pack(
        &temporal.receipt.output_profile,
        "ari_vale_temporal",
        "1.0.0",
        "Ari Vale Reviewed Temporal Context",
    )?;
    let relationships = relationship_fixture(&profile)?;

    write_pair(&fixture, "profile.character", &profile, write)?;
    write_pair(&fixture, "template.character", &template, write)?;
    write_pair(&fixture, "overlay.character", &overlay, write)?;
    write_pair(&fixture, "synthesis.character", &synthesis, write)?;
    write_pair(&fixture, "omitted-extensions.character", &omitted, write)?;
    write_pair(
        &fixture,
        "unknown-extension-preserved.character",
        &unknown_extension,
        write,
    )?;
    write_pair(&fixture, "module.weave-module", &module_manifest, write)?;
    write_pair(&fixture, "ari_vale.weave-domain", &domain_pack, write)?;

    let alignment_dir = fixture.join("alignment");
    write_pair(
        &alignment_dir,
        "input.character",
        &alignment.input_profile,
        write,
    )?;

    let presentation_dir = fixture.join("presentation");
    write_pair(
        &presentation_dir,
        "input.character-collection",
        &presentation.input_collection,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "glasswind.presentation-catalog",
        &presentation.catalog,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "allocation.presentation-request",
        &presentation.request,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "proposal.presentation-proposal",
        &presentation.proposal,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "decisions.presentation-review",
        &presentation.review.decisions,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "review.presentation-review",
        &presentation.review,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "receipt.presentation-receipt",
        &presentation.receipt,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "applied.character-collection",
        &presentation.receipt.output_collection,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "unlock-avatar.presentation-lock-revision",
        &presentation.lock_revision,
        write,
    )?;
    write_pair(
        &presentation_dir,
        "unlocked.character-collection",
        &presentation.unlocked_collection,
        write,
    )?;
    write_pair(&presentation_dir, "ari_vale.character", &profile, write)?;
    write_pair(
        &alignment_dir,
        "wayfinder_compass.alignment-pack",
        &alignment.pack,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "selection.alignment-config",
        &alignment.config,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "proposal.alignment-proposal",
        &alignment.proposal,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "decisions.alignment-review",
        &alignment.review.decisions,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "review.alignment-review",
        &alignment.review,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "receipt.alignment-receipt",
        &alignment.receipt,
        write,
    )?;
    write_pair(
        &alignment_dir,
        "approved.character",
        &alignment.receipt.output_profile,
        write,
    )?;

    let operations = fixture.join("operations");
    write_pair(
        &operations,
        "collection.character-collection",
        &collection,
        write,
    )?;
    write_pair(
        &operations,
        "rename.character-request",
        &rename_request,
        write,
    )?;
    write_pair(
        &operations,
        "rename.character-proposal",
        &rename_proposal,
        write,
    )?;
    write_pair(
        &operations,
        "rename.character-review",
        &rename_review,
        write,
    )?;
    write_pair(&operations, "rename.character-progress", &progress, write)?;
    write_pair(
        &operations,
        "renamed.character-collection",
        &renamed_collection,
        write,
    )?;

    let context = fixture.join("context");
    let runtime = context.join("runtime");
    write_pair(&runtime, "module.weave-module", &module_manifest, write)?;
    write_pair(
        &runtime,
        "ari_vale_temporal.weave-domain",
        &temporal_domain_pack,
        write,
    )?;
    write_pair(&context, "input.character", &temporal.input_profile, write)?;
    for pack in &temporal.packs {
        let stem = match pack.id.as_str() {
            "org.weave.context.apollo_11" => "apollo_11.temporal-pack",
            "org.weave.context.original_calendar" => "calendar.temporal-pack",
            "org.weave.context.world_projection" => "world.temporal-pack",
            _ => return Err("unexpected temporal fixture pack".into()),
        };
        write_pair(&context, stem, pack, write)?;
    }
    write_pair(&context, "ranking.temporal-config", &temporal.config, write)?;
    write_pair(
        &context,
        "proposal.temporal-proposal",
        &temporal.proposal,
        write,
    )?;
    write_pair(
        &context,
        "decisions.temporal-review",
        &temporal.review.decisions,
        write,
    )?;
    write_pair(&context, "review.temporal-review", &temporal.review, write)?;
    write_pair(
        &context,
        "receipt.temporal-receipt",
        &temporal.receipt,
        write,
    )?;
    write_pair(
        &context,
        "enriched.character",
        &temporal.receipt.output_profile,
        write,
    )?;

    let relationship_dir = fixture.join("relationships");
    write_pair(
        &relationship_dir,
        "blank.character-collection",
        &relationships.blank_collection,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "reference.relationship-kind-pack",
        &relationships.pack,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "project.relationship-policy",
        &relationships.policy,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "authored.relationship-revision",
        &relationships.revision,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "input.character-collection",
        &relationships.input_collection,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "scoring.relationship-config",
        &relationships.config,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "proposal.relationship-proposal",
        &relationships.proposal,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "decisions.relationship-review",
        &relationships.review.decisions,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "review.relationship-review",
        &relationships.review,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "receipt.relationship-receipt",
        &relationships.receipt,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "applied.character-collection",
        &relationships.receipt.output_collection,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "conflicted.character-collection",
        &relationships.conflicted_collection,
        write,
    )?;
    write_pair(
        &relationship_dir,
        "reconciliation.relationship-reconciliation",
        &relationships.reconciliation,
        write,
    )?;
    write_or_check(
        &relationship_dir.join("edges.review.csv"),
        relationships.edge_csv.as_bytes(),
        write,
    )?;
    write_or_check(
        &relationship_dir.join("matrix.review.csv"),
        relationships.matrix_csv.as_bytes(),
        write,
    )?;

    let mut stale_proposal = serde_json::to_value(&temporal.proposal)?;
    stale_proposal["profile_sha256"] = serde_json::Value::String("0".repeat(64));
    write_raw_json(
        &fixture.join("invalid/stale-temporal-proposal.json"),
        &stale_proposal,
        write,
    )?;
    let mut incomplete_review = serde_json::to_value(&temporal.review)?;
    if let Some(decisions) = incomplete_review["decisions"].as_object_mut() {
        let first = decisions.keys().next().cloned();
        if let Some(first) = first {
            decisions.remove(&first);
        }
    }
    write_raw_json(
        &fixture.join("invalid/incomplete-temporal-review.json"),
        &incomplete_review,
        write,
    )?;

    let mut stale_alignment = serde_json::to_value(&alignment.proposal)?;
    stale_alignment["profile_sha256"] = serde_json::Value::String("0".repeat(64));
    write_raw_json(
        &fixture.join("invalid/stale-alignment-proposal.json"),
        &stale_alignment,
        write,
    )?;
    let mut incomplete_alignment = serde_json::to_value(&alignment.review)?;
    if let Some(decisions) = incomplete_alignment["decisions"].as_object_mut() {
        let first = decisions.keys().next().cloned();
        if let Some(first) = first {
            decisions.remove(&first);
        }
    }
    write_raw_json(
        &fixture.join("invalid/incomplete-alignment-review.json"),
        &incomplete_alignment,
        write,
    )?;

    let mut stale_relationship = serde_json::to_value(&relationships.proposal)?;
    stale_relationship["input_sha256"] = serde_json::Value::String("0".repeat(64));
    write_raw_json(
        &fixture.join("invalid/stale-relationship-proposal.json"),
        &stale_relationship,
        write,
    )?;
    let mut incomplete_relationship = serde_json::to_value(&relationships.review)?;
    if let Some(decisions) = incomplete_relationship["decisions"].as_object_mut() {
        let first = decisions.keys().next().cloned();
        if let Some(first) = first {
            decisions.remove(&first);
        }
    }
    write_raw_json(
        &fixture.join("invalid/incomplete-relationship-review.json"),
        &incomplete_relationship,
        write,
    )?;

    let mut stale_presentation = serde_json::to_value(&presentation.proposal)?;
    stale_presentation["input_sha256"] = serde_json::Value::String("0".repeat(64));
    write_raw_json(
        &fixture.join("invalid/stale-presentation-proposal.json"),
        &stale_presentation,
        write,
    )?;
    let mut missing_asset_request = presentation.request.clone();
    missing_asset_request.available_asset_paths.clear();
    write_raw_json(
        &fixture.join("invalid/missing-presentation-asset.presentation-request.json"),
        &missing_asset_request,
        write,
    )?;
    let mut invalid_palette_catalog = presentation.catalog.clone();
    let PresentationCatalogValue::PaletteColor { palette_slot, .. } = &mut invalid_palette_catalog
        .entries
        .get_mut("amber_accent")
        .expect("checked palette entry")
        .value
    else {
        return Err("presentation palette fixture changed kind".into());
    };
    *palette_slot = "Accent Color".to_owned();
    write_raw_json(
        &fixture.join("invalid/invalid-palette-slot.presentation-catalog.json"),
        &invalid_palette_catalog,
        write,
    )?;
    let mut incompatible_decisions = presentation.review.decisions.clone();
    incompatible_decisions
        .get_mut("org.weave.character.ari_vale")
        .ok_or("presentation decision fixture omitted Ari Vale")?
        .insert(
            "avatar".to_owned(),
            PresentationReviewDecision::Override {
                value: PresentationCatalogValue::StyleTag {
                    tag: "incompatible_avatar_style".to_owned(),
                },
                lock: LockState::Unlocked,
                rationale: "Exercise the closed presentation value-kind boundary.".to_owned(),
            },
        );
    write_raw_json(
        &fixture.join("invalid/incompatible-presentation-override.json"),
        &incompatible_decisions,
        write,
    )?;

    let invalid = fixture.join("invalid");
    let mut unknown_profile_version = serde_json::to_value(&profile)?;
    unknown_profile_version["profile_format_version"] = serde_json::Value::from(2);
    write_json_value(
        &invalid.join("unknown-profile-version.character.json"),
        &unknown_profile_version,
        write,
    )?;

    let mut duplicate_alias = profile.clone();
    duplicate_alias
        .canon
        .identity
        .aliases
        .as_mut()
        .expect("checked aliases")
        .value = vec!["Ari".to_owned(), "Ari".to_owned()];
    write_raw_json(
        &invalid.join("duplicate-alias.character.json"),
        &duplicate_alias,
        write,
    )?;

    let mut conflicting_overlay = overlay.clone();
    let mut duplicate = conflicting_overlay.operations[0].clone();
    duplicate.id = "rename_again".to_owned();
    conflicting_overlay.operations.insert(1, duplicate);
    write_raw_json(
        &invalid.join("conflicting-overlay.character.json"),
        &conflicting_overlay,
        write,
    )?;

    let mut stale_overlay = overlay.clone();
    stale_overlay
        .template
        .as_mut()
        .expect("checked template reference")
        .sha256 = "0".repeat(64);
    write_raw_json(
        &invalid.join("stale-overlay.character.json"),
        &stale_overlay,
        write,
    )?;

    let mut invalid_reference = profile.clone();
    let CharacterExtension::Relationships(relationships) = invalid_reference
        .extensions
        .get_mut("org.weave.character.relationships")
        .expect("relationship fixture")
    else {
        return Err("relationship fixture changed kind".into());
    };
    let invalid_edge = relationships
        .value
        .edges
        .get_mut("mentor_sable")
        .expect("relationship edge");
    invalid_edge.source_character_id = "org.weave.character.sable_reed".to_owned();
    write_raw_json(
        &invalid.join("invalid-reference.character.json"),
        &invalid_reference,
        write,
    )?;

    let mut unknown_typed_extension = profile.clone();
    let CharacterExtension::AlignmentView(alignment) = unknown_typed_extension
        .extensions
        .get_mut("org.weave.character.alignment")
        .expect("alignment fixture")
    else {
        return Err("alignment fixture changed kind".into());
    };
    alignment.header.extension_version = 2;
    write_raw_json(
        &invalid.join("unknown-typed-extension-version.character.json"),
        &unknown_typed_extension,
        write,
    )?;

    let mut derived_in_canon = profile.clone();
    derived_in_canon
        .canon
        .personality
        .openness
        .creativity
        .as_mut()
        .expect("creativity fixture")
        .state = ValueState::Derived;
    write_raw_json(
        &invalid.join("derived-canonical-evidence.character.json"),
        &derived_in_canon,
        write,
    )?;

    let mut stale_review = rename_review.clone();
    stale_review.input_sha256 = "0".repeat(64);
    write_raw_json(
        &invalid.join("stale-character-review.json"),
        &stale_review,
        write,
    )?;

    let mut stale_progress = progress.clone();
    stale_progress.request_sha256 = "0".repeat(64);
    write_raw_json(
        &invalid.join("stale-character-progress.json"),
        &stale_progress,
        write,
    )?;

    let mut malformed_proposal = rename_proposal.clone();
    malformed_proposal.output_sha256 = "0".repeat(64);
    write_raw_json(
        &invalid.join("malformed-character-proposal.json"),
        &malformed_proposal,
        write,
    )?;

    for (name, contents) in [
        (
            "weave-character-profile-v1.schema.json",
            character_profile_schema()?,
        ),
        (
            "weave-character-template-v1.schema.json",
            character_template_schema()?,
        ),
        (
            "weave-character-overlay-v1.schema.json",
            character_overlay_schema()?,
        ),
        (
            "weave-character-synthesis-v1.schema.json",
            character_synthesis_schema()?,
        ),
        (
            "weave-character-diagnostic-v1.schema.json",
            character_diagnostic_schema()?,
        ),
        (
            "weave-character-collection-v1.schema.json",
            character_collection_schema()?,
        ),
        (
            "weave-character-operation-request-v1.schema.json",
            character_operation_request_schema()?,
        ),
        (
            "weave-character-proposal-v1.schema.json",
            character_proposal_schema()?,
        ),
        (
            "weave-character-review-v1.schema.json",
            character_review_schema()?,
        ),
        (
            "weave-character-progress-v1.schema.json",
            character_progress_schema()?,
        ),
        (
            "weave-character-temporal-pack-v1.schema.json",
            temporal_context_pack_schema()?,
        ),
        (
            "weave-character-temporal-config-v1.schema.json",
            temporal_context_config_schema()?,
        ),
        (
            "weave-character-temporal-proposal-v1.schema.json",
            temporal_context_proposal_schema()?,
        ),
        (
            "weave-character-temporal-review-v1.schema.json",
            temporal_context_review_schema()?,
        ),
        (
            "weave-character-temporal-receipt-v1.schema.json",
            temporal_context_receipt_schema()?,
        ),
        (
            "weave-character-alignment-pack-v1.schema.json",
            alignment_pack_schema()?,
        ),
        (
            "weave-character-alignment-config-v1.schema.json",
            alignment_config_schema()?,
        ),
        (
            "weave-character-alignment-proposal-v1.schema.json",
            alignment_proposal_schema()?,
        ),
        (
            "weave-character-alignment-review-v1.schema.json",
            alignment_review_schema()?,
        ),
        (
            "weave-character-alignment-receipt-v1.schema.json",
            alignment_receipt_schema()?,
        ),
        (
            "weave-character-presentation-catalog-v1.schema.json",
            presentation_catalog_schema()?,
        ),
        (
            "weave-character-presentation-request-v1.schema.json",
            presentation_allocation_request_schema()?,
        ),
        (
            "weave-character-presentation-proposal-v1.schema.json",
            presentation_proposal_schema()?,
        ),
        (
            "weave-character-presentation-review-v1.schema.json",
            presentation_review_schema()?,
        ),
        (
            "weave-character-presentation-receipt-v1.schema.json",
            presentation_receipt_schema()?,
        ),
        (
            "weave-character-presentation-lock-revision-v1.schema.json",
            presentation_lock_revision_schema()?,
        ),
        (
            "weave-character-relationship-kind-pack-v1.schema.json",
            relationship_kind_pack_schema()?,
        ),
        (
            "weave-character-relationship-policy-v1.schema.json",
            relationship_policy_schema()?,
        ),
        (
            "weave-character-relationship-config-v1.schema.json",
            relationship_config_schema()?,
        ),
        (
            "weave-character-relationship-proposal-v1.schema.json",
            relationship_proposal_schema()?,
        ),
        (
            "weave-character-relationship-review-v1.schema.json",
            relationship_review_schema()?,
        ),
        (
            "weave-character-relationship-receipt-v1.schema.json",
            relationship_receipt_schema()?,
        ),
        (
            "weave-character-relationship-revision-v1.schema.json",
            relationship_revision_schema()?,
        ),
        (
            "weave-character-relationship-reconciliation-v1.schema.json",
            relationship_reconciliation_schema()?,
        ),
    ] {
        write_or_check(&schemas.join(name), contents.as_bytes(), write)?;
    }
    Ok(())
}

const RELATIONSHIP_ARI: &str = "org.weave.character.ari_vale";
const RELATIONSHIP_SABLE: &str = "org.weave.character.sable_reed";
const RELATIONSHIP_TAVI: &str = "org.weave.character.tavi_quill";

struct RelationshipFixture {
    blank_collection: CharacterCollection,
    pack: RelationshipKindPack,
    policy: RelationshipGraphPolicy,
    revision: RelationshipGraphRevision,
    input_collection: CharacterCollection,
    config: RelationshipProposalConfig,
    proposal: RelationshipProposal,
    review: RelationshipReview,
    receipt: RelationshipReceipt,
    conflicted_collection: CharacterCollection,
    reconciliation: RelationshipReconciliationReport,
    edge_csv: String,
    matrix_csv: String,
}

fn relationship_fixture(
    base: &CharacterProfile,
) -> Result<RelationshipFixture, Box<dyn std::error::Error>> {
    let blank_collection = relationship_roster(base);
    let pack = relationship_kind_pack();
    let kind_pack = relationship_kind_pack_ref(&pack)?;
    let safeguards = RelationshipSafeguards {
        minimum_partnership_age_years: Some(18),
        forbid_close_kin_partnership: true,
        require_affirmed_partnership_consent: true,
        maximum_concurrent_partnerships: Some(1),
        allow_reviewed_exceptions: true,
    };
    let reference_date = RelationshipDate {
        year: 2035,
        month: 6,
        day: 15,
    };
    let policy = RelationshipGraphPolicy {
        policy_format_version: RELATIONSHIP_POLICY_FORMAT_VERSION,
        reference_date,
        safeguards: safeguards.clone(),
    };

    let mut parent = fixture_relationship_edge(
        "parent_sable_tavi",
        RELATIONSHIP_SABLE,
        RELATIONSHIP_TAVI,
        "org.weave.relationship.parent_of",
        RelationshipEdgeOrigin::Authored,
        ReviewState::NotRequired,
    );
    parent.inverse_edge_id = Some("child_tavi_sable".to_owned());
    let mut child = fixture_relationship_edge(
        "child_tavi_sable",
        RELATIONSHIP_TAVI,
        RELATIONSHIP_SABLE,
        "org.weave.relationship.child_of",
        RelationshipEdgeOrigin::Authored,
        ReviewState::NotRequired,
    );
    child.inverse_edge_id = Some("parent_sable_tavi".to_owned());
    let revision = RelationshipGraphRevision {
        revision_format_version: RELATIONSHIP_REVISION_FORMAT_VERSION,
        id: "org.weave.relationship.reference_authored_graph".to_owned(),
        expected_input_sha256: collection_fingerprint(&blank_collection)?,
        kind_pack: kind_pack.clone(),
        reference_date,
        safeguards: safeguards.clone(),
        additions: vec![
            fixture_relationship_edge(
                "friend_ari_tavi",
                RELATIONSHIP_ARI,
                RELATIONSHIP_TAVI,
                "org.weave.relationship.friend",
                RelationshipEdgeOrigin::Authored,
                ReviewState::NotRequired,
            ),
            fixture_relationship_edge(
                "mentor_ari_sable",
                RELATIONSHIP_ARI,
                RELATIONSHIP_SABLE,
                "org.weave.relationship.mentor",
                RelationshipEdgeOrigin::Imported,
                ReviewState::Accepted,
            ),
            parent,
            child,
        ],
        removals: Vec::new(),
        rationale: "Author a synthetic graph containing directed, symmetric, and inverse-paired kinds while keeping imported and authored layers explicit.".to_owned(),
        provenance: relationship_provenance(
            "weave_relationship_revision",
            "relationships",
            "Original synthetic authored and imported relationship graph fixture.",
        ),
    };
    let input_collection = apply_relationship_graph_revision(&blank_collection, &pack, &revision)?;

    let config_lineage = vec!["weave_relationship_config".to_owned()];
    let validity = Some(RelationshipValidityPeriod {
        start: Some(RelationshipDate {
            year: 2035,
            month: 1,
            day: 1,
        }),
        end: None,
    });
    let target_note = |id: &str, content: &str| {
        BTreeMap::from([(
            id.to_owned(),
            RelationshipNote {
                id: id.to_owned(),
                content: content.to_owned(),
                lineage: config_lineage.clone(),
            },
        )])
    };
    let affirmed_consent = |id: &str, source: &str, target: &str| RelationshipConsentRecord {
        id: id.to_owned(),
        source_character_id: source.to_owned(),
        target_character_id: target.to_owned(),
        kind_id: "org.weave.relationship.partner".to_owned(),
        consent: RelationshipConsent {
            state: RelationshipConsentState::Affirmed,
            reviewed_by: Some("org.weave.reviewer.fixture".to_owned()),
            rationale: Some(
                "Synthetic adults explicitly affirm this fictional partnership fixture.".to_owned(),
            ),
            lineage: config_lineage.clone(),
        },
    };
    let config = RelationshipProposalConfig {
        config_format_version: RELATIONSHIP_CONFIG_FORMAT_VERSION,
        id: "org.weave.relationship.reference_scoring".to_owned(),
        expected_input_sha256: collection_fingerprint(&input_collection)?,
        kind_pack,
        reference_date,
        seed: 2_035_061_500,
        roster: vec![
            RELATIONSHIP_ARI.to_owned(),
            RELATIONSHIP_SABLE.to_owned(),
            RELATIONSHIP_TAVI.to_owned(),
        ],
        targets: vec![
            RelationshipProposalTarget {
                id: "affinity".to_owned(),
                kind_id: "org.weave.relationship.shared_affinity".to_owned(),
                origin: RelationshipEdgeOrigin::ComputedAffinity,
                minimum_score_micros: 500_000,
                maximum_candidates: None,
                confidence: Confidence::Moderate,
                validity: validity.clone(),
                notes: target_note(
                    "affinity_prompt",
                    "This score is an inspectable authoring prompt, never objective interpersonal truth.",
                ),
                rationale: "Offer optional affinity prompts from only the four explicitly approved evidence rules.".to_owned(),
            },
            RelationshipProposalTarget {
                id: "mentorship".to_owned(),
                kind_id: "org.weave.relationship.mentor".to_owned(),
                origin: RelationshipEdgeOrigin::SuggestedNarrative,
                minimum_score_micros: 500_000,
                maximum_candidates: None,
                confidence: Confidence::Moderate,
                validity: validity.clone(),
                notes: target_note(
                    "narrative_prompt",
                    "Optional fictional mentorship direction for human review.",
                ),
                rationale: "Suggest a possible story direction without asserting canon or mutating canonical character evidence.".to_owned(),
            },
            RelationshipProposalTarget {
                id: "partnership".to_owned(),
                kind_id: "org.weave.relationship.partner".to_owned(),
                origin: RelationshipEdgeOrigin::SuggestedNarrative,
                minimum_score_micros: 500_000,
                maximum_candidates: None,
                confidence: Confidence::Low,
                validity,
                notes: target_note(
                    "partnership_prompt",
                    "Optional fictional partnership prompt subject to explicit project safeguards.",
                ),
                rationale: "Exercise age, kinship, concurrency, and affirmative-consent gates before any author decision.".to_owned(),
            },
        ],
        evidence_rules: vec![
            RelationshipEvidenceRule::TraitSimilarity {
                id: "openness_similarity".to_owned(),
                trait_id: HexacoTrait::Openness,
                weight_micros: 250_000,
            },
            RelationshipEvidenceRule::PreferenceOverlap {
                id: "communication_preference_overlap".to_owned(),
                category: "org.weave.preference.communication".to_owned(),
                weight_micros: 250_000,
            },
            RelationshipEvidenceRule::SharedContext {
                id: "wayfinder_context".to_owned(),
                context_ref: "org.weave.identity.wayfinder".to_owned(),
                weight_micros: 250_000,
            },
            RelationshipEvidenceRule::ExistingCanon {
                id: "existing_mentorship".to_owned(),
                relationship_kind_id: "org.weave.relationship.mentor".to_owned(),
                weight_micros: 250_000,
            },
        ],
        consent_records: vec![
            affirmed_consent(
                "ari_tavi_partnership_consent",
                RELATIONSHIP_ARI,
                RELATIONSHIP_TAVI,
            ),
            affirmed_consent(
                "sable_tavi_partnership_consent",
                RELATIONSHIP_SABLE,
                RELATIONSHIP_TAVI,
            ),
        ],
        safeguards: safeguards.clone(),
        provenance: relationship_provenance(
            "weave_relationship_config",
            "configuration",
            "Original deterministic relationship scoring and safeguard fixture.",
        ),
    };
    let proposal = propose_relationships(&input_collection, &pack, &config)?;
    let decisions = proposal
        .candidates
        .iter()
        .map(|(id, candidate)| {
            let pair = (
                candidate.source_character_id.as_str(),
                candidate.target_character_id.as_str(),
            );
            let decision = match (candidate.target_id.as_str(), pair) {
                ("affinity", (RELATIONSHIP_ARI, RELATIONSHIP_SABLE)) => {
                    RelationshipReviewDecision::Accept {
                        lock: LockState::Locked,
                        rationale: Some(
                            "Retain this transparent computed prompt as a reviewed optional edge."
                                .to_owned(),
                        ),
                    }
                }
                ("affinity", (RELATIONSHIP_ARI, RELATIONSHIP_TAVI)) => {
                    let mut edges = candidate.edges.clone();
                    add_relationship_review_note(
                        &mut edges,
                        "edited_context",
                        "The reviewer narrows this prompt to their shared route-planning scenes.",
                    );
                    RelationshipReviewDecision::Edit {
                        edges,
                        lock: LockState::Unlocked,
                        rationale: "Edit metadata while preserving the exact proposed topology and evidence trace.".to_owned(),
                    }
                }
                ("affinity", (RELATIONSHIP_SABLE, RELATIONSHIP_TAVI)) => {
                    let mut edges = candidate.edges.clone();
                    for edge in &mut edges {
                        edge.confidence = Confidence::High;
                    }
                    add_relationship_review_note(
                        &mut edges,
                        "override_context",
                        "The reviewer supplies project-specific confidence and context.",
                    );
                    RelationshipReviewDecision::Override {
                        edges,
                        replacements: Vec::new(),
                        lock: LockState::Locked,
                        rationale: "Override proposal metadata without claiming that the computed score is objective truth.".to_owned(),
                    }
                }
                ("mentorship", (RELATIONSHIP_TAVI, RELATIONSHIP_ARI)) => {
                    RelationshipReviewDecision::Accept {
                        lock: LockState::Unlocked,
                        rationale: Some(
                            "Accept one optional narrative suggestion after explicit human review."
                                .to_owned(),
                        ),
                    }
                }
                ("mentorship", _) => RelationshipReviewDecision::Reject {
                    rationale: "Do not add this alternate mentorship direction to the current draft."
                        .to_owned(),
                },
                ("partnership", (RELATIONSHIP_ARI, RELATIONSHIP_SABLE)) => {
                    RelationshipReviewDecision::Exception {
                        edges: candidate.edges.clone(),
                        exception_codes: vec![RelationshipDiagnosticCode::ConsentSafeguard],
                        replacements: Vec::new(),
                        lock: LockState::Locked,
                        rationale: "Exercise the explicit, retained author-exception path for a wholly synthetic fictional fixture.".to_owned(),
                    }
                }
                ("partnership", (RELATIONSHIP_ARI, RELATIONSHIP_TAVI)) => {
                    RelationshipReviewDecision::Withhold {
                        rationale: "Keep this complete proposal in the authoring receipt without publishing an edge.".to_owned(),
                    }
                }
                ("partnership", _) => RelationshipReviewDecision::Reject {
                    rationale: "The reviewed partnership prompt does not serve this synthetic draft."
                        .to_owned(),
                },
                _ => unreachable!("reference fixture covers every target and roster pair"),
            };
            (id.clone(), decision)
        })
        .collect::<BTreeMap<_, _>>();
    let review = create_relationship_review(
        &proposal,
        decisions,
        "org.weave.reviewer.fixture",
        "Review every deterministic candidate, preserve every rationale, and publish only explicit human decisions.",
    )?;
    let receipt = apply_reviewed_relationships(&input_collection, &proposal, &review)?;
    let edge_csv = relationship_edge_review_csv(
        &receipt.output_collection,
        &pack,
        reference_date,
        &safeguards,
        &Default::default(),
    )?;
    let matrix_csv = relationship_dense_matrix_review_csv(
        &receipt.output_collection,
        &pack,
        reference_date,
        &safeguards,
        &Default::default(),
    )?;

    let mut conflicted_collection = input_collection.clone();
    relationship_edges_mut(&mut conflicted_collection, RELATIONSHIP_TAVI)?
        .edges
        .remove("child_tavi_sable");
    let ari_edges = relationship_edges_mut(&mut conflicted_collection, RELATIONSHIP_ARI)?;
    ari_edges
        .edges
        .get_mut("mentor_ari_sable")
        .ok_or("relationship fixture omitted imported mentor edge")?
        .freshness = Freshness::Stale;
    let mut duplicate = ari_edges
        .edges
        .get("friend_ari_tavi")
        .cloned()
        .ok_or("relationship fixture omitted authored friend edge")?;
    duplicate.id = "friend_ari_tavi_duplicate".to_owned();
    ari_edges.edges.insert(duplicate.id.clone(), duplicate);
    let reconciliation =
        reconcile_relationship_graph(&conflicted_collection, &pack, reference_date, &safeguards)?;

    Ok(RelationshipFixture {
        blank_collection,
        pack,
        policy,
        revision,
        input_collection,
        config,
        proposal,
        review,
        receipt,
        conflicted_collection,
        reconciliation,
        edge_csv,
        matrix_csv,
    })
}

fn relationship_roster(base: &CharacterProfile) -> CharacterCollection {
    let characters = [
        (RELATIONSHIP_ARI, "Ari Vale"),
        (RELATIONSHIP_SABLE, "Sable Reed"),
        (RELATIONSHIP_TAVI, "Tavi Quill"),
    ]
    .into_iter()
    .map(|(id, display_name)| {
        let mut profile = base.clone();
        profile.id = id.to_owned();
        retarget_expression_owner(&mut profile, id);
        profile.canon.identity.display_name.value = display_name.to_owned();
        profile.canon.identity.aliases = None;
        let CharacterExtension::Relationships(relationships) = profile
            .extensions
            .get_mut("org.weave.character.relationships")
            .expect("reference profile includes a relationship graph")
        else {
            unreachable!("reference relationship extension changed kind")
        };
        relationships.value.edges.clear();
        (id.to_owned(), profile)
    })
    .collect();
    CharacterCollection {
        collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
        id: "org.weave.character.relationship_reference_roster".to_owned(),
        revision: 0,
        characters,
    }
}

fn relationship_kind_pack() -> RelationshipKindPack {
    let parent = "org.weave.relationship.parent_of";
    let child = "org.weave.relationship.child_of";
    RelationshipKindPack {
        pack_format_version: RELATIONSHIP_KIND_PACK_FORMAT_VERSION,
        id: "org.weave.relationship.reference".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Weave Reference Relationship Kinds".to_owned(),
        description: "Original, neutral relationship vocabulary for deterministic synthetic fixtures and offline tooling.".to_owned(),
        independently_authored: true,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        kinds: BTreeMap::from([
            (
                child.to_owned(),
                relationship_kind_definition(
                    child,
                    "Child of",
                    RelationshipKindFamily::Kinship,
                    Some(RelationshipKinshipSemantics::ChildOf),
                    RelationshipDirectionality::InversePaired {
                        inverse_kind_id: parent.to_owned(),
                    },
                    false,
                    false,
                    vec![RelationshipMetadataRequirement::Confidence],
                ),
            ),
            (
                "org.weave.relationship.friend".to_owned(),
                relationship_kind_definition(
                    "org.weave.relationship.friend",
                    "Friend",
                    RelationshipKindFamily::Friendship,
                    None,
                    RelationshipDirectionality::Symmetric,
                    false,
                    true,
                    Vec::new(),
                ),
            ),
            (
                "org.weave.relationship.mentor".to_owned(),
                relationship_kind_definition(
                    "org.weave.relationship.mentor",
                    "Mentor",
                    RelationshipKindFamily::Mentorship,
                    None,
                    RelationshipDirectionality::Directed,
                    false,
                    true,
                    vec![RelationshipMetadataRequirement::Notes],
                ),
            ),
            (
                parent.to_owned(),
                relationship_kind_definition(
                    parent,
                    "Parent of",
                    RelationshipKindFamily::Kinship,
                    Some(RelationshipKinshipSemantics::ParentOf),
                    RelationshipDirectionality::InversePaired {
                        inverse_kind_id: child.to_owned(),
                    },
                    false,
                    false,
                    vec![RelationshipMetadataRequirement::Confidence],
                ),
            ),
            (
                "org.weave.relationship.partner".to_owned(),
                relationship_kind_definition(
                    "org.weave.relationship.partner",
                    "Partner",
                    RelationshipKindFamily::Partnership,
                    None,
                    RelationshipDirectionality::Symmetric,
                    false,
                    false,
                    vec![RelationshipMetadataRequirement::Validity],
                ),
            ),
            (
                "org.weave.relationship.shared_affinity".to_owned(),
                relationship_kind_definition(
                    "org.weave.relationship.shared_affinity",
                    "Shared affinity",
                    RelationshipKindFamily::Affinity,
                    None,
                    RelationshipDirectionality::Symmetric,
                    false,
                    true,
                    vec![RelationshipMetadataRequirement::Evidence],
                ),
            ),
            (
                "org.weave.relationship.sibling".to_owned(),
                relationship_kind_definition(
                    "org.weave.relationship.sibling",
                    "Sibling",
                    RelationshipKindFamily::Kinship,
                    Some(RelationshipKinshipSemantics::SiblingOf),
                    RelationshipDirectionality::Symmetric,
                    false,
                    true,
                    Vec::new(),
                ),
            ),
        ]),
        provenance: relationship_provenance(
            "weave_relationship_reference_pack",
            "kinds",
            "Original neutral relationship vocabulary, semantics, safeguards, and limitations.",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn relationship_kind_definition(
    id: &str,
    label: &str,
    family: RelationshipKindFamily,
    kinship_semantics: Option<RelationshipKinshipSemantics>,
    directionality: RelationshipDirectionality,
    allows_self: bool,
    allows_multiple_concurrent: bool,
    required_metadata: Vec<RelationshipMetadataRequirement>,
) -> RelationshipKindDefinition {
    RelationshipKindDefinition {
        id: id.to_owned(),
        label: label.to_owned(),
        description: format!(
            "Original {label} relationship kind for fictional character graph authoring."
        ),
        family,
        kinship_semantics,
        directionality,
        allows_self,
        allows_multiple_concurrent,
        required_metadata,
        limitations: vec![
            "A relationship edge is explicit fictional authoring context, not objective interpersonal truth, diagnosis, protected-class inference, or causal prediction.".to_owned(),
        ],
    }
}

fn fixture_relationship_edge(
    id: &str,
    source: &str,
    target: &str,
    kind: &str,
    origin: RelationshipEdgeOrigin,
    review: ReviewState,
) -> RelationshipEdge {
    RelationshipEdge {
        id: id.to_owned(),
        source_character_id: source.to_owned(),
        target_character_id: target.to_owned(),
        kind: kind.to_owned(),
        confidence: Confidence::High,
        origin,
        review,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        validity: None,
        inverse_edge_id: None,
        notes: BTreeMap::from([(
            "fixture_context".to_owned(),
            RelationshipNote {
                id: "fixture_context".to_owned(),
                content: "Explicit synthetic relationship context for portable tests.".to_owned(),
                lineage: vec!["weave_relationship_revision".to_owned()],
            },
        )]),
        consent: None,
        safeguard_exceptions: BTreeMap::new(),
        affinity_score_micros: None,
        evidence: Vec::new(),
        lineage: vec!["weave_relationship_revision".to_owned()],
        rationale: Some("Authored only for the public synthetic relationship fixture.".to_owned()),
    }
}

fn add_relationship_review_note(edges: &mut [RelationshipEdge], id: &str, content: &str) {
    for edge in edges {
        edge.notes.insert(
            id.to_owned(),
            RelationshipNote {
                id: id.to_owned(),
                content: content.to_owned(),
                lineage: vec!["weave_relationship_config".to_owned()],
            },
        );
    }
}

fn relationship_edges_mut<'a>(
    collection: &'a mut CharacterCollection,
    character_id: &str,
) -> Result<&'a mut RelationshipEdges, Box<dyn std::error::Error>> {
    let extension = collection
        .characters
        .get_mut(character_id)
        .and_then(|profile| {
            profile
                .extensions
                .get_mut("org.weave.character.relationships")
        })
        .ok_or("relationship fixture omitted graph extension")?;
    let CharacterExtension::Relationships(record) = extension else {
        return Err("relationship fixture graph extension changed kind".into());
    };
    Ok(&mut record.value)
}

fn relationship_provenance(source_id: &str, claim: &str, attribution: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: source_id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "relationship-graph-v1".to_owned(),
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

struct PresentationFixture {
    input_collection: CharacterCollection,
    catalog: PresentationCatalog,
    request: PresentationAllocationRequest,
    proposal: PresentationProposal,
    review: PresentationReview,
    receipt: PresentationReceipt,
    lock_revision: PresentationLockRevision,
    unlocked_collection: CharacterCollection,
}

fn presentation_fixture(
    base: &CharacterProfile,
) -> Result<PresentationFixture, Box<dyn std::error::Error>> {
    let mut authored_profile = base.clone();
    let lineage = vec!["character_original".to_owned()];
    let CharacterExtension::IdentityPresentation(presentation) = authored_profile
        .extensions
        .get_mut("org.weave.character.identity_presentation")
        .ok_or("identity presentation fixture is absent")?
    else {
        return Err("identity presentation fixture changed kind".into());
    };
    presentation.value.identity_refs = vec!["org.weave.identity.wayfinder".to_owned()];
    presentation.value.presentation_refs = vec![
        "presentation/assets/ari-vale-avatar.svg".to_owned(),
        "presentation/assets/sable-reed-avatar.svg".to_owned(),
    ];
    presentation.value.pronouns = Some(authored(
        PronounSet {
            subject: "they".to_owned(),
            object: "them".to_owned(),
            possessive_determiner: "their".to_owned(),
            possessive_pronoun: "theirs".to_owned(),
            reflexive: "themself".to_owned(),
        },
        &lineage,
    ));
    presentation.value.context_notes = BTreeMap::from([
        (
            "glasswind_origin".to_owned(),
            IdentityContextNote {
                id: "glasswind_origin".to_owned(),
                kind: IdentityContextKind::Origin,
                content: authored(
                    "Raised among the synthetic Glasswind coast's public wayfinding houses."
                        .to_owned(),
                    &lineage,
                ),
            },
        ),
        (
            "current_context".to_owned(),
            IdentityContextNote {
                id: "current_context".to_owned(),
                kind: IdentityContextKind::Context,
                content: authored(
                    "Carries a folded route card for the next fictional crossing.".to_owned(),
                    &lineage,
                ),
            },
        ),
    ]);
    presentation.value.appearance = BTreeMap::from([(
        "travel_layers".to_owned(),
        AppearanceDescriptor {
            id: "travel_layers".to_owned(),
            category: "org.weave.appearance.clothing".to_owned(),
            content: authored(
                "Layered cedar-green travel cloth with a pale reflective hem.".to_owned(),
                &lineage,
            ),
        },
    )]);
    presentation.value.palette = Some(authored(
        PresentationPalette {
            colors: BTreeMap::from([
                ("accent".to_owned(), "#D6A24A".to_owned()),
                ("background".to_owned(), "#102825".to_owned()),
                ("foreground".to_owned(), "#D9F4E3".to_owned()),
            ]),
        },
        &lineage,
    ));
    presentation.value.style_tags = Some(authored(
        vec!["cedar_ink".to_owned(), "route_marks".to_owned()],
        &lineage,
    ));
    presentation.value.assets = BTreeMap::from([(
        "authored_avatar".to_owned(),
        authored(
            PresentationAssetReference {
                id: "authored_avatar".to_owned(),
                kind: PresentationAssetKind::Avatar,
                path: "presentation/assets/ari-vale-avatar.svg".to_owned(),
                media_type: "image/svg+xml".to_owned(),
                sha256: None,
                alt_text: "Geometric cedar and gold wayfinder avatar.".to_owned(),
            },
            &lineage,
        ),
    )]);

    let input_collection = character_collection(&authored_profile)?;
    let source_id = "weave_glasswind_presentation";
    let catalog = PresentationCatalog {
        catalog_format_version: PRESENTATION_CATALOG_FORMAT_VERSION,
        id: "org.weave.character.presentation.glasswind".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Glasswind Presentation Catalog".to_owned(),
        description: "Original synthetic appearance, color, style, and avatar choices for deterministic public fixture allocation.".to_owned(),
        independently_authored: true,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        slots: BTreeMap::from([
            (
                "accent_color".to_owned(),
                presentation_slot(
                    "accent_color",
                    "Accent color",
                    PresentationCatalogValueKind::PaletteColor,
                ),
            ),
            (
                "avatar".to_owned(),
                presentation_slot(
                    "avatar",
                    "Avatar",
                    PresentationCatalogValueKind::Asset,
                ),
            ),
            (
                "silhouette".to_owned(),
                presentation_slot(
                    "silhouette",
                    "Silhouette",
                    PresentationCatalogValueKind::Appearance,
                ),
            ),
            (
                "visual_tone".to_owned(),
                presentation_slot(
                    "visual_tone",
                    "Visual tone",
                    PresentationCatalogValueKind::StyleTag,
                ),
            ),
        ]),
        entries: [
            (
                "amber_accent",
                "accent_color",
                "Amber accent",
                PresentationCatalogValue::PaletteColor {
                    palette_slot: "accent".to_owned(),
                    color: "#D6A24A".to_owned(),
                },
            ),
            (
                "violet_accent",
                "accent_color",
                "Violet accent",
                PresentationCatalogValue::PaletteColor {
                    palette_slot: "accent".to_owned(),
                    color: "#7C5CFF".to_owned(),
                },
            ),
            (
                "ari_avatar",
                "avatar",
                "Ari avatar",
                PresentationCatalogValue::Asset {
                    asset: PresentationAssetReference {
                        id: "ari_avatar".to_owned(),
                        kind: PresentationAssetKind::Avatar,
                        path: "presentation/assets/ari-vale-avatar.svg".to_owned(),
                        media_type: "image/svg+xml".to_owned(),
                        sha256: None,
                        alt_text: "Geometric cedar and gold wayfinder avatar.".to_owned(),
                    },
                },
            ),
            (
                "sable_avatar",
                "avatar",
                "Sable avatar",
                PresentationCatalogValue::Asset {
                    asset: PresentationAssetReference {
                        id: "sable_avatar".to_owned(),
                        kind: PresentationAssetKind::Avatar,
                        path: "presentation/assets/sable-reed-avatar.svg".to_owned(),
                        media_type: "image/svg+xml".to_owned(),
                        sha256: None,
                        alt_text: "Geometric blue and silver routekeeper avatar.".to_owned(),
                    },
                },
            ),
            (
                "long_coat",
                "silhouette",
                "Long coat",
                PresentationCatalogValue::Appearance {
                    category: "org.weave.appearance.silhouette".to_owned(),
                    descriptor: "Long layered travel coat with a narrow shoulder line.".to_owned(),
                },
            ),
            (
                "short_cape",
                "silhouette",
                "Short cape",
                PresentationCatalogValue::Appearance {
                    category: "org.weave.appearance.silhouette".to_owned(),
                    descriptor: "Short route cape over a compact travel silhouette.".to_owned(),
                },
            ),
            (
                "cedar_ink",
                "visual_tone",
                "Cedar ink",
                PresentationCatalogValue::StyleTag {
                    tag: "cedar_ink".to_owned(),
                },
            ),
            (
                "river_glass",
                "visual_tone",
                "River glass",
                PresentationCatalogValue::StyleTag {
                    tag: "river_glass".to_owned(),
                },
            ),
        ]
        .into_iter()
        .map(|(id, slot_id, label, value)| {
            (
                id.to_owned(),
                PresentationCatalogEntry {
                    id: id.to_owned(),
                    slot_id: slot_id.to_owned(),
                    label: label.to_owned(),
                    value,
                    eligible_character_ids: Vec::new(),
                    eligible_id_prefixes: vec!["org.weave.character".to_owned()],
                    excluded_character_ids: Vec::new(),
                    capacity: Some(1),
                },
            )
        })
        .collect(),
        provenance: Provenance {
            sources: vec![ProvenanceSource {
                id: source_id.to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "glasswind-presentation-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE"
                    .to_owned(),
                attribution: "Original synthetic Glasswind presentation catalog and vector assets."
                    .to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                ("assets".to_owned(), vec![source_id.to_owned()]),
                ("entries".to_owned(), vec![source_id.to_owned()]),
                ("slots".to_owned(), vec![source_id.to_owned()]),
            ]),
        },
    };
    let request = PresentationAllocationRequest {
        request_format_version: PRESENTATION_ALLOCATION_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.presentation.glasswind_allocation".to_owned(),
        expected_input_sha256: collection_fingerprint(&input_collection)?,
        catalog: presentation_catalog_ref(&catalog)?,
        seed: 20_260_822,
        character_ids: input_collection.characters.keys().cloned().collect(),
        slot_ids: vec![
            "accent_color".to_owned(),
            "avatar".to_owned(),
            "silhouette".to_owned(),
            "visual_tone".to_owned(),
        ],
        mode: PresentationAllocationMode::FillMissing,
        available_asset_paths: vec![
            "presentation/assets/ari-vale-avatar.svg".to_owned(),
            "presentation/assets/sable-reed-avatar.svg".to_owned(),
        ],
    };
    let proposal = propose_presentation_allocations(&input_collection, &catalog, &request)?;
    let decisions = proposal
        .allocations
        .iter()
        .map(|(character_id, slots)| {
            (
                character_id.clone(),
                slots
                    .iter()
                    .filter(|(_, allocation)| {
                        allocation.disposition
                            == weave_character::PresentationAllocationDisposition::Proposed
                    })
                    .map(|(slot_id, _)| {
                        let decision = if character_id == "org.weave.character.sable_reed"
                            && slot_id == "accent_color"
                        {
                            PresentationReviewDecision::Override {
                                value: PresentationCatalogValue::PaletteColor {
                                    palette_slot: "accent".to_owned(),
                                    color: "#7C5CFF".to_owned(),
                                },
                                lock: LockState::Locked,
                                rationale: "Use the explicitly authored violet accent while preserving the balanced proposal in the receipt.".to_owned(),
                            }
                        } else {
                            PresentationReviewDecision::Accept {
                                lock: if character_id == "org.weave.character.ari_vale"
                                    && slot_id == "avatar"
                                {
                                    LockState::Locked
                                } else {
                                    LockState::Unlocked
                                },
                                rationale: None,
                            }
                        };
                        (slot_id.clone(), decision)
                    })
                    .collect(),
            )
        })
        .collect();
    let review = review_presentation_proposal(
        &proposal,
        "org.weave.reviewer.presentation_fixture",
        "Review every synthetic presentation allocation; retain catalog coordinates, balance traces, locks, and explicit override rationale.",
        decisions,
    )?;
    let receipt = apply_presentation_review(&input_collection, &proposal, &review)?;
    let lock_revision = PresentationLockRevision {
        revision_format_version: PRESENTATION_LOCK_REVISION_FORMAT_VERSION,
        id: "org.weave.character.presentation.unlock_ari_avatar".to_owned(),
        expected_input_sha256: collection_fingerprint(&receipt.output_collection)?,
        targets: vec![PresentationLockTarget {
            character_id: "org.weave.character.ari_vale".to_owned(),
            slot_id: "avatar".to_owned(),
        }],
        lock: LockState::Unlocked,
        rationale: "Unlock the reviewed synthetic avatar assignment for a later rebalance."
            .to_owned(),
        provenance: catalog.provenance.clone(),
    };
    let unlocked_collection =
        apply_presentation_lock_revision(&receipt.output_collection, &lock_revision)?;
    Ok(PresentationFixture {
        input_collection,
        catalog,
        request,
        proposal,
        review,
        receipt,
        lock_revision,
        unlocked_collection,
    })
}

fn presentation_slot(
    id: &str,
    label: &str,
    value_kind: PresentationCatalogValueKind,
) -> PresentationCatalogSlot {
    PresentationCatalogSlot {
        id: id.to_owned(),
        label: label.to_owned(),
        description: format!("Original synthetic {label} presentation slot."),
        value_kind,
    }
}

struct AlignmentFixture {
    input_profile: CharacterProfile,
    pack: AlignmentPack,
    config: AlignmentConfig,
    proposal: AlignmentProposal,
    review: AlignmentReview,
    receipt: AlignmentReceipt,
}

fn alignment_fixture(
    base: &CharacterProfile,
) -> Result<AlignmentFixture, Box<dyn std::error::Error>> {
    let mut input_profile = base.clone();
    input_profile
        .extensions
        .remove("org.weave.character.alignment");
    let pack = wayfinder_compass_alignment_pack()?;
    let config = AlignmentConfig {
        config_format_version: ALIGNMENT_CONFIG_FORMAT_VERSION,
        id: "org.weave.alignment.wayfinder_reference".to_owned(),
        selected_axes: ["horizon", "reciprocity", "signal", "structure", "tempo"]
            .map(str::to_owned)
            .to_vec(),
        minimum_coverage_micros: 1_000_000,
        override_locked_view: false,
        override_rationale: None,
    };
    let proposal = propose_alignment(&input_profile, &pack, &config, 20_260_822)?;
    let decisions = proposal
        .values
        .keys()
        .map(|axis_id| {
            let action = match axis_id.as_str() {
                "horizon" => AlignmentReviewAction::Accept {
                    rationale: Some(
                        "Accept the proposed fictional shorthand after inspecting its complete fixed-point explanation and non-diagnostic limitations.".to_owned(),
                    ),
                },
                "reciprocity" => AlignmentReviewAction::Edit {
                    label_id: "mutual".to_owned(),
                    rationale: "Select a different declared narrative label because the authored scene context emphasizes exchange rather than stewardship; canonical evidence remains unchanged.".to_owned(),
                },
                "signal" => AlignmentReviewAction::Reject {
                    rationale: "Do not expose this optional axis to narrative logic for the reference character.".to_owned(),
                },
                "structure" => AlignmentReviewAction::Override {
                    label_id: "adapting".to_owned(),
                    rationale: "Deliberately override the computed shorthand for this project while retaining the exact proposal, score, and reviewer rationale in the authoring receipt.".to_owned(),
                },
                "tempo" => AlignmentReviewAction::Withhold {
                    rationale: "Keep this complete proposal in the authoring review without publishing it to runtime narrative logic.".to_owned(),
                },
                _ => unreachable!("reference config selects only declared fixture axes"),
            };
            (
                axis_id.clone(),
                AlignmentReviewDecision {
                    axis_id: axis_id.clone(),
                    action,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let review = create_alignment_review(
        &proposal,
        &pack,
        "org.weave.reviewer.fixture",
        "Review every original Wayfinder Compass axis independently; publish only approved fictional shorthand and never treat a label as diagnosis, moral rank, canonical evidence, or runtime authority.",
        decisions,
    )?;
    let receipt = apply_reviewed_alignment(&input_profile, &pack, &proposal, &review)?;
    Ok(AlignmentFixture {
        input_profile,
        pack,
        config,
        proposal,
        review,
        receipt,
    })
}

fn wayfinder_compass_alignment_pack() -> Result<AlignmentPack, Box<dyn std::error::Error>> {
    let inputs: BTreeMap<String, AlignmentInputField> = [
        (
            "agreeableness",
            HexacoTrait::Agreeableness,
            "canon.personality.agreeableness.factor",
            "Agreeableness",
        ),
        (
            "conscientiousness",
            HexacoTrait::Conscientiousness,
            "canon.personality.conscientiousness.factor",
            "Conscientiousness",
        ),
        (
            "diligence",
            HexacoTrait::Diligence,
            "canon.personality.conscientiousness.diligence",
            "Diligence",
        ),
        (
            "emotionality",
            HexacoTrait::Emotionality,
            "canon.personality.emotionality.factor",
            "Emotionality",
        ),
        (
            "extraversion",
            HexacoTrait::Extraversion,
            "canon.personality.extraversion.factor",
            "Extraversion",
        ),
        (
            "fairness",
            HexacoTrait::Fairness,
            "canon.personality.honesty_humility.fairness",
            "Fairness",
        ),
        (
            "honesty_humility",
            HexacoTrait::HonestyHumility,
            "canon.personality.honesty_humility.factor",
            "Honesty-Humility",
        ),
        (
            "inquisitiveness",
            HexacoTrait::Inquisitiveness,
            "canon.personality.openness.inquisitiveness",
            "Inquisitiveness",
        ),
        (
            "openness",
            HexacoTrait::Openness,
            "canon.personality.openness.factor",
            "Openness",
        ),
        (
            "organization",
            HexacoTrait::Organization,
            "canon.personality.conscientiousness.organization",
            "Organization",
        ),
    ]
    .map(|(id, trait_id, profile_path, label)| {
        (
            id.to_owned(),
            AlignmentInputField {
                id: id.to_owned(),
                trait_id,
                profile_path: profile_path.to_owned(),
                label: label.to_owned(),
                description: format!(
                    "Canonical {label} evidence used only as one transparent input to optional fictional narrative shorthand."
                ),
            },
        )
    })
    .into();

    let axes = BTreeMap::from([
        (
            "horizon".to_owned(),
            alignment_axis(
                "horizon",
                "Horizon",
                "How a fictional character balances continuity with unfamiliar possibilities in the current story.",
                [("inquisitiveness", 400), ("openness", 600)],
                [
                    (
                        "anchoring",
                        "Anchoring",
                        "Leans toward continuity in the current fictional situation.",
                    ),
                    (
                        "bridging",
                        "Bridging",
                        "Moves between continuity and possibility in the current fictional situation.",
                    ),
                    (
                        "seeking",
                        "Seeking",
                        "Leans toward unfamiliar possibilities in the current fictional situation.",
                    ),
                ],
            ),
        ),
        (
            "reciprocity".to_owned(),
            alignment_axis(
                "reciprocity",
                "Reciprocity",
                "How a fictional character frames exchange, mutual obligation, and care in the current story.",
                [
                    ("agreeableness", 300),
                    ("fairness", 350),
                    ("honesty_humility", 350),
                ],
                [
                    (
                        "guarded",
                        "Guarded",
                        "Keeps exchanges bounded in the current fictional situation.",
                    ),
                    (
                        "mutual",
                        "Mutual",
                        "Frames exchange as reciprocal in the current fictional situation.",
                    ),
                    (
                        "stewarding",
                        "Stewarding",
                        "Takes responsibility for sustaining exchange in the current fictional situation.",
                    ),
                ],
            ),
        ),
        (
            "signal".to_owned(),
            alignment_axis(
                "signal",
                "Signal",
                "How visibly a fictional character tends to signal their internal response in the current story.",
                [("emotionality", 500), ("extraversion", 500)],
                [
                    (
                        "inward",
                        "Inward",
                        "Keeps more response internal in the current fictional situation.",
                    ),
                    (
                        "modulated",
                        "Modulated",
                        "Varies how much response becomes visible in the current fictional situation.",
                    ),
                    (
                        "outward",
                        "Outward",
                        "Makes more response visible in the current fictional situation.",
                    ),
                ],
            ),
        ),
        (
            "structure".to_owned(),
            alignment_axis(
                "structure",
                "Structure",
                "How a fictional character balances improvisation and prior structure in the current story.",
                [("conscientiousness", 600), ("organization", 400)],
                [
                    (
                        "improvising",
                        "Improvising",
                        "Relies more on in-the-moment structure in the current fictional situation.",
                    ),
                    (
                        "adapting",
                        "Adapting",
                        "Moves between prior structure and improvisation in the current fictional situation.",
                    ),
                    (
                        "planning",
                        "Planning",
                        "Relies more on prior structure in the current fictional situation.",
                    ),
                ],
            ),
        ),
        (
            "tempo".to_owned(),
            alignment_axis(
                "tempo",
                "Tempo",
                "How a fictional character balances deliberation and momentum in the current story.",
                [("diligence", 500), ("extraversion", 500)],
                [
                    (
                        "measured",
                        "Measured",
                        "Favors deliberation in the current fictional situation.",
                    ),
                    (
                        "alternating",
                        "Alternating",
                        "Alternates between deliberation and momentum in the current fictional situation.",
                    ),
                    (
                        "quickening",
                        "Quickening",
                        "Favors momentum in the current fictional situation.",
                    ),
                ],
            ),
        ),
    ]);
    let calibration = |id: &str, value: u32, score: i32, labels: [&str; 5]| {
        (
            id.to_owned(),
            AlignmentCalibrationFixture {
                id: id.to_owned(),
                description: format!(
                    "Every synthetic input is {value} millionths, proving the exact shared signed fixed-point boundary."
                ),
                inputs_micros: inputs.keys().map(|key| (key.clone(), value)).collect(),
                expected: ["horizon", "reciprocity", "signal", "structure", "tempo"]
                    .into_iter()
                    .zip(labels)
                    .map(|(axis, label)| {
                        (
                            axis.to_owned(),
                            AlignmentCalibrationExpected {
                                score_micros: score,
                                label_id: label.to_owned(),
                            },
                        )
                    })
                    .collect(),
            },
        )
    };
    let calibrations = BTreeMap::from([
        calibration(
            "lower_boundary",
            100_000,
            -800_000,
            ["anchoring", "guarded", "inward", "improvising", "measured"],
        ),
        calibration(
            "middle_boundary",
            500_000,
            0,
            ["bridging", "mutual", "modulated", "adapting", "alternating"],
        ),
        calibration(
            "upper_boundary",
            900_000,
            800_000,
            ["seeking", "stewarding", "outward", "planning", "quickening"],
        ),
    ]);
    let source_id = "weave_wayfinder_compass";
    let mut pack = AlignmentPack {
        pack_format_version: ALIGNMENT_PACK_FORMAT_VERSION,
        id: "org.weave.alignment.wayfinder_compass".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Wayfinder Compass".to_owned(),
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        methodology: "For each selected axis, normalize declared canonical HEXACO scores to integer millionths, use documented band anchors when only a band exists, center present values around one half, multiply by non-zero signed thousandth weights, divide the signed sum by covered absolute weight with deterministic half-away-from-zero rounding, measure missing-input coverage separately, and select the first inclusive declared threshold. The seed changes only the trace fingerprint because v1 scoring has no random branch.".to_owned(),
        limitations: "Wayfinder Compass is original fictional storytelling shorthand. Its labels are contextual prompts, not clinical or psychometric diagnoses, moral rankings, protected-class inferences, causal predictions, canonical personality evidence, or runtime authority. Reviewers may reject, edit, withhold, or override every proposal.".to_owned(),
        provider: AlignmentPackProvider::Standalone,
        inputs,
        axes,
        calibrations,
        provenance: Provenance {
            sources: vec![ProvenanceSource {
                id: source_id.to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "wayfinder-compass-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE"
                    .to_owned(),
                attribution: "Original Wayfinder Compass axes, neutral labels, fixed-point methodology, explanations, limitations, and synthetic calibration fixtures.".to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: ["axes", "calibrations", "inputs", "methodology"]
                .map(|claim| (claim.to_owned(), vec![source_id.to_owned()]))
                .into(),
        },
    };
    let content_sha256 = alignment_provider_content_fingerprint(&pack)?;
    pack.provider = AlignmentPackProvider::DomainModule {
        module_id: "org.weave.alignment.wayfinder_compass".to_owned(),
        module_version: "1.0.0".to_owned(),
        pack_id: "reference".to_owned(),
        pack_version: "1.0.0".to_owned(),
        content_sha256,
    };
    Ok(pack)
}

fn alignment_axis<const N: usize>(
    id: &str,
    label: &str,
    description: &str,
    inputs: [(&str, i16); N],
    labels: [(&str, &str, &str); 3],
) -> AlignmentAxis {
    let bounds = [-250_000, 250_000, 1_000_000];
    AlignmentAxis {
        id: id.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        inputs: inputs
            .map(|(input, weight)| (input.to_owned(), weight))
            .into(),
        thresholds: labels
            .into_iter()
            .zip(bounds)
            .map(|((id, label, description), upper_bound_micros)| AlignmentThreshold {
                id: id.to_owned(),
                label: label.to_owned(),
                upper_bound_micros,
                description: description.to_owned(),
            })
            .collect(),
        limitations: "This original axis is optional fictional shorthand, not a diagnosis, moral classification, causal claim, or prediction of actual behavior.".to_owned(),
    }
}

struct TemporalFixture {
    input_profile: CharacterProfile,
    packs: Vec<TemporalContextPack>,
    config: TemporalContextConfig,
    proposal: TemporalContextProposal,
    review: TemporalContextReview,
    receipt: TemporalContextReceipt,
}

fn temporal_fixture(
    base: &CharacterProfile,
) -> Result<TemporalFixture, Box<dyn std::error::Error>> {
    let mut input_profile = base.clone();
    input_profile
        .canon
        .birth_date
        .as_mut()
        .ok_or("complete fixture requires a birth date")?
        .value = BirthDate::Full {
        calendar: Calendar::ProlepticGregorian,
        year: 1969,
        month: 7,
        day: 20,
    };
    input_profile
        .extensions
        .remove("org.weave.character.date_context");

    let packs = vec![
        apollo_11_context_pack(),
        original_calendar_context_pack(),
        world_projection_context_pack(),
    ];
    let config = TemporalContextConfig {
        config_format_version: TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION,
        id: "org.weave.context.reference_ranking".to_owned(),
        minimum_relevance_micros: 350_000,
        minimum_trait_coverage_micros: 100_000,
        maximum_candidates: 16,
        allowed_record_kinds: vec![
            TemporalRecordKind::CalendricalFact,
            TemporalRecordKind::SeasonalFact,
            TemporalRecordKind::EnvironmentalFact,
            TemporalRecordKind::CelestialFact,
            TemporalRecordKind::Commemoration,
            TemporalRecordKind::HistoricalEvent,
        ],
        allowed_cue_kinds: vec![
            DateContextCueKind::Affinity,
            DateContextCueKind::Tension,
            DateContextCueKind::Value,
            DateContextCueKind::Memory,
            DateContextCueKind::Voice,
        ],
        allowed_sensitivities: vec![
            DateContextSensitivity::Low,
            DateContextSensitivity::Moderate,
        ],
        required_tags: Vec::new(),
        place_scope_ids: vec!["org.weave.place.synthetic_coast".to_owned()],
        time_zone: TemporalTimeZone::Iana {
            id: "America/Los_Angeles".to_owned(),
        },
        allow_global_without_place: true,
        auto_approve: TemporalAutoApprovePolicy::Disabled,
        override_locked_context: false,
        override_rationale: None,
    };
    let proposal = propose_temporal_context(&input_profile, &packs, &config, 19_690_720)?;
    let decisions = proposal
        .candidates
        .iter()
        .map(|candidate| {
            let action = match candidate.record_id.as_str() {
                "apollo_11_lunar_landing" => TemporalReviewAction::Accept {
                    rationale: Some(
                        "Accept the original fictional value prompt after verifying that the historical date is context only and not causal personality evidence.".to_owned(),
                    ),
                },
                "calendar_midsummer_period" => TemporalReviewAction::Edit {
                    content: "Recall a long-light gathering where the character chose to listen before speaking.".to_owned(),
                    rationale: "Make the original seasonal prompt specific to this fictional character while retaining its non-causal status.".to_owned(),
                },
                "world_coastal_fog_cycle" => TemporalReviewAction::Override {
                    content: "Treat the fictional coast's returning fog as a remembered invitation to slow down and notice small changes.".to_owned(),
                    rationale: "Override the generic World-compatible cue with a project-specific fictional memory; the environmental record remains separate.".to_owned(),
                },
                _ => TemporalReviewAction::Reject {
                    rationale: "The reviewer chose not to carry this otherwise valid fictional cue into the accepted context view.".to_owned(),
                },
            };
            (
                candidate.id.clone(),
                TemporalReviewDecision {
                    candidate_id: candidate.id.clone(),
                    action,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let review = create_temporal_context_review(
        &proposal,
        "org.weave.reviewer.fixture",
        "Review every ranked temporal cue, preserve fact and fictional-cue lineage separately, and keep all accepted material outside canon.",
        decisions,
    )?;
    let receipt = apply_reviewed_temporal_context(&input_profile, &packs, &proposal, &review)?;
    Ok(TemporalFixture {
        input_profile,
        packs,
        config,
        proposal,
        review,
        receipt,
    })
}

fn apollo_11_context_pack() -> TemporalContextPack {
    let public_source = ProvenanceSource {
        id: "apollo_11_wikidata".to_owned(),
        kind: ProvenanceKind::PublicSource,
        url: "https://www.wikidata.org/wiki/Special:EntityData/Q43653.json?revision=2526069885&flavor=simple".to_owned(),
        revision: "2526069885 (2026-08-02T06:29:38Z)".to_owned(),
        sha256: Some(
            "52c78c8a7c1320e970a6aa1c2737c9e0a89e786be957f380f389d02ab84c006b"
                .to_owned(),
        ),
        license: "CC0-1.0".to_owned(),
        license_url: "https://www.wikidata.org/wiki/Wikidata:Licensing".to_owned(),
        attribution: "Wikidata contributors, Apollo 11 (Q43653), revision 2526069885. Attribution retained although CC0 does not require it.".to_owned(),
        modified: false,
    };
    let cue_source = original_context_source(
        "weave_historical_cues",
        "Original fictional historical-context prompts and declared HEXACO vectors.",
    );
    TemporalContextPack {
        pack_format_version: TEMPORAL_CONTEXT_PACK_FORMAT_VERSION,
        id: "org.weave.context.apollo_11".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Apollo 11 structured-date context".to_owned(),
        license: "CC0-1.0 AND MIT".to_owned(),
        license_url: "https://spdx.org/licenses/".to_owned(),
        provider: TemporalContextProvider::Standalone,
        records: BTreeMap::from([(
            "apollo_11_lunar_landing".to_owned(),
            TemporalContextRecord {
                id: "apollo_11_lunar_landing".to_owned(),
                kind: TemporalRecordKind::HistoricalEvent,
                evidence_class: TemporalEvidenceClass::CalendricalFact,
                extent: TemporalExtent::Date {
                    date: temporal_date(1969, 7, 20),
                },
                place_scope: TemporalPlaceScope::Global,
                time_zone: TemporalTimeZone::Utc,
                reference_period: exact_reference_period(
                    "Apollo 11 structured event date",
                    1969,
                    7,
                    20,
                ),
                fact: DomainValue::Object(BTreeMap::from([
                    (
                        "date".to_owned(),
                        DomainValue::String("1969-07-20".to_owned()),
                    ),
                    (
                        "entity".to_owned(),
                        DomainValue::Symbol("wikidata_q43653".to_owned()),
                    ),
                    (
                        "event".to_owned(),
                        DomainValue::String("Apollo 11 lunar landing".to_owned()),
                    ),
                    (
                        "precision".to_owned(),
                        DomainValue::Symbol("day".to_owned()),
                    ),
                    (
                        "revision".to_owned(),
                        DomainValue::String("2526069885".to_owned()),
                    ),
                ])),
                tags: vec!["historical".to_owned(), "spaceflight".to_owned()],
                cues: BTreeMap::from([(
                    "shared_horizon".to_owned(),
                    TemporalAuthoringCue {
                        id: "shared_horizon".to_owned(),
                        kind: DateContextCueKind::Value,
                        content: "Consider whether this fictional character values difficult work whose meaning becomes visible only when many people share one horizon.".to_owned(),
                        trait_vector: BTreeMap::from([
                            (HexacoTrait::Diligence, 350),
                            (HexacoTrait::Inquisitiveness, 250),
                            (HexacoTrait::Openness, 400),
                        ]),
                        tags: vec!["collective_effort".to_owned()],
                        limitations: "This original fictional prompt is merely discoverable through a date match; Apollo 11 neither causes nor predicts personality and the cue makes no claim about a real person.".to_owned(),
                        source_ids: vec!["weave_historical_cues".to_owned()],
                    },
                )]),
                source_ids: vec!["apollo_11_wikidata".to_owned()],
                uncertainty: TemporalUncertainty {
                    level: DateContextUncertainty::Exact,
                    reason: "The pinned structured entity records the event date at day precision."
                        .to_owned(),
                },
                sensitivity: TemporalSensitivity {
                    level: DateContextSensitivity::Moderate,
                    topics: vec!["history".to_owned(), "spaceflight".to_owned()],
                    requires_explicit_review: true,
                    note: "A real historical event is retained only as sourced context for an independently original fictional prompt.".to_owned(),
                },
            },
        )]),
        provenance: Provenance {
            sources: vec![public_source, cue_source],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                (
                    "records.apollo_11_lunar_landing.cues".to_owned(),
                    vec!["weave_historical_cues".to_owned()],
                ),
                (
                    "records.apollo_11_lunar_landing.fact".to_owned(),
                    vec!["apollo_11_wikidata".to_owned()],
                ),
            ]),
        },
    }
}

fn original_calendar_context_pack() -> TemporalContextPack {
    let fact_source = original_context_source(
        "weave_calendar_facts",
        "Original synthetic calendrical, seasonal, and celestial fixture facts.",
    );
    let cue_source = original_context_source(
        "weave_calendar_cues",
        "Original fictional calendar prompts and declared HEXACO vectors.",
    );
    let cues = |id: &str,
                kind: DateContextCueKind,
                content: &str,
                vector: BTreeMap<HexacoTrait, i16>| {
        BTreeMap::from([(
            id.to_owned(),
            TemporalAuthoringCue {
                id: id.to_owned(),
                kind,
                content: content.to_owned(),
                trait_vector: vector,
                tags: vec!["fictional_prompt".to_owned()],
                limitations: "Original fictional authoring cue only; the matched calendar context does not cause or predict personality.".to_owned(),
                source_ids: vec!["weave_calendar_cues".to_owned()],
            },
        )])
    };
    let standard = |level, reason: &str| TemporalUncertainty {
        level,
        reason: reason.to_owned(),
    };
    let low = TemporalSensitivity {
        level: DateContextSensitivity::Low,
        topics: Vec::new(),
        requires_explicit_review: false,
        note: "Neutral original fictional prompt.".to_owned(),
    };
    let records = BTreeMap::from([
        (
            "calendar_disputed_celestial".to_owned(),
            TemporalContextRecord {
                id: "calendar_disputed_celestial".to_owned(),
                kind: TemporalRecordKind::CelestialFact,
                evidence_class: TemporalEvidenceClass::CalendricalFact,
                extent: TemporalExtent::Date {
                    date: temporal_date(1969, 7, 20),
                },
                place_scope: TemporalPlaceScope::Global,
                time_zone: TemporalTimeZone::Utc,
                reference_period: exact_reference_period(
                    "Synthetic disputed celestial reference",
                    1969,
                    7,
                    20,
                ),
                fact: DomainValue::Object(BTreeMap::from([(
                    "fixture_status".to_owned(),
                    DomainValue::Symbol("synthetic_disputed".to_owned()),
                )])),
                tags: vec!["celestial".to_owned(), "synthetic".to_owned()],
                cues: cues(
                    "uncertain_light",
                    DateContextCueKind::Tension,
                    "Imagine a fictional disagreement over whether an uncertain light was a warning or an invitation.",
                    BTreeMap::from([(HexacoTrait::Anxiety, 450), (HexacoTrait::Openness, 550)]),
                ),
                source_ids: vec!["weave_calendar_facts".to_owned()],
                uncertainty: standard(
                    DateContextUncertainty::Disputed,
                    "The synthetic fixture deliberately marks this record disputed so ranking must downgrade it.",
                ),
                sensitivity: low.clone(),
            },
        ),
        (
            "calendar_midsummer_period".to_owned(),
            TemporalContextRecord {
                id: "calendar_midsummer_period".to_owned(),
                kind: TemporalRecordKind::SeasonalFact,
                evidence_class: TemporalEvidenceClass::CalendricalFact,
                extent: TemporalExtent::DateRange {
                    start: temporal_date(1969, 6, 21),
                    end: temporal_date(1969, 9, 22),
                },
                place_scope: TemporalPlaceScope::Global,
                time_zone: TemporalTimeZone::Utc,
                reference_period: TemporalReferencePeriod {
                    label: "Synthetic 1969 northern midsummer interval".to_owned(),
                    start: temporal_date(1969, 6, 21),
                    end: temporal_date(1969, 9, 22),
                    resolution: TemporalResolution::Period,
                },
                fact: DomainValue::Object(BTreeMap::from([(
                    "season".to_owned(),
                    DomainValue::Symbol("synthetic_northern_midsummer".to_owned()),
                )])),
                tags: vec!["seasonal".to_owned(), "synthetic".to_owned()],
                cues: cues(
                    "long_light_gathering",
                    DateContextCueKind::Memory,
                    "Imagine a fictional gathering held while the evening light lingered, with room for both celebration and quiet observation.",
                    BTreeMap::from([
                        (HexacoTrait::AestheticAppreciation, 450),
                        (HexacoTrait::Sociability, 550),
                    ]),
                ),
                source_ids: vec!["weave_calendar_facts".to_owned()],
                uncertainty: standard(
                    DateContextUncertainty::Bounded,
                    "The original fixture supplies a bounded seasonal interval rather than an instant.",
                ),
                sensitivity: low.clone(),
            },
        ),
        (
            "calendar_twentieth_day_observance".to_owned(),
            TemporalContextRecord {
                id: "calendar_twentieth_day_observance".to_owned(),
                kind: TemporalRecordKind::Commemoration,
                evidence_class: TemporalEvidenceClass::InterpretiveContext,
                extent: TemporalExtent::MonthDay { month: 7, day: 20 },
                place_scope: TemporalPlaceScope::Global,
                time_zone: TemporalTimeZone::Utc,
                reference_period: TemporalReferencePeriod {
                    label: "Original recurring fictional observance".to_owned(),
                    start: temporal_date(1, 7, 20),
                    end: temporal_date(9_999, 7, 20),
                    resolution: TemporalResolution::RecurringDay,
                },
                fact: DomainValue::Object(BTreeMap::from([(
                    "observance".to_owned(),
                    DomainValue::Symbol("fictional_twentieth_day".to_owned()),
                )])),
                tags: vec!["commemoration".to_owned(), "synthetic".to_owned()],
                cues: cues(
                    "returning_table",
                    DateContextCueKind::Affinity,
                    "Consider whether the fictional character is drawn to a table that is rebuilt each year for whoever arrives.",
                    BTreeMap::from([
                        (HexacoTrait::Sentimentality, 600),
                        (HexacoTrait::Sociability, 400),
                    ]),
                ),
                source_ids: vec!["weave_calendar_facts".to_owned()],
                uncertainty: standard(
                    DateContextUncertainty::Exact,
                    "The recurring month-day is explicit original fixture data.",
                ),
                sensitivity: low.clone(),
            },
        ),
        (
            "calendar_unmatched_leap_day".to_owned(),
            TemporalContextRecord {
                id: "calendar_unmatched_leap_day".to_owned(),
                kind: TemporalRecordKind::CalendricalFact,
                evidence_class: TemporalEvidenceClass::CalendricalFact,
                extent: TemporalExtent::MonthDay { month: 2, day: 29 },
                place_scope: TemporalPlaceScope::Global,
                time_zone: TemporalTimeZone::Utc,
                reference_period: TemporalReferencePeriod {
                    label: "Proleptic Gregorian recurring leap day".to_owned(),
                    start: temporal_date(4, 2, 29),
                    end: temporal_date(9_996, 2, 29),
                    resolution: TemporalResolution::RecurringDay,
                },
                fact: DomainValue::Object(BTreeMap::from([(
                    "month_day".to_owned(),
                    DomainValue::String("02-29".to_owned()),
                )])),
                tags: vec!["calendar".to_owned(), "leap_day".to_owned()],
                cues: cues(
                    "rare_cadence",
                    DateContextCueKind::Voice,
                    "Imagine a fictional speaking cadence that saves its most unusual image for rare occasions.",
                    BTreeMap::from([(HexacoTrait::Creativity, 1_000)]),
                ),
                source_ids: vec!["weave_calendar_facts".to_owned()],
                uncertainty: standard(
                    DateContextUncertainty::Exact,
                    "The recurring leap-day rule is explicit in the proleptic Gregorian fixture.",
                ),
                sensitivity: low,
            },
        ),
    ]);
    TemporalContextPack {
        pack_format_version: TEMPORAL_CONTEXT_PACK_FORMAT_VERSION,
        id: "org.weave.context.original_calendar".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Original synthetic calendar context".to_owned(),
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        provider: TemporalContextProvider::Standalone,
        records,
        provenance: Provenance {
            sources: vec![cue_source, fact_source],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                (
                    "records.cues".to_owned(),
                    vec!["weave_calendar_cues".to_owned()],
                ),
                (
                    "records.facts".to_owned(),
                    vec!["weave_calendar_facts".to_owned()],
                ),
            ]),
        },
    }
}

fn world_projection_context_pack() -> TemporalContextPack {
    let cue_source = original_context_source(
        "weave_world_cues",
        "Original fictional environmental prompts and declared HEXACO vectors.",
    );
    let projection_source = original_context_source(
        "weave_world_projection",
        "Original synthetic World-compatible environmental projection fixture.",
    );
    let records = BTreeMap::from([(
            "world_coastal_fog_cycle".to_owned(),
            TemporalContextRecord {
                id: "world_coastal_fog_cycle".to_owned(),
                kind: TemporalRecordKind::EnvironmentalFact,
                evidence_class: TemporalEvidenceClass::MeasuredFact,
                extent: TemporalExtent::DateRange {
                    start: temporal_date(1969, 7, 1),
                    end: temporal_date(1969, 8, 31),
                },
                place_scope: TemporalPlaceScope::Place {
                    id: "org.weave.place.synthetic_coast".to_owned(),
                    kind: "org.weave.world.fictional_region".to_owned(),
                },
                time_zone: TemporalTimeZone::Iana {
                    id: "America/Los_Angeles".to_owned(),
                },
                reference_period: TemporalReferencePeriod {
                    label: "Original fictional coast summer projection".to_owned(),
                    start: temporal_date(1969, 7, 1),
                    end: temporal_date(1969, 8, 31),
                    resolution: TemporalResolution::Period,
                },
                fact: DomainValue::Object(BTreeMap::from([
                    ("fictional".to_owned(), DomainValue::Bool(true)),
                    (
                        "pattern".to_owned(),
                        DomainValue::Symbol("recurring_coastal_fog".to_owned()),
                    ),
                    (
                        "world_place".to_owned(),
                        DomainValue::Symbol("org_weave_place_synthetic_coast".to_owned()),
                    ),
                ])),
                tags: vec!["environmental".to_owned(), "world_projection".to_owned()],
                cues: BTreeMap::from([(
                    "returning_fog".to_owned(),
                    TemporalAuthoringCue {
                        id: "returning_fog".to_owned(),
                        kind: DateContextCueKind::Memory,
                        content: "Imagine a fictional memory in which familiar fog made subtle changes easier to notice.".to_owned(),
                        trait_vector: BTreeMap::from([
                            (HexacoTrait::AestheticAppreciation, 450),
                            (HexacoTrait::Patience, 300),
                            (HexacoTrait::Perfectionism, 250),
                        ]),
                        tags: vec!["fictional_prompt".to_owned()],
                        limitations: "This original fictional cue is a non-causal authoring option; the environment record does not predict personality.".to_owned(),
                        source_ids: vec!["weave_world_cues".to_owned()],
                    },
                )]),
                source_ids: vec!["weave_world_projection".to_owned()],
                uncertainty: TemporalUncertainty {
                    level: DateContextUncertainty::Bounded,
                    reason: "The World-compatible fictional projection covers a declared date interval and place scope.".to_owned(),
                },
                sensitivity: TemporalSensitivity {
                    level: DateContextSensitivity::Low,
                    topics: Vec::new(),
                    requires_explicit_review: false,
                    note: "Neutral original fictional environmental prompt.".to_owned(),
                },
            },
        )]);
    let content_sha256 = temporal_provider_content_fingerprint(&records)
        .expect("the synthetic World projection is serializable");
    TemporalContextPack {
        pack_format_version: TEMPORAL_CONTEXT_PACK_FORMAT_VERSION,
        id: "org.weave.context.world_projection".to_owned(),
        version: "1.0.0".to_owned(),
        title: "World-compatible fictional environment context".to_owned(),
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        provider: TemporalContextProvider::DomainModule {
            module_id: "org.weave.world".to_owned(),
            module_version: "1.1.0".to_owned(),
            pack_id: "synthetic_temporal_projection".to_owned(),
            pack_version: "1.0.0".to_owned(),
            content_sha256,
        },
        records,
        provenance: Provenance {
            sources: vec![cue_source, projection_source],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                (
                    "records.world_coastal_fog_cycle.cues".to_owned(),
                    vec!["weave_world_cues".to_owned()],
                ),
                (
                    "records.world_coastal_fog_cycle.fact".to_owned(),
                    vec!["weave_world_projection".to_owned()],
                ),
            ]),
        },
    }
}

fn original_context_source(id: &str, attribution: &str) -> ProvenanceSource {
    ProvenanceSource {
        id: id.to_owned(),
        kind: ProvenanceKind::Original,
        url: "https://github.com/chrisgliddon/weave".to_owned(),
        revision: "temporal-context-v1".to_owned(),
        sha256: None,
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        attribution: attribution.to_owned(),
        modified: false,
    }
}

fn temporal_date(year: i32, month: u8, day: u8) -> TemporalDate {
    TemporalDate {
        calendar: Calendar::ProlepticGregorian,
        year,
        month,
        day,
    }
}

fn exact_reference_period(label: &str, year: i32, month: u8, day: u8) -> TemporalReferencePeriod {
    let date = temporal_date(year, month, day);
    TemporalReferencePeriod {
        label: label.to_owned(),
        start: date,
        end: date,
        resolution: TemporalResolution::Day,
    }
}

fn character_collection(
    ari: &CharacterProfile,
) -> Result<CharacterCollection, Box<dyn std::error::Error>> {
    let mut sable = ari.clone();
    sable.id = "org.weave.character.sable_reed".to_owned();
    retarget_expression_owner(&mut sable, "org.weave.character.sable_reed");
    sable.canon.identity.display_name.value = "Sable Reed".to_owned();
    let CharacterExtension::Relationships(relationships) = sable
        .extensions
        .get_mut("org.weave.character.relationships")
        .expect("relationship fixture")
    else {
        return Err("relationship fixture changed kind".into());
    };
    let edge = relationships
        .value
        .edges
        .get_mut("mentor_sable")
        .expect("relationship edge");
    edge.source_character_id.clone_from(&sable.id);
    edge.target_character_id.clone_from(&ari.id);
    recompute_derived(&mut sable);

    Ok(CharacterCollection {
        collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
        id: "org.weave.character.collection.glasswind".to_owned(),
        revision: 1,
        characters: BTreeMap::from([(ari.id.clone(), ari.clone()), (sable.id.clone(), sable)]),
    })
}

fn retarget_expression_owner(profile: &mut CharacterProfile, character_id: &str) {
    if let Some(CharacterExtension::Expression(record)) =
        profile.extensions.get_mut("org.weave.character.expression")
    {
        record.value.character_id = character_id.to_owned();
        for term in record.value.lexicon.values_mut() {
            term.character_id = character_id.to_owned();
        }
        for preference in record.value.preferences.values_mut() {
            preference.character_id = character_id.to_owned();
        }
        for pool in record.value.vocabulary_pools.values_mut() {
            pool.character_id = character_id.to_owned();
        }
        for constraint in record.value.voice_constraints.values_mut() {
            constraint.character_id = character_id.to_owned();
        }
        for assignment in record.value.template_assignments.values_mut() {
            assignment.character_id = character_id.to_owned();
        }
        record.value.behavioral_signature_refs = record
            .value
            .behavioral_signature_refs
            .iter()
            .filter_map(|reference| reference.rsplit_once(".signature."))
            .map(|(_, id)| format!("{character_id}.signature.{id}"))
            .collect();
    }
    if let Some(CharacterExtension::BehavioralSignatures(record)) = profile
        .extensions
        .get_mut("org.weave.character.behavioral_signatures")
    {
        record.value.character_id = character_id.to_owned();
        for signature in record.value.signatures.values_mut() {
            signature.character_id = character_id.to_owned();
        }
    }
}

fn rename_request(
    collection: &CharacterCollection,
) -> Result<CharacterOperationRequest, Box<dyn std::error::Error>> {
    Ok(CharacterOperationRequest {
        request_format_version: CHARACTER_OPERATION_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.operation.rename_ari".to_owned(),
        expected_input_sha256: collection_fingerprint(collection)?,
        seed: 4_271,
        scope: CharacterScope::Characters {
            ids: vec!["org.weave.character.ari_vale".to_owned()],
        },
        action: CharacterCorpusAction::Rename {
            character_id: "org.weave.character.ari_vale".to_owned(),
            new_id: Some("org.weave.character.ari_vale_wayfinder".to_owned()),
            new_display_name: Some("Ari Vale, Wayfinder".to_owned()),
            override_locked: false,
            rationale: "Migrate the stable identifier and every relationship reference together."
                .to_owned(),
        },
        provenance: original_provenance("operation_original", "operation"),
    })
}

fn complete_profile() -> CharacterProfile {
    let lineage = vec!["character_original".to_owned()];
    let mut personality = HexacoProfile {
        honesty_humility: HonestyHumility {
            factor: Some(trait_value(0.72, &lineage)),
            sincerity: Some(trait_value(0.76, &lineage)),
            fairness: Some(trait_value(0.81, &lineage)),
            greed_avoidance: Some(trait_value(0.66, &lineage)),
            modesty: Some(trait_value(0.65, &lineage)),
        },
        emotionality: Emotionality {
            factor: Some(trait_value(0.57, &lineage)),
            fearfulness: Some(trait_value(0.42, &lineage)),
            anxiety: Some(trait_value(0.58, &lineage)),
            dependence: Some(trait_value(0.49, &lineage)),
            sentimentality: Some(trait_value(0.79, &lineage)),
        },
        extraversion: Extraversion {
            factor: Some(trait_value(0.68, &lineage)),
            social_self_esteem: Some(trait_value(0.73, &lineage)),
            social_boldness: Some(trait_value(0.61, &lineage)),
            sociability: Some(trait_value(0.64, &lineage)),
            liveliness: Some(trait_value(0.74, &lineage)),
        },
        agreeableness: Agreeableness {
            factor: Some(trait_value(0.63, &lineage)),
            forgivingness: Some(trait_value(0.55, &lineage)),
            gentleness: Some(trait_value(0.71, &lineage)),
            flexibility: Some(trait_value(0.59, &lineage)),
            patience: Some(trait_value(0.67, &lineage)),
        },
        conscientiousness: Conscientiousness {
            factor: Some(trait_value(0.78, &lineage)),
            organization: Some(trait_value(0.75, &lineage)),
            diligence: Some(trait_value(0.84, &lineage)),
            perfectionism: Some(trait_value(0.69, &lineage)),
            prudence: Some(trait_value(0.82, &lineage)),
        },
        openness: Openness {
            factor: Some(trait_value(0.83, &lineage)),
            aesthetic_appreciation: Some(trait_value(0.88, &lineage)),
            inquisitiveness: Some(trait_value(0.81, &lineage)),
            creativity: Some(trait_value(0.86, &lineage)),
            unconventionality: Some(trait_value(0.77, &lineage)),
        },
    };
    personality
        .conscientiousness
        .prudence
        .as_mut()
        .expect("prudence fixture")
        .lock = LockState::Locked;

    let relationship_pack = relationship_kind_pack();
    let relationship_pack_ref = relationship_kind_pack_ref(&relationship_pack)
        .expect("reference relationship pack is valid");
    let expression_pack = expression_support::reference_expression_pack();
    let expression_pack_ref = weave_character::expression_pack_ref(&expression_pack)
        .expect("reference expression pack is valid");
    let mut provenance = original_provenance("character_original", "profile");
    provenance
        .sources
        .extend(relationship_pack.provenance.sources.clone());
    provenance
        .sources
        .extend(expression_pack.provenance.sources.clone());
    provenance
        .sources
        .sort_by(|left, right| left.id.cmp(&right.id));
    let mut extension_lineage = lineage.clone();
    extension_lineage.push(expression_support::SOURCE_ID.to_owned());
    extension_lineage.sort();
    provenance.claims.insert(
        "extensions.org.weave.character.relationships".to_owned(),
        vec!["weave_relationship_reference_pack".to_owned()],
    );
    provenance.claims.insert(
        "extensions.org.weave.character.expression".to_owned(),
        vec![expression_support::SOURCE_ID.to_owned()],
    );
    let mut profile = CharacterProfile {
        profile_format_version: CHARACTER_PROFILE_FORMAT_VERSION,
        id: "org.weave.character.ari_vale".to_owned(),
        canon: CharacterCanon {
            identity: CharacterIdentity {
                display_name: authored("Ari Vale".to_owned(), &lineage),
                aliases: Some(authored(
                    vec!["Ari".to_owned(), "Vale".to_owned()],
                    &lineage,
                )),
            },
            birth_date: Some(authored(
                BirthDate::Full {
                    calendar: Calendar::ProlepticGregorian,
                    year: 998,
                    month: 3,
                    day: 14,
                },
                &lineage,
            )),
            personality,
            inner_life: BTreeMap::from([(
                "keeps_promises".to_owned(),
                AuthoredNote {
                    id: "keeps_promises".to_owned(),
                    category: InnerLifeCategory::Value,
                    content: authored(
                        "Promises become paths Ari can follow through uncertainty.".to_owned(),
                        &lineage,
                    ),
                },
            )]),
            voice: BTreeMap::from([(
                "measured_warmth".to_owned(),
                VoiceDirection {
                    id: "measured_warmth".to_owned(),
                    category: VoiceCategory::Cadence,
                    content: authored(
                        "Short observations followed by one generous question.".to_owned(),
                        &lineage,
                    ),
                },
            )]),
        },
        extensions: extensions(
            &extension_lineage,
            relationship_pack_ref,
            expression_pack_ref,
        ),
        suggestions: BTreeMap::from([(
            "night_market_memory".to_owned(),
            CharacterSuggestion {
                id: "night_market_memory".to_owned(),
                target_path: "canon.inner_life.night_market_memory".to_owned(),
                proposal: suggested(
                    DomainValue::String(
                        "A remembered lantern exchange may create a useful tension.".to_owned(),
                    ),
                    &lineage,
                ),
            },
        )]),
        derived: CharacterDerivedViews {
            ocean: weave_character::derive_ocean(&HexacoProfile::default()),
        },
        provenance,
    };
    recompute_derived(&mut profile);
    profile
}

fn extensions(
    lineage: &[String],
    relationship_kind_pack: RelationshipKindPackRef,
    expression_pack: ExpressionPackRef,
) -> BTreeMap<String, CharacterExtension> {
    let identity_namespace = "org.weave.character.identity_presentation";
    let expression_namespace = "org.weave.character.expression";
    let behavior_namespace = "org.weave.character.behavioral_signatures";
    let role_namespace = "org.weave.character.role_projections";
    let relationship_namespace = "org.weave.character.relationships";
    let date_namespace = "org.weave.character.date_context";
    let tabletop_namespace = "org.weave.character.tabletop";
    BTreeMap::from([
        (
            behavior_namespace.to_owned(),
            CharacterExtension::BehavioralSignatures(VersionedExtension {
                header: extension_header(behavior_namespace, 1, lineage),
                value: BehavioralSignatures {
                    character_id: "org.weave.character.ari_vale".to_owned(),
                    signatures: BTreeMap::from([(
                        "maps_before_moving".to_owned(),
                        BehavioralSignature {
                            id: "maps_before_moving".to_owned(),
                            character_id: "org.weave.character.ari_vale".to_owned(),
                            category: "org.weave.expression.planning".to_owned(),
                            cue: "Sketches a route before committing the group.".to_owned(),
                            strength: 0.8,
                            applicability: ExpressionApplicability::default(),
                            origin: ExpressionRecordOrigin::Authored,
                            review: ReviewState::NotRequired,
                            source_ids: vec!["character_original".to_owned()],
                            rationale: None,
                        },
                    )]),
                },
            }),
        ),
        (
            date_namespace.to_owned(),
            CharacterExtension::DateContext(VersionedExtension {
                header: extension_header(date_namespace, 1, lineage),
                value: DateContext {
                    context_pack: "org.weave.context.synthetic_calendar".to_owned(),
                    context_version: "1.0.0".to_owned(),
                    context_hash: "a".repeat(64),
                    additional_context_packs: Vec::new(),
                    accepted_record_ids: vec!["early_rains".to_owned()],
                    accepted_cues: BTreeMap::new(),
                },
            }),
        ),
        (
            expression_namespace.to_owned(),
            CharacterExtension::Expression(VersionedExtension {
                header: extension_header(expression_namespace, 1, lineage),
                value: ExpressionData {
                    expression_format_version: 1,
                    character_id: "org.weave.character.ari_vale".to_owned(),
                    lexicon: BTreeMap::from([
                        (
                            "trailmark".to_owned(),
                            NormalizedExpressionTerm {
                                id: "trailmark".to_owned(),
                                character_id: "org.weave.character.ari_vale".to_owned(),
                                category: "org.weave.expression.navigation".to_owned(),
                                kind: ExpressionTermKind::Term,
                                surface: "trailmark".to_owned(),
                                normalized: "trailmark".to_owned(),
                                strength: 0.9,
                                applicability: ExpressionApplicability::default(),
                                origin: ExpressionRecordOrigin::PackAssigned,
                                review: ReviewState::Accepted,
                                source_ids: vec![expression_support::SOURCE_ID.to_owned()],
                                rationale: Some(
                                    "Accept the original public reference-pack term for deterministic dialogue fixtures."
                                        .to_owned(),
                                ),
                            },
                        ),
                        (
                            "waymark".to_owned(),
                            NormalizedExpressionTerm {
                                id: "waymark".to_owned(),
                                character_id: "org.weave.character.ari_vale".to_owned(),
                                category: "org.weave.expression.navigation".to_owned(),
                                kind: ExpressionTermKind::Term,
                                surface: "waymark".to_owned(),
                                normalized: "waymark".to_owned(),
                                strength: 0.8,
                                applicability: ExpressionApplicability::default(),
                                origin: ExpressionRecordOrigin::Authored,
                                review: ReviewState::NotRequired,
                                source_ids: vec!["character_original".to_owned()],
                                rationale: None,
                            },
                        ),
                    ]),
                    preferences: BTreeMap::from([(
                        "clear_questions".to_owned(),
                        NormalizedPreference {
                            id: "clear_questions".to_owned(),
                            character_id: "org.weave.character.ari_vale".to_owned(),
                            category: "org.weave.preference.communication".to_owned(),
                            target: "clear questions".to_owned(),
                            polarity: PreferencePolarity::Prefer,
                            strength: 0.9,
                            applicability: ExpressionApplicability::default(),
                            origin: ExpressionRecordOrigin::Authored,
                            review: ReviewState::NotRequired,
                            source_ids: vec!["character_original".to_owned()],
                            rationale: None,
                        },
                    )]),
                    vocabulary_pools: BTreeMap::from([(
                        "navigation_words".to_owned(),
                        ExpressionVocabularyPool {
                            id: "navigation_words".to_owned(),
                            character_id: "org.weave.character.ari_vale".to_owned(),
                            category: "org.weave.expression.navigation".to_owned(),
                            term_ids: vec!["trailmark".to_owned(), "waymark".to_owned()],
                            applicability: ExpressionApplicability::default(),
                            origin: ExpressionRecordOrigin::Authored,
                            review: ReviewState::NotRequired,
                            source_ids: vec!["character_original".to_owned()],
                            rationale: None,
                        },
                    )]),
                    voice_constraints: BTreeMap::from([(
                        "prefer_clear_questions".to_owned(),
                        ExpressionVoiceConstraint {
                            id: "prefer_clear_questions".to_owned(),
                            character_id: "org.weave.character.ari_vale".to_owned(),
                            category: "org.weave.expression.clarity".to_owned(),
                            medium: ExpressionMedium::Both,
                            effect: ExpressionConstraintEffect::Prefer,
                            target: "clear questions".to_owned(),
                            instruction: "Ask one clear question after a short observation."
                                .to_owned(),
                            strength: 0.9,
                            applicability: ExpressionApplicability::default(),
                            origin: ExpressionRecordOrigin::Authored,
                            review: ReviewState::NotRequired,
                            source_ids: vec!["character_original".to_owned()],
                            rationale: None,
                        },
                    )]),
                    template_assignments: BTreeMap::from([(
                        "arrival_greeting".to_owned(),
                        ExpressionTemplateAssignment {
                            id: "arrival_greeting".to_owned(),
                            character_id: "org.weave.character.ari_vale".to_owned(),
                            scenario_id: "arrival".to_owned(),
                            pack: expression_pack.clone(),
                            template_id: "arrival_greeting".to_owned(),
                            state: ValueState::Reviewed,
                            review: ReviewState::Accepted,
                            lock: LockState::Unlocked,
                            source_ids: vec![expression_support::SOURCE_ID.to_owned()],
                            rationale: "Accept the original public arrival template for the synthetic runtime fixture."
                                .to_owned(),
                        },
                    )]),
                    behavioral_signature_refs: vec![
                        "org.weave.character.ari_vale.signature.maps_before_moving".to_owned(),
                    ],
                    source_pack_refs: vec![expression_pack],
                },
            }),
        ),
        (
            identity_namespace.to_owned(),
            CharacterExtension::IdentityPresentation(VersionedExtension {
                header: extension_header(identity_namespace, 1, lineage),
                value: IdentityPresentation {
                    identity_refs: vec!["org.weave.identity.wayfinder".to_owned()],
                    presentation_refs: vec![
                        "assets/avatars/ari_vale.png".to_owned(),
                        "assets/palettes/cedar_snow.json".to_owned(),
                    ],
                    ..IdentityPresentation::default()
                },
            }),
        ),
        (
            relationship_namespace.to_owned(),
            CharacterExtension::Relationships(VersionedExtension {
                header: extension_header(relationship_namespace, 1, lineage),
                value: RelationshipEdges {
                    graph_format_version: 1,
                    kind_pack: Some(relationship_kind_pack),
                    edges: BTreeMap::from([(
                        "mentor_sable".to_owned(),
                        RelationshipEdge {
                            id: "mentor_sable".to_owned(),
                            source_character_id: "org.weave.character.ari_vale".to_owned(),
                            target_character_id: "org.weave.character.sable_reed".to_owned(),
                            kind: "org.weave.relationship.mentor".to_owned(),
                            confidence: Confidence::High,
                            origin: weave_character::RelationshipEdgeOrigin::Authored,
                            review: ReviewState::NotRequired,
                            lock: LockState::Unlocked,
                            freshness: Freshness::Current,
                            validity: None,
                            inverse_edge_id: None,
                            notes: BTreeMap::from([(
                                "fixture_context".to_owned(),
                                RelationshipNote {
                                    id: "fixture_context".to_owned(),
                                    content: "Explicit synthetic relationship context for portable tests."
                                        .to_owned(),
                                    lineage: lineage.to_vec(),
                                },
                            )]),
                            consent: None,
                            safeguard_exceptions: BTreeMap::new(),
                            affinity_score_micros: None,
                            evidence: Vec::new(),
                            lineage: lineage.to_vec(),
                            rationale: Some(
                                "Authored only for the public synthetic Character fixture."
                                    .to_owned(),
                            ),
                        },
                    )]),
                },
            }),
        ),
        (
            role_namespace.to_owned(),
            CharacterExtension::RoleProjections(VersionedExtension {
                header: extension_header(role_namespace, 1, lineage),
                value: RoleProjections {
                    roles: BTreeMap::from([(
                        "route_steward".to_owned(),
                        RoleProjection {
                            id: "route_steward".to_owned(),
                            taxonomy: "org.weave.roles.glasswind".to_owned(),
                            role: "Route steward".to_owned(),
                            rationale: "Accepted as a narrative role, not a personality fact."
                                .to_owned(),
                            input_paths: vec![
                                "canon.personality.conscientiousness.factor".to_owned(),
                                "canon.personality.openness.factor".to_owned(),
                            ],
                        },
                    )]),
                },
            }),
        ),
        (
            tabletop_namespace.to_owned(),
            CharacterExtension::Tabletop(VersionedExtension {
                header: extension_header(tabletop_namespace, 1, lineage),
                value: OpaqueExtensionData {
                    interpretation: OpaqueInterpretation::PreservedInactive,
                    payload: DomainValue::Object(BTreeMap::from([(
                        "ruleset_ref".to_owned(),
                        DomainValue::String("org.weave.rules.synthetic@1".to_owned()),
                    )])),
                },
            }),
        ),
    ])
}

fn overlay(template: &CharacterTemplate) -> Result<CharacterOverlay, Box<dyn std::error::Error>> {
    let lineage = vec!["overlay_original".to_owned()];
    let display_hash = value_hash(&template.profile.canon.identity.display_name)?;
    let prudence_hash = value_hash(
        template
            .profile
            .canon
            .personality
            .conscientiousness
            .prudence
            .as_ref()
            .expect("prudence fixture"),
    )?;
    Ok(CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.ari_vale_revision".to_owned(),
        character_id: template.profile.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(template)?,
        }),
        operations: vec![
            CharacterOperation {
                id: "rename_for_glasswind".to_owned(),
                expected_prior_sha256: Some(display_hash),
                rationale: "Make the authored setting identity explicit.".to_owned(),
                action: CharacterOperationAction::SetDisplayName {
                    value: overridden("Ari Vale of Glasswind".to_owned(), &lineage),
                },
            },
            CharacterOperation {
                id: "confirm_prudence".to_owned(),
                expected_prior_sha256: Some(prudence_hash),
                rationale: "Explicitly replace a locked template trait after review.".to_owned(),
                action: CharacterOperationAction::SetHexacoTrait {
                    trait_id: weave_character::HexacoTrait::Prudence,
                    value: overridden(
                        TraitMeasurement::Band {
                            band: weave_character::TraitBand::VeryHigh,
                        },
                        &lineage,
                    ),
                },
            },
            CharacterOperation {
                id: "preserve_future_palette".to_owned(),
                expected_prior_sha256: None,
                rationale: "Retain an unknown optional extension without interpreting it."
                    .to_owned(),
                action: CharacterOperationAction::UpsertExtension {
                    namespace: "org.weave.character.future_palette".to_owned(),
                    extension: CharacterExtension::Opaque(VersionedExtension {
                        header: ExtensionHeader {
                            namespace: "org.weave.character.future_palette".to_owned(),
                            extension_version: 99,
                            authority: "org.weave.extension.future_palette".to_owned(),
                            rationale: "Preserved for a future compatible host.".to_owned(),
                            state: ValueState::Imported,
                            review: ReviewState::Accepted,
                            lock: LockState::Locked,
                            freshness: Freshness::Current,
                            lineage: lineage.clone(),
                            canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                        },
                        value: OpaqueExtensionData {
                            interpretation: OpaqueInterpretation::PreservedInactive,
                            payload: DomainValue::Object(BTreeMap::from([(
                                "accent".to_owned(),
                                DomainValue::Symbol("pine".to_owned()),
                            )])),
                        },
                    }),
                },
            },
        ],
        provenance: original_provenance("overlay_original", "overlay"),
    })
}

fn trait_value(score: f64, lineage: &[String]) -> Attributed<TraitMeasurement> {
    authored(TraitMeasurement::Score { score }, lineage)
}

fn authored<T>(value: T, lineage: &[String]) -> Attributed<T> {
    Attributed {
        value,
        state: ValueState::Authored,
        confidence: Confidence::High,
        review: ReviewState::NotRequired,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: None,
    }
}

fn overridden<T>(value: T, lineage: &[String]) -> Attributed<T> {
    Attributed {
        value,
        state: ValueState::Overridden,
        confidence: Confidence::High,
        review: ReviewState::Accepted,
        lock: LockState::Locked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: Some("Reviewed fictional author decision.".to_owned()),
    }
}

fn suggested(value: DomainValue, lineage: &[String]) -> Attributed<DomainValue> {
    Attributed {
        value,
        state: ValueState::Suggested,
        confidence: Confidence::Low,
        review: ReviewState::Pending,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: Some("Optional synthetic authoring prompt.".to_owned()),
    }
}

fn extension_header(namespace: &str, version: u32, lineage: &[String]) -> ExtensionHeader {
    ExtensionHeader {
        namespace: namespace.to_owned(),
        extension_version: version,
        authority: "org.weave.character.contract".to_owned(),
        rationale: "Explicit optional projection authored for the synthetic fixture.".to_owned(),
        state: ValueState::Reviewed,
        review: ReviewState::Accepted,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        canonical_personality_write_back: ExtensionWriteBack::Forbidden,
    }
}

fn original_provenance(source_id: &str, claim: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: source_id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "character-contract-v1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            attribution: "Original synthetic Weave Character contract fixture.".to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([(claim.to_owned(), vec![source_id.to_owned()])]),
    }
}

fn value_hash(value: &impl serde::Serialize) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn write_pair<T: serde::Serialize>(
    directory: &Path,
    stem: &str,
    value: &T,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = weave_domain::to_pretty_json(value)?;
    let ron = weave_domain::to_pretty_ron(value)?;
    write_or_check(
        &directory.join(format!("{stem}.json")),
        json.as_bytes(),
        write,
    )?;
    write_or_check(
        &directory.join(format!("{stem}.ron")),
        ron.as_bytes(),
        write,
    )
}

fn write_raw_json(
    path: &Path,
    value: &impl serde::Serialize,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = to_pretty_json(value)?;
    write_or_check(path, json.as_bytes(), write)
}

fn write_json_value(
    path: &Path,
    value: &serde_json::Value,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = to_pretty_json(value)?;
    write_or_check(path, json.as_bytes(), write)
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
