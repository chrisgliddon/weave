use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    EventVisibility, OPEN_ADVENTURE_ADAPTER_ID, OPEN_ADVENTURE_ADAPTER_VERSION,
    OPEN_ADVENTURE_CREATION_FORMAT_VERSION, OPEN_ADVENTURE_EDITION,
    OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN, OPEN_ADVENTURE_LICENSE_STATEMENT,
    OPEN_ADVENTURE_SOURCE_RECORD, OPEN_ADVENTURE_SOURCE_RECORD_SHA256, OPEN_ADVENTURE_SOURCE_URL,
    OpenAdventureAttribute, OpenAdventureAttributes, OpenAdventureCreationPreview,
    OpenAdventureCreationRequest, OpenAdventureEquipment, OpenAdventureEquipmentKind,
    OpenAdventureFavoredAction, OpenAdventureModifierTarget, OpenAdventureModusOperandi,
    OpenAdventureOrigin, OpenAdventureResolver, OpenAdventureSpecialization,
    OpenAdventureTraitModifier, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, TabletopCapability, TabletopState, canonical_fingerprint,
    create_open_adventure_character, open_adventure_action_dice, open_adventure_derived,
    open_adventure_manifest, open_adventure_source_record_sha256, validate_adapter_manifest,
    validate_resolution_receipt,
};

fn origin(id: &str, label: &str, note: &str) -> OpenAdventureOrigin {
    OpenAdventureOrigin {
        id: id.to_owned(),
        label: label.to_owned(),
        note: note.to_owned(),
    }
}

fn creation_request() -> OpenAdventureCreationRequest {
    OpenAdventureCreationRequest {
        creation_format_version: OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.ivy_marrow".to_owned(),
        name: "Ivy Marrow".to_owned(),
        seed: 7,
        lineage: origin(
            "glassborn",
            "Glassborn",
            "Grew up among tide-cut observatories.",
        ),
        culture: origin(
            "lantern_ward",
            "Lantern Ward",
            "Learned to read weather by lamplight.",
        ),
        background: origin(
            "survey_runner",
            "Survey Runner",
            "Mapped shifting shorelines for a living.",
        ),
        modus_operandi: OpenAdventureModusOperandi {
            id: "careful_craft".to_owned(),
            label: "Careful Craft".to_owned(),
            score: 3,
        },
        attributes: OpenAdventureAttributes {
            agility: 3,
            empathy: 2,
            endurance: 3,
            focus: 3,
            insight: 4,
            might: 3,
            presence: 2,
            reasoning: 3,
            will: 4,
        },
        specializations: vec![
            OpenAdventureSpecialization {
                id: "shoreline_lore".to_owned(),
                label: "Shoreline Lore".to_owned(),
                attribute: OpenAdventureAttribute::Reasoning,
                bonus: 2,
            },
            OpenAdventureSpecialization {
                id: "steady_hands".to_owned(),
                label: "Steady Hands".to_owned(),
                attribute: OpenAdventureAttribute::Agility,
                bonus: 2,
            },
        ],
        favored_actions: vec![OpenAdventureFavoredAction {
            id: "chart_the_channel".to_owned(),
            label: "Chart the Channel".to_owned(),
            attribute: OpenAdventureAttribute::Reasoning,
        }],
        equipment: vec![
            OpenAdventureEquipment {
                id: "sounding_line".to_owned(),
                label: "Sounding Line".to_owned(),
                kind: OpenAdventureEquipmentKind::Tool,
            },
            OpenAdventureEquipment {
                id: "weathered_coat".to_owned(),
                label: "Weathered Coat".to_owned(),
                kind: OpenAdventureEquipmentKind::Armor,
            },
        ],
        benefits: vec![
            OpenAdventureTraitModifier {
                id: "long_practice".to_owned(),
                label: "Long Practice".to_owned(),
                target: OpenAdventureModifierTarget::ActionScore,
                modifier: 3,
            },
            OpenAdventureTraitModifier {
                id: "settled_nerves".to_owned(),
                label: "Settled Nerves".to_owned(),
                target: OpenAdventureModifierTarget::SpiritPoints,
                modifier: 2,
            },
        ],
        drawbacks: vec![OpenAdventureTraitModifier {
            id: "loud_boots".to_owned(),
            label: "Loud Boots".to_owned(),
            target: OpenAdventureModifierTarget::Reaction,
            modifier: -2,
        }],
    }
}

