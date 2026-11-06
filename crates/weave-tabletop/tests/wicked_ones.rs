use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    EntropyStream, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest, ResolverRegistry,
    SOURCE_RECORD, TabletopCapability, TabletopError, TabletopResolver, TabletopState,
    WICKED_ONES_CREATION_FORMAT_VERSION, WICKED_ONES_DOCUMENTATION, WICKED_ONES_LICENSE,
    WICKED_ONES_SOURCE_EDITION, WICKED_ONES_SOURCE_RECORD_SHA256, WICKED_ONES_SOURCE_URL,
    WickedOnesActionGroupTotals, WickedOnesActionRatings, WickedOnesCalling, WickedOnesCampaign,
    WickedOnesCreationPreview, WickedOnesCreationRequest, WickedOnesGearItem,
    WickedOnesGoldenCycle, WickedOnesResolver, WickedOnesVileFriend, canonical_fingerprint,
    create_wicked_ones_character, sha256_bytes, validate_adapter_manifest,
    validate_wicked_ones_creation_preview, validate_wicked_ones_creation_preview_with_request,
    wicked_ones_default_campaign, wicked_ones_editor_views, wicked_ones_forbidden_artifact,
    wicked_ones_golden_campaign, wicked_ones_golden_cycle, wicked_ones_manifest,
    wicked_ones_source_url_allowed,
};

fn creation_request() -> WickedOnesCreationRequest {
    WickedOnesCreationRequest {
        creation_format_version: WICKED_ONES_CREATION_FORMAT_VERSION,
        character_id: "org.weave.wicked_ones.fixture".to_owned(),
        name: "Marrow Quill".to_owned(),
        race: "Emberkin".to_owned(),
        look: "Ash-grey horns and a patched coat".to_owned(),
        calling: WickedOnesCalling::Whisperer,
        actions: WickedOnesActionRatings {
            analyze: 1,
            invent: 0,
            perceive: 1,
            brawl: 1,
            demolish: 0,
            sneak: 2,
            command: 0,
            persuade: 1,
            endure: 0,
        },
        dark_impulse: "Collect every hidden name".to_owned(),
        revelry: "Counting stolen coins by candlelight".to_owned(),
        gear: vec![
            WickedOnesGearItem {
                id: "chalk_knife".to_owned(),
                label: "Chalk Knife".to_owned(),
                quantity: 1,
            },
            WickedOnesGearItem {
                id: "coil_of_rope".to_owned(),
                label: "Coil of Rope".to_owned(),
                quantity: 2,
            },
        ],
        abilities: vec!["silent_step".to_owned(), "silver_tongue".to_owned()],
        vile_friend: WickedOnesVileFriend {
            name: "Grist".to_owned(),
            detail: "A patient tunnel rat that remembers grudges".to_owned(),
        },
        starting_resources: 3,
        seed: 42,
    }
}

fn preview_with_seed(seed: u64) -> WickedOnesCreationPreview {
    let mut request = creation_request();
    request.seed = seed;
    create_wicked_ones_character(&request).unwrap()
}

fn value_object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn action_input(
    approach: &str,
    action: &str,
    position: &str,
    effect: &str,
    teamwork: &[(&str, u32)],
    going_hard: bool,
    resistance: Option<(&str, u32)>,
) -> DomainValue {
    let teamwork_value = DomainValue::List(
        teamwork
            .iter()
            .map(|(helper_id, dice)| {
                value_object([
                    ("helper_id", DomainValue::String((*helper_id).to_owned())),
                    ("dice", DomainValue::Number(f64::from(*dice))),
                ])
            })
            .collect(),
    );
    let mut fields = BTreeMap::from([
        ("action".to_owned(), DomainValue::Symbol(action.to_owned())),
        (
            "approach".to_owned(),
            DomainValue::Symbol(approach.to_owned()),
        ),
        ("bonus".to_owned(), DomainValue::Number(0.0)),
        ("effect".to_owned(), DomainValue::Symbol(effect.to_owned())),
        ("going_hard".to_owned(), DomainValue::Bool(going_hard)),
        (
            "intention".to_owned(),
            DomainValue::String("Slip past the ledger room".to_owned()),
        ),
        (
            "position".to_owned(),
            DomainValue::Symbol(position.to_owned()),
        ),
        ("teamwork".to_owned(), teamwork_value),
    ]);
    if let Some((resistance_action, bonus)) = resistance {
        fields.insert(
            "resistance".to_owned(),
            value_object([
                ("action", DomainValue::Symbol(resistance_action.to_owned())),
                ("bonus", DomainValue::Number(f64::from(bonus))),
            ]),
        );
    }
    DomainValue::Object(fields)
}

fn execute(
    registry: &ResolverRegistry,
    preview: &WickedOnesCreationPreview,
    state: &TabletopState,
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> (ResolutionRequest, ResolutionReceipt) {
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: request_id.to_owned(),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state).unwrap(),
        input,
    };
    let receipt = registry
        .execute(&wicked_ones_manifest(), &request, state)
        .unwrap();
    (request, receipt)
}

fn request_for(
    preview: &WickedOnesCreationPreview,
    state: &TabletopState,
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> ResolutionRequest {
    ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: request_id.to_owned(),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state).unwrap(),
        input,
    }
}

fn campaign_input(campaign: &WickedOnesCampaign) -> DomainValue {
    value_object([("campaign", campaign.to_domain_value())])
}

fn state_campaign(state: &TabletopState) -> WickedOnesCampaign {
    let DomainValue::Object(fields) = &state.value else {
        panic!("state is not an object");
    };
    WickedOnesCampaign::from_domain_value(&fields["campaign"]).unwrap()
}

