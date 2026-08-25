use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    FREEHACK_CAMPAIGN_SCHEMA_VERSION, FREEHACK_CREATION_FORMAT_VERSION,
    FREEHACK_PROBABILITY_FORMAT_VERSION, FreehackArchetypeOption, FreehackArchetypeProcedure,
    FreehackCampaignSchema, FreehackContribution, FreehackContributionSource,
    FreehackCreationPreview, FreehackCreationRequest, FreehackDisclosure, FreehackFeatureOption,
    FreehackFeatureSelection, FreehackInventoryOption, FreehackInventorySelection,
    FreehackMemorySeed, FreehackModifierGeneration, FreehackModifierSpec,
    FreehackProbabilityRequest, FreehackRarity, FreehackResolver, FreehackSectionAbstraction,
    FreehackSectionMapping, FreehackSectionStatus, FreehackSectionTiming, FreehackTrackOutcome,
    FreehackTrackTemplate, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, TabletopCapability, TabletopState, canonical_fingerprint,
    create_freehack_character, freehack_manifest, freehack_probability_preview,
    project_freehack_authority_receipt, project_freehack_public_receipt,
    project_freehack_public_state, validate_adapter_manifest, validate_resolution_receipt,
};

fn campaign() -> FreehackCampaignSchema {
    FreehackCampaignSchema {
        schema_version: FREEHACK_CAMPAIGN_SCHEMA_VERSION,
        id: "org.weave.freehack.harbor_test".to_owned(),
        title: "Harbor Test".to_owned(),
        archetype_procedure: FreehackArchetypeProcedure::RarityOffer,
        archetypes: vec![
            FreehackArchetypeOption {
                id: "courier".to_owned(),
                label: "Courier".to_owned(),
                description: "Moves messages and small cargo between districts.".to_owned(),
                rarity: FreehackRarity::Common,
            },
            FreehackArchetypeOption {
                id: "scribe".to_owned(),
                label: "Scribe".to_owned(),
                description: "Keeps exact public and private records.".to_owned(),
                rarity: FreehackRarity::Common,
            },
            FreehackArchetypeOption {
                id: "diver".to_owned(),
                label: "Diver".to_owned(),
                description: "Works below the tide line.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "pilot".to_owned(),
                label: "Pilot".to_owned(),
                description: "Guides vessels through narrow channels.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "rigger".to_owned(),
                label: "Rigger".to_owned(),
                description: "Builds temporary lines and lifts.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "cartographer".to_owned(),
                label: "Cartographer".to_owned(),
                description: "Maintains charts of changing passages.".to_owned(),
                rarity: FreehackRarity::Rare,
            },
            FreehackArchetypeOption {
                id: "signal_keeper".to_owned(),
                label: "Signal Keeper".to_owned(),
                description: "Coordinates long-range harbor signals.".to_owned(),
                rarity: FreehackRarity::Rare,
            },
        ],
        modifiers: vec![
            FreehackModifierSpec {
                id: "focus".to_owned(),
                label: "Focus".to_owned(),
                description: "Sustained attention under pressure.".to_owned(),
                minimum: -10,
                maximum: 10,
                generation: FreehackModifierGeneration::Authored,
                player_visible: true,
            },
            FreehackModifierSpec {
                id: "balance".to_owned(),
                label: "Balance".to_owned(),
                description: "Footing on moving or narrow surfaces.".to_owned(),
                minimum: 1,
                maximum: 10,
                generation: FreehackModifierGeneration::RandomInclusive,
                player_visible: true,
            },
            FreehackModifierSpec {
                id: "clearance".to_owned(),
                label: "Clearance".to_owned(),
                description: "Campaign-specific access rating.".to_owned(),
                minimum: 2,
                maximum: 2,
                generation: FreehackModifierGeneration::Fixed { value: 2 },
                player_visible: false,
            },
        ],
        feature_count: 2,
        feature_options: vec![FreehackFeatureOption {
            id: "quiet_step".to_owned(),
            label: "Quiet Step".to_owned(),
            description: "Moves carefully across resonant decking.".to_owned(),
        }],
        allow_authored_features: true,
        inventory_budget: Some(10),
        inventory_options: vec![FreehackInventoryOption {
            id: "line_launcher".to_owned(),
            label: "Line Launcher".to_owned(),
            description: "Projects a light cord across a gap.".to_owned(),
            cost: Some(3),
        }],
        allow_authored_inventory: true,
        initial_tracks: vec![
            FreehackTrackTemplate {
                id: "fatigue".to_owned(),
                label: "Fatigue".to_owned(),
                secret: false,
                interval: "after strenuous movement".to_owned(),
                target_count: 3,
                advances_on: FreehackTrackOutcome::Failure,
                consequence: "The character must pause and recover.".to_owned(),
            },
            FreehackTrackTemplate {
                id: "hidden_watch".to_owned(),
                label: "Hidden Watch".to_owned(),
                secret: true,
                interval: "after a loud disturbance".to_owned(),
                target_count: 2,
                advances_on: FreehackTrackOutcome::Success,
                consequence: "An unseen observer changes position.".to_owned(),
            },
        ],
    }
}

