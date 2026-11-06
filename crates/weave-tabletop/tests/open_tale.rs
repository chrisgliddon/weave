//! End-to-end tests for the Open Tale adapter.
//!
//! Fixtures use original synthetic names and prose. The upstream rules reference contributes no
//! copied text, artwork, or character names.

use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    AdapterSourceClass, EntropyState, EntropyStream, EventVisibility, OPEN_TALE_ADAPTER_ID,
    OPEN_TALE_EDITION, OPEN_TALE_LICENSE_TEXT, OPEN_TALE_LICENSE_TEXT_SHA256,
    OPEN_TALE_SOURCE_RECORD, OPEN_TALE_SOURCE_SHA256, OpenTaleAttributes, OpenTaleCreationPreview,
    OpenTaleCreationRequest, OpenTaleItemKind, OpenTaleOutcomeBand, OpenTaleQuality,
    OpenTaleResolver, OpenTaleRollInput, OpenTaleRollMode, OpenTaleSkillKind,
    OpenTaleSkillSelection, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, TabletopCapability, TabletopError, TabletopEvent, TabletopResolver,
    TabletopState, canonical_fingerprint, create_open_tale_character, has_capability,
    open_tale_item_catalog, open_tale_manifest, open_tale_outcome_band, open_tale_quality_formulas,
    open_tale_resolve_roll, open_tale_skill_catalog, sha256_bytes, validate_adapter_manifest,
    validate_open_tale_creation_preview,
};

fn manifest() -> weave_tabletop::AdapterManifest {
    open_tale_manifest()
}

fn creation_request() -> OpenTaleCreationRequest {
    OpenTaleCreationRequest {
        attributes: OpenTaleAttributes {
            dexterity: 3,
            intelligence: 1,
            magicy: 1,
            motion: 3,
            perception: 2,
            persuasion: 1,
            strength: 3,
            vitality: 3,
            volition: 1,
        },
        character_id: "org.weave.fixture.rowan".to_owned(),
        creation_format_version: 1,
        experience: 0,
        heroic_badges: 1,
        items: vec!["short_blade".to_owned(), "travel_cloak".to_owned()],
        level: 1,
        name: "Rowan Vex".to_owned(),
        seed: 42,
        skills: vec![
            OpenTaleSkillSelection {
                id: "athletics".to_owned(),
                rank: 2,
            },
            OpenTaleSkillSelection {
                id: "composure".to_owned(),
                rank: 2,
            },
        ],
        starting_wealth: 6,
    }
}

fn preview() -> OpenTaleCreationPreview {
    create_open_tale_character(&creation_request()).expect("creation succeeds")
}

fn registry() -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry
        .register(&manifest(), OpenTaleResolver)
        .expect("resolver registers");
    registry
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn number(value: u32) -> DomainValue {
    DomainValue::Number(f64::from(value))
}

fn signed(value: i32) -> DomainValue {
    DomainValue::Number(f64::from(value))
}

fn text(value: &str) -> DomainValue {
    DomainValue::String(value.to_owned())
}

fn symbol(value: &str) -> DomainValue {
    DomainValue::Symbol(value.to_owned())
}

fn field<'a>(value: &'a DomainValue, name: &str) -> &'a DomainValue {
    match value {
        DomainValue::Object(fields) => fields.get(name).expect("declared field"),
        _ => panic!("expected object"),
    }
}

fn field_number(value: &DomainValue, name: &str) -> f64 {
    match field(value, name) {
        DomainValue::Number(number) => *number,
        _ => panic!("expected number at {name}"),
    }
}

fn field_text<'a>(value: &'a DomainValue, name: &str) -> &'a str {
    match field(value, name) {
        DomainValue::String(text) => text,
        _ => panic!("expected string at {name}"),
    }
}

fn field_symbol<'a>(value: &'a DomainValue, name: &str) -> &'a str {
    match field(value, name) {
        DomainValue::Symbol(symbol) => symbol,
        _ => panic!("expected symbol at {name}"),
    }
}

