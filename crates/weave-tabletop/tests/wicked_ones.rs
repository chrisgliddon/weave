use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest, ResolverRegistry, SOURCE_RECORD,
    TabletopCapability, TabletopError, TabletopState, WICKED_ONES_CREATION_FORMAT_VERSION,
    WICKED_ONES_LICENSE, WICKED_ONES_SOURCE_EDITION, WICKED_ONES_SOURCE_RECORD_SHA256,
    WICKED_ONES_SOURCE_URL, WickedOnesActionRatings, WickedOnesCalling, WickedOnesCreationPreview,
    WickedOnesCreationRequest, WickedOnesGearItem, WickedOnesResolver, WickedOnesVileFriend,
    canonical_fingerprint, create_wicked_ones_character, sha256_bytes, validate_adapter_manifest,
    validate_wicked_ones_creation_preview, wicked_ones_manifest,
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