fn phase_input(next_phase: &str, note: &str) -> DomainValue {
    value_object([
        ("next_phase", DomainValue::Symbol(next_phase.to_owned())),
        ("note", DomainValue::String(note.to_owned())),
    ])
}

fn payload<'a>(receipt: &'a ResolutionReceipt, kind: &str) -> &'a DomainValue {
    receipt
        .events
        .iter()
        .find(|event| event.kind == kind)
        .and_then(|event| event.payload.as_ref())
        .unwrap()
}

fn payload_u32(payload: &DomainValue, field: &str) -> u32 {
    let DomainValue::Object(fields) = payload else {
        panic!("payload is not an object");
    };
    let DomainValue::Number(value) = &fields[field] else {
        panic!("field is not a number");
    };
    u32::try_from(*value as u64).unwrap()
}

fn payload_text<'a>(payload: &'a DomainValue, field: &str) -> &'a str {
    let DomainValue::Object(fields) = payload else {
        panic!("payload is not an object");
    };
    match &fields[field] {
        DomainValue::String(value) | DomainValue::Symbol(value) => value,
        _ => panic!("field is not text"),
    }
}

fn payload_bool(payload: &DomainValue, field: &str) -> bool {
    let DomainValue::Object(fields) = payload else {
        panic!("payload is not an object");
    };
    match &fields[field] {
        DomainValue::Bool(value) => *value,
        _ => panic!("field is not a boolean"),
    }
}

fn payload_list_len(payload: &DomainValue, field: &str) -> usize {
    let DomainValue::Object(fields) = payload else {
        panic!("payload is not an object");
    };
    match &fields[field] {
        DomainValue::List(values) => values.len(),
        _ => panic!("field is not a list"),
    }
}

fn payload_numbers(payload: &DomainValue, field: &str) -> Vec<u32> {
    let DomainValue::Object(fields) = payload else {
        panic!("payload is not an object");
    };
    let DomainValue::List(values) = &fields[field] else {
        panic!("field is not a list");
    };
    values
        .iter()
        .map(|value| match value {
            DomainValue::Number(number) => *number as u32,
            _ => panic!("list item is not a number"),
        })
        .collect()
}

fn dice_band(dice: &[u32]) -> &'static str {
    let sixes = dice.iter().filter(|value| **value == 6).count();
    let best = dice.iter().copied().min().unwrap_or(1);
    if sixes >= 2 {
        "critical"
    } else if best == 6 {
        "success"
    } else if best >= 4 {
        "mixed"
    } else {
        "failure"
    }
}

#[test]
fn manifest_pins_the_checked_in_public_source_record() {
    let manifest = wicked_ones_manifest();
    validate_adapter_manifest(&manifest).unwrap();
    assert_eq!(manifest.provenance.source_url, WICKED_ONES_SOURCE_URL);
    assert_eq!(manifest.provenance.revision, WICKED_ONES_SOURCE_EDITION);
    assert_eq!(manifest.provenance.license, WICKED_ONES_LICENSE);
    assert_eq!(manifest.provenance.sha256, WICKED_ONES_SOURCE_RECORD_SHA256);
    assert_eq!(
        sha256_bytes(SOURCE_RECORD.as_bytes()),
        WICKED_ONES_SOURCE_RECORD_SHA256
    );
    assert_eq!(WICKED_ONES_SOURCE_RECORD_SHA256.len(), 64);
    assert!(SOURCE_RECORD.contains(WICKED_ONES_SOURCE_URL));
    assert!(SOURCE_RECORD.contains(WICKED_ONES_SOURCE_EDITION));
    assert!(SOURCE_RECORD.contains(WICKED_ONES_LICENSE));
    assert!(
        manifest
            .provenance
            .notices
            .iter()
            .any(|notice| notice.contains("War for the Overworld"))
    );
    assert!(
        manifest
            .provenance
            .notices
            .iter()
            .any(|notice| notice.contains("Undead Awakening"))
    );
}

#[test]
fn creation_round_trips_every_choice_and_is_reproducible() {
    let request = creation_request();
    assert_eq!(
        WickedOnesCreationRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        WickedOnesCreationRequest::from_ron(&request.to_ron().unwrap()).unwrap(),
        request
    );

    let first = create_wicked_ones_character(&request).unwrap();
    let second = create_wicked_ones_character(&request).unwrap();
    assert_eq!(first, second);
    validate_wicked_ones_creation_preview(&first).unwrap();
    validate_wicked_ones_creation_preview_with_request(&first, &request).unwrap();
    let mut mismatched_request = request.clone();
    mismatched_request.seed = mismatched_request.seed.saturating_add(1);
    assert!(
        validate_wicked_ones_creation_preview_with_request(&first, &mismatched_request).is_err()
    );
    assert_eq!(
        WickedOnesCreationPreview::from_json(&first.to_json().unwrap()).unwrap(),
        first
    );
    assert_eq!(
        WickedOnesCreationPreview::from_ron(&first.to_ron().unwrap()).unwrap(),
        first
    );

    assert_eq!(first.action_group_totals.brains, 2);
    assert_eq!(first.action_group_totals.muscles, 3);
    assert_eq!(first.action_group_totals.guts, 1);
    assert_eq!(first.ability_labels, ["Silent Step", "Silver Tongue"]);

    let DomainValue::Object(definition) = &first.definition.definition else {
        panic!("definition is not an object");
    };
    assert_eq!(
        definition["calling"],
        DomainValue::Symbol("whisperer".to_owned())
    );
    assert_eq!(definition["starting_resources"], DomainValue::Number(3.0));
    let DomainValue::Object(actions) = &definition["actions"] else {
        panic!("actions are not an object");
    };
    assert_eq!(actions["sneak"], DomainValue::Number(2.0));
    assert_eq!(actions["endure"], DomainValue::Number(0.0));
    assert_eq!(
        definition["vile_friend"],
        value_object([
            (
                "detail",
                DomainValue::String("A patient tunnel rat that remembers grudges".to_owned())
            ),
            ("name", DomainValue::String("Grist".to_owned())),
        ])
    );
    assert_eq!(
        definition["gear"],
        DomainValue::List(vec![
            value_object([
                ("id", DomainValue::String("chalk_knife".to_owned())),
                ("label", DomainValue::String("Chalk Knife".to_owned())),
                ("quantity", DomainValue::Number(1.0)),
            ]),
            value_object([
                ("id", DomainValue::String("coil_of_rope".to_owned())),
                ("label", DomainValue::String("Coil of Rope".to_owned())),
                ("quantity", DomainValue::Number(2.0)),
            ]),
        ])
    );

    let DomainValue::Object(state) = &first.initial_state.value else {
        panic!("state is not an object");
    };
    assert_eq!(state["stress"], DomainValue::Number(0.0));
    assert_eq!(state["shock"], DomainValue::Number(0.0));
    assert_eq!(state["dark_hearts"], DomainValue::Number(0.0));
    assert_eq!(state["resources"], DomainValue::Number(3.0));
    assert_eq!(state["condition"], DomainValue::Symbol("steady".to_owned()));
}

