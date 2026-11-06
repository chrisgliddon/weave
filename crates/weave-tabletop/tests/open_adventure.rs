use std::collections::{BTreeMap, BTreeSet};

use weave_domain::DomainValue;
use weave_tabletop::{
    EventVisibility, OPEN_ADVENTURE_ADAPTER_ID, OPEN_ADVENTURE_ADAPTER_VERSION,
    OPEN_ADVENTURE_CREATION_FORMAT_VERSION, OPEN_ADVENTURE_EDITION,
    OPEN_ADVENTURE_ENCOUNTER_FORMAT_VERSION, OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN,
    OPEN_ADVENTURE_LICENSE_STATEMENT, OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES,
    OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256, OPEN_ADVENTURE_MODULE_FORMAT_VERSION,
    OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION, OPEN_ADVENTURE_NAMESPACE,
    OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES, OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256,
    OPEN_ADVENTURE_SOURCE_RECORD, OPEN_ADVENTURE_SOURCE_RECORD_SHA256, OPEN_ADVENTURE_SOURCE_URL,
    OpenAdventureAdversary, OpenAdventureAttribute, OpenAdventureAttributes,
    OpenAdventureCapabilityReplacement, OpenAdventureChallengeEntity, OpenAdventureChallengeKind,
    OpenAdventureCreationPreview, OpenAdventureCreationRequest, OpenAdventureDocumentation,
    OpenAdventureEffectiveModules, OpenAdventureEncounterDocument, OpenAdventureEncounterEquipment,
    OpenAdventureEncounterOutcomes, OpenAdventureEncounterParticipant, OpenAdventureEquipment,
    OpenAdventureEquipmentKind, OpenAdventureFavoredAction, OpenAdventureGoldenAdventure,
    OpenAdventureModifierTarget, OpenAdventureModuleArtifact, OpenAdventureModuleContentClass,
    OpenAdventureModuleDescriptor, OpenAdventureModuleExperience, OpenAdventureModuleSelection,
    OpenAdventureModusOperandi, OpenAdventureOrigin, OpenAdventureParticipantKind,
    OpenAdventurePool, OpenAdventureRange, OpenAdventureResolver, OpenAdventureReward,
    OpenAdventureRuntimeEncounter, OpenAdventureSpecialization, OpenAdventureSpell,
    OpenAdventureTraitModifier, OpenAdventureWound, RESOLVER_FORMAT_VERSION, ResolutionReceipt,
    ResolutionRequest, ResolverRegistry, TabletopCapability, TabletopState,
    apply_open_adventure_modules, canonical_fingerprint, create_open_adventure_character,
    open_adventure_action_dice, open_adventure_derived, open_adventure_documentation,
    open_adventure_golden_adventures, open_adventure_manifest, open_adventure_module_options,
    open_adventure_source_record_sha256, project_open_adventure_runtime_encounter,
    replay_open_adventure_golden, sha256_bytes, validate_adapter_manifest,
    validate_open_adventure_creation_preview, validate_open_adventure_encounter_document,
    validate_open_adventure_runtime_encounter, validate_resolution_receipt,
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

fn encounter_document() -> OpenAdventureEncounterDocument {
    OpenAdventureEncounterDocument {
        document_format_version: OPEN_ADVENTURE_ENCOUNTER_FORMAT_VERSION,
        namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
        id: "harbor_negotiation".to_owned(),
        label: "Harbor Negotiation".to_owned(),
        goal: "Convince the harbor council to open the eastern quay.".to_owned(),
        outcomes: OpenAdventureEncounterOutcomes {
            success: "The council opens the quay.".to_owned(),
            failure: "The council keeps the quay closed.".to_owned(),
            withdrawal: "The delegation leaves without a decision.".to_owned(),
        },
        ranges: vec![
            OpenAdventureRange {
                id: "near".to_owned(),
                label: "Near".to_owned(),
                order: 0,
            },
            OpenAdventureRange {
                id: "far".to_owned(),
                label: "Far".to_owned(),
                order: 1,
            },
        ],
        adversaries: vec![OpenAdventureAdversary {
            id: "reef_warden".to_owned(),
            label: "Reef Warden".to_owned(),
            defense: 9,
            armor: 1,
            body_points: 8,
            mind_points: 6,
            spirit_points: 6,
            attack_bonus: 8,
            effect: 2,
            effect_pool: OpenAdventurePool::Body,
            spells: vec![OpenAdventureSpell {
                id: "undertow".to_owned(),
                label: "Undertow".to_owned(),
                cost: 1,
                target: OpenAdventurePool::Spirit,
                effect: 2,
                range: "near".to_owned(),
            }],
        }],
        participants: vec![
            OpenAdventureEncounterParticipant {
                id: "envoy".to_owned(),
                label: "Envoy".to_owned(),
                kind: OpenAdventureParticipantKind::Character,
                range: "near".to_owned(),
                attribute: OpenAdventureAttribute::Presence,
                defense: 12,
                armor: 1,
                body_points: 9,
                mind_points: 9,
                spirit_points: 9,
                attack_bonus: 5,
                effect: 2,
                effect_pool: OpenAdventurePool::Mind,
                story_points: 3,
                wounds: vec![OpenAdventureWound {
                    id: "rattled".to_owned(),
                    label: "Rattled".to_owned(),
                    target: OpenAdventurePool::Mind,
                    severity: 1,
                    effect: "The envoy must steady their voice.".to_owned(),
                }],
                equipment: vec![
                    OpenAdventureEncounterEquipment {
                        id: "seal_ring".to_owned(),
                        label: "Seal Ring".to_owned(),
                        kind: OpenAdventureEquipmentKind::Tool,
                        attack_bonus: 1,
                        armor: 0,
                    },
                    OpenAdventureEncounterEquipment {
                        id: "speaker_staff".to_owned(),
                        label: "Speaker Staff".to_owned(),
                        kind: OpenAdventureEquipmentKind::Weapon,
                        attack_bonus: 1,
                        armor: 0,
                    },
                ],
                spells: vec![OpenAdventureSpell {
                    id: "clear_voice".to_owned(),
                    label: "Clear Voice".to_owned(),
                    cost: 1,
                    target: OpenAdventurePool::Spirit,
                    effect: 3,
                    range: "far".to_owned(),
                }],
                adversary_id: String::new(),
            },
            OpenAdventureEncounterParticipant {
                id: "warden".to_owned(),
                label: "Reef Warden".to_owned(),
                kind: OpenAdventureParticipantKind::Adversary,
                range: "far".to_owned(),
                attribute: OpenAdventureAttribute::Will,
                defense: 9,
                armor: 1,
                body_points: 8,
                mind_points: 6,
                spirit_points: 6,
                attack_bonus: 8,
                effect: 2,
                effect_pool: OpenAdventurePool::Body,
                story_points: 0,
                wounds: vec![OpenAdventureWound {
                    id: "battered".to_owned(),
                    label: "Battered".to_owned(),
                    target: OpenAdventurePool::Body,
                    severity: 2,
                    effect: "The warden loses footing.".to_owned(),
                }],
                equipment: Vec::new(),
                spells: vec![OpenAdventureSpell {
                    id: "undertow".to_owned(),
                    label: "Undertow".to_owned(),
                    cost: 1,
                    target: OpenAdventurePool::Spirit,
                    effect: 2,
                    range: "near".to_owned(),
                }],
                adversary_id: "reef_warden".to_owned(),
            },
        ],
        challenges: vec![OpenAdventureChallengeEntity {
            id: "open_quay".to_owned(),
            label: "Open the Quay".to_owned(),
            kind: OpenAdventureChallengeKind::Environmental,
            difficulty_score: 10,
            challenge_points: 2,
            effect: 1,
            participant_ids: vec!["envoy".to_owned()],
        }],
        turn_order: vec!["envoy".to_owned(), "warden".to_owned()],
        rewards: vec![OpenAdventureReward {
            id: "council_favor".to_owned(),
            label: "Council Favor".to_owned(),
            story_points: 2,
            experience: 3,
        }],
    }
}

#[test]
fn encounter_entities_validate_and_round_trip_ron_and_json() {
    let document = encounter_document();
    validate_open_adventure_encounter_document(&document).unwrap();
    assert_eq!(
        OpenAdventureEncounterDocument::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
    assert_eq!(
        OpenAdventureEncounterDocument::from_ron(&document.to_ron().unwrap()).unwrap(),
        document
    );
    let runtime = project_open_adventure_runtime_encounter(&document).unwrap();
    validate_open_adventure_runtime_encounter(&runtime).unwrap();
    assert_eq!(
        OpenAdventureRuntimeEncounter::from_json(&runtime.to_json().unwrap()).unwrap(),
        runtime
    );
    assert_eq!(
        OpenAdventureRuntimeEncounter::from_ron(&runtime.to_ron().unwrap()).unwrap(),
        runtime
    );
    assert_eq!(runtime.turn_order, document.turn_order);
    assert_eq!(
        runtime
            .participants
            .iter()
            .map(|participant| participant.id.as_str())
            .collect::<Vec<_>>(),
        ["envoy", "warden"]
    );
    assert_eq!(
        runtime
            .challenges
            .iter()
            .map(|challenge| challenge.id.as_str())
            .collect::<Vec<_>>(),
        ["open_quay"]
    );
    assert!(document.to_json().unwrap().contains("Reef Warden"));
    assert!(!runtime.to_json().unwrap().contains("Reef Warden"));
}

#[test]
fn encounter_dangling_references_report_exact_paths() {
    let mut document = encounter_document();
    document.participants[0].range = "missing_range".to_owned();
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants[0].range"), "{error}");

    let mut document = encounter_document();
    document.participants[1].adversary_id = "missing_adversary".to_owned();
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants[1].adversary_id"), "{error}");

    let mut document = encounter_document();
    document.participants[0].spells[0].range = "missing_range".to_owned();
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants[0].spells[0].range"), "{error}");

    let mut document = encounter_document();
    document.challenges[0].participant_ids = vec!["missing".to_owned()];
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("challenges[0].participant_ids[0]"),
        "{error}"
    );

    let mut document = encounter_document();
    document.turn_order[1] = "missing".to_owned();
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("turn_order[1]"), "{error}");

    let mut document = encounter_document();
    document.participants[0].adversary_id = "reef_warden".to_owned();
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants[0].adversary_id"), "{error}");

    let mut document = encounter_document();
    document.participants[1].defense = 10;
    let error = validate_open_adventure_encounter_document(&document)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants[1].adversary_id"), "{error}");
}

