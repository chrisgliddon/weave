use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use weave_character::{
    Agreeableness, AlignmentView, Attributed, AuthoredNote, BehavioralSignature,
    BehavioralSignatures, BirthDate, CHARACTER_COLLECTION_FORMAT_VERSION,
    CHARACTER_OPERATION_REQUEST_FORMAT_VERSION, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_PROFILE_FORMAT_VERSION, CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar, CharacterCanon,
    CharacterCollection, CharacterCorpusAction, CharacterDerivedViews, CharacterExtension,
    CharacterIdentity, CharacterOperation, CharacterOperationAction, CharacterOperationRequest,
    CharacterOverlay, CharacterProfile, CharacterReviewDecision, CharacterScope,
    CharacterSuggestion, CharacterTemplate, CharacterTemplateRef, Confidence, Conscientiousness,
    DateContext, DateContextCueKind, DateContextSensitivity, DateContextUncertainty, Emotionality,
    ExpressionData, ExtensionHeader, ExtensionWriteBack, Extraversion, Freshness, HexacoProfile,
    HexacoTrait, HonestyHumility, IdentityPresentation, InnerLifeCategory, LockState,
    NormalizedExpressionTerm, NormalizedPreference, OpaqueExtensionData, OpaqueInterpretation,
    Openness, PreferencePolarity, RelationshipEdge, RelationshipEdges, ReviewState, RoleProjection,
    RoleProjections, TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION, TEMPORAL_CONTEXT_PACK_FORMAT_VERSION,
    TemporalAuthoringCue, TemporalAutoApprovePolicy, TemporalContextConfig, TemporalContextPack,
    TemporalContextProposal, TemporalContextProvider, TemporalContextReceipt,
    TemporalContextRecord, TemporalContextReview, TemporalDate, TemporalEvidenceClass,
    TemporalExtent, TemporalPlaceScope, TemporalRecordKind, TemporalReferencePeriod,
    TemporalResolution, TemporalReviewAction, TemporalReviewDecision, TemporalSensitivity,
    TemporalTimeZone, TemporalUncertainty, TraitMeasurement, ValueState, VersionedExtension,
    VoiceCategory, VoiceDirection, apply_reviewed_character_proposal,
    apply_reviewed_temporal_context, character_collection_schema, character_diagnostic_schema,
    character_domain_pack, character_module_manifest, character_operation_request_schema,
    character_overlay_schema, character_profile_schema, character_progress_schema,
    character_proposal_schema, character_review_schema, character_synthesis_schema,
    character_template_schema, collection_fingerprint, create_temporal_context_review,
    propose_character_operation, propose_temporal_context, recompute_derived,
    resume_character_operation, review_character_proposal, synthesize_character,
    template_fingerprint, temporal_context_config_schema, temporal_context_pack_schema,
    temporal_context_proposal_schema, temporal_context_receipt_schema,
    temporal_context_review_schema, temporal_provider_content_fingerprint,
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

    let profile = complete_profile();
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
    let collection = character_collection(&profile)?;
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

    let invalid = fixture.join("invalid");
    let mut unknown_profile_version = serde_json::to_value(&profile)?;
    unknown_profile_version["profile_format_version"] = serde_json::Value::from(2);
    write_json_value(
        &invalid.join("unknown-profile-version.character.json"),
        &unknown_profile_version,
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
    relationships
        .value
        .edges
        .get_mut("mentor_sable")
        .expect("relationship edge")
        .target_character_id
        .clone_from(&profile.id);
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
    ] {
        write_or_check(&schemas.join(name), contents.as_bytes(), write)?;
    }
    Ok(())
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

    let provenance = original_provenance("character_original", "profile");
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
        extensions: extensions(&lineage),
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

fn extensions(lineage: &[String]) -> BTreeMap<String, CharacterExtension> {
    let identity_namespace = "org.weave.character.identity_presentation";
    let expression_namespace = "org.weave.character.expression";
    let behavior_namespace = "org.weave.character.behavioral_signatures";
    let role_namespace = "org.weave.character.role_projections";
    let relationship_namespace = "org.weave.character.relationships";
    let alignment_namespace = "org.weave.character.alignment";
    let date_namespace = "org.weave.character.date_context";
    let tabletop_namespace = "org.weave.character.tabletop";
    BTreeMap::from([
        (
            alignment_namespace.to_owned(),
            CharacterExtension::AlignmentView(VersionedExtension {
                header: extension_header(alignment_namespace, 1, lineage),
                value: AlignmentView {
                    view_id: "org.weave.alignment.compass".to_owned(),
                    values: DomainValue::Object(BTreeMap::from([(
                        "orientation".to_owned(),
                        DomainValue::Symbol("steward".to_owned()),
                    )])),
                    input_paths: vec![
                        "canon.personality.agreeableness.factor".to_owned(),
                        "canon.personality.honesty_humility.factor".to_owned(),
                    ],
                },
            }),
        ),
        (
            behavior_namespace.to_owned(),
            CharacterExtension::BehavioralSignatures(VersionedExtension {
                header: extension_header(behavior_namespace, 1, lineage),
                value: BehavioralSignatures {
                    signatures: BTreeMap::from([(
                        "maps_before_moving".to_owned(),
                        BehavioralSignature {
                            id: "maps_before_moving".to_owned(),
                            cue: "Sketches a route before committing the group.".to_owned(),
                            strength: 0.8,
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
                    lexicon: BTreeMap::from([(
                        "waymark".to_owned(),
                        NormalizedExpressionTerm {
                            id: "waymark".to_owned(),
                            category: "org.weave.expression.navigation".to_owned(),
                            normalized: "waymark".to_owned(),
                            strength: 0.8,
                        },
                    )]),
                    preferences: BTreeMap::from([(
                        "clear_questions".to_owned(),
                        NormalizedPreference {
                            id: "clear_questions".to_owned(),
                            category: "org.weave.preference.communication".to_owned(),
                            target: "clear questions".to_owned(),
                            polarity: PreferencePolarity::Prefer,
                            strength: 0.9,
                        },
                    )]),
                    behavioral_signature_refs: vec![
                        "org.weave.signature.maps_before_moving".to_owned(),
                    ],
                    source_pack_refs: vec!["org.weave.expression.glasswind".to_owned()],
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
                },
            }),
        ),
        (
            relationship_namespace.to_owned(),
            CharacterExtension::Relationships(VersionedExtension {
                header: extension_header(relationship_namespace, 1, lineage),
                value: RelationshipEdges {
                    edges: BTreeMap::from([(
                        "mentor_sable".to_owned(),
                        RelationshipEdge {
                            id: "mentor_sable".to_owned(),
                            source_character_id: "org.weave.character.ari_vale".to_owned(),
                            target_character_id: "org.weave.character.sable_reed".to_owned(),
                            kind: "org.weave.relationship.mentor".to_owned(),
                            confidence: Confidence::High,
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