#[test]
fn preview_validation_enforces_action_allocation_invariants() {
    let mut preview = create_wicked_ones_character(&creation_request()).unwrap();
    let DomainValue::Object(definition) = &mut preview.definition.definition else {
        panic!("definition is not an object");
    };
    let Some(DomainValue::Object(actions)) = definition.get_mut("actions") else {
        panic!("actions are not an object");
    };
    for value in actions.values_mut() {
        *value = DomainValue::Number(0.0);
    }
    preview.action_group_totals = WickedOnesActionGroupTotals {
        brains: 0,
        muscles: 0,
        guts: 0,
    };
    let definition_sha256 = canonical_fingerprint(&preview.definition.definition).unwrap();
    preview.definition.definition_sha256 = definition_sha256.clone();
    preview.initial_state.definition_sha256 = definition_sha256;
    let error = validate_wicked_ones_creation_preview(&preview)
        .unwrap_err()
        .to_string();
    assert!(error.contains("exactly six points"), "{error}");
}

#[test]
fn resolver_rejects_stored_resistance_rating_above_maximum() {
    let preview = preview_with_seed(1);
    let mut definition = preview.definition.definition.clone();
    let DomainValue::Object(fields) = &mut definition else {
        panic!("definition is not an object");
    };
    let Some(DomainValue::Object(actions)) = fields.get_mut("actions") else {
        panic!("actions are not an object");
    };
    actions.insert("endure".to_owned(), DomainValue::Number(4.0));
    let input = action_input(
        "force",
        "brawl",
        "risky",
        "standard",
        &[],
        false,
        Some(("endure", 0)),
    );
    let mut entropy = EntropyStream::from_state(preview.initial_state.entropy.clone()).unwrap();
    let error = WickedOnesResolver
        .resolve(
            "resolve_action",
            &definition,
            &input,
            &preview.initial_state.value,
            &mut entropy,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("resistance action rating"), "{error}");
}

#[test]
fn teamwork_going_hard_and_resistance_resolve_and_replay() {
    let preview = preview_with_seed(7);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();

    let mut input = action_input(
        "force",
        "brawl",
        "risky",
        "standard",
        &[("grist", 1), ("cinder", 1)],
        true,
        Some(("endure", 1)),
    );
    if let DomainValue::Object(fields) = &mut input {
        fields.insert("bonus".to_owned(), DomainValue::Number(1.0));
    }
    let (request, receipt) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "action_teamwork",
        "resolve_action",
        TabletopCapability::ChecksAndConflicts,
        input,
    );
    let resolved = payload(&receipt, "action_resolved");
    assert_eq!(payload_list_len(resolved, "dice"), 5);
    assert_eq!(payload_text(resolved, "position"), "risky");
    assert_eq!(payload_text(resolved, "effect"), "standard");
    assert!(payload_bool(resolved, "going_hard"));
    assert!(payload_u32(resolved, "stress_cost") >= 1);
    assert!(payload_u32(resolved, "stress_after") <= 9);
    assert!(
        ["critical", "failure", "mixed", "success"].contains(&payload_text(resolved, "outcome"))
    );
    assert!(receipt.entropy_consumed >= 5);
    registry
        .replay(
            &wicked_ones_manifest(),
            &request,
            &preview.initial_state,
            &receipt,
        )
        .unwrap();
}

#[test]
fn consequence_and_resistance_are_structured_and_deterministic() {
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let mut found = false;
    for seed in 0..128 {
        let preview = preview_with_seed(seed);
        let (request, receipt) = execute(
            &registry,
            &preview,
            &preview.initial_state,
            "action_consequence",
            "resolve_action",
            TabletopCapability::ChecksAndConflicts,
            action_input("force", "brawl", "desperate", "standard", &[], false, None),
        );
        let resolved = payload(&receipt, "action_resolved");
        let outcome = payload_text(resolved, "outcome").to_owned();
        if outcome == "mixed" || outcome == "failure" {
            let baseline = payload_text(resolved, "consequence_severity").to_owned();
            assert_ne!(baseline, "none", "seed {seed} produced a consequence");
            assert!(!payload_bool(resolved, "resisted"));
            registry
                .replay(
                    &wicked_ones_manifest(),
                    &request,
                    &preview.initial_state,
                    &receipt,
                )
                .unwrap();

            let (_, resisted_receipt) = execute(
                &registry,
                &preview,
                &preview.initial_state,
                "action_resisted",
                "resolve_action",
                TabletopCapability::ChecksAndConflicts,
                action_input(
                    "force",
                    "brawl",
                    "desperate",
                    "standard",
                    &[],
                    false,
                    Some(("endure", 0)),
                ),
            );
            let resisted = payload(&resisted_receipt, "action_resolved");
            assert!(payload_bool(resisted, "resisted"));
            assert_ne!(payload_text(resisted, "resistance_outcome"), "none");
            assert!(
                severity_rank(payload_text(resisted, "consequence_severity"))
                    <= severity_rank(&baseline)
            );
            found = true;
            break;
        }
    }
    assert!(found, "a bounded seed scan must find a consequence");
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "minor" => 1,
        "moderate" => 2,
        "severe" => 3,
        _ => 0,
    }
}