fn field_map<'a>(value: &'a DomainValue, name: &str) -> &'a BTreeMap<String, DomainValue> {
    match field(value, name) {
        DomainValue::Object(values) => values,
        _ => panic!("expected map at {name}"),
    }
}

fn state_field_number(state: &TabletopState, name: &str) -> f64 {
    field_number(&state.value, name)
}

fn request(
    preview: &OpenTaleCreationPreview,
    state: &TabletopState,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> ResolutionRequest {
    ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: format!("req_{operation}"),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: canonical_fingerprint(&preview.definition.definition)
            .expect("definition fingerprint"),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state).expect("state fingerprint"),
        input,
    }
}

fn execute(
    registry: &ResolverRegistry,
    preview: &OpenTaleCreationPreview,
    state: &TabletopState,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> (ResolutionReceipt, ResolutionRequest) {
    let request = request(preview, state, operation, capability, input);
    let receipt = registry
        .execute(&manifest(), &request, state)
        .expect("operation succeeds");
    (receipt, request)
}

fn event<'a>(receipt: &'a ResolutionReceipt, kind: &str) -> &'a TabletopEvent {
    receipt
        .events
        .iter()
        .find(|event| event.kind == kind)
        .unwrap_or_else(|| panic!("missing event {kind}"))
}

fn has_event(receipt: &ResolutionReceipt, kind: &str) -> bool {
    receipt.events.iter().any(|event| event.kind == kind)
}

fn challenge_input(
    id: &str,
    difficulty: u32,
    distance: u32,
    mode: OpenTaleRollMode,
    heroic_badge: bool,
    skill_id: &str,
) -> DomainValue {
    object([
        ("attribute", symbol("strength")),
        ("challenge_id", text(id)),
        ("difficulty", number(difficulty)),
        ("distance", number(distance)),
        ("heroic_badge", DomainValue::Bool(heroic_badge)),
        ("label", text("Cross the broken span")),
        ("mode", symbol(roll_mode_name(mode))),
        ("modifier", signed(0)),
        ("skill_id", text(skill_id)),
    ])
}

fn roll_mode_name(mode: OpenTaleRollMode) -> &'static str {
    match mode {
        OpenTaleRollMode::Advantage => "advantage",
        OpenTaleRollMode::Disadvantage => "disadvantage",
        OpenTaleRollMode::Normal => "normal",
    }
}

fn start_combat_input() -> DomainValue {
    object([
        ("adversary_agility", number(0)),
        ("adversary_damage", number(1)),
        ("adversary_defense", number(1)),
        ("adversary_health", number(1)),
        ("adversary_name", text("Tallow Wisp")),
        ("combat_id", text("span_guard")),
        ("distance", number(0)),
        ("label", text("Hold the span")),
    ])
}

fn combat_turn_input() -> DomainValue {
    object([
        ("combat_id", text("span_guard")),
        ("heroic_badge", DomainValue::Bool(false)),
        ("mode", symbol("normal")),
        ("weapon_id", text("short_blade")),
    ])
}

fn entropy(seed: u64, cursor: u64) -> EntropyStream {
    EntropyStream::from_state(EntropyState {
        algorithm: "sha256_counter_v1".to_owned(),
        seed,
        cursor,
    })
    .expect("valid entropy state")
}