fn creation_request() -> FreehackCreationRequest {
    FreehackCreationRequest {
        creation_format_version: FREEHACK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.tavi_quill".to_owned(),
        name: "Tavi Quill".to_owned(),
        seed: 137,
        campaign: campaign(),
        selected_archetype: Some("courier".to_owned()),
        authored_modifiers: BTreeMap::from([("focus".to_owned(), 4)]),
        features: vec![
            FreehackFeatureSelection::Catalog {
                id: "quiet_step".to_owned(),
            },
            FreehackFeatureSelection::Authored {
                id: "tide_memory".to_owned(),
                label: "Tide Memory".to_owned(),
                description: "Remembers recurring harbor currents.".to_owned(),
            },
        ],
        inventory: vec![
            FreehackInventorySelection::Catalog {
                id: "line_launcher".to_owned(),
                quantity: 2,
            },
            FreehackInventorySelection::Authored {
                id: "wax_tablet".to_owned(),
                label: "Wax Tablet".to_owned(),
                quantity: 2,
                cost_per_item: 1,
            },
        ],
        memories: vec![
            FreehackMemorySeed {
                id: "old_channel".to_owned(),
                topic: "the old freight channel".to_owned(),
                has_experience: true,
                rationale: "Established explicitly during character creation.".to_owned(),
                disclosure: FreehackDisclosure::Public,
            },
            FreehackMemorySeed {
                id: "sealed_route".to_owned(),
                topic: "a sealed maintenance route".to_owned(),
                has_experience: false,
                rationale: "Held for authority review until play establishes it.".to_owned(),
                disclosure: FreehackDisclosure::HostOnly,
            },
        ],
    }
}

fn contribution(id: &str, value: u32) -> FreehackContribution {
    FreehackContribution {
        id: id.to_owned(),
        label: id.replace('_', " "),
        value,
        source: FreehackContributionSource::Situation,
    }
}

fn value_object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn value_contribution(id: &str, label: &str, value: u32, source: &str) -> DomainValue {
    value_object([
        ("id", DomainValue::String(id.to_owned())),
        ("label", DomainValue::String(label.to_owned())),
        ("source", DomainValue::Symbol(source.to_owned())),
        ("value", DomainValue::Number(f64::from(value))),
    ])
}

fn check_input(
    label: &str,
    opposition_id: &str,
    opposition_label: &str,
    disclosure: &str,
) -> DomainValue {
    value_object([
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("label", DomainValue::String(label.to_owned())),
        (
            "opposition",
            DomainValue::List(vec![value_contribution(
                opposition_id,
                opposition_label,
                3,
                "situation",
            )]),
        ),
        ("reveal_opposition", DomainValue::Bool(false)),
        ("reveal_probability", DomainValue::Bool(false)),
        ("reveal_support", DomainValue::Bool(disclosure == "public")),
        (
            "support",
            DomainValue::List(vec![value_contribution("focus", "Focus", 4, "character")]),
        ),
    ])
}