#[test]
fn flashback_shock_bloodied_feral_and_death_knell_mutate_state() {
    let preview = preview_with_seed(11);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();

    let (_, flashback) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "flashback_one",
        "flashback",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            (
                "summary",
                DomainValue::String("Prepared a false ledger key".to_owned()),
            ),
            ("stress_cost", DomainValue::Number(2.0)),
        ]),
    );
    let recorded = payload(&flashback, "flashback_recorded");
    assert_eq!(payload_u32(recorded, "stress_after"), 2);
    assert_eq!(payload_u32(recorded, "flashbacks"), 1);

    let (_, dark_heart) = execute(
        &registry,
        &preview,
        &flashback.after_state,
        "dark_heart_one",
        "gain_dark_heart",
        TabletopCapability::ResourcesAndConditions,
        value_object([(
            "reason",
            DomainValue::String("Kept a promise to the wrong person".to_owned()),
        )]),
    );
    assert_eq!(
        payload_u32(payload(&dark_heart, "dark_heart_gained"), "dark_hearts"),
        1
    );

    let (_, partial_shock) = execute(
        &registry,
        &preview,
        &dark_heart.after_state,
        "shock_partial",
        "apply_shock",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            ("amount", DomainValue::Number(2.0)),
            (
                "source",
                DomainValue::String("A collapsing stone stair".to_owned()),
            ),
        ]),
    );
    let partial = payload(&partial_shock, "shock_applied");
    assert_eq!(payload_u32(partial, "shock_after"), 2);
    assert!(!payload_bool(partial, "bloodied"));
    assert_eq!(payload_text(partial, "condition"), "steady");

    let (_, full_shock) = execute(
        &registry,
        &preview,
        &partial_shock.after_state,
        "shock_full",
        "apply_shock",
        TabletopCapability::ResourcesAndConditions,
        value_object([
            ("amount", DomainValue::Number(2.0)),
            (
                "source",
                DomainValue::String("A second collapsing stair".to_owned()),
            ),
        ]),
    );
    assert_eq!(full_shock.events.len(), 2);
    assert_eq!(full_shock.events[0].kind, "condition_changed");
    assert_eq!(full_shock.events[1].kind, "shock_applied");
    let bloodied = payload(&full_shock, "shock_applied");
    assert_eq!(payload_u32(bloodied, "shock_after"), 4);
    assert!(payload_bool(bloodied, "bloodied"));
    assert_eq!(payload_text(bloodied, "condition"), "bloodied");

    let (_, feral) = execute(
        &registry,
        &preview,
        &full_shock.after_state,
        "go_feral_one",
        "go_feral",
        TabletopCapability::ResourcesAndConditions,
        value_object([(
            "trigger",
            DomainValue::String("The hoard was threatened".to_owned()),
        )]),
    );
    let transition = payload(&feral, "condition_changed");
    assert_eq!(payload_text(transition, "from"), "bloodied");
    assert_eq!(payload_text(transition, "to"), "feral");

    let restored_json = TabletopState::from_json(&feral.after_state.to_json().unwrap()).unwrap();
    let restored_ron = TabletopState::from_ron(&feral.after_state.to_ron().unwrap()).unwrap();
    assert_eq!(restored_json, feral.after_state);
    assert_eq!(restored_ron, feral.after_state);

    let (_, knell) = execute(
        &registry,
        &preview,
        &restored_json,
        "death_knell_one",
        "death_knell",
        TabletopCapability::ResourcesAndConditions,
        value_object([(
            "knell",
            DomainValue::String("The ledger burns with the name inside it".to_owned()),
        )]),
    );
    assert_eq!(knell.events.len(), 2);
    assert_eq!(knell.events[0].kind, "condition_changed");
    assert_eq!(knell.events[1].kind, "death_knell_recorded");
    let recorded_knell = payload(&knell, "death_knell_recorded");
    assert_eq!(payload_text(recorded_knell, "condition"), "dead");
    assert_eq!(
        payload_text(recorded_knell, "knell"),
        "The ledger burns with the name inside it"
    );

    let DomainValue::Object(state) = &knell.after_state.value else {
        panic!("state is not an object");
    };
    assert_eq!(state["dead"], DomainValue::Bool(true));
    assert_eq!(state["feral"], DomainValue::Bool(true));
    assert_eq!(state["bloodied"], DomainValue::Bool(true));
    assert_eq!(state["condition"], DomainValue::Symbol("dead".to_owned()));

    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "after_death".to_owned(),
        adapter: preview.adapter.clone(),
        operation: "flashback".to_owned(),
        capability: TabletopCapability::ResourcesAndConditions,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&knell.after_state).unwrap(),
        input: value_object([
            (
                "summary",
                DomainValue::String("Too late for this preparation".to_owned()),
            ),
            ("stress_cost", DomainValue::Number(1.0)),
        ]),
    };
    assert!(
        registry
            .execute(&wicked_ones_manifest(), &request, &knell.after_state)
            .is_err()
    );

    let mut full_registry = ResolverRegistry::new();
    full_registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let fresh = preview_with_seed(12);
    let go_feral_request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "feral_before_bloodied".to_owned(),
        adapter: fresh.adapter.clone(),
        operation: "go_feral".to_owned(),
        capability: TabletopCapability::ResourcesAndConditions,
        definition_sha256: fresh.definition.definition_sha256.clone(),
        definition: fresh.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&fresh.initial_state).unwrap(),
        input: value_object([(
            "trigger",
            DomainValue::String("An early, invalid transition".to_owned()),
        )]),
    };
    assert!(matches!(
        full_registry.execute(
            &wicked_ones_manifest(),
            &go_feral_request,
            &fresh.initial_state
        ),
        Err(TabletopError::InvalidField { .. })
    ));
}