#[test]
fn manifest_pins_apache_provenance_and_source_record() {
    let manifest = manifest();
    validate_adapter_manifest(&manifest).expect("manifest is valid");
    assert_eq!(manifest.id, OPEN_TALE_ADAPTER_ID);
    assert_eq!(
        manifest.provenance.source_class,
        AdapterSourceClass::SeparatelyLicensedApache2
    );
    assert_eq!(manifest.provenance.license, "Apache-2.0");
    assert_eq!(
        manifest.provenance.source_url,
        "https://open-tale.github.io/"
    );
    assert_eq!(manifest.provenance.sha256, OPEN_TALE_SOURCE_SHA256);
    assert_eq!(
        sha256_bytes(OPEN_TALE_SOURCE_RECORD.as_bytes()),
        OPEN_TALE_SOURCE_SHA256
    );
    assert!(OPEN_TALE_SOURCE_RECORD.contains("https://open-tale.github.io/"));
    assert!(OPEN_TALE_SOURCE_RECORD.contains("Apache-2.0"));
    assert!(OPEN_TALE_SOURCE_RECORD.contains(OPEN_TALE_EDITION));
    assert_eq!(OPEN_TALE_SOURCE_SHA256.len(), 64);

    assert!(OPEN_TALE_LICENSE_TEXT.contains("Apache License"));
    assert!(OPEN_TALE_LICENSE_TEXT.contains("Version 2.0, January 2004"));
    assert!(OPEN_TALE_LICENSE_TEXT.contains("END OF TERMS AND CONDITIONS"));
    assert_eq!(
        sha256_bytes(OPEN_TALE_LICENSE_TEXT.as_bytes()),
        OPEN_TALE_LICENSE_TEXT_SHA256
    );
    assert_eq!(
        manifest.provenance.required_license_text.sha256,
        OPEN_TALE_LICENSE_TEXT_SHA256
    );

    for capability in [
        TabletopCapability::CharacterCreation,
        TabletopCapability::DerivedValues,
        TabletopCapability::ChecksAndConflicts,
        TabletopCapability::ResourcesAndConditions,
        TabletopCapability::EquipmentAndAbilities,
        TabletopCapability::Advancement,
        TabletopCapability::Encounters,
    ] {
        assert!(has_capability(&manifest, capability));
    }
    assert!(manifest.operations.contains_key("resolve_challenge"));
    assert!(manifest.operations.contains_key("resolve_combat_turn"));
    assert!(manifest.operations.contains_key("inspect_character"));
}

#[test]
fn creation_allocates_budget_and_round_trips() {
    let preview = preview();
    validate_open_tale_creation_preview(&preview).expect("preview validates");
    assert_eq!(preview.wealth_spent, 4);
    assert_eq!(preview.wealth_remaining, 2);
    assert_eq!(preview.skill_ids, vec!["athletics", "composure"]);
    assert_eq!(preview.item_ids, vec!["short_blade", "travel_cloak"]);
    assert_eq!(
        preview.qualities,
        weave_tabletop::OpenTaleQualities {
            capacity: 8,
            defense: 8,
            hit_points: 10,
            resolve: 4,
            toughness: 8,
        }
    );
    assert_eq!(state_field_number(&preview.initial_state, "health"), 10.0);
    assert_eq!(state_field_number(&preview.initial_state, "resolve"), 4.0);
    assert_eq!(state_field_number(&preview.initial_state, "wealth"), 2.0);
    assert_eq!(state_field_number(&preview.initial_state, "level"), 1.0);
    assert_eq!(
        state_field_number(&preview.initial_state, "experience"),
        0.0
    );

    let definition_fields = field(&preview.definition.definition, "character_id");
    assert_eq!(
        definition_fields,
        &DomainValue::String("org.weave.fixture.rowan".to_owned())
    );
    let skills = field_map(&preview.definition.definition, "skills");
    assert!(skills.contains_key("athletics"));
    assert!(skills.contains_key("composure"));
    let items = field_map(&preview.definition.definition, "items");
    assert!(items.contains_key("short_blade"));
    assert!(items.contains_key("travel_cloak"));

    let json = preview.to_json().expect("json");
    let ron = preview.to_ron().expect("ron");
    let decoded_json = OpenTaleCreationPreview::from_json(&json).expect("decode json");
    let decoded_ron = OpenTaleCreationPreview::from_ron(&ron).expect("decode ron");
    assert_eq!(decoded_json, preview);
    assert_eq!(decoded_ron, preview);
    assert_eq!(decoded_json, decoded_ron);

    let state_json = preview.initial_state.to_json().expect("state json");
    let state_ron = preview.initial_state.to_ron().expect("state ron");
    assert_eq!(
        TabletopState::from_json(&state_json).expect("state json decode"),
        preview.initial_state
    );
    assert_eq!(
        TabletopState::from_ron(&state_ron).expect("state ron decode"),
        preview.initial_state
    );
}