#[test]
fn encounter_ordering_is_deterministic_and_rejects_unsorted_entities() {
    let document = encounter_document();
    let mut reordered = document.clone();
    reordered.participants.swap(0, 1);
    let error = validate_open_adventure_encounter_document(&reordered)
        .unwrap_err()
        .to_string();
    assert!(error.contains("participants"), "{error}");

    let mut duplicate_order = document.clone();
    duplicate_order.ranges[1].order = 0;
    let error = validate_open_adventure_encounter_document(&duplicate_order)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ranges"), "{error}");

    let runtime = project_open_adventure_runtime_encounter(&document).unwrap();
    assert_eq!(runtime.ranges[0].order, 0);
    assert_eq!(runtime.ranges[1].order, 1);
    assert_eq!(runtime.turn_order, ["envoy", "warden"]);
}

fn document_json() -> String {
    encounter_document().to_json().unwrap()
}

fn start_input(encounter_id: &str, document_json: &str, disclosure: &str) -> DomainValue {
    object([
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        (
            "document_json",
            DomainValue::String(document_json.to_owned()),
        ),
        (
            "document_sha256",
            DomainValue::String(sha256_bytes(document_json.as_bytes())),
        ),
        ("encounter_id", DomainValue::String(encounter_id.to_owned())),
    ])
}

#[allow(clippy::too_many_arguments)]
fn encounter_action_input(
    encounter_id: &str,
    actor_id: &str,
    target_id: &str,
    target_kind: &str,
    action: &str,
    spell_id: Option<&str>,
    equipment_id: Option<&str>,
    situation_modifier: i32,
    spend_story_point: bool,
    disclosure: &str,
) -> DomainValue {
    let mut fields = BTreeMap::from([
        ("action".to_owned(), DomainValue::Symbol(action.to_owned())),
        (
            "actor_id".to_owned(),
            DomainValue::String(actor_id.to_owned()),
        ),
        (
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.to_owned()),
        ),
        (
            "encounter_id".to_owned(),
            DomainValue::String(encounter_id.to_owned()),
        ),
        (
            "situation_modifier".to_owned(),
            DomainValue::Number(f64::from(situation_modifier)),
        ),
        (
            "spend_story_point".to_owned(),
            DomainValue::Bool(spend_story_point),
        ),
        (
            "target_id".to_owned(),
            DomainValue::String(target_id.to_owned()),
        ),
        (
            "target_kind".to_owned(),
            DomainValue::Symbol(target_kind.to_owned()),
        ),
    ]);
    if let Some(id) = spell_id {
        fields.insert("spell_id".to_owned(), DomainValue::String(id.to_owned()));
    }
    if let Some(id) = equipment_id {
        fields.insert(
            "equipment_id".to_owned(),
            DomainValue::String(id.to_owned()),
        );
    }
    DomainValue::Object(fields)
}