#[test]
fn invalid_action_contexts_fail_closed() {
    let preview = preview_with_seed(3);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let mut input = action_input("force", "brawl", "impossible", "standard", &[], false, None);
    if let DomainValue::Object(fields) = &mut input {
        fields.insert(
            "position".to_owned(),
            DomainValue::Symbol("impossible".to_owned()),
        );
    }
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "invalid_position".to_owned(),
        adapter: preview.adapter.clone(),
        operation: "resolve_action".to_owned(),
        capability: TabletopCapability::ChecksAndConflicts,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
        input,
    };
    assert!(
        registry
            .execute(&wicked_ones_manifest(), &request, &preview.initial_state)
            .is_err()
    );

    let mut invalid_teamwork = action_input(
        "force",
        "brawl",
        "risky",
        "standard",
        &[("grist", 2), ("cinder", 2), ("ash", 1)],
        false,
        None,
    );
    if let DomainValue::Object(fields) = &mut invalid_teamwork {
        fields.insert(
            "intention".to_owned(),
            DomainValue::String("Overreach with too many dice".to_owned()),
        );
    }
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "invalid_teamwork".to_owned(),
        adapter: preview.adapter.clone(),
        operation: "resolve_action".to_owned(),
        capability: TabletopCapability::ChecksAndConflicts,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
        input: invalid_teamwork,
    };
    assert!(
        registry
            .execute(&wicked_ones_manifest(), &request, &preview.initial_state)
            .is_err()
    );
}

fn run_phase_cycle(
    registry: &ResolverRegistry,
    preview: &WickedOnesCreationPreview,
    phases: &[&str],
    pause: bool,
    save: bool,
) -> TabletopState {
    let mut state = preview.initial_state.clone();
    let (_, seeded) = execute(
        registry,
        preview,
        &state,
        "cycle_seed",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&wicked_ones_golden_campaign()),
    );
    state = seeded.after_state;
    for (index, phase) in phases.iter().enumerate() {
        let (_, advanced) = execute(
            registry,
            preview,
            &state,
            &format!("phase_{index}"),
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            phase_input(phase, "advance"),
        );
        state = advanced.after_state;
        if pause {
            let (_, paused) = execute(
                registry,
                preview,
                &state,
                &format!("pause_{index}"),
                "pause_campaign",
                TabletopCapability::CampaignState,
                value_object([("note", DomainValue::String("hold".to_owned()))]),
            );
            let (_, resumed) = execute(
                registry,
                preview,
                &paused.after_state,
                &format!("resume_{index}"),
                "resume_campaign",
                TabletopCapability::CampaignState,
                value_object([("note", DomainValue::String("continue".to_owned()))]),
            );
            state = resumed.after_state;
        }
        if save {
            let json = state.to_json().unwrap();
            state = TabletopState::from_json(&json).unwrap();
            let ron = state.to_ron().unwrap();
            state = TabletopState::from_ron(&ron).unwrap();
        }
    }
    state
}

fn campaign_with_phase(phase: &str) -> WickedOnesCampaign {
    let mut campaign = wicked_ones_golden_campaign();
    campaign.cycle.phase = phase.to_owned();
    campaign
}

fn raid_plan_input(plan: &str, goal: &str) -> DomainValue {
    value_object([
        (
            "advantages",
            DomainValue::List(vec![DomainValue::String("quiet approach".to_owned())]),
        ),
        (
            "disadvantages",
            DomainValue::List(vec![DomainValue::String("armed patrols".to_owned())]),
        ),
        ("goal", DomainValue::String(goal.to_owned())),
        ("label", DomainValue::String("Salt Run".to_owned())),
        ("plan", DomainValue::String(plan.to_owned())),
        ("raid_id", DomainValue::String("salt_run".to_owned())),
        (
            "target_id",
            DomainValue::String("salt_warehouse".to_owned()),
        ),
    ])
}