fn high_score_request() -> OpenAdventureCreationRequest {
    let mut request = creation_request();
    request.character_id = "org.weave.character.bram_kestrel".to_owned();
    request.name = "Bram Kestrel".to_owned();
    request.attributes = OpenAdventureAttributes {
        agility: 2,
        empathy: 2,
        endurance: 2,
        focus: 2,
        insight: 2,
        might: 5,
        presence: 2,
        reasoning: 2,
        will: 2,
    };
    request.specializations = vec![OpenAdventureSpecialization {
        id: "braced_strike".to_owned(),
        label: "Braced Strike".to_owned(),
        attribute: OpenAdventureAttribute::Might,
        bonus: 3,
    }];
    request.modus_operandi.score = 5;
    request.favored_actions.clear();
    request.benefits = vec![OpenAdventureTraitModifier {
        id: "iron_habit".to_owned(),
        label: "Iron Habit".to_owned(),
        target: OpenAdventureModifierTarget::ActionScore,
        modifier: 10,
    }];
    request.drawbacks.clear();
    request
}

fn execute(
    registry: &ResolverRegistry,
    preview: &OpenAdventureCreationPreview,
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
        .execute(&open_adventure_manifest(), &request, state)
        .unwrap();
    validate_resolution_receipt(&receipt, &open_adventure_manifest()).unwrap();
    (request, receipt)
}

fn environmental_input(
    id: &str,
    difficulty: u32,
    points: u32,
    effect: u32,
    disclosure: &str,
) -> DomainValue {
    object([
        ("challenge_id", DomainValue::String(id.to_owned())),
        ("challenge_points", DomainValue::Number(f64::from(points))),
        (
            "difficulty_score",
            DomainValue::Number(f64::from(difficulty)),
        ),
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("effect", DomainValue::Number(f64::from(effect))),
        ("kind", DomainValue::Symbol("environmental".to_owned())),
        (
            "label",
            DomainValue::String("Synthetic environmental challenge".to_owned()),
        ),
    ])
}

fn npc_input(
    id: &str,
    defense: u32,
    npc_effect: u32,
    target: &str,
    points: u32,
    effect: u32,
    disclosure: &str,
) -> DomainValue {
    object([
        ("challenge_id", DomainValue::String(id.to_owned())),
        ("challenge_points", DomainValue::Number(f64::from(points))),
        ("difficulty_score", DomainValue::Number(5.0)),
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("effect", DomainValue::Number(f64::from(effect))),
        ("kind", DomainValue::Symbol("npc".to_owned())),
        (
            "label",
            DomainValue::String("Synthetic NPC challenge".to_owned()),
        ),
        ("npc_defense", DomainValue::Number(f64::from(defense))),
        ("npc_effect", DomainValue::Number(f64::from(npc_effect))),
        ("npc_target", DomainValue::Symbol(target.to_owned())),
    ])
}

fn action_input(
    challenge_id: &str,
    attribute: &str,
    favored_action_id: Option<&str>,
    situation_modifier: i32,
    disclosure: &str,
) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "attribute".to_owned(),
            DomainValue::Symbol(attribute.to_owned()),
        ),
        (
            "challenge_id".to_owned(),
            DomainValue::String(challenge_id.to_owned()),
        ),
        (
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.to_owned()),
        ),
        (
            "situation_modifier".to_owned(),
            DomainValue::Number(f64::from(situation_modifier)),
        ),
    ]);
    if let Some(id) = favored_action_id {
        fields.insert(
            "favored_action_id".to_owned(),
            DomainValue::String(id.to_owned()),
        );
    }
    DomainValue::Object(fields)
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn value_fields(value: &DomainValue) -> &BTreeMap<String, DomainValue> {
    match value {
        DomainValue::Object(fields) => fields,
        _ => panic!("expected an object value"),
    }
}