#[test]
fn overspending_budget_and_invalid_level_are_refused() {
    let mut request = creation_request();
    request.attributes.strength = 4;
    let error = create_open_tale_character(&request).expect_err("overspend fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "attributes"),
        other => panic!("unexpected error: {other:?}"),
    }

    let mut request = creation_request();
    request.attributes.motion = 2;
    let error = create_open_tale_character(&request).expect_err("underspend fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "attributes"),
        other => panic!("unexpected error: {other:?}"),
    }

    let mut request = creation_request();
    request.level = 2;
    let error = create_open_tale_character(&request).expect_err("invalid level fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "level"),
        other => panic!("unexpected error: {other:?}"),
    }

    let mut request = creation_request();
    request.starting_wealth = 2;
    let error = create_open_tale_character(&request).expect_err("wealth overspend fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "items"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn quality_formulas_are_inspectable_and_match_declared_values() {
    let preview = preview();
    let request = creation_request();
    let passive_ranks = 2;
    let computed = weave_tabletop::open_tale_compute_qualities(&request.attributes, passive_ranks);
    assert_eq!(computed, preview.qualities);

    let formulas = open_tale_quality_formulas();
    assert_eq!(formulas.len(), 5);
    for formula in &formulas {
        let mut expected = formula.base;
        for term in &formula.terms {
            expected += request.attributes.get(term.attribute) * term.factor;
        }
        if formula.includes_passive_skills {
            expected += passive_ranks;
        }
        let actual = match formula.quality {
            OpenTaleQuality::Capacity => preview.qualities.capacity,
            OpenTaleQuality::Defense => preview.qualities.defense,
            OpenTaleQuality::HitPoints => preview.qualities.hit_points,
            OpenTaleQuality::Resolve => preview.qualities.resolve,
            OpenTaleQuality::Toughness => preview.qualities.toughness,
        };
        assert_eq!(actual, expected, "{}", formula.expression);
    }

    let qualities = field(&preview.definition.definition, "qualities");
    assert_eq!(field_number(qualities, "capacity"), 8.0);
    assert_eq!(field_number(qualities, "defense"), 8.0);
    assert_eq!(field_number(qualities, "hit_points"), 10.0);
    assert_eq!(field_number(qualities, "resolve"), 4.0);
    assert_eq!(field_number(qualities, "toughness"), 8.0);
}

#[test]
fn rolls_are_deterministic_and_bands_hold() {
    let normal = OpenTaleRollInput {
        attribute_value: 3,
        difficulty: 10,
        distance: 20,
        heroic_badge: false,
        mark_penalty: 0,
        mode: OpenTaleRollMode::Normal,
        modifier: 0,
        skill_bonus: 2,
    };
    let first = open_tale_resolve_roll(&normal, &mut entropy(42, 0)).expect("roll");
    let second = open_tale_resolve_roll(&normal, &mut entropy(42, 0)).expect("roll");
    assert_eq!(first, second);
    assert_eq!(first.dice.len(), 2);
    assert_eq!(first.kept.len(), 2);

    let advantage = OpenTaleRollInput {
        mode: OpenTaleRollMode::Advantage,
        ..normal
    };
    let advantage = open_tale_resolve_roll(&advantage, &mut entropy(42, 0)).expect("roll");
    assert_eq!(advantage.dice.len(), 3);
    let mut sorted = advantage.dice.clone();
    sorted.sort_unstable();
    assert_eq!(advantage.kept, sorted[1..].to_vec());

    let disadvantage = OpenTaleRollInput {
        mode: OpenTaleRollMode::Disadvantage,
        ..normal
    };
    let disadvantage = open_tale_resolve_roll(&disadvantage, &mut entropy(42, 0)).expect("roll");
    assert_eq!(disadvantage.dice.len(), 3);
    let mut sorted = disadvantage.dice.clone();
    sorted.sort_unstable();
    assert_eq!(disadvantage.kept, sorted[..2].to_vec());

    let heroic = OpenTaleRollInput {
        heroic_badge: true,
        ..normal
    };
    let heroic = open_tale_resolve_roll(&heroic, &mut entropy(42, 0)).expect("roll");
    assert_eq!(heroic.dice.len(), 3);
    assert!(heroic.heroic_badge_spent);
    let mut sorted = heroic.dice.clone();
    sorted.sort_unstable();
    assert_eq!(heroic.kept, sorted[1..].to_vec());

    let heroic_disadvantage = OpenTaleRollInput {
        mode: OpenTaleRollMode::Disadvantage,
        heroic_badge: true,
        ..normal
    };
    let heroic_disadvantage =
        open_tale_resolve_roll(&heroic_disadvantage, &mut entropy(42, 0)).expect("roll");
    assert_eq!(heroic_disadvantage.dice.len(), 3);
    let mut sorted = heroic_disadvantage.dice.clone();
    sorted.sort_unstable();
    assert_eq!(heroic_disadvantage.kept, sorted[1..].to_vec());

    assert_eq!(
        open_tale_outcome_band(15, 10),
        OpenTaleOutcomeBand::Critical
    );
    assert_eq!(open_tale_outcome_band(10, 10), OpenTaleOutcomeBand::Success);
    assert_eq!(open_tale_outcome_band(8, 10), OpenTaleOutcomeBand::Partial);
    assert_eq!(open_tale_outcome_band(7, 10), OpenTaleOutcomeBand::Failure);
}

#[test]
fn challenge_resolves_deterministically_and_replays() {
    let preview = preview();
    let registry = registry();
    let input = challenge_input("span", 1, 0, OpenTaleRollMode::Normal, false, "athletics");
    let (receipt, request) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "resolve_challenge",
        TabletopCapability::ChecksAndConflicts,
        input,
    );
    assert_eq!(receipt.after_state.revision, 1);
    assert!(has_event(&receipt, "roll_recorded"));
    assert!(has_event(&receipt, "challenge_updated"));
    assert!(has_event(&receipt, "challenge_resolved"));
    assert!(!has_event(&receipt, "mark_applied"));

    let challenges = field_map(&receipt.after_state.value, "challenges");
    let challenge = challenges.get("span").expect("challenge recorded");
    assert_eq!(field_symbol(challenge, "status"), "succeeded");
    assert_eq!(field_number(challenge, "turns"), 1.0);

    registry
        .replay(&manifest(), &request, &preview.initial_state, &receipt)
        .expect("receipt replays exactly");
    let repeated = registry
        .execute(&manifest(), &request, &preview.initial_state)
        .expect("same seed and choices");
    assert_eq!(repeated, receipt);

    let payload = event(&receipt, "roll_recorded")
        .payload
        .as_ref()
        .expect("public payload");
    assert_eq!(field_text(payload, "actor"), "character");
    assert!(matches!(
        field_symbol(payload, "band"),
        "success" | "critical"
    ));
    assert_eq!(field_text(payload, "context_id"), "span");
}