#[test]
fn campaign_entities_round_trip_and_validate_references() {
    let preview = preview_with_seed(31);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    assert_eq!(
        state_campaign(&preview.initial_state),
        wicked_ones_default_campaign()
    );

    let golden = wicked_ones_golden_campaign();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "campaign_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&golden),
    );
    assert_eq!(state_campaign(&receipt.after_state), golden);

    let json = receipt.after_state.to_json().unwrap();
    let ron = receipt.after_state.to_ron().unwrap();
    assert_eq!(
        TabletopState::from_json(&json).unwrap(),
        receipt.after_state
    );
    assert_eq!(TabletopState::from_ron(&ron).unwrap(), receipt.after_state);

    let cycle = wicked_ones_golden_cycle();
    assert_eq!(
        WickedOnesGoldenCycle::from_json(&cycle.to_json().unwrap()).unwrap(),
        cycle
    );
    assert_eq!(
        WickedOnesGoldenCycle::from_ron(&cycle.to_ron().unwrap()).unwrap(),
        cycle
    );

    let room_ids = golden
        .dungeon
        .rooms
        .iter()
        .map(|room| room.id.as_str())
        .collect::<Vec<_>>();
    let mut sorted = room_ids.clone();
    sorted.sort_unstable();
    assert_eq!(room_ids, sorted);

    let mut broken = golden.clone();
    broken.dungeon.structures[0].room_id = "missing_room".to_owned();
    let request = request_for(
        &preview,
        &preview.initial_state,
        "broken_reference",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&broken),
    );
    let error = registry
        .execute(&wicked_ones_manifest(), &request, &preview.initial_state)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("campaign.dungeon.structures[0].room_id"),
        "{error}"
    );
}

#[test]
fn campaign_cycle_pauses_resumes_and_saves_at_every_boundary() {
    let preview = preview_with_seed(41);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let (_, seeded) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "cycle_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&wicked_ones_golden_campaign()),
    );
    let request = request_for(
        &preview,
        &seeded.after_state,
        "bad_transition",
        "advance_campaign_phase",
        TabletopCapability::CampaignState,
        phase_input("raiding", "skip ahead"),
    );
    let error = registry
        .execute(&wicked_ones_manifest(), &request, &seeded.after_state)
        .unwrap_err()
        .to_string();
    assert!(error.contains("campaign.cycle.phase.recovery"), "{error}");

    let phases = [
        "downtime",
        "dungeon_events",
        "calamity",
        "raiding",
        "blowback",
        "recovery",
        "advancement",
        "complete",
    ];
    let uninterrupted = run_phase_cycle(&registry, &preview, &phases, false, false);
    let paused = run_phase_cycle(&registry, &preview, &phases, true, false);
    let saved = run_phase_cycle(&registry, &preview, &phases, false, true);

    let mut expected = state_campaign(&uninterrupted);
    let mut interrupted = state_campaign(&paused);
    expected.cycle.history.clear();
    interrupted.cycle.history.clear();
    assert_eq!(expected, interrupted);
    assert_eq!(state_campaign(&uninterrupted), state_campaign(&saved));
    assert_eq!(state_campaign(&uninterrupted).cycle.phase, "complete");
}

#[test]
fn raid_engagement_feeds_action_loop_and_blowback_resolves() {
    let preview = preview_with_seed(51);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let (_, seeded) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "raid_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&campaign_with_phase("raiding")),
    );
    let before = seeded.after_state;

    for (request_id, plan, goal, needle) in [
        ("raid_no_plan", "", "Take the ledgers", "declared plan"),
        (
            "raid_no_goal",
            "Enter through the drain",
            "",
            "declared goal",
        ),
    ] {
        let request = request_for(
            &preview,
            &before,
            request_id,
            "plan_raid",
            TabletopCapability::CampaignState,
            raid_plan_input(plan, goal),
        );
        let error = registry
            .execute(&wicked_ones_manifest(), &request, &before)
            .unwrap_err()
            .to_string();
        assert!(error.contains(needle), "{error}");
    }

    let (_, planned) = execute(
        &registry,
        &preview,
        &before,
        "raid_plan",
        "plan_raid",
        TabletopCapability::CampaignState,
        raid_plan_input("Enter through the drain", "Take the salt ledgers"),
    );
    let raid_input = value_object([("raid_id", DomainValue::String("salt_run".to_owned()))]);
    let (_, first_engage) = execute(
        &registry,
        &preview,
        &planned.after_state,
        "raid_engage",
        "engage_raid",
        TabletopCapability::CampaignState,
        raid_input.clone(),
    );
    let (_, second_engage) = execute(
        &registry,
        &preview,
        &planned.after_state,
        "raid_engage",
        "engage_raid",
        TabletopCapability::CampaignState,
        raid_input,
    );
    assert_eq!(first_engage, second_engage);
    let engagement = payload(&first_engage, "raid_engagement_ready");
    let position = payload_text(engagement, "position").to_owned();
    let effect = payload_text(engagement, "effect").to_owned();
    let bonus = payload_u32(engagement, "bonus");
    assert_eq!(position, "desperate");
    assert_eq!(effect, "standard");
    assert_eq!(bonus, 0);

    let mut action = action_input("guile", "sneak", &position, &effect, &[], false, None);
    if let DomainValue::Object(fields) = &mut action {
        fields.insert("bonus".to_owned(), DomainValue::Number(f64::from(bonus)));
    }
    let (_, action_receipt) = execute(
        &registry,
        &preview,
        &first_engage.after_state,
        "raid_action",
        "resolve_action",
        TabletopCapability::ChecksAndConflicts,
        action,
    );
    let resolved = payload(&action_receipt, "action_resolved");
    assert!(
        ["critical", "failure", "mixed", "success"].contains(&payload_text(resolved, "outcome"))
    );

    let (_, outcome) = execute(
        &registry,
        &preview,
        &first_engage.after_state,
        "raid_outcome",
        "resolve_raid_outcome",
        TabletopCapability::CampaignState,
        value_object([
            ("note", DomainValue::String("A costly escape".to_owned())),
            ("outcome", DomainValue::Symbol("mixed".to_owned())),
            ("raid_id", DomainValue::String("salt_run".to_owned())),
        ]),
    );
    let loot = payload(&outcome, "raid_loot_resolved");
    assert!(payload_u32(loot, "loot") > 0);
    let blowback = payload(&outcome, "raid_blowback");
    assert_eq!(payload_u32(blowback, "severity"), 1);
    assert_eq!(payload_text(blowback, "position"), "risky");
    let restored = TabletopState::from_json(&outcome.after_state.to_json().unwrap()).unwrap();
    assert_eq!(restored, outcome.after_state);

    let (_, clock) = execute(
        &registry,
        &preview,
        &restored,
        "raid_clock",
        "adjust_faction_clock",
        TabletopCapability::Factions,
        value_object([
            ("clock_id", DomainValue::String("ledger_audit".to_owned())),
            ("delta", DomainValue::Number(1.0)),
            ("faction_id", DomainValue::String("salt_guild".to_owned())),
            ("note", DomainValue::String("Audit begins".to_owned())),
        ]),
    );
    let clock_payload = payload(&clock, "faction_clock_changed");
    assert_eq!(payload_u32(clock_payload, "from"), 2);
    assert_eq!(payload_u32(clock_payload, "to"), 3);

    let (_, master) = execute(
        &registry,
        &preview,
        &clock.after_state,
        "raid_master_plan",
        "advance_master_plan",
        TabletopCapability::Factions,
        value_object([
            ("delta", DomainValue::Number(2.0)),
            ("note", DomainValue::String("The gate opens".to_owned())),
        ]),
    );
    let master_payload = payload(&master, "master_plan_advanced");
    assert_eq!(payload_u32(master_payload, "from"), 2);
    assert_eq!(payload_u32(master_payload, "to"), 4);
}