fn section_open_input(id: &str, timing: &str, mapping: &str, abstraction: &str) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "abstraction".to_owned(),
            DomainValue::Symbol(abstraction.to_owned()),
        ),
        ("id".to_owned(), DomainValue::String(id.to_owned())),
        (
            "label".to_owned(),
            DomainValue::String(format!("{id} label")),
        ),
        (
            "mapping".to_owned(),
            DomainValue::Symbol(mapping.to_owned()),
        ),
        (
            "participants".to_owned(),
            DomainValue::List(vec![
                DomainValue::String("first".to_owned()),
                DomainValue::String("second".to_owned()),
            ]),
        ),
        (
            "reveal_after_resolution".to_owned(),
            DomainValue::Bool(false),
        ),
        ("timing".to_owned(), DomainValue::Symbol(timing.to_owned())),
    ]);
    if timing == "timed" {
        fields.insert("turn_ticks".to_owned(), DomainValue::Number(8.0));
    }
    DomainValue::Object(fields)
}

fn section_submission_input(
    section_id: &str,
    participant_id: &str,
    submission_id: &str,
    mapping: &str,
) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "action".to_owned(),
            DomainValue::String(format!("{participant_id} acts")),
        ),
        (
            "participant_id".to_owned(),
            DomainValue::String(participant_id.to_owned()),
        ),
        (
            "section_id".to_owned(),
            DomainValue::String(section_id.to_owned()),
        ),
        (
            "submission_id".to_owned(),
            DomainValue::String(submission_id.to_owned()),
        ),
    ]);
    if mapping == "mapped" {
        fields.insert(
            "map_intent".to_owned(),
            DomainValue::String(format!("{participant_id} changes position")),
        );
    }
    DomainValue::Object(fields)
}

fn execute(
    registry: &ResolverRegistry,
    preview: &FreehackCreationPreview,
    state: &TabletopState,
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> (ResolutionRequest, ResolutionReceipt) {
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: request_id
            .rsplit('.')
            .next()
            .expect("test request id has one segment")
            .to_owned(),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state).unwrap(),
        input,
    };
    let receipt = registry
        .execute(&freehack_manifest(), &request, state)
        .unwrap();
    validate_resolution_receipt(&receipt, &freehack_manifest()).unwrap();
    (request, receipt)
}

#[test]
fn manifest_and_configurable_creation_validate_and_round_trip() {
    let manifest = freehack_manifest();
    validate_adapter_manifest(&manifest).unwrap();
    assert_eq!(
        manifest.provenance.source_url,
        weave_tabletop::FREEHACK_RELEASE_URL
    );
    assert_eq!(manifest.provenance.revision, "2.1 (released 2026-07-13)");
    assert_eq!(
        manifest.provenance.sha256,
        weave_tabletop::FREEHACK_PDF_SHA256
    );
    assert_eq!(manifest.provenance.license, "CC0-1.0");
    assert_eq!(manifest.provenance.additional_artifacts.len(), 3);
    assert!(
        manifest
            .provenance
            .additional_artifacts
            .iter()
            .all(|artifact| {
                artifact.revision == weave_tabletop::FREEHACK_SOURCE_REVISION
                    && artifact
                        .source_url
                        .starts_with(weave_tabletop::FREEHACK_SOURCE_URL)
                    && artifact.sha256.len() == 64
            })
    );
    let request = creation_request();
    assert_eq!(
        FreehackCreationRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        FreehackCreationRequest::from_ron(&request.to_ron().unwrap()).unwrap(),
        request
    );
    let preview = create_freehack_character(&request).unwrap();
    assert_eq!(preview.offered_archetypes.len(), 5);
    assert!(preview.offered_archetypes.contains(&"courier".to_owned()));
    assert_eq!(preview.modifiers["focus"], 4);
    assert!((1..=10).contains(&preview.modifiers["balance"]));
    assert_eq!(preview.modifiers["clearance"], 2);
    assert_eq!(preview.inventory_spent, 8);
    assert_eq!(preview.feature_ids, ["quiet_step", "tide_memory"]);
    assert_eq!(preview.inventory_ids, ["line_launcher", "wax_tablet"]);
    assert_eq!(preview.initial_state.entropy.cursor, 4);
    assert_eq!(
        FreehackCreationPreview::from_json(&preview.to_json().unwrap()).unwrap(),
        preview
    );
    assert_eq!(
        FreehackCreationPreview::from_ron(&preview.to_ron().unwrap()).unwrap(),
        preview
    );
}