#[test]
fn failed_challenge_applies_marks_and_recovery_preserves_state() {
    let preview = preview();
    let registry = registry();
    let input = challenge_input(
        "broken_bridge",
        30,
        1_000,
        OpenTaleRollMode::Normal,
        false,
        "",
    );
    let (receipt, _) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "resolve_challenge",
        TabletopCapability::ChecksAndConflicts,
        input,
    );
    assert!(has_event(&receipt, "mark_applied"));
    assert_eq!(state_field_number(&receipt.after_state, "marks"), 1.0);
    let challenges = field_map(&receipt.after_state.value, "challenges");
    assert_eq!(
        field_symbol(
            challenges.get("broken_bridge").expect("challenge"),
            "status"
        ),
        "failed"
    );

    let saved = receipt.after_state.to_json().expect("saved state");
    let restored = TabletopState::from_json(&saved).expect("restored state");
    assert_eq!(restored, receipt.after_state);

    let (recovered, _) = execute(
        &registry,
        &preview,
        &restored,
        "apply_recovery",
        TabletopCapability::ResourcesAndConditions,
        object([("mode", symbol("treat"))]),
    );
    assert!(has_event(&recovered, "recovery_applied"));
    assert_eq!(state_field_number(&recovered.after_state, "marks"), 0.0);

    let recovered_json = recovered.after_state.to_json().expect("recovered json");
    let recovered_ron = recovered.after_state.to_ron().expect("recovered ron");
    assert_eq!(
        TabletopState::from_json(&recovered_json).expect("json restore"),
        recovered.after_state
    );
    assert_eq!(
        TabletopState::from_ron(&recovered_ron).expect("ron restore"),
        recovered.after_state
    );
}

