use std::collections::{BTreeMap, BTreeSet};

use weave_domain::DomainValue;
use weave_tabletop::{
    EntropyState, EntropyStream, PLUG_AND_PLAY_CREATION_FORMAT_VERSION, PlugAndPlayAttributes,
    PlugAndPlayCreationRequest, PlugAndPlayMentalAttribute, PlugAndPlayModifier,
    PlugAndPlayPhysicalAttribute, PlugAndPlayResolver, PlugAndPlayRollAssignment,
    PlugAndPlayStatSource, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, TabletopCapability, TabletopState, canonical_fingerprint,
    create_plug_and_play_character, plug_and_play_manifest, validate_adapter_manifest,
    validate_plug_and_play_creation_preview, validate_tabletop_state, verify_adapter_source_bundle,
};

fn authored_request(seed: u64) -> PlugAndPlayCreationRequest {
    PlugAndPlayCreationRequest {
        creation_format_version: PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.ember_vale".to_owned(),
        name: "Ember Vale".to_owned(),
        age: 31,
        seed,
        stat_source: PlugAndPlayStatSource::Authored {
            attributes: PlugAndPlayAttributes {
                agility: 4,
                brains: 4,
                brawn: 4,
                wits: 4,
            },
            fortune: 3,
        },
        survivability_body: PlugAndPlayPhysicalAttribute::Brawn,
        survivability_mind: PlugAndPlayMentalAttribute::Brains,
        modifiers: vec![
            PlugAndPlayModifier {
                id: "fleet".to_owned(),
                label: "Fleet courier".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 1,
                    brains: 0,
                    brawn: -1,
                    wits: 0,
                },
                fortune: 0,
                explanation: "Quick footwork is offset by a lighter frame.".to_owned(),
            },
            PlugAndPlayModifier {
                id: "studious".to_owned(),
                label: "Studious observer".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 0,
                    brains: 1,
                    brawn: 0,
                    wits: -1,
                },
                fortune: 0,
                explanation: "Careful study is offset by slower improvisation.".to_owned(),
            },
        ],
        inventory: vec!["field knife".to_owned(), "signal pistol".to_owned()],
    }
}

fn rolled_request(seed: u64) -> PlugAndPlayCreationRequest {
    PlugAndPlayCreationRequest {
        stat_source: PlugAndPlayStatSource::Rolled {
            selected_set: 1,
            assignment: PlugAndPlayRollAssignment {
                agility: 3,
                brains: 1,
                brawn: 0,
                wits: 2,
            },
        },
        ..authored_request(seed)
    }
}

#[test]
fn manifest_and_both_creation_modes_are_exact_and_portable() {
    let manifest = plug_and_play_manifest();
    validate_adapter_manifest(&manifest).unwrap();

    let authored = create_plug_and_play_character(&authored_request(17)).unwrap();
    assert!(authored.roll_sets.is_empty());
    assert_eq!(authored.initial_state.entropy.cursor, 0);
    assert_eq!(authored.effective_attributes.agility, 5);
    assert_eq!(authored.effective_attributes.brains, 5);
    assert_eq!(authored.effective_attributes.brawn, 3);
    assert_eq!(authored.effective_attributes.wits, 3);
    assert_eq!(authored.survivability, 6);
    validate_tabletop_state(&authored.initial_state, &manifest).unwrap();

    let request = rolled_request(2026);
    let rolled = create_plug_and_play_character(&request).unwrap();
    validate_plug_and_play_creation_preview(&rolled).unwrap();
    let repeated = create_plug_and_play_character(&request).unwrap();
    assert_eq!(rolled, repeated);
    assert_eq!(rolled.roll_sets.len(), 2);
    assert_eq!(rolled.selected_set, Some(1));
    assert_eq!(rolled.roll_sets[0].entropy_start, 0);
    assert_eq!(
        rolled.roll_sets[1].entropy_end,
        rolled.initial_state.entropy.cursor
    );
    assert!(rolled.initial_state.entropy.cursor >= 10);

    let request_json = request.to_json().unwrap();
    let request_ron = request.to_ron().unwrap();
    assert_eq!(
        PlugAndPlayCreationRequest::from_json(&request_json).unwrap(),
        request
    );
    assert_eq!(
        PlugAndPlayCreationRequest::from_ron(&request_ron).unwrap(),
        request
    );
    let preview_json = rolled.to_json().unwrap();
    let preview_ron = rolled.to_ron().unwrap();
    assert_eq!(
        weave_tabletop::PlugAndPlayCreationPreview::from_json(&preview_json).unwrap(),
        rolled
    );
    assert_eq!(
        weave_tabletop::PlugAndPlayCreationPreview::from_ron(&preview_ron).unwrap(),
        rolled
    );
}