fn end_input(encounter_id: &str, ending: &str, disclosure: &str) -> DomainValue {
    object([
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("encounter_id", DomainValue::String(encounter_id.to_owned())),
        ("ending", DomainValue::Symbol(ending.to_owned())),
    ])
}

fn heal_input(
    encounter_id: &str,
    participant_id: &str,
    pool: &str,
    amount: u32,
    disclosure: &str,
) -> DomainValue {
    object([
        ("amount", DomainValue::Number(f64::from(amount))),
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("encounter_id", DomainValue::String(encounter_id.to_owned())),
        (
            "participant_id",
            DomainValue::String(participant_id.to_owned()),
        ),
        ("pool", DomainValue::Symbol(pool.to_owned())),
    ])
}

fn advance_input(
    encounter_id: &str,
    participant_id: &str,
    attribute: &str,
    cost: u32,
    disclosure: &str,
) -> DomainValue {
    object([
        ("attribute", DomainValue::Symbol(attribute.to_owned())),
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("encounter_id", DomainValue::String(encounter_id.to_owned())),
        ("experience_cost", DomainValue::Number(f64::from(cost))),
        (
            "participant_id",
            DomainValue::String(participant_id.to_owned()),
        ),
    ])
}

fn encounter_state<'a>(
    state: &'a TabletopState,
    encounter_id: &str,
) -> &'a BTreeMap<String, DomainValue> {
    let fields = state_fields(state);
    let encounters = value_fields(fields.get("encounters").unwrap());
    value_fields(encounters.get(encounter_id).unwrap())
}

fn encounter_ended(state: &TabletopState, encounter_id: &str) -> bool {
    value_bool(encounter_state(state, encounter_id), "ended")
}

fn encounter_ending(state: &TabletopState, encounter_id: &str) -> String {
    value_string(encounter_state(state, encounter_id), "ending").to_owned()
}

fn current_actor(state: &TabletopState, encounter_id: &str) -> String {
    let encounter = encounter_state(state, encounter_id);
    let document =
        OpenAdventureEncounterDocument::from_json(value_string(encounter, "document")).unwrap();
    let index = value_number(encounter, "turn_index") as usize;
    document.turn_order[index].clone()
}

fn participant_field(
    state: &TabletopState,
    encounter_id: &str,
    participant_id: &str,
    field: &str,
) -> u32 {
    let encounter = encounter_state(state, encounter_id);
    let participants = value_fields(encounter.get("participants").unwrap());
    value_number(
        value_fields(participants.get(participant_id).unwrap()),
        field,
    ) as u32
}

fn challenge_completed(state: &TabletopState, encounter_id: &str, challenge_id: &str) -> bool {
    let encounter = encounter_state(state, encounter_id);
    let challenges = value_fields(encounter.get("challenges").unwrap());
    value_bool(
        value_fields(challenges.get(challenge_id).unwrap()),
        "completed",
    )
}

fn collect_event_kinds(receipt: &ResolutionReceipt, kinds: &mut BTreeSet<String>) {
    for event in &receipt.events {
        kinds.insert(event.kind.clone());
    }
}

#[test]
fn encounter_runs_through_combat_magic_injury_healing_rewards_and_advancement() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let mut kinds = BTreeSet::new();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "start_social",
        "start_encounter",
        TabletopCapability::Encounters,
        start_input("harbor_negotiation", &document_json(), "public"),
    );
    collect_event_kinds(&begun, &mut kinds);
    let mut state = begun.after_state;
    let mut spell_cast = false;
    for index in 0..60 {
        if encounter_ended(&state, "harbor_negotiation") {
            break;
        }
        let actor = current_actor(&state, "harbor_negotiation");
        let input = if actor == "envoy" {
            let warden_spirit =
                participant_field(&state, "harbor_negotiation", "warden", "spirit_points");
            if !spell_cast && warden_spirit > 2 {
                spell_cast = true;
                encounter_action_input(
                    "harbor_negotiation",
                    "envoy",
                    "warden",
                    "participant",
                    "spell",
                    Some("clear_voice"),
                    None,
                    0,
                    false,
                    "public",
                )
            } else if !challenge_completed(&state, "harbor_negotiation", "open_quay") {
                encounter_action_input(
                    "harbor_negotiation",
                    "envoy",
                    "open_quay",
                    "challenge",
                    "attack",
                    None,
                    Some("speaker_staff"),
                    0,
                    false,
                    "public",
                )
            } else {
                encounter_action_input(
                    "harbor_negotiation",
                    "envoy",
                    "warden",
                    "participant",
                    "attack",
                    None,
                    Some("speaker_staff"),
                    0,
                    false,
                    "public",
                )
            }
        } else {
            encounter_action_input(
                "harbor_negotiation",
                "warden",
                "envoy",
                "participant",
                "attack",
                None,
                None,
                0,
                false,
                "public",
            )
        };
        let request_id = format!("encounter_action_{index}");
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            &request_id,
            "resolve_encounter_action",
            TabletopCapability::Encounters,
            input,
        );
        collect_event_kinds(&receipt, &mut kinds);
        state = receipt.after_state;
    }
    assert!(
        encounter_ended(&state, "harbor_negotiation"),
        "encounter did not conclude: {kinds:?}"
    );
    assert_eq!(encounter_ending(&state, "harbor_negotiation"), "success");
    let advancement = value_fields(state_fields(&state).get("advancement").unwrap());
    assert!(value_number(advancement, "experience") >= 3.0);
    assert!(value_number(advancement, "story_points") >= 2.0);

    let (_, healed) = execute(
        &registry,
        &preview,
        &state,
        "heal_envoy",
        "heal_encounter_participant",
        TabletopCapability::ResourcesAndConditions,
        heal_input("harbor_negotiation", "envoy", "body", 2, "public"),
    );
    collect_event_kinds(&healed, &mut kinds);
    state = healed.after_state;

    let (_, advanced) = execute(
        &registry,
        &preview,
        &state,
        "advance_envoy",
        "advance_encounter_participant",
        TabletopCapability::Advancement,
        advance_input("harbor_negotiation", "envoy", "presence", 1, "public"),
    );
    collect_event_kinds(&advanced, &mut kinds);
    state = advanced.after_state;
    let advancement = value_fields(state_fields(&state).get("advancement").unwrap());
    assert_eq!(value_number(advancement, "rank"), 1.0);
    let bonuses = value_fields(advancement.get("attribute_bonuses").unwrap());
    assert_eq!(value_number(bonuses, "presence"), 1.0);

    assert_eq!(
        TabletopState::from_json(&state.to_json().unwrap()).unwrap(),
        state
    );
    assert_eq!(
        TabletopState::from_ron(&state.to_ron().unwrap()).unwrap(),
        state
    );
    for kind in [
        "encounter_started",
        "encounter_action_resolved",
        "encounter_ended",
        "encounter_reward_granted",
        "participant_healed",
        "advancement_recorded",
    ] {
        assert!(kinds.contains(kind), "missing event kind {kind}: {kinds:?}");
    }
}