fn run_golden_cycle(
    registry: &ResolverRegistry,
    preview: &WickedOnesCreationPreview,
) -> Vec<ResolutionReceipt> {
    let cycle = wicked_ones_golden_cycle();
    let mut state = preview.initial_state.clone();
    let mut receipts = Vec::new();
    for (index, step) in cycle.steps.iter().enumerate() {
        let (request, receipt) = execute(
            registry,
            preview,
            &state,
            &format!("golden_{index}"),
            &step.operation,
            step.capability,
            step.input.clone(),
        );
        registry
            .replay(&wicked_ones_manifest(), &request, &state, &receipt)
            .unwrap();
        state = receipt.after_state.clone();
        receipts.push(receipt);
    }
    receipts
}

#[test]
fn editor_views_golden_cycle_and_exclusion_boundary() {
    let campaign = wicked_ones_golden_campaign();
    let views = wicked_ones_editor_views(&campaign);
    let ids = views
        .iter()
        .map(|view| view.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "dungeon_layout",
            "clocks",
            "factions",
            "projects",
            "minions",
            "raid_planning",
            "recovery",
            "advancement",
        ]
    );
    for view in &views {
        assert!(!view.lines.is_empty(), "{}", view.id);
        for line in &view.lines {
            let lower = line.to_ascii_lowercase();
            assert!(!lower.contains("war for the over"));
            assert!(!lower.contains("undead awakening"));
        }
    }
    assert_eq!(
        WickedOnesCampaign::from_domain_value(&campaign.to_domain_value()).unwrap(),
        campaign
    );

    let cycle = wicked_ones_golden_cycle();
    assert_eq!(
        WickedOnesGoldenCycle::from_json(&cycle.to_json().unwrap()).unwrap(),
        cycle
    );
    assert_eq!(
        WickedOnesGoldenCycle::from_ron(&cycle.to_ron().unwrap()).unwrap(),
        cycle
    );

    let preview = preview_with_seed(61);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let first = run_golden_cycle(&registry, &preview);
    let second = run_golden_cycle(&registry, &preview);
    assert_eq!(first, second);
    let final_state = first.last().unwrap().after_state.clone();
    assert_eq!(state_campaign(&final_state).cycle.phase, "complete");
    assert_eq!(
        TabletopState::from_json(&final_state.to_json().unwrap()).unwrap(),
        final_state
    );
    assert_eq!(
        TabletopState::from_ron(&final_state.to_ron().unwrap()).unwrap(),
        final_state
    );

    assert!(WICKED_ONES_DOCUMENTATION.contains("v1.4"));
    assert!(WICKED_ONES_DOCUMENTATION.contains("CC0-1.0"));
    assert!(WICKED_ONES_DOCUMENTATION.contains("War for the Overworld"));
    assert!(WICKED_ONES_DOCUMENTATION.contains("Undead Awakening"));
    assert!(WICKED_ONES_DOCUMENTATION.contains("allowlist"));
    assert!(WICKED_ONES_DOCUMENTATION.contains("Compatibility naming"));
    assert!(wicked_ones_source_url_allowed(WICKED_ONES_SOURCE_URL));
    assert!(!wicked_ones_source_url_allowed("https://example.com/rules"));
    assert!(wicked_ones_forbidden_artifact("Undead Awakening"));
    assert!(wicked_ones_forbidden_artifact("war-for-the-overworld.pdf"));
    assert!(!wicked_ones_forbidden_artifact("ember_vault_notes.txt"));
}

