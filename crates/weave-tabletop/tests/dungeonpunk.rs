use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    DUNGEONPUNK_CREATION_FORMAT_VERSION, DungeonpunkAttributes, DungeonpunkClock,
    DungeonpunkClockTrigger, DungeonpunkCreationPreview, DungeonpunkCreationRequest,
    DungeonpunkGear, DungeonpunkGearKind, DungeonpunkMoveChoice, DungeonpunkResolver,
    DungeonpunkThreat, DungeonpunkThreatKind, EntropyState, EntropyStream, RESOLVER_FORMAT_VERSION,
    ResolutionReceipt, ResolutionRequest, ResolverRegistry, TabletopCapability, TabletopState,
    canonical_fingerprint, create_dungeonpunk_character, dungeonpunk_manifest,
    validate_adapter_manifest, validate_dungeonpunk_creation_preview, validate_tabletop_state,
    verify_adapter_source_bundle,
};

fn creation_request(seed: u64) -> DungeonpunkCreationRequest {
    DungeonpunkCreationRequest {
        creation_format_version: DUNGEONPUNK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.vesper_ash".to_owned(),
        name: "Vesper Ash".to_owned(),
        seed,
        attributes: DungeonpunkAttributes {
            strength: 2,
            dexterity: 1,
            constitution: 1,
            intelligence: 1,
            charisma: 0,
            wisdom: 0,
        },
        special_moves: ["hale_and_hearty", "loaded_for_bear", "warriors_strike"]
            .into_iter()
            .map(|id| DungeonpunkMoveChoice::Core { id: id.to_owned() })
            .collect(),
        gear: vec![
            gear(
                "armor",
                "Patched plate",
                DungeonpunkGearKind::Armor,
                4,
                true,
            ),
            gear(
                "field_pack",
                "Field pack",
                DungeonpunkGearKind::Pack,
                4,
                false,
            ),
            gear(
                "rations",
                "Road rations",
                DungeonpunkGearKind::Rations,
                2,
                false,
            ),
            gear(
                "shield",
                "Scrap shield",
                DungeonpunkGearKind::Shield,
                2,
                true,
            ),
            gear(
                "sword",
                "Notched sword",
                DungeonpunkGearKind::Weapon,
                4,
                true,
            ),
            gear("tools", "Repair tools", DungeonpunkGearKind::Tool, 1, false),
        ],
        bonds: vec![
            "I owe Rook a road home.".to_owned(),
            "Mara knows what woke below the bridge.".to_owned(),
        ],
        clocks: vec![DungeonpunkClock {
            id: "bridge_wakes".to_owned(),
            label: "The bridge wakes".to_owned(),
            segments: 4,
            filled: 2,
            trigger: DungeonpunkClockTrigger::Full,
        }],
        threats: vec![DungeonpunkThreat {
            id: "ash_hound".to_owned(),
            label: "Ash hound".to_owned(),
            kind: DungeonpunkThreatKind::Foe,
            hit_points: 8,
            armor: 1,
            clock_id: Some("bridge_wakes".to_owned()),
        }],
    }
}

fn gear(
    id: &str,
    label: &str,
    kind: DungeonpunkGearKind,
    weight: u8,
    equipped: bool,
) -> DungeonpunkGear {
    DungeonpunkGear {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
        weight,
        uses: None,
        equipped,
    }
}