fn value_number(fields: &BTreeMap<String, DomainValue>, name: &str) -> f64 {
    match fields.get(name) {
        Some(DomainValue::Number(value)) => *value,
        _ => panic!("expected number field {name}"),
    }
}

fn value_string<'a>(fields: &'a BTreeMap<String, DomainValue>, name: &str) -> &'a str {
    match fields.get(name) {
        Some(DomainValue::String(value)) => value,
        _ => panic!("expected string field {name}"),
    }
}

fn value_symbol<'a>(fields: &'a BTreeMap<String, DomainValue>, name: &str) -> &'a str {
    match fields.get(name) {
        Some(DomainValue::Symbol(value)) => value,
        _ => panic!("expected symbol field {name}"),
    }
}

fn value_bool(fields: &BTreeMap<String, DomainValue>, name: &str) -> bool {
    match fields.get(name) {
        Some(DomainValue::Bool(value)) => *value,
        _ => panic!("expected bool field {name}"),
    }
}

fn value_list<'a>(fields: &'a BTreeMap<String, DomainValue>, name: &str) -> &'a Vec<DomainValue> {
    match fields.get(name) {
        Some(DomainValue::List(values)) => values,
        _ => panic!("expected list field {name}"),
    }
}

fn state_fields(state: &TabletopState) -> &BTreeMap<String, DomainValue> {
    value_fields(&state.value)
}

fn event_payload<'a>(receipt: &'a ResolutionReceipt, kind: &str) -> &'a DomainValue {
    receipt
        .events
        .iter()
        .find(|event| event.kind == kind)
        .and_then(|event| event.payload.as_ref())
        .unwrap_or_else(|| panic!("missing event {kind}"))
}

fn registry() -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry
        .register(&open_adventure_manifest(), OpenAdventureResolver)
        .unwrap();
    registry
}

#[test]
fn manifest_is_valid_and_pins_the_in_repository_source_record() {
    let manifest = open_adventure_manifest();
    validate_adapter_manifest(&manifest).unwrap();
    assert_eq!(manifest.id, OPEN_ADVENTURE_ADAPTER_ID);
    assert_eq!(manifest.version, OPEN_ADVENTURE_ADAPTER_VERSION);
    assert_eq!(manifest.namespace, "open_adventure");
    assert_eq!(manifest.provenance.source_url, OPEN_ADVENTURE_SOURCE_URL);
    assert_eq!(manifest.provenance.revision, OPEN_ADVENTURE_EDITION);
    assert_eq!(
        manifest.provenance.sha256,
        OPEN_ADVENTURE_SOURCE_RECORD_SHA256
    );
    assert_eq!(
        manifest.provenance.license,
        OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN
    );
    assert_eq!(OPEN_ADVENTURE_LICENSE_STATEMENT, "CC0");
    assert!(OPEN_ADVENTURE_SOURCE_RECORD.contains("https://openadventurerpg.com/"));
    assert!(OPEN_ADVENTURE_SOURCE_RECORD.contains("CC0"));
    assert!(OPEN_ADVENTURE_SOURCE_RECORD.contains("Open Adventure: Fantasy"));
    assert_eq!(OPEN_ADVENTURE_SOURCE_RECORD_SHA256.len(), 64);
    assert_eq!(
        open_adventure_source_record_sha256(),
        OPEN_ADVENTURE_SOURCE_RECORD_SHA256
    );
    assert!(manifest.provenance.additional_artifacts.is_empty());
    assert_eq!(
        manifest.provenance.exclusions,
        vec![
            weave_tabletop::ExcludedMaterial::CommunitySupplements,
            weave_tabletop::ExcludedMaterial::Logos,
            weave_tabletop::ExcludedMaterial::TradeDress,
            weave_tabletop::ExcludedMaterial::Artwork,
            weave_tabletop::ExcludedMaterial::Layout,
            weave_tabletop::ExcludedMaterial::UnverifiedAssets,
        ]
    );
    assert_eq!(
        manifest
            .creation_steps
            .iter()
            .map(|step| step.id.as_str())
            .collect::<Vec<_>>(),
        ["attributes", "details", "identity"]
    );
    for capability in [
        TabletopCapability::CharacterCreation,
        TabletopCapability::DerivedValues,
        TabletopCapability::ChecksAndConflicts,
        TabletopCapability::ResourcesAndConditions,
        TabletopCapability::Encounters,
    ] {
        assert!(
            manifest
                .capabilities
                .iter()
                .any(|declaration| declaration.capability == capability)
        );
    }
    assert!(manifest.operations.contains_key("begin_challenge"));
    assert!(manifest.operations.contains_key("resolve_challenge_action"));
}