#[test]
fn malformed_creation_requests_fail_closed() {
    let mut request = authored_request(7);
    request.modifiers[0].fortune = 1;
    assert!(create_plug_and_play_character(&request).is_err());

    let mut request = rolled_request(7);
    if let PlugAndPlayStatSource::Rolled { assignment, .. } = &mut request.stat_source {
        assignment.wits = assignment.agility;
    }
    assert!(create_plug_and_play_character(&request).is_err());

    let mut request = authored_request(7);
    request.inventory.push("field knife".to_owned());
    assert!(create_plug_and_play_character(&request).is_err());

    let json = authored_request(7).to_json().unwrap();
    let duplicate = json.replacen(
        "\"creation_format_version\": 1,",
        "\"creation_format_version\": 1,\n  \"creation_format_version\": 1,",
        1,
    );
    assert!(PlugAndPlayCreationRequest::from_json(&duplicate).is_err());
}

#[test]
fn source_bundle_requires_every_exact_companion_and_license_hash() {
    let rules = b"reviewed rules bytes";
    let sheet = b"reviewed sheet bytes";
    let license = b"reviewed license bytes";
    let mut manifest = plug_and_play_manifest();
    manifest.provenance.sha256 = weave_tabletop::sha256_bytes(rules);
    manifest.provenance.additional_artifacts[0].sha256 = weave_tabletop::sha256_bytes(sheet);
    manifest.provenance.required_license_text.sha256 = weave_tabletop::sha256_bytes(license);
    let companions = BTreeMap::from([(
        "Plug-And-Play Character Sheet.pdf".to_owned(),
        sheet.to_vec(),
    )]);
    verify_adapter_source_bundle(&manifest, rules, &companions, license).unwrap();
    assert!(verify_adapter_source_bundle(&manifest, rules, &BTreeMap::new(), license).is_err());
    let changed = BTreeMap::from([(
        "Plug-And-Play Character Sheet.pdf".to_owned(),
        b"changed".to_vec(),
    )]);
    assert!(verify_adapter_source_bundle(&manifest, rules, &changed, license).is_err());
}

#[test]
fn checks_cover_advantage_disadvantage_fortune_and_replay() {
    let preview = create_plug_and_play_character(&authored_request(1)).unwrap();
    let manifest = plug_and_play_manifest();
    let registry = registry(&manifest);
    let mut state = preview.initial_state.clone();
    state.entropy.seed = seed_for_d6_sequence(&[2, 6]);
    let request = resolution_request(
        "fortune_reroll",
        "check",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &state,
        object([
            ("attribute", DomainValue::Symbol("brains".to_owned())),
            ("difficulty", DomainValue::Number(18.0)),
            ("modifier", DomainValue::Number(0.0)),
            ("roll_mode", DomainValue::Symbol("normal".to_owned())),
            ("spend_fortune_on_failure", DomainValue::Bool(true)),
        ]),
    );
    let receipt = registry.execute(&manifest, &request, &state).unwrap();
    registry
        .replay(&manifest, &request, &state, &receipt)
        .unwrap();
    let check = event_payload(&receipt, "check_resolved");
    assert_eq!(check["die"], DomainValue::Number(6.0));
    assert_eq!(check["critical"], DomainValue::Bool(true));
    assert_eq!(check["rerolled"], DomainValue::Bool(true));
    assert_eq!(state_integer(&receipt.after_state, "fortune_remaining"), 2);
    assert_eq!(receipt.entropy_consumed, 2);

    let mut advantage_state = preview.initial_state.clone();
    advantage_state.entropy.seed = seed_for_d6_sequence(&[2, 5]);
    let advantage = resolution_request(
        "advantage_check",
        "check",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &advantage_state,
        object([
            ("attribute", DomainValue::Symbol("wits".to_owned())),
            ("difficulty", DomainValue::Number(6.0)),
            ("modifier", DomainValue::Number(0.0)),
            ("roll_mode", DomainValue::Symbol("advantage".to_owned())),
            ("spend_fortune_on_failure", DomainValue::Bool(false)),
        ]),
    );
    let advantage = registry
        .execute(&manifest, &advantage, &advantage_state)
        .unwrap();
    assert_eq!(
        event_payload(&advantage, "check_resolved")["die"],
        DomainValue::Number(5.0)
    );

    let mut disadvantage_state = preview.initial_state.clone();
    disadvantage_state.entropy.seed = seed_for_d6_sequence(&[2, 5]);
    let disadvantage = resolution_request(
        "disadvantage_check",
        "check",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &disadvantage_state,
        object([
            ("attribute", DomainValue::Symbol("wits".to_owned())),
            ("difficulty", DomainValue::Number(6.0)),
            ("modifier", DomainValue::Number(0.0)),
            ("roll_mode", DomainValue::Symbol("disadvantage".to_owned())),
            ("spend_fortune_on_failure", DomainValue::Bool(false)),
        ]),
    );
    let disadvantage = registry
        .execute(&manifest, &disadvantage, &disadvantage_state)
        .unwrap();
    assert_eq!(
        event_payload(&disadvantage, "check_resolved")["die"],
        DomainValue::Number(2.0)
    );
}