#[test]
fn combat_sequence_concludes_deterministically() {
    let preview = preview();
    let registry = registry();
    let (started, start_request) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "start_combat",
        TabletopCapability::Encounters,
        start_combat_input(),
    );
    assert!(has_event(&started, "combat_started"));
    assert!(has_event(&started, "initiative_recorded"));
    assert!(started.entropy_consumed >= 4);

    let (turn, turn_request) = execute(
        &registry,
        &preview,
        &started.after_state,
        "resolve_combat_turn",
        TabletopCapability::ChecksAndConflicts,
        combat_turn_input(),
    );
    assert!(has_event(&turn, "roll_recorded"));
    assert!(has_event(&turn, "damage_applied"));
    assert!(has_event(&turn, "combat_concluded"));
    let combats = field_map(&turn.after_state.value, "combats");
    let combat = combats.get("span_guard").expect("combat recorded");
    assert_eq!(field_symbol(combat, "status"), "concluded");
    assert_eq!(field_number(combat, "adversary_health"), 0.0);
    let concluded = event(&turn, "combat_concluded")
        .payload
        .as_ref()
        .expect("public payload");
    assert_eq!(field_symbol(concluded, "outcome"), "victory");

    registry
        .replay(
            &manifest(),
            &start_request,
            &preview.initial_state,
            &started,
        )
        .expect("initiative replays");
    registry
        .replay(&manifest(), &turn_request, &started.after_state, &turn)
        .expect("combat turn replays");

    let (started_again, _) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "start_combat",
        TabletopCapability::Encounters,
        start_combat_input(),
    );
    assert_eq!(started_again, started);
    let (turn_again, _) = execute(
        &registry,
        &preview,
        &started_again.after_state,
        "resolve_combat_turn",
        TabletopCapability::ChecksAndConflicts,
        combat_turn_input(),
    );
    assert_eq!(turn_again, turn);
}

#[test]
fn advancement_requires_conditions_and_records_change() {
    let preview = preview();
    let registry = registry();
    let (receipt, _) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "advance_character",
        TabletopCapability::Advancement,
        object([
            ("experience_gain", number(10)),
            ("milestone", text("Crossed the broken span")),
        ]),
    );
    assert!(has_event(&receipt, "advancement_applied"));
    assert_eq!(state_field_number(&receipt.after_state, "experience"), 10.0);
    assert_eq!(state_field_number(&receipt.after_state, "level"), 2.0);
    let payload = event(&receipt, "advancement_applied")
        .payload
        .as_ref()
        .expect("public payload");
    assert_eq!(field_number(payload, "level_before"), 1.0);
    assert_eq!(field_number(payload, "level_after"), 2.0);
}