#[test]
fn creation_derives_every_named_value_and_round_trips_ron_and_json() {
    let request = creation_request();
    let preview = create_open_adventure_character(&request).unwrap();
    assert_eq!(
        preview.definition.completed_creation_steps,
        ["attributes", "details", "identity"]
    );
    assert_eq!(preview.initial_state.entropy.seed, 7);
    assert_eq!(preview.initial_state.entropy.cursor, 0);
    assert_eq!(preview.initial_state.revision, 0);
    assert_eq!(
        open_adventure_derived(&preview.definition.definition).unwrap(),
        preview.derived
    );

    assert_eq!(preview.derived.defense, 13);
    assert_eq!(preview.derived.reaction, 15);
    assert_eq!(preview.derived.body_points, 11);
    assert_eq!(preview.derived.mind_points, 11);
    assert_eq!(preview.derived.spirit_points, 13);

    let reasoning = preview
        .derived
        .action_scores
        .iter()
        .find(|score| score.attribute == OpenAdventureAttribute::Reasoning)
        .unwrap();
    assert_eq!(reasoning.base, 3);
    assert_eq!(reasoning.specialization, 2);
    assert_eq!(reasoning.modus_operandi, 3);
    assert_eq!(reasoning.benefits, 3);
    assert_eq!(reasoning.drawbacks, 0);
    assert_eq!(reasoning.total, 11);
    assert_eq!((reasoning.dice.count, reasoning.dice.sides), (4, 10));

    let empathy = preview
        .derived
        .action_scores
        .iter()
        .find(|score| score.attribute == OpenAdventureAttribute::Empathy)
        .unwrap();
    assert_eq!((empathy.dice.count, empathy.dice.sides), (3, 8));

    let fields = state_fields(&preview.initial_state);
    assert_eq!(value_number(fields, "body_points"), 11.0);
    assert_eq!(value_number(fields, "mind_points"), 11.0);
    assert_eq!(value_number(fields, "spirit_points"), 13.0);
    assert!(!value_bool(fields, "defeated"));
    assert!(value_fields(fields.get("challenges").unwrap()).is_empty());

    assert_eq!(
        OpenAdventureCreationRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        OpenAdventureCreationRequest::from_ron(&request.to_ron().unwrap()).unwrap(),
        request
    );
    assert_eq!(
        OpenAdventureCreationPreview::from_json(&preview.to_json().unwrap()).unwrap(),
        preview
    );
    assert_eq!(
        OpenAdventureCreationPreview::from_ron(&preview.to_ron().unwrap()).unwrap(),
        preview
    );
}