#[test]
fn fortune_tests_have_no_critical_or_fumble_semantics() {
    let preview = create_plug_and_play_character(&authored_request(1)).unwrap();
    let manifest = plug_and_play_manifest();
    let registry = registry(&manifest);
    let mut state = preview.initial_state.clone();
    state.entropy.seed = seed_for_d6_sequence(&[1]);
    let request = resolution_request(
        "fortune_test",
        "fortune_test",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &state,
        object([("roll_mode", DomainValue::Symbol("normal".to_owned()))]),
    );
    let receipt = registry.execute(&manifest, &request, &state).unwrap();
    let result = event_payload(&receipt, "fortune_test_resolved");
    assert_eq!(result["die"], DomainValue::Number(1.0));
    assert_eq!(result["success"], DomainValue::Bool(true));
    assert!(!result.contains_key("critical"));
    assert!(!result.contains_key("fumble"));
}

#[test]
fn attacks_cover_critical_damage_ranged_jams_and_melee_self_damage() {
    let preview = create_plug_and_play_character(&authored_request(1)).unwrap();
    let manifest = plug_and_play_manifest();
    let registry = registry(&manifest);

    let mut critical_state = preview.initial_state.clone();
    critical_state.entropy.seed = seed_for_d6_sequence(&[6]);
    let critical = attack_request(
        "critical_attack",
        &preview,
        &critical_state,
        "field knife",
        "melee",
        8,
    );
    let critical = registry
        .execute(&manifest, &critical, &critical_state)
        .unwrap();
    let result = event_payload(&critical, "attack_resolved");
    assert_eq!(result["critical"], DomainValue::Bool(true));
    assert_eq!(result["damage"], DomainValue::Number(8.0));

    let mut ranged_state = preview.initial_state.clone();
    ranged_state.entropy.seed = seed_for_d6_sequence(&[1]);
    let ranged = attack_request(
        "ranged_fumble",
        &preview,
        &ranged_state,
        "signal pistol",
        "ranged",
        6,
    );
    let ranged = registry.execute(&manifest, &ranged, &ranged_state).unwrap();
    let result = event_payload(&ranged, "attack_resolved");
    assert_eq!(result["jammed"], DomainValue::Bool(true));
    assert!(state_strings(&ranged.after_state, "jammed_items").contains("signal pistol"));

    let mut melee_state = preview.initial_state.clone();
    melee_state.entropy.seed = seed_for_d6_sequence(&[1]);
    let melee = attack_request(
        "melee_fumble",
        &preview,
        &melee_state,
        "field knife",
        "melee",
        5,
    );
    let melee = registry.execute(&manifest, &melee, &melee_state).unwrap();
    let result = event_payload(&melee, "attack_resolved");
    assert_eq!(result["self_damage"], DomainValue::Number(3.0));
    assert_eq!(
        state_integer(&melee.after_state, "survivability_current"),
        3
    );
}