#[test]
fn probability_is_exact_and_rejects_invalid_domains() {
    let request = FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: vec![contribution("support", 3)],
        opposition: vec![contribution("opposition", 4)],
    };
    let preview = freehack_probability_preview(&request).unwrap();
    assert_eq!(preview.favorable_weight, 9);
    assert_eq!(preview.unfavorable_weight, 16);
    assert_eq!(preview.total_weight, 25);
    assert_eq!(preview.success_basis_points_floor, 3_600);

    let boundary = FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: vec![contribution("support", 1_000_000)],
        opposition: vec![contribution("opposition", 1_000_000)],
    };
    let boundary = freehack_probability_preview(&boundary).unwrap();
    assert_eq!(boundary.favorable_weight, 1_000_000_000_000);
    assert_eq!(boundary.unfavorable_weight, 1_000_000_000_000);
    assert_eq!(boundary.total_weight, 2_000_000_000_000);
    assert_eq!(boundary.success_basis_points_floor, 5_000);

    let oversized = FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: vec![
            contribution("first", 600_000),
            contribution("second", 600_000),
        ],
        opposition: vec![contribution("opposition", 1)],
    };
    assert!(freehack_probability_preview(&oversized).is_err());
}

#[test]
fn visible_and_hidden_checks_replay_without_crossing_the_public_boundary() {
    let preview = create_freehack_character(&creation_request()).unwrap();
    let before = preview.initial_state.clone();
    let mut registry = ResolverRegistry::new();
    registry
        .register(&freehack_manifest(), FreehackResolver)
        .unwrap();

    let (visible_request, visible) = execute(
        &registry,
        &preview,
        &before,
        "org.weave.request.freehack_visible",
        "resolve_check",
        TabletopCapability::ChecksAndConflicts,
        check_input(
            "Cross the wet gantry",
            "sealed_current",
            "Sealed Current",
            "public",
        ),
    );
    registry
        .replay(&freehack_manifest(), &visible_request, &before, &visible)
        .unwrap();
    assert_eq!(visible.entropy_consumed, 1);
    let authority = project_freehack_authority_receipt(&visible).unwrap();
    let authority_json = authority.to_json().unwrap();
    assert!(authority_json.contains("Sealed Current"));
    assert!(authority_json.contains("draw_index"));

    let public = project_freehack_public_receipt(&visible).unwrap().unwrap();
    let public_json = public.to_json().unwrap();
    assert!(public_json.contains("Cross the wet gantry"));
    assert!(public_json.contains("support_total"));
    assert!(!public_json.contains("Sealed Current"));
    assert!(!public_json.contains("sealed_current"));
    assert!(!public_json.contains("opposition_total"));
    assert!(!public_json.contains("draw_index"));
    assert!(!public_json.contains("entropy"));
    assert!(!public_json.contains("request_sha256"));
    assert!(!public_json.contains("revision"));
    assert_eq!(public.events[0].sequence, 0);

    let (_, hidden) = execute(
        &registry,
        &preview,
        &visible.after_state,
        "org.weave.request.freehack_hidden",
        "resolve_check",
        TabletopCapability::ChecksAndConflicts,
        check_input(
            "Notice a hidden signal",
            "secret_opposition",
            "Secret Opposition Marker",
            "host_only",
        ),
    );
    assert!(project_freehack_public_receipt(&hidden).unwrap().is_none());
    let hidden_authority = project_freehack_authority_receipt(&hidden)
        .unwrap()
        .to_json()
        .unwrap();
    assert!(hidden_authority.contains("Secret Opposition Marker"));
}

#[test]
fn invalid_host_only_inputs_do_not_echo_secret_values_in_diagnostics() {
    let preview = create_freehack_character(&creation_request()).unwrap();
    let mut registry = ResolverRegistry::new();
    registry
        .register(&freehack_manifest(), FreehackResolver)
        .unwrap();
    let secret_marker = format!("PRIVATE_OPPOSITION_MARKER_{}", "x".repeat(256));
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "redacted_diagnostic".to_owned(),
        adapter: preview.adapter.clone(),
        operation: "resolve_check".to_owned(),
        capability: TabletopCapability::ChecksAndConflicts,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
        input: check_input(
            "Private diagnostic check",
            "secret_opposition",
            &secret_marker,
            "host_only",
        ),
    };
    let error = registry
        .execute(&freehack_manifest(), &request, &preview.initial_state)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("PRIVATE_OPPOSITION_MARKER"));
    assert!(!error.contains(&secret_marker));
}