#[test]
fn action_dice_progression_and_exploding_dice_are_deterministic() {
    for (score, count, sides) in [
        (2, 1, 6),
        (3, 2, 6),
        (5, 2, 8),
        (7, 3, 8),
        (9, 3, 10),
        (11, 4, 10),
        (13, 4, 12),
        (15, 5, 12),
        (40, 5, 12),
    ] {
        let dice = open_adventure_action_dice(score);
        assert_eq!((dice.count, dice.sides), (count, sides));
    }

    let preview = create_open_adventure_character(&high_score_request()).unwrap();
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "explode_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        environmental_input("long_drill", 1, 1_000_000, 1, "public"),
    );
    let mut state = begun.after_state;
    let mut found_explosion = false;
    for index in 0..100 {
        let request_id = format!("explode_{index}");
        let (request, receipt) = execute(
            &registry,
            &preview,
            &state,
            &request_id,
            "resolve_challenge_action",
            TabletopCapability::ChecksAndConflicts,
            action_input("long_drill", "might", None, 0, "public"),
        );
        let authority = value_fields(event_payload(&receipt, "action_resolved_authority"));
        let dice = value_list(authority, "dice");
        assert_eq!(dice.len(), 5);
        for die in dice {
            let die_fields = value_fields(die);
            assert_eq!(value_number(die_fields, "sides"), 12.0);
            let rolls = value_list(die_fields, "rolls");
            assert!(!rolls.is_empty());
            if rolls.len() > 1 {
                found_explosion = true;
            }
        }
        if found_explosion {
            registry
                .replay(&open_adventure_manifest(), &request, &state, &receipt)
                .unwrap();
            break;
        }
        state = receipt.after_state;
    }
    assert!(
        found_explosion,
        "expected an exploding die within 100 rolls"
    );
}

#[test]
fn environmental_challenge_completes_and_exposes_structured_events() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "env_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        environmental_input("tide_channel", 12, 3, 1, "public"),
    );
    assert_eq!(begun.entropy_consumed, 0);
    assert_eq!(
        value_symbol(
            value_fields(event_payload(&begun, "challenge_begun")),
            "kind"
        ),
        "environmental"
    );
    assert_eq!(begun.after_state.revision, 1);

    let (first_request, first_receipt) = execute(
        &registry,
        &preview,
        &begun.after_state,
        "env_action_1",
        "resolve_challenge_action",
        TabletopCapability::ChecksAndConflicts,
        action_input(
            "tide_channel",
            "reasoning",
            Some("chart_the_channel"),
            0,
            "public",
        ),
    );
    assert!(first_receipt.entropy_consumed > 0);
    registry
        .replay(
            &open_adventure_manifest(),
            &first_request,
            &begun.after_state,
            &first_receipt,
        )
        .unwrap();
    let public = value_fields(event_payload(&first_receipt, "action_resolved"));
    assert_eq!(value_symbol(public, "attribute"), "reasoning");
    assert_eq!(
        value_string(public, "favored_action_id"),
        "chart_the_channel"
    );
    assert_eq!(value_number(public, "action_score"), 13.0);
    assert_eq!(value_number(public, "difficulty"), 12.0);
    assert_eq!(value_number(public, "effect"), 1.0);
    assert_eq!(value_number(public, "challenge_points_before"), 3.0);
    assert!(matches!(
        value_symbol(public, "outcome"),
        "success" | "failure"
    ));
    let modifiers = value_fields(public.get("modifiers").unwrap());
    assert_eq!(value_number(modifiers, "favored"), 2.0);
    assert_eq!(value_number(modifiers, "total"), 13.0);
    let dice = value_list(public, "dice");
    assert!(!dice.is_empty());
    for die in dice {
        let die_fields = value_fields(die);
        assert!(!value_list(die_fields, "rolls").is_empty());
        assert!(value_number(die_fields, "total") > 0.0);
    }

    let authority = value_fields(event_payload(&first_receipt, "action_resolved_authority"));
    assert_eq!(value_symbol(authority, "disclosure"), "public");
    let benefit_ids = value_list(authority, "benefit_ids");
    assert_eq!(benefit_ids.len(), 1);
    assert!(matches!(
        &benefit_ids[0],
        DomainValue::String(value) if value == "long_practice"
    ));
    assert!(value_list(authority, "drawback_ids").is_empty());

    let json = TabletopState::from_json(&first_receipt.after_state.to_json().unwrap()).unwrap();
    let ron = TabletopState::from_ron(&first_receipt.after_state.to_ron().unwrap()).unwrap();
    assert_eq!(json, first_receipt.after_state);
    assert_eq!(ron, first_receipt.after_state);

    let mut state = first_receipt.after_state;
    let mut completed = false;
    for index in 0..40 {
        if completed {
            break;
        }
        let request_id = format!("env_action_{}", index + 2);
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            &request_id,
            "resolve_challenge_action",
            TabletopCapability::ChecksAndConflicts,
            action_input(
                "tide_channel",
                "reasoning",
                Some("chart_the_channel"),
                0,
                "public",
            ),
        );
        let fields = state_fields(&receipt.after_state);
        let challenges = value_fields(fields.get("challenges").unwrap());
        let challenge = value_fields(challenges.get("tide_channel").unwrap());
        completed = value_bool(challenge, "completed");
        assert!(value_number(challenge, "remaining_challenge_points") >= 0.0);
        if completed {
            assert_eq!(value_number(challenge, "remaining_challenge_points"), 0.0);
            let completion = value_fields(event_payload(&receipt, "challenge_completed"));
            assert_eq!(value_string(completion, "challenge_id"), "tide_channel");
            assert_eq!(value_symbol(completion, "outcome"), "success");
        }
        state = receipt.after_state;
    }
    assert!(completed, "environmental challenge never completed");

    let finished = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "env_after_completion".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "resolve_challenge_action".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&state).unwrap(),
            input: action_input("tide_channel", "reasoning", None, 0, "public"),
        },
        &state,
    );
    assert!(
        finished.is_err(),
        "completed challenges must reject actions"
    );
}