#[test]
fn downtime_upgrades_revelry_and_both_advancement_kinds() {
    let preview = preview_with_seed(71);
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let (_, seeded) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "downtime_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&campaign_with_phase("downtime")),
    );

    let (_, project) = execute(
        &registry,
        &preview,
        &seeded.after_state,
        "downtime_project",
        "perform_downtime_action",
        TabletopCapability::CampaignState,
        value_object([
            ("kind", DomainValue::Symbol("project".to_owned())),
            ("note", DomainValue::String("Carve the sigil".to_owned())),
            ("project_id", DomainValue::String("gate_ritual".to_owned())),
        ]),
    );
    assert_eq!(
        payload_u32(payload(&project, "downtime_action_resolved"), "filled"),
        2
    );

    let (_, revel) = execute(
        &registry,
        &preview,
        &project.after_state,
        "downtime_revel",
        "perform_downtime_action",
        TabletopCapability::CampaignState,
        value_object([
            ("kind", DomainValue::Symbol("revel".to_owned())),
            (
                "note",
                DomainValue::String("Sing the vault song".to_owned()),
            ),
            ("project_id", DomainValue::String(String::new())),
        ]),
    );
    assert_eq!(
        state_campaign(&revel.after_state).cycle.revelry,
        "Sing the vault song"
    );

    let (_, lurk) = execute(
        &registry,
        &preview,
        &revel.after_state,
        "downtime_lurk",
        "perform_downtime_action",
        TabletopCapability::CampaignState,
        value_object([
            ("kind", DomainValue::Symbol("lurk".to_owned())),
            ("note", DomainValue::String("Listen at the well".to_owned())),
            ("project_id", DomainValue::String(String::new())),
        ]),
    );
    assert_eq!(
        state_campaign(&lurk.after_state).cycle.lurking,
        vec!["Listen at the well".to_owned()]
    );

    let (_, upgrade) = execute(
        &registry,
        &preview,
        &lurk.after_state,
        "upgrade_buy",
        "purchase_upgrade",
        TabletopCapability::EquipmentAndAbilities,
        value_object([
            ("note", DomainValue::String("Brace the gate".to_owned())),
            (
                "upgrade_id",
                DomainValue::String("reinforced_gate".to_owned()),
            ),
        ]),
    );
    let purchased = payload(&upgrade, "upgrade_purchased");
    assert_eq!(payload_u32(purchased, "cost"), 8);
    assert_eq!(payload_u32(purchased, "gold_after"), 4);
    assert!(
        state_campaign(&upgrade.after_state)
            .upgrades
            .iter()
            .any(|item| item.id == "reinforced_gate" && item.purchased)
    );

    let mut wicked_campaign = campaign_with_phase("advancement");
    wicked_campaign.advancement.wicked = 1;
    let (_, wicked_seeded) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "wicked_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&wicked_campaign),
    );
    let (_, wicked) = execute(
        &registry,
        &preview,
        &wicked_seeded.after_state,
        "wicked_spend",
        "spend_wicked_advancement",
        TabletopCapability::Advancement,
        value_object([
            ("ability", DomainValue::String("shadow_ledger".to_owned())),
            ("note", DomainValue::String("A quieter trade".to_owned())),
        ]),
    );
    let wicked_after = state_campaign(&wicked.after_state);
    assert_eq!(wicked_after.advancement.wicked, 0);
    assert!(
        wicked_after
            .advancement
            .abilities
            .iter()
            .any(|ability| ability == "shadow_ledger")
    );

    let mut reckless_campaign = campaign_with_phase("advancement");
    reckless_campaign.advancement.reckless = 1;
    let (_, reckless_seeded) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "reckless_set",
        "set_campaign",
        TabletopCapability::CampaignState,
        campaign_input(&reckless_campaign),
    );
    let (_, reckless) = execute(
        &registry,
        &preview,
        &reckless_seeded.after_state,
        "reckless_spend",
        "spend_reckless_advancement",
        TabletopCapability::Advancement,
        value_object([
            ("action", DomainValue::Symbol("sneak".to_owned())),
            ("note", DomainValue::String("Harder lessons".to_owned())),
            ("to_rating", DomainValue::Number(3.0)),
        ]),
    );
    let reckless_payload = payload(&reckless, "reckless_advancement_spent");
    assert_eq!(payload_u32(reckless_payload, "from"), 2);
    assert_eq!(payload_u32(reckless_payload, "to"), 3);
    assert_eq!(payload_u32(reckless_payload, "stress_after"), 1);

    let (_, action) = execute(
        &registry,
        &preview,
        &reckless.after_state,
        "advanced_action",
        "resolve_action",
        TabletopCapability::ChecksAndConflicts,
        action_input("guile", "sneak", "risky", "standard", &[], false, None),
    );
    assert_eq!(
        payload_list_len(payload(&action, "action_resolved"), "dice"),
        3
    );
}

#[test]
fn zero_pool_action_and_resistance_use_two_dice_and_the_lowest() {
    let mut registry = ResolverRegistry::new();
    registry
        .register(&wicked_ones_manifest(), WickedOnesResolver)
        .unwrap();
    let mut found = false;
    for seed in 0..128 {
        let preview = preview_with_seed(seed);
        let (_, receipt) = execute(
            &registry,
            &preview,
            &preview.initial_state,
            "zero_pool_resistance",
            "resolve_action",
            TabletopCapability::ChecksAndConflicts,
            action_input(
                "force",
                "demolish",
                "desperate",
                "standard",
                &[],
                false,
                Some(("endure", 0)),
            ),
        );
        let resolved = payload(&receipt, "action_resolved");
        let outcome = payload_text(resolved, "outcome");
        if outcome != "mixed" && outcome != "failure" {
            continue;
        }
        let dice = payload_numbers(resolved, "dice");
        let resistance_dice = payload_numbers(resolved, "resistance_dice");
        assert_eq!(payload_u32(resolved, "pool"), 2, "seed {seed}");
        assert_eq!(dice.len(), 2, "seed {seed}");
        assert_eq!(resistance_dice.len(), 2, "seed {seed}");
        assert_eq!(dice_band(&dice), outcome, "seed {seed}");
        assert_eq!(
            dice_band(&resistance_dice),
            payload_text(resolved, "resistance_outcome"),
            "seed {seed}"
        );
        assert!(payload_bool(resolved, "resisted"), "seed {seed}");
        found = true;
        break;
    }
    assert!(
        found,
        "a bounded seed scan must find a zero-pool consequence"
    );
}