#[test]
fn simultaneous_sections_save_cancel_order_resolve_and_timeout_deterministically() {
    let preview = create_freehack_character(&creation_request()).unwrap();
    let mut registry = ResolverRegistry::new();
    registry
        .register(&freehack_manifest(), FreehackResolver)
        .unwrap();
    let (_, opened) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "org.weave.request.section_open",
        "open_section",
        TabletopCapability::Scenes,
        value_object([
            ("abstraction", DomainValue::Symbol("objective".to_owned())),
            ("id", DomainValue::String("gantry_turn".to_owned())),
            ("label", DomainValue::String("Gantry Turn".to_owned())),
            ("mapping", DomainValue::Symbol("mapped".to_owned())),
            (
                "participants",
                DomainValue::List(vec![
                    DomainValue::String("ira".to_owned()),
                    DomainValue::String("moss".to_owned()),
                ]),
            ),
            ("reveal_after_resolution", DomainValue::Bool(false)),
            ("timing", DomainValue::Symbol("timed".to_owned())),
            ("turn_ticks", DomainValue::Number(30.0)),
        ]),
    );
    let (_, first_submission) = execute(
        &registry,
        &preview,
        &opened.after_state,
        "org.weave.request.section_submit_ira",
        "submit_action",
        TabletopCapability::Scenes,
        value_object([
            (
                "action",
                DomainValue::String("Private action marker: secure the west cable.".to_owned()),
            ),
            (
                "map_intent",
                DomainValue::String("Move to west anchor.".to_owned()),
            ),
            ("participant_id", DomainValue::String("ira".to_owned())),
            ("section_id", DomainValue::String("gantry_turn".to_owned())),
            ("submission_id", DomainValue::String("ira_first".to_owned())),
        ]),
    );
    let first_public = project_freehack_public_receipt(&first_submission)
        .unwrap()
        .unwrap()
        .to_json()
        .unwrap();
    assert!(!first_public.contains("Private action marker"));
    assert!(!first_public.contains("ira_first"));
    assert!(!first_public.contains("west anchor"));
    let restored_json =
        TabletopState::from_json(&first_submission.after_state.to_json().unwrap()).unwrap();
    let restored_ron =
        TabletopState::from_ron(&first_submission.after_state.to_ron().unwrap()).unwrap();
    assert_eq!(restored_json, first_submission.after_state);
    assert_eq!(restored_ron, first_submission.after_state);

    let (_, cancelled) = execute(
        &registry,
        &preview,
        &restored_json,
        "org.weave.request.section_cancel_ira",
        "cancel_submission",
        TabletopCapability::Scenes,
        value_object([
            ("participant_id", DomainValue::String("ira".to_owned())),
            ("section_id", DomainValue::String("gantry_turn".to_owned())),
        ]),
    );
    let (_, ira_resubmitted) = execute(
        &registry,
        &preview,
        &cancelled.after_state,
        "org.weave.request.section_resubmit_ira",
        "submit_action",
        TabletopCapability::Scenes,
        value_object([
            (
                "action",
                DomainValue::String("Brace the east cable instead.".to_owned()),
            ),
            (
                "map_intent",
                DomainValue::String("Move to east anchor.".to_owned()),
            ),
            ("participant_id", DomainValue::String("ira".to_owned())),
            ("section_id", DomainValue::String("gantry_turn".to_owned())),
            (
                "submission_id",
                DomainValue::String("ira_second".to_owned()),
            ),
        ]),
    );
    let (_, all_submitted) = execute(
        &registry,
        &preview,
        &ira_resubmitted.after_state,
        "org.weave.request.section_submit_moss",
        "submit_action",
        TabletopCapability::Scenes,
        value_object([
            (
                "action",
                DomainValue::String("Lower a counterweight.".to_owned()),
            ),
            (
                "map_intent",
                DomainValue::String("Remain by the central winch.".to_owned()),
            ),
            ("participant_id", DomainValue::String("moss".to_owned())),
            ("section_id", DomainValue::String("gantry_turn".to_owned())),
            (
                "submission_id",
                DomainValue::String("moss_first".to_owned()),
            ),
        ]),
    );
    let (_, resolved) = execute(
        &registry,
        &preview,
        &all_submitted.after_state,
        "org.weave.request.section_resolve",
        "resolve_section",
        TabletopCapability::Scenes,
        value_object([
            (
                "outcomes",
                DomainValue::List(vec![
                    value_object([
                        (
                            "submission_id",
                            DomainValue::String("moss_first".to_owned()),
                        ),
                        (
                            "summary",
                            DomainValue::String("The counterweight settles.".to_owned()),
                        ),
                    ]),
                    value_object([
                        (
                            "submission_id",
                            DomainValue::String("ira_second".to_owned()),
                        ),
                        (
                            "summary",
                            DomainValue::String("The east cable holds.".to_owned()),
                        ),
                    ]),
                ]),
            ),
            (
                "public_summary",
                DomainValue::String("The gantry stabilizes.".to_owned()),
            ),
            ("section_id", DomainValue::String("gantry_turn".to_owned())),
        ]),
    );
    let public = project_freehack_public_receipt(&resolved).unwrap().unwrap();
    let section = &public.public_state.sections[0];
    assert_eq!(section.resolved_order, ["ira", "moss"]);
    assert!(section.revealed_submissions.is_empty());
    let public_json = public.to_json().unwrap();
    assert!(public_json.contains("The gantry stabilizes"));
    assert!(!public_json.contains("east cable"));
    assert!(!public_json.contains("counterweight"));

    let (_, timeout_opened) = execute(
        &registry,
        &preview,
        &resolved.after_state,
        "org.weave.request.timeout_open",
        "open_section",
        TabletopCapability::Scenes,
        value_object([
            ("abstraction", DomainValue::Symbol("strategic".to_owned())),
            ("id", DomainValue::String("route_turn".to_owned())),
            ("label", DomainValue::String("Route Turn".to_owned())),
            ("mapping", DomainValue::Symbol("unmapped".to_owned())),
            (
                "participants",
                DomainValue::List(vec![
                    DomainValue::String("ira".to_owned()),
                    DomainValue::String("moss".to_owned()),
                ]),
            ),
            ("reveal_after_resolution", DomainValue::Bool(false)),
            ("timing", DomainValue::Symbol("timed".to_owned())),
            ("turn_ticks", DomainValue::Number(5.0)),
        ]),
    );
    let (_, timed_out) = execute(
        &registry,
        &preview,
        &timeout_opened.after_state,
        "org.weave.request.timeout_apply",
        "timeout_section",
        TabletopCapability::Scenes,
        value_object([
            ("elapsed_ticks", DomainValue::Number(5.0)),
            ("section_id", DomainValue::String("route_turn".to_owned())),
        ]),
    );
    let timeout_public = project_freehack_public_receipt(&timed_out)
        .unwrap()
        .unwrap()
        .to_json()
        .unwrap();
    weave_tabletop::validate_resolution_receipt(&timed_out, &freehack_manifest()).unwrap();
    assert!(timeout_public.contains("ira"));
    assert!(timeout_public.contains("moss"));
    assert!(timeout_public.contains("timed_out"));
}