#[test]
fn encounter_concludes_by_withdrawal_host_decision_and_defeat() {
    for ending in ["withdrawal", "host_decision", "defeat"] {
        let preview = create_open_adventure_character(&creation_request()).unwrap();
        let registry = registry();
        let (_, begun) = execute(
            &registry,
            &preview,
            &preview.initial_state,
            &format!("start_{ending}"),
            "start_encounter",
            TabletopCapability::Encounters,
            start_input("harbor_negotiation", &document_json(), "public"),
        );
        let (_, ended) = execute(
            &registry,
            &preview,
            &begun.after_state,
            &format!("end_{ending}"),
            "end_encounter",
            TabletopCapability::Encounters,
            end_input("harbor_negotiation", ending, "public"),
        );
        let state = ended.after_state;
        assert!(encounter_ended(&state, "harbor_negotiation"));
        assert_eq!(encounter_ending(&state, "harbor_negotiation"), ending);
        assert!(
            !ended
                .events
                .iter()
                .any(|event| event.kind == "encounter_reward_granted"),
            "reward was granted for an unsupported ending {ending}"
        );
        assert_eq!(
            TabletopState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
        assert_eq!(
            TabletopState::from_ron(&state.to_ron().unwrap()).unwrap(),
            state
        );
    }
}

#[test]
fn encounter_rejects_unearned_success_and_grants_earned_success() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "start_success_check",
        "start_encounter",
        TabletopCapability::Encounters,
        start_input("harbor_negotiation", &document_json(), "public"),
    );
    let unearned = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "end_unearned_success".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "end_encounter".to_owned(),
            capability: TabletopCapability::Encounters,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&begun.after_state).unwrap(),
            input: end_input("harbor_negotiation", "success", "public"),
        },
        &begun.after_state,
    );
    let error = unearned.unwrap_err().to_string();
    assert!(error.contains("does not satisfy"), "{error}");

    let mut state = begun.after_state.clone();
    let fields = match &mut state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("state must be an object"),
    };
    let encounters = match fields.get_mut("encounters") {
        Some(DomainValue::Object(encounters)) => encounters,
        _ => panic!("encounters must be an object"),
    };
    let encounter = match encounters.get_mut("harbor_negotiation") {
        Some(DomainValue::Object(encounter)) => encounter,
        _ => panic!("encounter must be an object"),
    };
    let challenges = match encounter.get_mut("challenges") {
        Some(DomainValue::Object(challenges)) => challenges,
        _ => panic!("challenges must be an object"),
    };
    let challenge = match challenges.get_mut("open_quay") {
        Some(DomainValue::Object(challenge)) => challenge,
        _ => panic!("challenge must be an object"),
    };
    challenge.insert("completed".to_owned(), DomainValue::Bool(true));
    challenge.insert(
        "remaining_challenge_points".to_owned(),
        DomainValue::Number(0.0),
    );
    let (_, earned) = execute(
        &registry,
        &preview,
        &state,
        "end_earned_success",
        "end_encounter",
        TabletopCapability::Encounters,
        end_input("harbor_negotiation", "success", "public"),
    );
    let state = earned.after_state;
    assert!(encounter_ended(&state, "harbor_negotiation"));
    assert_eq!(encounter_ending(&state, "harbor_negotiation"), "success");
    assert!(
        earned
            .events
            .iter()
            .any(|event| event.kind == "encounter_reward_granted")
    );
    let advancement = value_fields(state_fields(&state).get("advancement").unwrap());
    assert!(value_number(advancement, "experience") >= 3.0);
    assert!(value_number(advancement, "story_points") >= 2.0);
    assert_eq!(
        TabletopState::from_json(&state.to_json().unwrap()).unwrap(),
        state
    );
}