#[test]
fn advancement_rejects_zero_gain_empty_milestone_and_max_level() {
    let preview = preview();
    let registry = registry();
    for (gain, milestone, expected_path) in [
        (0, "No progress", "input.experience_gain"),
        (1, "", "input.milestone"),
    ] {
        let request = request(
            &preview,
            &preview.initial_state,
            "advance_character",
            TabletopCapability::Advancement,
            object([
                ("experience_gain", number(gain)),
                ("milestone", text(milestone)),
            ]),
        );
        let error = registry
            .execute(&manifest(), &request, &preview.initial_state)
            .expect_err("invalid advancement fails");
        match error {
            TabletopError::InvalidField { path, .. } => assert_eq!(path, expected_path),
            TabletopError::SchemaMismatch { path } => assert_eq!(path, "input"),
            other => panic!("unexpected error: {other:?}"),
        }

        let mut direct_entropy = entropy(7, 0);
        let direct = OpenTaleResolver.resolve(
            "advance_character",
            &preview.definition.definition,
            &object([
                ("experience_gain", number(gain)),
                ("milestone", text(milestone)),
            ]),
            &preview.initial_state.value,
            &mut direct_entropy,
        );
        let error = direct.expect_err("resolver refuses invalid advancement");
        match error {
            TabletopError::InvalidField { path, .. } => assert_eq!(path, expected_path),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    let mut maximum = preview.initial_state.clone();
    {
        let fields = match &mut maximum.value {
            DomainValue::Object(fields) => fields,
            _ => panic!("state is an object"),
        };
        fields.insert("experience".to_owned(), number(190));
        fields.insert("level".to_owned(), number(20));
    }
    let request = request(
        &preview,
        &maximum,
        "advance_character",
        TabletopCapability::Advancement,
        object([
            ("experience_gain", number(10)),
            ("milestone", text("Final milestone")),
        ]),
    );
    let error = registry
        .execute(&manifest(), &request, &maximum)
        .expect_err("maximum level fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "level"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn inspect_and_purchase_emit_structured_events() {
    let preview = preview();
    let registry = registry();
    let (inspected, _) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "inspect_character",
        TabletopCapability::DerivedValues,
        object([]),
    );
    assert!(has_event(&inspected, "character_inspected"));
    let payload = event(&inspected, "character_inspected")
        .payload
        .as_ref()
        .expect("public payload");
    assert_eq!(
        field_number(field(payload, "qualities"), "hit_points"),
        10.0
    );
    assert_eq!(field_number(field(payload, "attributes"), "strength"), 3.0);
    assert_eq!(field_number(payload, "wealth"), 2.0);

    let (purchased, _) = execute(
        &registry,
        &preview,
        &preview.initial_state,
        "purchase_item",
        TabletopCapability::EquipmentAndAbilities,
        object([("item_id", text("rope")), ("quantity", number(2))]),
    );
    assert!(has_event(&purchased, "item_acquired"));
    assert!(has_event(&purchased, "wealth_spent"));
    assert_eq!(state_field_number(&purchased.after_state, "wealth"), 0.0);
    let inventory = field_map(&purchased.after_state.value, "inventory");
    assert_eq!(
        field_number(inventory.get("rope").expect("rope"), "quantity"),
        2.0
    );

    let request = request(
        &preview,
        &purchased.after_state,
        "purchase_item",
        TabletopCapability::EquipmentAndAbilities,
        object([("item_id", text("rope")), ("quantity", number(1))]),
    );
    let error = registry
        .execute(&manifest(), &request, &purchased.after_state)
        .expect_err("unaffordable purchase fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "input.item_id"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn malformed_and_stale_requests_fail_closed() {
    let preview = preview();
    let registry = registry();

    let missing_skill = request(
        &preview,
        &preview.initial_state,
        "resolve_challenge",
        TabletopCapability::ChecksAndConflicts,
        challenge_input(
            "unknown_skill",
            10,
            0,
            OpenTaleRollMode::Normal,
            false,
            "missing",
        ),
    );
    let error = registry
        .execute(&manifest(), &missing_skill, &preview.initial_state)
        .expect_err("unknown skill fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "input.skill_id"),
        other => panic!("unexpected error: {other:?}"),
    }

    let mut no_badges = preview.initial_state.clone();
    {
        let fields = match &mut no_badges.value {
            DomainValue::Object(fields) => fields,
            _ => panic!("state is an object"),
        };
        fields.insert("heroic_badges".to_owned(), number(0));
    }
    let no_badge_request = request(
        &preview,
        &no_badges,
        "resolve_challenge",
        TabletopCapability::ChecksAndConflicts,
        challenge_input("no_badge", 10, 0, OpenTaleRollMode::Normal, true, ""),
    );
    let error = registry
        .execute(&manifest(), &no_badge_request, &no_badges)
        .expect_err("missing heroic badge fails");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "input.heroic_badge"),
        other => panic!("unexpected error: {other:?}"),
    }

    let stale = ResolutionRequest {
        expected_state_sha256: "0".repeat(64),
        ..request(
            &preview,
            &preview.initial_state,
            "inspect_character",
            TabletopCapability::DerivedValues,
            object([]),
        )
    };
    assert_eq!(
        registry
            .execute(&manifest(), &stale, &preview.initial_state)
            .expect_err("stale state fails"),
        TabletopError::StaleState
    );

    let error = OpenTaleResolver
        .resolve(
            "resolve_challenge",
            &preview.definition.definition,
            &DomainValue::Null,
            &preview.initial_state.value,
            &mut entropy(1, 0),
        )
        .expect_err("malformed input fails without panic");
    match error {
        TabletopError::InvalidField { path, .. } => assert_eq!(path, "input"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn event_declarations_cover_every_emitted_kind() {
    let manifest = manifest();
    let mut declared = std::collections::BTreeSet::new();
    for operation in manifest.operations.values() {
        for kind in &operation.event_kinds {
            declared.insert(kind.as_str());
            assert!(manifest.event_types.contains_key(kind));
        }
    }
    for required in [
        "advancement_applied",
        "challenge_resolved",
        "challenge_updated",
        "character_inspected",
        "combat_concluded",
        "combat_moved",
        "combat_started",
        "combat_updated",
        "damage_applied",
        "initiative_recorded",
        "item_acquired",
        "mark_applied",
        "recovery_applied",
        "roll_recorded",
        "wealth_spent",
    ] {
        assert!(declared.contains(required), "missing {required}");
    }
    assert_eq!(
        manifest.visibility.authority_host,
        vec![
            EventVisibility::Public,
            EventVisibility::Authoring,
            EventVisibility::HostOnly,
        ]
    );
    assert_eq!(manifest.visibility.runtime, vec![EventVisibility::Public]);
}

#[test]
fn catalog_kinds_and_bounds_are_stable() {
    let skills = open_tale_skill_catalog();
    assert!(skills.contains_key("athletics"));
    assert_eq!(
        skills.get("athletics").expect("athletics").kind,
        OpenTaleSkillKind::Active
    );
    assert_eq!(
        skills.get("composure").expect("composure").kind,
        OpenTaleSkillKind::Passive
    );
    let items = open_tale_item_catalog();
    assert_eq!(
        items.get("short_blade").expect("short blade").kind,
        OpenTaleItemKind::Weapon
    );
    assert_eq!(
        items.get("round_shield").expect("round shield").kind,
        OpenTaleItemKind::Armor
    );

    let mut request = creation_request();
    request.skills = vec![OpenTaleSkillSelection {
        id: "athletics".to_owned(),
        rank: 4,
    }];
    assert!(create_open_tale_character(&request).is_err());

    let mut request = creation_request();
    request.skills = vec![
        OpenTaleSkillSelection {
            id: "athletics".to_owned(),
            rank: 3,
        },
        OpenTaleSkillSelection {
            id: "stealth".to_owned(),
            rank: 3,
        },
        OpenTaleSkillSelection {
            id: "lore".to_owned(),
            rank: 1,
        },
    ];
    assert!(create_open_tale_character(&request).is_err());
}