#[test]
fn every_section_timing_mapping_and_abstraction_mode_round_trips_and_resolves() {
    let preview = create_freehack_character(&creation_request()).unwrap();
    let mut registry = ResolverRegistry::new();
    registry
        .register(&freehack_manifest(), FreehackResolver)
        .unwrap();
    let timings = [
        ("timed", FreehackSectionTiming::Timed),
        ("untimed", FreehackSectionTiming::Untimed),
    ];
    let mappings = [
        ("mapped", FreehackSectionMapping::Mapped),
        ("unmapped", FreehackSectionMapping::Unmapped),
    ];
    let abstractions = [
        ("automatic", FreehackSectionAbstraction::Automatic),
        ("objective", FreehackSectionAbstraction::Objective),
        ("strategic", FreehackSectionAbstraction::Strategic),
    ];

    for (timing, expected_timing) in timings {
        for (mapping, expected_mapping) in mappings {
            for (abstraction, expected_abstraction) in abstractions {
                let section_id = format!("{timing}_{mapping}_{abstraction}");
                let (_, opened) = execute(
                    &registry,
                    &preview,
                    &preview.initial_state,
                    &format!("org.weave.request.{section_id}_open"),
                    "open_section",
                    TabletopCapability::Scenes,
                    section_open_input(&section_id, timing, mapping, abstraction),
                );
                let (_, submitted_second) = execute(
                    &registry,
                    &preview,
                    &opened.after_state,
                    &format!("org.weave.request.{section_id}_submit_second"),
                    "submit_action",
                    TabletopCapability::Scenes,
                    section_submission_input(&section_id, "second", "second_submission", mapping),
                );
                let restored_json =
                    TabletopState::from_json(&submitted_second.after_state.to_json().unwrap())
                        .unwrap();
                let restored_ron =
                    TabletopState::from_ron(&submitted_second.after_state.to_ron().unwrap())
                        .unwrap();
                assert_eq!(restored_json, submitted_second.after_state);
                assert_eq!(restored_ron, submitted_second.after_state);
                let (_, cancelled) = execute(
                    &registry,
                    &preview,
                    &restored_json,
                    &format!("org.weave.request.{section_id}_cancel_second"),
                    "cancel_submission",
                    TabletopCapability::Scenes,
                    value_object([
                        ("participant_id", DomainValue::String("second".to_owned())),
                        ("section_id", DomainValue::String(section_id.clone())),
                    ]),
                );
                let (_, resubmitted_second) = execute(
                    &registry,
                    &preview,
                    &cancelled.after_state,
                    &format!("org.weave.request.{section_id}_resubmit_second"),
                    "submit_action",
                    TabletopCapability::Scenes,
                    section_submission_input(&section_id, "second", "second_submission", mapping),
                );
                let (_, all_submitted) = execute(
                    &registry,
                    &preview,
                    &resubmitted_second.after_state,
                    &format!("org.weave.request.{section_id}_submit_first"),
                    "submit_action",
                    TabletopCapability::Scenes,
                    section_submission_input(&section_id, "first", "first_submission", mapping),
                );
                let (_, resolved) = execute(
                    &registry,
                    &preview,
                    &all_submitted.after_state,
                    &format!("org.weave.request.{section_id}_resolve"),
                    "resolve_section",
                    TabletopCapability::Scenes,
                    value_object([
                        (
                            "outcomes",
                            DomainValue::List(vec![
                                value_object([
                                    (
                                        "submission_id",
                                        DomainValue::String("second_submission".to_owned()),
                                    ),
                                    ("summary", DomainValue::String("Second outcome.".to_owned())),
                                ]),
                                value_object([
                                    (
                                        "submission_id",
                                        DomainValue::String("first_submission".to_owned()),
                                    ),
                                    ("summary", DomainValue::String("First outcome.".to_owned())),
                                ]),
                            ]),
                        ),
                        (
                            "public_summary",
                            DomainValue::String("The section resolves.".to_owned()),
                        ),
                        ("section_id", DomainValue::String(section_id.clone())),
                    ]),
                );
                let public = project_freehack_public_receipt(&resolved).unwrap().unwrap();
                let section = public
                    .public_state
                    .sections
                    .iter()
                    .find(|section| section.id == section_id)
                    .unwrap();
                assert_eq!(section.timing, expected_timing);
                assert_eq!(section.mapping, expected_mapping);
                assert_eq!(section.abstraction, expected_abstraction);
                assert_eq!(section.status, FreehackSectionStatus::Resolved);
                assert_eq!(section.resolved_order, ["first", "second"]);

                if timing == "timed" {
                    let timeout_id = format!("{section_id}_timeout");
                    let (_, timeout_opened) = execute(
                        &registry,
                        &preview,
                        &preview.initial_state,
                        &format!("org.weave.request.{timeout_id}_open"),
                        "open_section",
                        TabletopCapability::Scenes,
                        section_open_input(&timeout_id, timing, mapping, abstraction),
                    );
                    let (_, timeout_submitted) = execute(
                        &registry,
                        &preview,
                        &timeout_opened.after_state,
                        &format!("org.weave.request.{timeout_id}_submit"),
                        "submit_action",
                        TabletopCapability::Scenes,
                        section_submission_input(
                            &timeout_id,
                            "first",
                            "timeout_submission",
                            mapping,
                        ),
                    );
                    let (_, timed_out) = execute(
                        &registry,
                        &preview,
                        &timeout_submitted.after_state,
                        &format!("org.weave.request.{timeout_id}_apply"),
                        "timeout_section",
                        TabletopCapability::Scenes,
                        value_object([
                            ("elapsed_ticks", DomainValue::Number(8.0)),
                            ("section_id", DomainValue::String(timeout_id.clone())),
                        ]),
                    );
                    let timeout_public = project_freehack_public_receipt(&timed_out)
                        .unwrap()
                        .unwrap();
                    let timeout_section = timeout_public
                        .public_state
                        .sections
                        .iter()
                        .find(|section| section.id == timeout_id)
                        .unwrap();
                    assert_eq!(timeout_section.status, FreehackSectionStatus::TimedOut);
                    assert_eq!(timeout_section.resolved_order, ["first"]);
                }
            }
        }
    }
}