#[test]
fn encounter_condition_changes_and_defeat_are_structured() {
    let mut document = encounter_document();
    document.participants[0].wounds.push(OpenAdventureWound {
        id: "salted".to_owned(),
        label: "Salted".to_owned(),
        target: OpenAdventurePool::Body,
        severity: 1,
        effect: "Salt water slows the envoy.".to_owned(),
    });
    document.challenges[0].challenge_points = 100;
    document.adversaries[0].attack_bonus = 20;
    document.participants[1].attack_bonus = 20;
    let document_json = document.to_json().unwrap();
    let mut request = creation_request();
    request.drawbacks = vec![OpenAdventureTraitModifier {
        id: "exposed".to_owned(),
        label: "Exposed".to_owned(),
        target: OpenAdventureModifierTarget::Defense,
        modifier: -10,
    }];
    let preview = create_open_adventure_character(&request).unwrap();
    let mut initial_state = preview.initial_state.clone();
    let fields = match &mut initial_state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("state must be an object"),
    };
    fields.insert("body_points".to_owned(), DomainValue::Number(1.0));
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &initial_state,
        "start_fragile",
        "start_encounter",
        TabletopCapability::Encounters,
        start_input("harbor_negotiation", &document_json, "public"),
    );
    let (_, first) = execute(
        &registry,
        &preview,
        &begun.after_state,
        "fragile_envoy",
        "resolve_encounter_action",
        TabletopCapability::Encounters,
        encounter_action_input(
            "harbor_negotiation",
            "envoy",
            "open_quay",
            "challenge",
            "attack",
            None,
            Some("speaker_staff"),
            0,
            false,
            "public",
        ),
    );
    assert!(!encounter_ended(&first.after_state, "harbor_negotiation"));
    let (_, second) = execute(
        &registry,
        &preview,
        &first.after_state,
        "fragile_warden",
        "resolve_encounter_action",
        TabletopCapability::Encounters,
        encounter_action_input(
            "harbor_negotiation",
            "warden",
            "envoy",
            "participant",
            "attack",
            None,
            None,
            0,
            false,
            "public",
        ),
    );
    let condition = second
        .events
        .iter()
        .find(|event| event.kind == "encounter_condition_changed")
        .expect("missing condition event");
    let condition_fields = value_fields(condition.payload.as_ref().unwrap());
    assert!(value_bool(condition_fields, "incapacitated"));
    assert_eq!(value_symbol(condition_fields, "pool"), "body");
    assert_eq!(value_string(condition_fields, "wound_id"), "salted");
    let envoy_state = {
        let encounter = encounter_state(&second.after_state, "harbor_negotiation");
        let participants = value_fields(encounter.get("participants").unwrap());
        value_fields(participants.get("envoy").unwrap()).clone()
    };
    let wounds = value_list(&envoy_state, "wounds");
    assert!(wounds.iter().any(|wound| matches!(
        wound,
        DomainValue::String(id) if id.as_str() == "salted"
    )));
    assert!(encounter_ended(&second.after_state, "harbor_negotiation"));
    assert_eq!(
        encounter_ending(&second.after_state, "harbor_negotiation"),
        "defeat"
    );
    let ended = second
        .events
        .iter()
        .find(|event| event.kind == "encounter_ended")
        .expect("missing ending event");
    let ended_fields = value_fields(ended.payload.as_ref().unwrap());
    assert_eq!(value_symbol(ended_fields, "ending"), "defeat");

    let (_, healed) = execute(
        &registry,
        &preview,
        &second.after_state,
        "fragile_heal",
        "heal_encounter_participant",
        TabletopCapability::ResourcesAndConditions,
        heal_input("harbor_negotiation", "envoy", "body", 1, "public"),
    );
    let healed_fields = value_fields(event_payload(&healed, "participant_healed"));
    assert_eq!(value_number(healed_fields, "remaining_points"), 1.0);
    let condition = healed
        .events
        .iter()
        .find(|event| event.kind == "encounter_condition_changed")
        .expect("missing recovery condition event");
    let condition_fields = value_fields(condition.payload.as_ref().unwrap());
    assert!(!value_bool(condition_fields, "incapacitated"));
    assert!(!value_bool(condition_fields, "defeated"));
    assert_eq!(value_string(condition_fields, "wound_id"), "salted");
    let envoy_state = {
        let encounter = encounter_state(&healed.after_state, "harbor_negotiation");
        let participants = value_fields(encounter.get("participants").unwrap());
        value_fields(participants.get("envoy").unwrap()).clone()
    };
    assert!(value_list(&envoy_state, "wounds").is_empty());
}

#[test]
fn healing_a_different_pool_does_not_clear_mind_exhaustion() {
    let mut document = encounter_document();
    document.adversaries[0].attack_bonus = 20;
    document.adversaries[0].effect_pool = OpenAdventurePool::Mind;
    document.participants[1].attack_bonus = 20;
    document.participants[1].effect_pool = OpenAdventurePool::Mind;
    document.challenges[0].challenge_points = 100;
    let document_json = document.to_json().unwrap();

    let mut request = creation_request();
    request.drawbacks = vec![OpenAdventureTraitModifier {
        id: "exposed".to_owned(),
        label: "Exposed".to_owned(),
        target: OpenAdventureModifierTarget::Defense,
        modifier: -10,
    }];
    let preview = create_open_adventure_character(&request).unwrap();
    let mut initial_state = preview.initial_state.clone();
    let fields = match &mut initial_state.value {
        DomainValue::Object(fields) => fields,
        _ => panic!("state must be an object"),
    };
    fields.insert("mind_points".to_owned(), DomainValue::Number(1.0));
    let registry = registry();
    let (_, begun) = execute(
        &registry,
        &preview,
        &initial_state,
        "start_mind_exhaustion",
        "start_encounter",
        TabletopCapability::Encounters,
        start_input("harbor_negotiation", &document_json, "public"),
    );
    let (_, first) = execute(
        &registry,
        &preview,
        &begun.after_state,
        "mind_exhaustion_envoy",
        "resolve_encounter_action",
        TabletopCapability::Encounters,
        encounter_action_input(
            "harbor_negotiation",
            "envoy",
            "open_quay",
            "challenge",
            "attack",
            None,
            Some("speaker_staff"),
            0,
            false,
            "public",
        ),
    );
    let (_, second) = execute(
        &registry,
        &preview,
        &first.after_state,
        "mind_exhaustion_warden",
        "resolve_encounter_action",
        TabletopCapability::Encounters,
        encounter_action_input(
            "harbor_negotiation",
            "warden",
            "envoy",
            "participant",
            "attack",
            None,
            None,
            0,
            false,
            "public",
        ),
    );
    let envoy_state = {
        let encounter = encounter_state(&second.after_state, "harbor_negotiation");
        let participants = value_fields(encounter.get("participants").unwrap());
        value_fields(participants.get("envoy").unwrap()).clone()
    };
    assert!(value_bool(&envoy_state, "incapacitated"));
    assert!(value_bool(&envoy_state, "defeated"));
    assert_eq!(value_string(&envoy_state, "exhausted_pool"), "mind");
    assert_eq!(value_number(&envoy_state, "mind_points"), 0.0);

    let (_, healed_body) = execute(
        &registry,
        &preview,
        &second.after_state,
        "heal_body_not_mind",
        "heal_encounter_participant",
        TabletopCapability::ResourcesAndConditions,
        heal_input("harbor_negotiation", "envoy", "body", 1, "public"),
    );
    let envoy_state = {
        let encounter = encounter_state(&healed_body.after_state, "harbor_negotiation");
        let participants = value_fields(encounter.get("participants").unwrap());
        value_fields(participants.get("envoy").unwrap()).clone()
    };
    assert!(value_bool(&envoy_state, "incapacitated"));
    assert!(value_bool(&envoy_state, "defeated"));
    assert_eq!(value_string(&envoy_state, "exhausted_pool"), "mind");

    let (_, healed_mind) = execute(
        &registry,
        &preview,
        &healed_body.after_state,
        "heal_mind",
        "heal_encounter_participant",
        TabletopCapability::ResourcesAndConditions,
        heal_input("harbor_negotiation", "envoy", "mind", 1, "public"),
    );
    let envoy_state = {
        let encounter = encounter_state(&healed_mind.after_state, "harbor_negotiation");
        let participants = value_fields(encounter.get("participants").unwrap());
        value_fields(participants.get("envoy").unwrap()).clone()
    };
    assert!(!value_bool(&envoy_state, "incapacitated"));
    assert!(!value_bool(&envoy_state, "defeated"));
    assert_eq!(value_string(&envoy_state, "exhausted_pool"), "");
    assert!(value_list(&envoy_state, "wounds").is_empty());
}