#[test]
fn manifest_creation_and_json_ron_round_trips_are_exact() {
    let manifest = dungeonpunk_manifest();
    validate_adapter_manifest(&manifest).unwrap();
    assert_eq!(manifest.provenance.revision, "google-doc-revision-15499");
    assert_eq!(manifest.provenance.retrieved_on, "2026-08-25");
    assert_eq!(manifest.provenance.additional_artifacts.len(), 1);

    let request = creation_request(2_026_082_501);
    let preview = create_dungeonpunk_character(&request).unwrap();
    validate_dungeonpunk_creation_preview(&preview).unwrap();
    validate_tabletop_state(&preview.initial_state, &manifest).unwrap();
    assert_eq!(preview.hp_rolls.len(), 3);
    assert_eq!(preview.fate, 1);
    assert_eq!(preview.none, 0);
    assert_eq!(preview.gear_weight_half_units, 24);
    assert!(!preview.starting_encumbered);
    assert!(matches!(
        &state_object(&preview.initial_state)["bonds"],
        DomainValue::List(values) if values.len() == 2
    ));
    assert_eq!(create_dungeonpunk_character(&request).unwrap(), preview);

    assert_eq!(
        DungeonpunkCreationRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        DungeonpunkCreationRequest::from_ron(&request.to_ron().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        DungeonpunkCreationPreview::from_json(&preview.to_json().unwrap()).unwrap(),
        preview
    );
    assert_eq!(
        DungeonpunkCreationPreview::from_ron(&preview.to_ron().unwrap()).unwrap(),
        preview
    );
}

#[test]
fn malformed_creation_and_source_bundles_fail_closed() {
    let mut request = creation_request(7);
    request.attributes.strength = 3;
    assert!(create_dungeonpunk_character(&request).is_err());

    let mut request = creation_request(7);
    request.bonds.pop();
    assert!(create_dungeonpunk_character(&request).is_err());

    let mut request = creation_request(7);
    request.special_moves[2] = request.special_moves[0].clone();
    assert!(create_dungeonpunk_character(&request).is_err());

    let json = creation_request(7).to_json().unwrap();
    let duplicate = json.replacen(
        "\"creation_format_version\": 1,",
        "\"creation_format_version\": 1,\n  \"creation_format_version\": 1,",
        1,
    );
    assert!(DungeonpunkCreationRequest::from_json(&duplicate).is_err());

    let pdf = b"reviewed Dungeonpunk PDF bytes";
    let text = b"reviewed Dungeonpunk text bytes";
    let license = b"reviewed CC0 legal code";
    let mut manifest = dungeonpunk_manifest();
    manifest.provenance.sha256 = weave_tabletop::sha256_bytes(pdf);
    manifest.provenance.additional_artifacts[0].sha256 = weave_tabletop::sha256_bytes(text);
    manifest.provenance.required_license_text.sha256 = weave_tabletop::sha256_bytes(license);
    let companions = BTreeMap::from([("dungeonpunk-google-doc.txt".to_owned(), text.to_vec())]);
    verify_adapter_source_bundle(&manifest, pdf, &companions, license).unwrap();
    assert!(verify_adapter_source_bundle(&manifest, pdf, &BTreeMap::new(), license).is_err());
}

#[test]
fn struggle_covers_help_push_fallback_failure_xp_and_replay() {
    let preview = create_dungeonpunk_character(&creation_request(9)).unwrap();
    let manifest = dungeonpunk_manifest();
    let registry = registry(&manifest);
    let state = preview.initial_state.clone();
    let helped_and_pushed = request(
        "raise_gate",
        "struggle",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &state,
        object([
            ("attribute", DomainValue::Symbol("strength".to_owned())),
            ("edge", DomainValue::Symbol("neutral".to_owned())),
            ("helper_encumbered", DomainValue::Bool(false)),
            ("helper_id", DomainValue::String("mara".to_owned())),
            ("push", DomainValue::Bool(true)),
        ]),
    );
    let pushed = registry
        .execute(&manifest, &helped_and_pushed, &state)
        .unwrap();
    registry
        .replay(&manifest, &helped_and_pushed, &state, &pushed)
        .unwrap();
    let roll = event_payload(&pushed, "roll_resolved");
    assert_eq!(roll["helped"], DomainValue::Bool(true));
    assert_eq!(roll["push"], DomainValue::Bool(true));
    assert_eq!(roll["selection"], DomainValue::Symbol("highest".to_owned()));
    assert_eq!(event_list(&pushed, "roll_resolved", "dice").len(), 3);
    assert_eq!(state_integer(&pushed.after_state, "stress"), 2);
    assert_eq!(
        state_integer(&pushed.after_state, "total_load_half_units"),
        28
    );
    assert!(state_bool(&pushed.after_state, "encumbered"));
    assert_eq!(
        event_payload(&pushed, "help_applied")["stress_delta"],
        DomainValue::Number(1.0)
    );

    let mut failure_state = preview.initial_state.clone();
    failure_state.entropy.seed = seed_for_d6_sequence(&[2, 3]);
    failure_state.entropy.cursor = 0;
    let fallback = request(
        "hold_fast",
        "struggle",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &failure_state,
        object([
            ("attribute", DomainValue::Symbol("charisma".to_owned())),
            ("edge", DomainValue::Symbol("disadvantage".to_owned())),
            ("push", DomainValue::Bool(false)),
        ]),
    );
    let failed = registry
        .execute(&manifest, &fallback, &failure_state)
        .unwrap();
    let roll = event_payload(&failed, "roll_resolved");
    assert_eq!(roll["dice_pool"], DomainValue::Number(-1.0));
    assert_eq!(roll["selected"], DomainValue::Number(2.0));
    assert_eq!(roll["selection"], DomainValue::Symbol("lowest".to_owned()));
    assert_eq!(roll["outcome"], DomainValue::Symbol("failure".to_owned()));
    assert_eq!(state_integer(&failed.after_state, "xp"), 1);
    assert!(
        failed
            .events
            .iter()
            .any(|event| event.kind == "gm_move_prompt")
    );
}

#[test]
fn harm_death_offer_rest_and_load_are_structured() {
    let preview = create_dungeonpunk_character(&creation_request(11)).unwrap();
    let manifest = dungeonpunk_manifest();
    let registry = registry(&manifest);
    let mut state = preview.initial_state.clone();
    set_state_integer(&mut state, "hp_current", 0);
    set_state_bool(&mut state, "unconscious", true);
    let harm = request(
        "fall_below_zero",
        "minor_damage",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &state,
        object([]),
    );
    let harmed = registry.execute(&manifest, &harm, &state).unwrap();
    assert_eq!(state_integer(&harmed.after_state, "hp_current"), -1);
    assert!(state_bool(&harmed.after_state, "death_check_pending"));
    assert_eq!(harmed.entropy_consumed, 0);

    let mut pending = harmed.after_state.clone();
    pending.entropy.seed = seed_for_d6_sequence(&[4]);
    pending.entropy.cursor = 0;
    let death_check = request(
        "roll_fate",
        "death_check",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &pending,
        object([]),
    );
    let offered = registry.execute(&manifest, &death_check, &pending).unwrap();
    assert_eq!(offered.entropy_consumed, 1);
    assert!(state_bool(&offered.after_state, "survival_offer_pending"));
    assert_eq!(
        event_payload(&offered, "death_resolved")["status"],
        DomainValue::Symbol("survival_offer".to_owned())
    );

    let accept = request(
        "accept_cost",
        "survival_decision",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &offered.after_state,
        object([("accept", DomainValue::Bool(true))]),
    );
    let survived = registry
        .execute(&manifest, &accept, &offered.after_state)
        .unwrap();
    assert!(state_bool(&survived.after_state, "unconscious"));
    assert!(!state_bool(&survived.after_state, "dead"));

    let mut rest_state = preview.initial_state.clone();
    set_state_integer(&mut rest_state, "stress", 3);
    set_state_integer(&mut rest_state, "total_load_half_units", 30);
    set_state_bool(&mut rest_state, "encumbered", true);
    set_state_integer(&mut rest_state, "hp_current", preview.max_hp - 3);
    let rest = request(
        "rest_six_hours",
        "rest",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &rest_state,
        object([("hours", DomainValue::Number(6.0))]),
    );
    let rested = registry.execute(&manifest, &rest, &rest_state).unwrap();
    assert_eq!(
        state_integer(&rested.after_state, "hp_current"),
        preview.max_hp
    );
    assert_eq!(state_integer(&rested.after_state, "stress"), 0);
    assert_eq!(
        state_integer(&rested.after_state, "total_load_half_units"),
        24
    );
    assert!(!state_bool(&rested.after_state, "encumbered"));
}

#[test]
fn clocks_threats_growth_and_save_restore_are_portable() {
    let preview = create_dungeonpunk_character(&creation_request(13)).unwrap();
    let manifest = dungeonpunk_manifest();
    let registry = registry(&manifest);
    let state = preview.initial_state.clone();

    let tick = request(
        "wake_bridge",
        "tick_clock",
        TabletopCapability::Clocks,
        &preview,
        &state,
        object([
            ("clock_id", DomainValue::String("bridge_wakes".to_owned())),
            ("delta", DomainValue::Number(2.0)),
        ]),
    );
    let ticked = registry.execute(&manifest, &tick, &state).unwrap();
    assert!(
        ticked
            .events
            .iter()
            .any(|event| event.kind == "clock_triggered")
    );

    let mut threat_state = ticked.after_state.clone();
    threat_state.entropy.seed = seed_for_d6_sequence(&[6]);
    threat_state.entropy.cursor = 0;
    let harm = request(
        "strike_hound",
        "harm_threat",
        TabletopCapability::Encounters,
        &preview,
        &threat_state,
        object([
            ("threat_id", DomainValue::String("ash_hound".to_owned())),
            ("tier", DomainValue::Symbol("standard".to_owned())),
        ]),
    );
    let harmed = registry.execute(&manifest, &harm, &threat_state).unwrap();
    let threat = event_payload(&harmed, "threat_changed");
    assert_eq!(threat["raw_damage"], DomainValue::Number(6.0));
    assert_eq!(threat["applied"], DomainValue::Number(5.0));
    assert_eq!(threat["hp_after"], DomainValue::Number(3.0));

    let mut growth_state = harmed.after_state.clone();
    set_state_integer(&mut growth_state, "xp", 3);
    let grow = request(
        "learn_bridgecraft",
        "grow",
        TabletopCapability::Advancement,
        &preview,
        &growth_state,
        object([
            ("kind", DomainValue::Symbol("add_move".to_owned())),
            ("move_id", DomainValue::String("bridgecraft".to_owned())),
            ("move_label", DomainValue::String("Bridgecraft".to_owned())),
        ]),
    );
    let grown = registry.execute(&manifest, &grow, &growth_state).unwrap();
    assert_eq!(state_integer(&grown.after_state, "xp"), 0);
    assert_eq!(
        event_payload(&grown, "advancement_applied")["cost"],
        DomainValue::Number(3.0)
    );

    let json = grown.after_state.to_json().unwrap();
    let ron = grown.after_state.to_ron().unwrap();
    assert_eq!(TabletopState::from_json(&json).unwrap(), grown.after_state);
    assert_eq!(TabletopState::from_ron(&ron).unwrap(), grown.after_state);
    validate_tabletop_state(&TabletopState::from_json(&json).unwrap(), &manifest).unwrap();
}

fn registry(manifest: &weave_tabletop::AdapterManifest) -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry.register(manifest, DungeonpunkResolver).unwrap();
    registry
}