#[test]
fn npc_challenge_completes_and_failed_actions_reduce_the_target_pool() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "npc_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        npc_input("reef_sentinel", 8, 1, "body", 2, 1, "public"),
    );
    let begun_fields = value_fields(event_payload(&begun, "challenge_begun"));
    assert_eq!(value_symbol(begun_fields, "kind"), "npc");
    assert_eq!(value_number(begun_fields, "npc_defense"), 8.0);
    assert_eq!(value_number(begun_fields, "npc_effect"), 1.0);
    assert_eq!(value_symbol(begun_fields, "npc_target"), "body");

    let mut state = begun.after_state;
    let mut completed = false;
    for index in 0..40 {
        if completed {
            break;
        }
        let request_id = format!("npc_action_{index}");
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            &request_id,
            "resolve_challenge_action",
            TabletopCapability::ChecksAndConflicts,
            action_input("reef_sentinel", "reasoning", None, 0, "public"),
        );
        let fields = state_fields(&receipt.after_state);
        let challenges = value_fields(fields.get("challenges").unwrap());
        let challenge = value_fields(challenges.get("reef_sentinel").unwrap());
        completed = value_bool(challenge, "completed");
        state = receipt.after_state;
    }
    assert!(completed, "NPC challenge never completed");

    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "npc_guard_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        npc_input("shield_guard", 501, 3, "body", 100, 1, "public"),
    );
    let body_before = value_number(state_fields(&begun.after_state), "body_points");
    let mut state = begun.after_state;
    let mut defeated = false;
    let mut final_receipt = None;
    for index in 0..10 {
        let request_id = format!("npc_guard_action_{index}");
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            &request_id,
            "resolve_challenge_action",
            TabletopCapability::ChecksAndConflicts,
            action_input("shield_guard", "reasoning", None, 0, "public"),
        );
        let authority = value_fields(event_payload(&receipt, "action_resolved_authority"));
        assert_eq!(value_symbol(authority, "outcome"), "failure");
        assert!(value_number(authority, "roll_total") <= 500.0);
        let fields = state_fields(&receipt.after_state);
        defeated = value_bool(fields, "defeated");
        state = receipt.after_state.clone();
        final_receipt = Some(receipt);
        if defeated {
            break;
        }
    }
    assert!(
        defeated,
        "guaranteed NPC failure did not defeat the character"
    );
    let fields = state_fields(&state);
    assert_eq!(value_number(fields, "body_points"), 0.0);
    assert!(value_number(fields, "body_points") < body_before);
    let receipt = final_receipt.unwrap();
    let defeat = value_fields(event_payload(&receipt, "character_defeated"));
    assert_eq!(value_symbol(defeat, "pool"), "body");
    assert_eq!(value_number(defeat, "remaining_points"), 0.0);
    assert_eq!(value_symbol(defeat, "outcome"), "failure");
}