fn module_id(id: &str) -> String {
    format!("{OPEN_ADVENTURE_NAMESPACE}.{id}")
}

fn module_descriptor(
    id: &str,
    experience: OpenAdventureModuleExperience,
) -> OpenAdventureModuleDescriptor {
    OpenAdventureModuleDescriptor {
        module_format_version: OPEN_ADVENTURE_MODULE_FORMAT_VERSION,
        id: module_id(id),
        version: "1.0.0".to_owned(),
        namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
        title: format!("{id} module"),
        scope: "Original synthetic optional module used by adapter tests.".to_owned(),
        experience,
        provides_capabilities: vec![TabletopCapability::Clocks],
        replaces_capabilities: Vec::new(),
        requires_modules: Vec::new(),
        incompatible_modules: Vec::new(),
        source_url: "https://openadventurerpg.com/".to_owned(),
        license: "CC0".to_owned(),
        content_class: OpenAdventureModuleContentClass::OfficialCommunityOwned,
        sha256: "a".repeat(64),
    }
}

fn module_selection(
    experience: OpenAdventureModuleExperience,
    enabled: &[&str],
) -> OpenAdventureModuleSelection {
    let mut enabled_modules: Vec<String> = enabled.iter().map(|id| module_id(id)).collect();
    enabled_modules.sort();
    OpenAdventureModuleSelection {
        selection_format_version: OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION,
        experience,
        enabled_modules,
    }
}

fn tide_clocks_module() -> OpenAdventureModuleDescriptor {
    let mut module = module_descriptor("tide_clocks", OpenAdventureModuleExperience::Extended);
    module.title = "Tide Clocks".to_owned();
    module.sha256 = OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256.to_owned();
    module
}

fn tide_clocks_artifact() -> OpenAdventureModuleArtifact {
    OpenAdventureModuleArtifact {
        module_id: module_id("tide_clocks"),
        bytes: OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES.to_vec(),
    }
}

fn tide_replacement_module() -> OpenAdventureModuleDescriptor {
    let mut module = module_descriptor("tide_replacement", OpenAdventureModuleExperience::Extended);
    module.title = "Tide Replacement".to_owned();
    module.provides_capabilities = vec![TabletopCapability::ChecksAndConflicts];
    module.replaces_capabilities = vec![TabletopCapability::ChecksAndConflicts];
    module.sha256 = OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256.to_owned();
    module
}

fn tide_replacement_artifact() -> OpenAdventureModuleArtifact {
    OpenAdventureModuleArtifact {
        module_id: module_id("tide_replacement"),
        bytes: OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES.to_vec(),
    }
}

#[test]
fn module_activation_round_trips_and_leaves_core_behavior_unchanged() {
    let manifest = open_adventure_manifest();
    let core: Vec<TabletopCapability> = manifest
        .capabilities
        .iter()
        .filter(|declaration| declaration.version == 1)
        .map(|declaration| declaration.capability)
        .collect();
    let available = vec![tide_clocks_module()];
    let artifacts = vec![tide_clocks_artifact()];
    let inactive = module_selection(OpenAdventureModuleExperience::Extended, &[]);
    let effective =
        apply_open_adventure_modules(&available, &artifacts, &inactive, &manifest).unwrap();
    assert_eq!(effective.capabilities, core);
    assert!(effective.replaced_capabilities.is_empty());

    let active = module_selection(OpenAdventureModuleExperience::Extended, &["tide_clocks"]);
    let effective =
        apply_open_adventure_modules(&available, &artifacts, &active, &manifest).unwrap();
    assert!(effective.capabilities.contains(&TabletopCapability::Clocks));
    assert_eq!(effective.enabled_modules, active.enabled_modules);
    assert_eq!(
        OpenAdventureModuleSelection::from_json(&active.to_json().unwrap()).unwrap(),
        active
    );
    assert_eq!(
        OpenAdventureModuleSelection::from_ron(&active.to_ron().unwrap()).unwrap(),
        active
    );
    assert_eq!(
        OpenAdventureEffectiveModules::from_json(&effective.to_json().unwrap()).unwrap(),
        effective
    );

    let inactive_again =
        apply_open_adventure_modules(&available, &artifacts, &inactive, &manifest).unwrap();
    assert_eq!(inactive_again.capabilities, core);
}

#[test]
fn module_can_add_and_explicitly_replace_capabilities() {
    let manifest = open_adventure_manifest();
    let add = tide_clocks_module();
    let replace = tide_replacement_module();
    let available = vec![add, replace];
    let artifacts = vec![tide_clocks_artifact(), tide_replacement_artifact()];
    let selection = module_selection(
        OpenAdventureModuleExperience::Extended,
        &["tide_clocks", "tide_replacement"],
    );
    let effective =
        apply_open_adventure_modules(&available, &artifacts, &selection, &manifest).unwrap();
    assert!(effective.capabilities.contains(&TabletopCapability::Clocks));
    assert!(
        effective
            .capabilities
            .contains(&TabletopCapability::ChecksAndConflicts)
    );
    assert_eq!(
        effective.replaced_capabilities,
        vec![OpenAdventureCapabilityReplacement {
            capability: TabletopCapability::ChecksAndConflicts,
            module_id: module_id("tide_replacement"),
        }]
    );
}