#[test]
fn secret_tracks_and_memories_are_absent_from_public_state_and_events() {
    let preview = create_freehack_character(&creation_request()).unwrap();
    let initial_public = project_freehack_public_state(&preview.initial_state).unwrap();
    assert_eq!(initial_public.tracks.len(), 1);
    assert_eq!(initial_public.tracks[0].id, "fatigue");
    assert_eq!(initial_public.memories.len(), 1);
    assert_eq!(initial_public.memories[0].id, "old_channel");
    let mut registry = ResolverRegistry::new();
    registry
        .register(&freehack_manifest(), FreehackResolver)
        .unwrap();
    let (_, secret_track) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "org.weave.request.secret_track",
        "advance_track",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            ("outcome", DomainValue::Symbol("success".to_owned())),
            ("track_id", DomainValue::String("hidden_watch".to_owned())),
        ]),
    );
    assert!(
        project_freehack_public_receipt(&secret_track)
            .unwrap()
            .is_none()
    );
    let secret_public_state = project_freehack_public_state(&secret_track.after_state)
        .unwrap()
        .to_json()
        .unwrap();
    assert!(!secret_public_state.contains("hidden_watch"));
    assert!(!secret_public_state.contains("unseen observer"));

    let (_, hidden_memory) = execute(
        &registry,
        &preview,
        &secret_track.after_state,
        "org.weave.request.secret_memory",
        "recall_memory",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            ("connected", DomainValue::Bool(true)),
            ("disclosure", DomainValue::Symbol("host_only".to_owned())),
            ("id", DomainValue::String("private_signal".to_owned())),
            ("obscurity", DomainValue::Number(7.0)),
            (
                "topic",
                DomainValue::String("Private memory marker".to_owned()),
            ),
        ]),
    );
    assert!(
        project_freehack_public_receipt(&hidden_memory)
            .unwrap()
            .is_none()
    );
    let hidden_public_state = project_freehack_public_state(&hidden_memory.after_state)
        .unwrap()
        .to_json()
        .unwrap();
    assert!(!hidden_public_state.contains("private_signal"));
    assert!(!hidden_public_state.contains("Private memory marker"));

    let (_, public_memory) = execute(
        &registry,
        &preview,
        &hidden_memory.after_state,
        "org.weave.request.public_memory",
        "recall_memory",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            ("connected", DomainValue::Bool(false)),
            ("disclosure", DomainValue::Symbol("public".to_owned())),
            ("id", DomainValue::String("public_route".to_owned())),
            ("obscurity", DomainValue::Number(5.0)),
            (
                "topic",
                DomainValue::String("a public ferry route".to_owned()),
            ),
        ]),
    );
    let public = project_freehack_public_receipt(&public_memory)
        .unwrap()
        .unwrap();
    assert!(
        public
            .public_state
            .memories
            .iter()
            .any(|memory| memory.id == "public_route")
    );
    assert_eq!(public_memory.entropy_consumed, 1);
}