#[test]
fn wounds_apply_a_physical_penalty_and_further_damage_is_fatal() {
    let preview = create_plug_and_play_character(&authored_request(1)).unwrap();
    let manifest = plug_and_play_manifest();
    let registry = registry(&manifest);
    let state = preview.initial_state.clone();
    let wound = resolution_request(
        "wound",
        "take_damage",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &state,
        object([("amount", DomainValue::Number(99.0))]),
    );
    let wound = registry.execute(&manifest, &wound, &state).unwrap();
    assert_eq!(
        state_integer(&wound.after_state, "survivability_current"),
        0
    );
    assert!(state_bool(&wound.after_state, "wounded"));
    assert!(!state_bool(&wound.after_state, "dead"));

    let mut wounded_state = wound.after_state.clone();
    wounded_state.entropy.seed = seed_for_d6_sequence(&[3]);
    wounded_state.entropy.cursor = 0;
    let physical = resolution_request(
        "wounded_check",
        "check",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &wounded_state,
        object([
            ("attribute", DomainValue::Symbol("agility".to_owned())),
            ("difficulty", DomainValue::Number(18.0)),
            ("modifier", DomainValue::Number(0.0)),
            ("roll_mode", DomainValue::Symbol("normal".to_owned())),
            ("spend_fortune_on_failure", DomainValue::Bool(false)),
        ]),
    );
    let physical = registry
        .execute(&manifest, &physical, &wounded_state)
        .unwrap();
    assert_eq!(
        event_payload(&physical, "check_resolved")["total"],
        DomainValue::Number(3.0)
    );

    let fatal = resolution_request(
        "fatal_damage",
        "take_damage",
        TabletopCapability::ResourcesAndConditions,
        &preview,
        &wound.after_state,
        object([("amount", DomainValue::Number(1.0))]),
    );
    let fatal = registry
        .execute(&manifest, &fatal, &wound.after_state)
        .unwrap();
    assert!(state_bool(&fatal.after_state, "dead"));
}

#[test]
fn group_initiative_chase_and_save_reload_are_deterministic() {
    let preview = create_plug_and_play_character(&authored_request(1)).unwrap();
    let manifest = plug_and_play_manifest();
    let registry = registry(&manifest);

    let mut group_state = preview.initial_state.clone();
    group_state.entropy.seed = seed_for_d6_sequence(&[6, 1]);
    let group = resolution_request(
        "group_tie",
        "group_check",
        TabletopCapability::ChecksAndConflicts,
        &preview,
        &group_state,
        object([
            ("difficulty", DomainValue::Number(5.0)),
            (
                "participants",
                DomainValue::List(vec![
                    participant("ember", 0, 0, "normal"),
                    participant("rook", 12, 0, "normal"),
                ]),
            ),
        ]),
    );
    let group = registry.execute(&manifest, &group, &group_state).unwrap();
    let group_result = event_payload(&group, "group_check_resolved");
    assert_eq!(group_result["successes"], DomainValue::Number(1.0));
    assert_eq!(group_result["failures"], DomainValue::Number(1.0));
    assert_eq!(group_result["success"], DomainValue::Bool(false));

    let initiative_state = preview.initial_state.clone();
    let initiative = resolution_request(
        "initiative",
        "initiative",
        TabletopCapability::Encounters,
        &preview,
        &initiative_state,
        object([(
            "participants",
            DomainValue::List(
                ["ember", "rook", "warden"]
                    .into_iter()
                    .map(|value| DomainValue::String(value.to_owned()))
                    .collect(),
            ),
        )]),
    );
    let initiative = registry
        .execute(&manifest, &initiative, &initiative_state)
        .unwrap();
    assert_eq!(initiative.entropy_consumed, 3);
    let entries = event_list(&initiative, "initiative_ordered", "entries");
    assert_eq!(entries.len(), 3);
    let labels = entries
        .iter()
        .map(|entry| object_string(entry, "label"))
        .collect::<BTreeSet<_>>();
    assert_eq!(labels.len(), 3);

    let chase_state = preview.initial_state.clone();
    let chase = resolution_request(
        "short_chase",
        "chase",
        TabletopCapability::Encounters,
        &preview,
        &chase_state,
        object([
            ("attribute", DomainValue::Symbol("agility".to_owned())),
            ("max_checks", DomainValue::Number(3.0)),
            ("opponent_modifier", DomainValue::Number(0.0)),
            ("opponent_rating", DomainValue::Number(4.0)),
            (
                "opponent_roll_mode",
                DomainValue::Symbol("normal".to_owned()),
            ),
            ("player_modifier", DomainValue::Number(0.0)),
            ("player_roll_mode", DomainValue::Symbol("normal".to_owned())),
        ]),
    );
    let chase = registry.execute(&manifest, &chase, &chase_state).unwrap();
    assert!(event_list(&chase, "chase_resolved", "rounds").len() <= 3);
    registry
        .replay(
            &manifest,
            &chase_request(&preview, &chase_state),
            &chase_state,
            &chase,
        )
        .unwrap();

    let saved = chase.after_state.to_json().unwrap();
    let reloaded = TabletopState::from_json(&saved).unwrap();
    assert_eq!(reloaded, chase.after_state);
    validate_tabletop_state(&reloaded, &manifest).unwrap();
}