#[test]
fn unverified_and_incompatible_module_sets_are_refused_with_actionable_diagnostics() {
    let manifest = open_adventure_manifest();

    let mut excluded = module_descriptor("excluded", OpenAdventureModuleExperience::Extended);
    excluded.content_class = OpenAdventureModuleContentClass::Excluded;
    let errors = apply_open_adventure_modules(
        &[excluded],
        &[],
        &module_selection(OpenAdventureModuleExperience::Extended, &["excluded"]),
        &manifest,
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|diagnostic| diagnostic.code == "OA201"
            && diagnostic.message.contains("Community-Owned"))
    );

    let mut unlicensed = module_descriptor("unlicensed", OpenAdventureModuleExperience::Extended);
    unlicensed.license = "MIT".to_owned();
    let errors = apply_open_adventure_modules(
        &[unlicensed],
        &[],
        &module_selection(OpenAdventureModuleExperience::Extended, &["unlicensed"]),
        &manifest,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|diagnostic| diagnostic.code == "OA201" && diagnostic.path.contains("license"))
    );

    let errors = apply_open_adventure_modules(
        &[],
        &[],
        &module_selection(OpenAdventureModuleExperience::Extended, &["missing"]),
        &manifest,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|diagnostic| diagnostic.code == "OA202" && diagnostic.message.contains("missing"))
    );

    let mut requires = module_descriptor("requires", OpenAdventureModuleExperience::Extended);
    requires.requires_modules = vec![module_id("missing")];
    let errors = apply_open_adventure_modules(
        &[requires],
        &[],
        &module_selection(OpenAdventureModuleExperience::Extended, &["requires"]),
        &manifest,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|diagnostic| diagnostic.code == "OA203" && diagnostic.message.contains("missing"))
    );

    let mut first = module_descriptor("first", OpenAdventureModuleExperience::Extended);
    first.incompatible_modules = vec![module_id("second")];
    let second = module_descriptor("second", OpenAdventureModuleExperience::Extended);
    let errors = apply_open_adventure_modules(
        &[first, second],
        &[],
        &module_selection(
            OpenAdventureModuleExperience::Extended,
            &["first", "second"],
        ),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.code == "OA204"
            && diagnostic.message.contains("first")
            && diagnostic.message.contains("second")
    }));

    let mut shadow = module_descriptor("shadow", OpenAdventureModuleExperience::Extended);
    shadow.provides_capabilities = vec![TabletopCapability::ChecksAndConflicts];
    let errors = apply_open_adventure_modules(
        &[shadow],
        &[],
        &module_selection(OpenAdventureModuleExperience::Extended, &["shadow"]),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic.code == "OA205"));

    let extended = module_descriptor("extended", OpenAdventureModuleExperience::Extended);
    let errors = apply_open_adventure_modules(
        &[extended],
        &[],
        &module_selection(OpenAdventureModuleExperience::Core, &["extended"]),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic.code == "OA208"));
}

#[test]
fn module_options_follow_the_selected_experience() {
    let core = module_descriptor("core_option", OpenAdventureModuleExperience::Core);
    let extended = module_descriptor("extended_option", OpenAdventureModuleExperience::Extended);
    let available = vec![extended.clone(), core.clone()];
    assert_eq!(
        open_adventure_module_options(&available, OpenAdventureModuleExperience::Core),
        vec![core.clone()]
    );
    assert_eq!(
        open_adventure_module_options(&available, OpenAdventureModuleExperience::Extended),
        vec![core, extended]
    );
}

#[test]
fn contributor_can_build_a_minimal_verified_module_without_core_changes() {
    let module = tide_clocks_module();
    let artifact = tide_clocks_artifact();
    assert_eq!(
        OpenAdventureModuleDescriptor::from_json(&module.to_json().unwrap()).unwrap(),
        module
    );
    assert_eq!(
        OpenAdventureModuleDescriptor::from_ron(&module.to_ron().unwrap()).unwrap(),
        module
    );
    let manifest = open_adventure_manifest();
    let effective = apply_open_adventure_modules(
        &[module],
        &[artifact],
        &module_selection(OpenAdventureModuleExperience::Extended, &["tide_clocks"]),
        &manifest,
    )
    .unwrap();
    assert!(effective.capabilities.contains(&TabletopCapability::Clocks));
}

#[test]
fn golden_adventure_replays_deterministically_and_round_trips_ron_and_json() {
    let adventures = open_adventure_golden_adventures();
    assert!(!adventures.is_empty());
    for adventure in &adventures {
        assert_eq!(
            adventure.golden_format_version,
            weave_tabletop::OPEN_ADVENTURE_GOLDEN_FORMAT_VERSION
        );
        let registry = registry();
        let first = replay_open_adventure_golden(adventure, &registry).unwrap();
        let second = replay_open_adventure_golden(adventure, &registry).unwrap();
        assert_eq!(first, second, "golden replay is not deterministic");
        assert_eq!(
            encounter_ending(&first.state, &adventure.document.id),
            adventure.expected_ending
        );
        let unique: BTreeSet<String> = first.event_kinds.iter().cloned().collect();
        let expected: BTreeSet<String> = adventure.expected_event_kinds.iter().cloned().collect();
        assert_eq!(unique, expected, "golden event kinds changed");
        for module in &adventure.modules {
            for capability in &module.provides_capabilities {
                assert!(
                    first.effective_modules.capabilities.contains(capability),
                    "golden module capability {capability:?} is missing"
                );
            }
        }
        assert_eq!(
            TabletopState::from_json(&first.state.to_json().unwrap()).unwrap(),
            first.state
        );
        assert_eq!(
            TabletopState::from_ron(&first.state.to_ron().unwrap()).unwrap(),
            first.state
        );

        let json_adventure =
            OpenAdventureGoldenAdventure::from_json(&adventure.to_json().unwrap()).unwrap();
        let ron_adventure =
            OpenAdventureGoldenAdventure::from_ron(&adventure.to_ron().unwrap()).unwrap();
        assert_eq!(json_adventure, *adventure);
        assert_eq!(ron_adventure, *adventure);
        let json_replay = replay_open_adventure_golden(&json_adventure, &registry).unwrap();
        let ron_replay = replay_open_adventure_golden(&ron_adventure, &registry).unwrap();
        assert_eq!(json_replay.state, ron_replay.state);
        assert_eq!(json_replay.event_kinds, ron_replay.event_kinds);
    }
}

#[test]
fn documentation_lists_scope_modules_exclusions_and_contributor_steps() {
    let adventure = &open_adventure_golden_adventures()[0];
    let documentation = open_adventure_documentation(&adventure.modules);
    assert!(
        documentation
            .supported_scope
            .iter()
            .any(|line| line.contains("Character creation"))
    );
    assert!(
        documentation
            .supported_scope
            .iter()
            .any(|line| line.contains("Encounters"))
    );
    assert!(
        documentation
            .optional_modules
            .iter()
            .any(|module| module.id == adventure.modules[0].id)
    );
    assert!(
        documentation
            .exclusions
            .iter()
            .any(|line| line.contains("Masterwork Edition layout"))
    );
    assert!(
        documentation
            .exclusions
            .iter()
            .any(|line| line.contains("third-party supplements"))
    );
    assert!(
        documentation
            .exclusions
            .iter()
            .any(|line| line.contains("unmarked or community-added content"))
    );
    assert!(
        documentation
            .compatibility_statement
            .contains("does not imply endorsement")
    );
    assert!(
        documentation
            .contributor_steps
            .iter()
            .any(|step| step.contains("apply_open_adventure_modules"))
    );
    assert!(
        documentation
            .contributor_steps
            .iter()
            .any(|step| step.contains("replaces_capabilities"))
    );
    assert!(documentation.golden_adventure_ids.contains(&adventure.id));
    assert_eq!(
        OpenAdventureDocumentation::from_json(&documentation.to_json().unwrap()).unwrap(),
        documentation
    );
    assert_eq!(
        OpenAdventureDocumentation::from_ron(&documentation.to_ron().unwrap()).unwrap(),
        documentation
    );
}