#[test]
fn host_only_disclosure_emits_no_public_event() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "hidden_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        environmental_input("hidden_patrol", 10, 2, 1, "host_only"),
    );
    assert!(
        begun
            .events
            .iter()
            .all(|event| event.visibility == EventVisibility::HostOnly)
    );
    let (_, receipt) = execute(
        &registry,
        &preview,
        &begun.after_state,
        "hidden_action",
        "resolve_challenge_action",
        TabletopCapability::ChecksAndConflicts,
        action_input("hidden_patrol", "reasoning", None, 0, "host_only"),
    );
    assert!(
        receipt
            .events
            .iter()
            .all(|event| event.visibility == EventVisibility::HostOnly)
    );
    let authority = value_fields(event_payload(&receipt, "action_resolved_authority"));
    assert_eq!(value_symbol(authority, "disclosure"), "host_only");
}

#[test]
fn invalid_creation_and_resolution_inputs_fail_closed() {
    let mut request = creation_request();
    request.attributes.might = 0;
    assert!(create_open_adventure_character(&request).is_err());

    let mut request = creation_request();
    request.specializations.push(OpenAdventureSpecialization {
        id: "duplicate_attribute".to_owned(),
        label: "Duplicate Attribute".to_owned(),
        attribute: OpenAdventureAttribute::Reasoning,
        bonus: 1,
    });
    assert!(create_open_adventure_character(&request).is_err());

    let mut request = creation_request();
    request.benefits[0].modifier = -1;
    assert!(create_open_adventure_character(&request).is_err());

    let mut request = creation_request();
    request.drawbacks[0].modifier = 1;
    assert!(create_open_adventure_character(&request).is_err());

    let mut request = creation_request();
    let shared_id = request.drawbacks[0].id.clone();
    request.benefits[0].id = shared_id;
    assert!(create_open_adventure_character(&request).is_err());

    let mut request = creation_request();
    request.character_id = "not_namespaced".to_owned();
    assert!(create_open_adventure_character(&request).is_err());

    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let unknown = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "unknown_action".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "resolve_challenge_action".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
            input: action_input("missing", "reasoning", None, 0, "public"),
        },
        &preview.initial_state,
    );
    assert!(unknown.is_err());

    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "duplicate_begin",
        "begin_challenge",
        TabletopCapability::Encounters,
        environmental_input("duplicate", 10, 2, 1, "public"),
    );
    let duplicate = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "duplicate_begin_two".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "begin_challenge".to_owned(),
            capability: TabletopCapability::Encounters,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&begun.after_state).unwrap(),
            input: environmental_input("duplicate", 10, 2, 1, "public"),
        },
        &begun.after_state,
    );
    assert!(duplicate.is_err());

    let mut malformed_environmental = environmental_input("invalid_kind", 10, 2, 1, "public");
    if let DomainValue::Object(fields) = &mut malformed_environmental {
        fields.insert("npc_defense".to_owned(), DomainValue::Number(8.0));
    }
    let environmental_with_npc = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "invalid_environmental".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "begin_challenge".to_owned(),
            capability: TabletopCapability::Encounters,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
            input: malformed_environmental,
        },
        &preview.initial_state,
    );
    assert!(environmental_with_npc.is_err());

    let mismatch = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "mismatched_favored".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "resolve_challenge_action".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&begun.after_state).unwrap(),
            input: action_input("duplicate", "might", Some("chart_the_channel"), 0, "public"),
        },
        &begun.after_state,
    );
    assert!(mismatch.is_err());
}