fn chase_request(
    preview: &weave_tabletop::PlugAndPlayCreationPreview,
    state: &TabletopState,
) -> ResolutionRequest {
    resolution_request(
        "short_chase",
        "chase",
        TabletopCapability::Encounters,
        preview,
        state,
        object([
            ("attribute", DomainValue::Symbol("agility".to_owned())),
            ("max_checks", DomainValue::Number(3.0)),
            ("opponent_modifier", DomainValue::Number(0.0)),
            ("opponent_rating", DomainValue::Number(4.0)),
            (
                "opponent_roll_mode",
                DomainValue::Symbol("normal".to_owned()),
            ),
            ("player_modifier", DomainValue::Number(0.0)),
            ("player_roll_mode", DomainValue::Symbol("normal".to_owned())),
        ]),
    )
}

fn attack_request(
    id: &str,
    preview: &weave_tabletop::PlugAndPlayCreationPreview,
    state: &TabletopState,
    weapon_id: &str,
    weapon_kind: &str,
    damage_die: i32,
) -> ResolutionRequest {
    resolution_request(
        id,
        "attack",
        TabletopCapability::ChecksAndConflicts,
        preview,
        state,
        object([
            ("attribute", DomainValue::Symbol("agility".to_owned())),
            ("damage_die", DomainValue::Number(f64::from(damage_die))),
            ("modifier", DomainValue::Number(0.0)),
            ("roll_mode", DomainValue::Symbol("normal".to_owned())),
            ("target_survivability", DomainValue::Number(18.0)),
            ("weapon_id", DomainValue::String(weapon_id.to_owned())),
            ("weapon_kind", DomainValue::Symbol(weapon_kind.to_owned())),
        ]),
    )
}

fn participant(id: &str, rating: i32, modifier: i32, mode: &str) -> DomainValue {
    object([
        ("id", DomainValue::String(id.to_owned())),
        ("modifier", DomainValue::Number(f64::from(modifier))),
        ("rating", DomainValue::Number(f64::from(rating))),
        ("roll_mode", DomainValue::Symbol(mode.to_owned())),
    ])
}

fn registry(manifest: &weave_tabletop::AdapterManifest) -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry.register(manifest, PlugAndPlayResolver).unwrap();
    registry
}

fn resolution_request(
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    preview: &weave_tabletop::PlugAndPlayCreationPreview,
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
    let payload = receipt
        .events
        .iter()
        .find(|event| event.kind == kind)
        .and_then(|event| event.payload.as_ref())
        .expect("fixture receipt contains the requested visible payload");
    match payload {
        DomainValue::Object(fields) => fields,
        _ => panic!("fixture event payload must be an object"),
    }
}

fn event_list<'a>(receipt: &'a ResolutionReceipt, kind: &str, field: &str) -> &'a [DomainValue] {
    match &event_payload(receipt, kind)[field] {
        DomainValue::List(values) => values,
        _ => panic!("fixture event field must be a list"),
    }
}

fn state_object(state: &TabletopState) -> &BTreeMap<String, DomainValue> {
    match &state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("fixture state must be an object"),
    }
}

fn state_integer(state: &TabletopState, field: &str) -> i32 {
    match &state_object(state)[field] {
        DomainValue::Number(value) => *value as i32,
        _ => panic!("fixture state field must be a number"),
    }
}

fn state_bool(state: &TabletopState, field: &str) -> bool {
    match &state_object(state)[field] {
        DomainValue::Bool(value) => *value,
        _ => panic!("fixture state field must be a bool"),
    }
}

fn state_strings<'a>(state: &'a TabletopState, field: &str) -> BTreeSet<&'a str> {
    match &state_object(state)[field] {
        DomainValue::List(values) => values
            .iter()
            .map(|value| match value {
                DomainValue::String(value) => value.as_str(),
                _ => panic!("fixture state list must contain strings"),
            })
            .collect(),
        _ => panic!("fixture state field must be a list"),
    }
}

fn object_string<'a>(value: &'a DomainValue, field: &str) -> &'a str {
    let DomainValue::Object(fields) = value else {
        panic!("fixture value must be an object");
    };
    match &fields[field] {
        DomainValue::String(value) => value,
        _ => panic!("fixture object field must be a string"),
    }
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}