#[test]
fn preview_rejects_duplicate_specializations_for_one_attribute() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let mut mutated = preview.clone();
    let fields = match &mut mutated.definition.definition {
        DomainValue::Object(fields) => fields,
        _ => panic!("definition must be an object"),
    };
    let specializations = match fields.get_mut("specializations") {
        Some(DomainValue::Object(specializations)) => specializations,
        _ => panic!("specializations must be an object"),
    };
    specializations.insert(
        "duplicate_agility".to_owned(),
        object([
            ("attribute", DomainValue::Symbol("agility".to_owned())),
            ("bonus", DomainValue::Number(3.0)),
            ("id", DomainValue::String("duplicate_agility".to_owned())),
            ("label", DomainValue::String("Duplicate Agility".to_owned())),
        ]),
    );
    mutated.definition.definition_sha256 =
        canonical_fingerprint(&mutated.definition.definition).unwrap();
    mutated.initial_state.definition_sha256 = mutated.definition.definition_sha256.clone();
    assert!(
        open_adventure_derived(&mutated.definition.definition).is_err(),
        "duplicate specialization attributes must not produce derived values"
    );
    let error = validate_open_adventure_creation_preview(&mutated)
        .unwrap_err()
        .to_string();
    assert!(error.contains("specializations"), "{error}");
}

#[test]
fn golden_adventure_covers_social_exploration_combat_magic_injury_recovery_and_advancement() {
    let adventure = &open_adventure_golden_adventures()[0];
    assert!(adventure.document.challenges.len() >= 2);
    assert!(
        adventure
            .document
            .participants
            .iter()
            .any(|participant| participant.kind == OpenAdventureParticipantKind::Character)
    );
    assert!(
        adventure
            .document
            .participants
            .iter()
            .any(|participant| participant.kind == OpenAdventureParticipantKind::Adversary)
    );
    assert!(
        adventure
            .document
            .participants
            .iter()
            .any(|participant| !participant.spells.is_empty())
    );
    assert!(
        adventure
            .steps
            .iter()
            .any(|step| step.operation == "heal_encounter_participant")
    );
    assert!(
        adventure
            .steps
            .iter()
            .any(|step| step.operation == "advance_encounter_participant")
    );
    assert!(adventure.steps.iter().any(|step| {
        matches!(
            &step.input,
            DomainValue::Object(fields)
                if matches!(
                    fields.get("action"),
                    Some(DomainValue::Symbol(action)) if action == "spell"
                )
        )
    }));
    assert!(adventure.steps.iter().any(|step| {
        matches!(
            &step.input,
            DomainValue::Object(fields)
                if matches!(
                    fields.get("actor_id"),
                    Some(DomainValue::String(actor)) if actor == "warden"
                )
        )
    }));
    assert!(!adventure.module_selection.enabled_modules.is_empty());
    assert!(!adventure.expected_event_kinds.is_empty());
    assert!(!adventure.expected_ending.is_empty());
}

#[test]
fn encounter_start_rejects_a_document_digest_mismatch() {
    let preview = create_open_adventure_character(&creation_request()).unwrap();
    let registry = registry();
    let mut input = start_input("harbor_negotiation", &document_json(), "public");
    if let DomainValue::Object(fields) = &mut input {
        fields.insert(
            "document_sha256".to_owned(),
            DomainValue::String("0".repeat(64)),
        );
    }
    let result = registry.execute(
        &open_adventure_manifest(),
        &ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: "bad_document_digest".to_owned(),
            adapter: preview.adapter.clone(),
            operation: "start_encounter".to_owned(),
            capability: TabletopCapability::Encounters,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&preview.initial_state).unwrap(),
            input,
        },
        &preview.initial_state,
    );
    let error = result.unwrap_err().to_string();
    assert!(error.contains("document digest"), "{error}");
}

#[test]
fn runtime_encounter_validation_rejects_malformed_stats_and_rewards() {
    let document = encounter_document();
    let runtime = project_open_adventure_runtime_encounter(&document).unwrap();
    validate_open_adventure_runtime_encounter(&runtime).unwrap();

    let mut malformed = runtime.clone();
    malformed.participants[0].defense = 0;
    assert!(validate_open_adventure_runtime_encounter(&malformed).is_err());

    let mut malformed = runtime.clone();
    malformed.rewards[0].experience = u32::MAX;
    assert!(validate_open_adventure_runtime_encounter(&malformed).is_err());

    let mut malformed = runtime;
    malformed.challenges[0].participant_ids = vec!["warden".to_owned(), "envoy".to_owned()];
    assert!(validate_open_adventure_runtime_encounter(&malformed).is_err());
}

#[test]
fn module_artifacts_allowlist_and_publisher_origin_fail_closed() {
    let manifest = open_adventure_manifest();

    let module = tide_clocks_module();
    let wrong_artifact = OpenAdventureModuleArtifact {
        module_id: module_id("tide_clocks"),
        bytes: b"wrong artifact bytes".to_vec(),
    };
    let errors = apply_open_adventure_modules(
        &[module],
        &[wrong_artifact],
        &module_selection(OpenAdventureModuleExperience::Extended, &["tide_clocks"]),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic.code == "OA211"));

    let mut unverified = module_descriptor(
        "unverified_but_valid",
        OpenAdventureModuleExperience::Extended,
    );
    unverified.sha256 = OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256.to_owned();
    let artifact = OpenAdventureModuleArtifact {
        module_id: module_id("unverified_but_valid"),
        bytes: OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES.to_vec(),
    };
    let errors = apply_open_adventure_modules(
        &[unverified],
        &[artifact],
        &module_selection(
            OpenAdventureModuleExperience::Extended,
            &["unverified_but_valid"],
        ),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic.code == "OA210"));

    let mut off_origin = tide_clocks_module();
    off_origin.source_url = "https://example.com/module".to_owned();
    let errors = apply_open_adventure_modules(
        &[off_origin],
        &[tide_clocks_artifact()],
        &module_selection(OpenAdventureModuleExperience::Extended, &["tide_clocks"]),
        &manifest,
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.code == "OA201" && diagnostic.path.contains("source_url")
    }));
}