fn request(
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    preview: &DungeonpunkCreationPreview,
    state: &TabletopState,
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

fn seed_for_d6_sequence(expected: &[u64]) -> u64 {
    (0..1_000_000)
        .find(|seed| {
            let mut entropy = EntropyStream::from_state(EntropyState {
                algorithm: "sha256_counter_v1".to_owned(),
                seed: *seed,
                cursor: 0,
            })
            .unwrap();
            expected
                .iter()
                .all(|expected| entropy.draw_bounded(6).unwrap() + 1 == *expected)
        })
        .expect("bounded search finds a short deterministic die sequence")
}

fn event_payload<'a>(
    receipt: &'a ResolutionReceipt,
    kind: &str,
) -> &'a BTreeMap<String, DomainValue> {
    match receipt
        .events
        .iter()
        .find(|event| event.kind == kind)
        .and_then(|event| event.payload.as_ref())
        .expect("receipt contains the expected visible event")
    {
        DomainValue::Object(fields) => fields,
        _ => panic!("event payload must be an object"),
    }
}

fn event_list<'a>(receipt: &'a ResolutionReceipt, kind: &str, field: &str) -> &'a [DomainValue] {
    match &event_payload(receipt, kind)[field] {
        DomainValue::List(values) => values,
        _ => panic!("event field must be a list"),
    }
}

fn state_object(state: &TabletopState) -> &BTreeMap<String, DomainValue> {
    match &state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("state must be an object"),
    }
}

fn state_object_mut(state: &mut TabletopState) -> &mut BTreeMap<String, DomainValue> {
    match &mut state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("state must be an object"),
    }
}

fn state_integer(state: &TabletopState, field: &str) -> i32 {
    match &state_object(state)[field] {
        DomainValue::Number(value) => *value as i32,
        _ => panic!("state field must be numeric"),
    }
}

fn state_bool(state: &TabletopState, field: &str) -> bool {
    match &state_object(state)[field] {
        DomainValue::Bool(value) => *value,
        _ => panic!("state field must be boolean"),
    }
}

fn set_state_integer(state: &mut TabletopState, field: &str, value: i32) {
    state_object_mut(state).insert(field.to_owned(), DomainValue::Number(f64::from(value)));
}

fn set_state_bool(state: &mut TabletopState, field: &str, value: bool) {
    state_object_mut(state).insert(field.to_owned(), DomainValue::Bool(value));
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}
