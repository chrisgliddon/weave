use std::collections::{BTreeMap, BTreeSet};

use weave_domain::DomainValue;
use weave_tabletop::{
    ExcludedMaterial, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, SPARK_ADAPTER_ID, SPARK_COLLABORATION_NOTES, SPARK_CREATION_FORMAT_VERSION,
    SPARK_EVOLUTION_FORMAT_VERSION, SPARK_SCENE_FRAME_FORMAT_VERSION,
    SPARK_SETTING_CREATION_FORMAT_VERSION, SPARK_SETTING_FORMAT_VERSION, SPARK_SOURCE_RECORD,
    SPARK_SOURCE_RECORD_SHA256, SPARK_SOURCE_URL, SparkAgenda, SparkAttribute, SparkAttributeSeed,
    SparkBeliefSeed, SparkBeliefStatus, SparkBeliefVerdict, SparkConflictStatus, SparkCostKind,
    SparkCreationPreview, SparkCreationRequest, SparkEvolutionChange, SparkEvolutionCondition,
    SparkEvolutionContext, SparkEvolutionRequest, SparkEvolutionResult, SparkFace, SparkFaction,
    SparkGmAttribute, SparkInfluenceMode, SparkParticipantRole, SparkRelationshipSeed,
    SparkResolver, SparkSceneFrameRequest, SparkSetting, SparkSettingBelief,
    SparkSettingContribution, SparkSettingContributionValue, SparkSettingCreation,
    SparkSettingCreationResult, SparkSettingParticipant, SparkStep, SparkTalentSeed,
    SparkTalentTier, SparkTie, SparkWorldReference, SparkWorldTargetKind, TabletopCapability,
    TabletopError, TabletopState, canonical_fingerprint, create_spark_character,
    equivalent_json_and_ron, evolve_spark_setting, frame_spark_scene,
    resolve_spark_setting_creation, resolved_adapter, sha256_bytes, spark_belief_lineage,
    spark_effective_belief_statement, spark_manifest, spark_setting_story_values,
    validate_adapter_manifest, validate_resolution_receipt, validate_spark_creation_preview,
    validate_spark_setting,
};

fn creation_request() -> SparkCreationRequest {
    SparkCreationRequest {
        creation_format_version: SPARK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.mara_vey".to_owned(),
        name: "Mara Vey".to_owned(),
        concept: "A lantern-keeper who charts safe routes through a flooded archive.".to_owned(),
        beliefs: vec![
            SparkBeliefSeed {
                id: "long_archive".to_owned(),
                statement: "The archive's oldest passages deserve protection.".to_owned(),
            },
            SparkBeliefSeed {
                id: "kept_promise".to_owned(),
                statement: "A promise made in fear still binds.".to_owned(),
            },
            SparkBeliefSeed {
                id: "first_warning".to_owned(),
                statement: "Every stranger deserves one honest warning.".to_owned(),
            },
        ],
        relationships: vec![
            SparkRelationshipSeed {
                id: "sable".to_owned(),
                name: "Sable".to_owned(),
                description: "A signal runner who owes a favour.".to_owned(),
            },
            SparkRelationshipSeed {
                id: "old_rook".to_owned(),
                name: "Old Rook".to_owned(),
                description: "The ferry keeper who knows every channel marker.".to_owned(),
            },
            SparkRelationshipSeed {
                id: "pell".to_owned(),
                name: "Pell".to_owned(),
                description: "A junior archivist with a steady lamp.".to_owned(),
            },
        ],
        attributes: vec![
            SparkAttributeSeed {
                attribute: SparkAttribute::Body,
                step: SparkStep::D4,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Heart,
                step: SparkStep::D6,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Mind,
                step: SparkStep::D12,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Spark,
                step: SparkStep::D8,
            },
        ],
        talents: vec![
            SparkTalentSeed {
                id: "steady_hands".to_owned(),
                name: "Steady Hands".to_owned(),
                attribute: SparkAttribute::Body,
                tier: SparkTalentTier::Broad,
            },
            SparkTalentSeed {
                id: "listening_ear".to_owned(),
                name: "Listening Ear".to_owned(),
                attribute: SparkAttribute::Heart,
                tier: SparkTalentTier::Common,
            },
            SparkTalentSeed {
                id: "archive_sense".to_owned(),
                name: "Archive Sense".to_owned(),
                attribute: SparkAttribute::Mind,
                tier: SparkTalentTier::Deep,
            },
            SparkTalentSeed {
                id: "far_sight".to_owned(),
                name: "Far Sight".to_owned(),
                attribute: SparkAttribute::Spark,
                tier: SparkTalentTier::Deep,
            },
        ],
        starting_influence: 16,
    }
}

fn registry() -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry
        .register(&spark_manifest(), SparkResolver)
        .expect("Spark resolver registers for the exact manifest");
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

fn as_object(value: &DomainValue) -> &BTreeMap<String, DomainValue> {
    match value {
        DomainValue::Object(fields) => fields,
        _ => panic!("expected an object"),
    }
}

fn field<'a>(value: &'a DomainValue, name: &str) -> &'a DomainValue {
    as_object(value)
        .get(name)
        .unwrap_or_else(|| panic!("missing field {name}"))
}

fn number(value: &DomainValue) -> u32 {
    match value {
        DomainValue::Number(value) => *value as u32,
        _ => panic!("expected a number"),
    }
}

fn symbol(value: &DomainValue) -> &str {
    match value {
        DomainValue::Symbol(value) => value,
        _ => panic!("expected a symbol"),
    }
}

fn string(value: &DomainValue) -> &str {
    match value {
        DomainValue::String(value) => value,
        _ => panic!("expected a string"),
    }
}

fn open_input(
    conflict_id: &str,
    max_escalation: u32,
    cost_kind: &str,
    cost_amount: u32,
    cost_attribute: &str,
) -> DomainValue {
    object([
        ("conflict_id", DomainValue::String(conflict_id.to_owned())),
        (
            "max_escalation",
            DomainValue::Number(f64::from(max_escalation)),
        ),
        (
            "stakes",
            DomainValue::String("Control of the flooded archive gate.".to_owned()),
        ),
        (
            "victory_cost_amount",
            DomainValue::Number(f64::from(cost_amount)),
        ),
        (
            "victory_cost_attribute",
            DomainValue::String(cost_attribute.to_owned()),
        ),
        (
            "victory_cost_kind",
            DomainValue::Symbol(cost_kind.to_owned()),
        ),
    ])
}

fn declare_input(
    conflict_id: &str,
    declaration_id: &str,
    actor_id: &str,
    attribute: &str,
    description: &str,
    talent_id: Option<&str>,
) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "actor_id".to_owned(),
            DomainValue::String(actor_id.to_owned()),
        ),
        (
            "attribute".to_owned(),
            DomainValue::Symbol(attribute.to_owned()),
        ),
        (
            "conflict_id".to_owned(),
            DomainValue::String(conflict_id.to_owned()),
        ),
        (
            "declaration_id".to_owned(),
            DomainValue::String(declaration_id.to_owned()),
        ),
        (
            "description".to_owned(),
            DomainValue::String(description.to_owned()),
        ),
    ]);
    if let Some(talent_id) = talent_id {
        fields.insert(
            "talent_id".to_owned(),
            DomainValue::String(talent_id.to_owned()),
        );
    }
    DomainValue::Object(fields)
}

fn support_input(conflict_id: &str, declaration_id: &str, relationship_id: &str) -> DomainValue {
    object([
        ("conflict_id", DomainValue::String(conflict_id.to_owned())),
        (
            "declaration_id",
            DomainValue::String(declaration_id.to_owned()),
        ),
        (
            "relationship_id",
            DomainValue::String(relationship_id.to_owned()),
        ),
    ])
}

fn conflict_input(conflict_id: &str) -> DomainValue {
    object([("conflict_id", DomainValue::String(conflict_id.to_owned()))])
}

fn boost_input(
    conflict_id: &str,
    declaration_id: &str,
    kind: &str,
    amount: u32,
    attribute: &str,
    rationale: &str,
) -> DomainValue {
    object([
        ("amount", DomainValue::Number(f64::from(amount))),
        ("attribute", DomainValue::String(attribute.to_owned())),
        ("conflict_id", DomainValue::String(conflict_id.to_owned())),
        (
            "declaration_id",
            DomainValue::String(declaration_id.to_owned()),
        ),
        ("kind", DomainValue::Symbol(kind.to_owned())),
        ("rationale", DomainValue::String(rationale.to_owned())),
    ])
}

fn review_input(belief_id: &str, rationale: &str, verdict: &str) -> DomainValue {
    object([
        ("belief_id", DomainValue::String(belief_id.to_owned())),
        ("rationale", DomainValue::String(rationale.to_owned())),
        ("verdict", DomainValue::Symbol(verdict.to_owned())),
    ])
}

fn recovery_input(attribute: &str, amount: u32) -> DomainValue {
    object([
        ("amount", DomainValue::Number(f64::from(amount))),
        ("attribute", DomainValue::Symbol(attribute.to_owned())),
    ])
}

#[allow(clippy::too_many_arguments)]
fn close_scene_input(
    scene_id: &str,
    label: &str,
    reviews: Vec<DomainValue>,
    mode: &str,
    amount: u32,
    rationale: &str,
    recovery: Vec<DomainValue>,
) -> DomainValue {
    object([
        ("belief_reviews", DomainValue::List(reviews)),
        ("influence_amount", DomainValue::Number(f64::from(amount))),
        ("influence_mode", DomainValue::Symbol(mode.to_owned())),
        (
            "influence_rationale",
            DomainValue::String(rationale.to_owned()),
        ),
        ("label", DomainValue::String(label.to_owned())),
        ("recovery", DomainValue::List(recovery)),
        ("scene_id", DomainValue::String(scene_id.to_owned())),
    ])
}

#[allow(clippy::too_many_arguments)]
fn try_execute(
    registry: &ResolverRegistry,
    preview: &SparkCreationPreview,
    state: &TabletopState,
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> Result<(ResolutionRequest, ResolutionReceipt), TabletopError> {
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: request_id.to_owned(),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state).expect("state fingerprints"),
        input,
    };
    let receipt = registry.execute(&spark_manifest(), &request, state)?;
    validate_resolution_receipt(&receipt, &spark_manifest())?;
    Ok((request, receipt))
}

#[allow(clippy::too_many_arguments)]
fn execute(
    registry: &ResolverRegistry,
    preview: &SparkCreationPreview,
    state: &TabletopState,
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> (ResolutionRequest, ResolutionReceipt) {
    try_execute(
        registry, preview, state, request_id, operation, capability, input,
    )
    .expect("resolution succeeds")
}

fn event_kinds(receipt: &ResolutionReceipt) -> Vec<&str> {
    receipt
        .events
        .iter()
        .map(|event| event.kind.as_str())
        .collect()
}

fn conflict<'a>(state: &'a TabletopState, conflict_id: &str) -> &'a DomainValue {
    as_object(field(&state.value, "conflicts"))
        .get(conflict_id)
        .unwrap_or_else(|| panic!("missing conflict {conflict_id}"))
}

fn declaration<'a>(
    state: &'a TabletopState,
    conflict_id: &str,
    declaration_id: &str,
) -> &'a DomainValue {
    as_object(field(conflict(state, conflict_id), "declarations"))
        .get(declaration_id)
        .unwrap_or_else(|| panic!("missing declaration {declaration_id}"))
}

fn attribute_state<'a>(state: &'a TabletopState, attribute: &str) -> &'a DomainValue {
    as_object(field(&state.value, "attributes"))
        .get(attribute)
        .unwrap_or_else(|| panic!("missing attribute {attribute}"))
}

fn state_after_harm_boost() -> (SparkCreationPreview, ResolverRegistry, TabletopState) {
    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        open_input("practice_gate", 0, "influence", 1, ""),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_mara",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "practice_gate",
            "mara_declaration",
            "org.weave.character.mara_vey",
            "mind",
            "Mara braces the gate while the last lamp is carried through.",
            Some("archive_sense"),
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_opponent",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "practice_gate",
            "opponent_declaration",
            "juno_vale",
            "body",
            "Juno pulls the gate closed before the last lamp.",
            None,
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_support",
        "add_supporter",
        TabletopCapability::ChecksAndConflicts,
        support_input("practice_gate", "mara_declaration", "sable"),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_roll",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("practice_gate"),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_harm_boost",
        "boost_roll",
        TabletopCapability::ChecksAndConflicts,
        boost_input(
            "practice_gate",
            "mara_declaration",
            "harm",
            2,
            "mind",
            "Mara leans on the flooded stair to hold the gate.",
        ),
    );
    (preview, registry, receipt.after_state)
}

#[test]
fn manifest_pins_the_checked_in_source_record() {
    let manifest = spark_manifest();
    validate_adapter_manifest(&manifest).expect("manifest validates");
    assert_eq!(manifest.id, SPARK_ADAPTER_ID);
    assert_eq!(manifest.namespace, "spark");
    assert_eq!(manifest.provenance.source_url, SPARK_SOURCE_URL);
    assert_eq!(manifest.provenance.license, "CC0-1.0");
    assert_eq!(manifest.provenance.sha256, SPARK_SOURCE_RECORD_SHA256);
    assert_eq!(manifest.provenance.sha256.len(), 64);
    assert_eq!(
        sha256_bytes(SPARK_SOURCE_RECORD.as_bytes()),
        SPARK_SOURCE_RECORD_SHA256
    );
    assert!(SPARK_SOURCE_RECORD.contains("https://www.genesisoflegend.com/the-spark-rpg/"));
    assert!(SPARK_SOURCE_RECORD.contains("Edition:"));
    assert!(SPARK_SOURCE_RECORD.contains("CC0-1.0"));
    assert!(SPARK_SOURCE_RECORD.contains("no upstream file bytes were downloaded"));
    assert!(
        manifest
            .provenance
            .exclusions
            .contains(&ExcludedMaterial::Artwork)
    );
    assert!(
        manifest
            .provenance
            .exclusions
            .contains(&ExcludedMaterial::Logos)
    );
    let coordinate = resolved_adapter(&manifest).expect("coordinate resolves");
    assert_eq!(coordinate.id, SPARK_ADAPTER_ID);
    assert_eq!(coordinate.content_sha256.len(), 64);
}

#[test]
fn creation_is_typed_reproducible_and_round_trips_json_and_ron() {
    let request = creation_request();
    let first = create_spark_character(&request).expect("creation succeeds");
    validate_spark_creation_preview(&first).expect("preview validates");
    let second = create_spark_character(&request).expect("creation is deterministic");
    assert_eq!(first, second);

    let definition = as_object(&first.definition.definition);
    assert_eq!(as_object(&definition["beliefs"]).len(), 3);
    assert_eq!(as_object(&definition["attributes"]).len(), 4);
    assert_eq!(as_object(&definition["relationships"]).len(), 3);
    assert_eq!(as_object(&definition["talents"]).len(), 4);
    assert_eq!(number(field(&first.initial_state.value, "influence")), 16);
    assert_eq!(
        number(field(attribute_state(&first.initial_state, "mind"), "harm")),
        0
    );
    let initial_beliefs = as_object(field(&first.initial_state.value, "beliefs"));
    assert_eq!(
        symbol(field(&initial_beliefs["long_archive"], "status")),
        SparkBeliefStatus::Held.symbol()
    );

    let request_json = request.to_json().expect("request JSON");
    let request_ron = request.to_ron().expect("request RON");
    assert!(
        equivalent_json_and_ron::<SparkCreationRequest>(&request_json, &request_ron)
            .expect("request equivalence")
    );
    let preview_json = first.to_json().expect("preview JSON");
    let preview_ron = first.to_ron().expect("preview RON");
    assert!(
        equivalent_json_and_ron::<SparkCreationPreview>(&preview_json, &preview_ron)
            .expect("preview equivalence")
    );
}

#[test]
fn multi_party_conflict_uses_talent_supporters_and_charges_the_winner() {
    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        open_input("flood_gate", 0, "influence", 2, ""),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_mara",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "flood_gate",
            "mara_declaration",
            "org.weave.character.mara_vey",
            "mind",
            "Mara holds the gate until every evacuee passes.",
            Some("archive_sense"),
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_juno",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "flood_gate",
            "juno_declaration",
            "juno_vale",
            "body",
            "Juno forces the gate closed early.",
            None,
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_orin",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "flood_gate",
            "orin_declaration",
            "orin_vale",
            "heart",
            "Orin argues for a partial opening.",
            None,
        ),
    );
    state = receipt.after_state;
    for (request_id, relationship_id) in [
        ("request_support_sable", "sable"),
        ("request_support_rook", "old_rook"),
        ("request_support_pell", "pell"),
    ] {
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            request_id,
            "add_supporter",
            TabletopCapability::ChecksAndConflicts,
            support_input("flood_gate", "mara_declaration", relationship_id),
        );
        state = receipt.after_state;
    }

    let before_roll = state.clone();
    let (roll_request, roll_receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_roll",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("flood_gate"),
    );
    let kinds = event_kinds(&roll_receipt);
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "roll_recorded")
            .count(),
        3
    );
    assert!(kinds.contains(&"entropy_trace"));
    state = roll_receipt.after_state.clone();
    assert_eq!(
        number(field(
            declaration(&state, "flood_gate", "mara_declaration"),
            "talent_bonus"
        )),
        3
    );
    assert_eq!(
        number(field(
            declaration(&state, "flood_gate", "mara_declaration"),
            "supporter_bonus"
        )),
        3
    );
    assert_eq!(
        string(field(
            declaration(&state, "flood_gate", "mara_declaration"),
            "talent_id"
        )),
        "archive_sense"
    );

    let before_resolve = state.clone();
    let (resolve_request, resolve_receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_resolve",
        "resolve_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("flood_gate"),
    );
    let kinds = event_kinds(&resolve_receipt);
    assert!(kinds.contains(&"conflict_resolved"));
    assert!(kinds.contains(&"victory_cost_paid"));
    assert!(kinds.contains(&"influence_changed"));
    state = resolve_receipt.after_state.clone();
    assert_eq!(number(field(&state.value, "influence")), 14);
    let conflict = conflict(&state, "flood_gate");
    assert_eq!(symbol(field(conflict, "status")), "resolved");
    assert_eq!(string(field(conflict, "winner_id")), "mara_declaration");
    let cost = resolve_receipt
        .events
        .iter()
        .find(|event| event.kind == "victory_cost_paid")
        .and_then(|event| event.payload.as_ref())
        .expect("public cost payload");
    assert_eq!(number(field(cost, "amount")), 2);
    assert_eq!(symbol(field(cost, "kind")), "influence");
    assert_eq!(field(cost, "charged"), &DomainValue::Bool(true));

    registry
        .replay(
            &spark_manifest(),
            &roll_request,
            &before_roll,
            &roll_receipt,
        )
        .expect("roll replay matches");
    registry
        .replay(
            &spark_manifest(),
            &resolve_request,
            &before_resolve,
            &resolve_receipt,
        )
        .expect("resolution replay matches");
    let repeated = registry
        .execute(&spark_manifest(), &resolve_request, &before_resolve)
        .expect("resolution is deterministic");
    assert_eq!(repeated, resolve_receipt);
}

#[test]
fn ties_escalate_then_resolve_with_an_explicit_post_roll_influence_boost() {
    let mut request = creation_request();
    request.attributes = vec![
        SparkAttributeSeed {
            attribute: SparkAttribute::Body,
            step: SparkStep::D4,
        },
        SparkAttributeSeed {
            attribute: SparkAttribute::Heart,
            step: SparkStep::D6,
        },
        SparkAttributeSeed {
            attribute: SparkAttribute::Mind,
            step: SparkStep::D4,
        },
        SparkAttributeSeed {
            attribute: SparkAttribute::Spark,
            step: SparkStep::D12,
        },
    ];
    request.talents = vec![
        SparkTalentSeed {
            id: "archive_sense".to_owned(),
            name: "Archive Sense".to_owned(),
            attribute: SparkAttribute::Mind,
            tier: SparkTalentTier::Deep,
        },
        SparkTalentSeed {
            id: "far_sight".to_owned(),
            name: "Far Sight".to_owned(),
            attribute: SparkAttribute::Spark,
            tier: SparkTalentTier::Deep,
        },
    ];
    let preview = create_spark_character(&request).expect("creation succeeds");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        open_input("tide_gate", 1, "influence", 0, ""),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_mara",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "tide_gate",
            "mara_declaration",
            "org.weave.character.mara_vey",
            "mind",
            "Mara reads the tide table aloud.",
            None,
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_juno",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "tide_gate",
            "juno_declaration",
            "juno_vale",
            "spark",
            "Juno calls on a remembered beacon.",
            Some("far_sight"),
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_roll",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("tide_gate"),
    );
    state = receipt.after_state;
    let owner_total = number(field(
        declaration(&state, "tide_gate", "mara_declaration"),
        "total",
    ));
    let opponent_total = number(field(
        declaration(&state, "tide_gate", "juno_declaration"),
        "total",
    ));
    assert!(owner_total <= opponent_total);
    if opponent_total > owner_total {
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            "request_boost",
            "boost_roll",
            TabletopCapability::ChecksAndConflicts,
            boost_input(
                "tide_gate",
                "mara_declaration",
                "influence",
                opponent_total - owner_total,
                "",
                "Mara spends Influence to match the beacon's call.",
            ),
        );
        assert!(event_kinds(&receipt).contains(&"influence_changed"));
        state = receipt.after_state;
    }
    assert_eq!(
        number(field(
            declaration(&state, "tide_gate", "mara_declaration"),
            "total"
        )),
        number(field(
            declaration(&state, "tide_gate", "juno_declaration"),
            "total"
        ))
    );
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_resolve_escalate",
        "resolve_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("tide_gate"),
    );
    let kinds = event_kinds(&receipt);
    assert!(kinds.contains(&"conflict_escalated"));
    assert!(kinds.contains(&"harm_changed"));
    assert!(!kinds.contains(&"conflict_resolved"));
    state = receipt.after_state;
    let conflict_state = conflict(&state, "tide_gate");
    assert_eq!(symbol(field(conflict_state, "status")), "open");
    assert_eq!(number(field(conflict_state, "escalation")), 1);
    assert_eq!(number(field(attribute_state(&state, "mind"), "harm")), 1);

    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_roll_again",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("tide_gate"),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_resolve_final",
        "resolve_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("tide_gate"),
    );
    assert!(event_kinds(&receipt).contains(&"conflict_resolved"));
    assert_eq!(
        symbol(field(conflict(&receipt.after_state, "tide_gate"), "status")),
        "resolved"
    );
}

#[test]
fn harm_boost_applies_harm_and_the_winner_pays_the_cost() {
    let (preview, registry, mut state) = state_after_harm_boost();
    assert_eq!(number(field(attribute_state(&state, "mind"), "harm")), 2);
    assert_eq!(
        number(field(
            declaration(&state, "practice_gate", "mara_declaration"),
            "harm_boost"
        )),
        2
    );
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_resolve",
        "resolve_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("practice_gate"),
    );
    let kinds = event_kinds(&receipt);
    assert!(kinds.contains(&"conflict_resolved"));
    assert!(kinds.contains(&"victory_cost_paid"));
    assert!(kinds.contains(&"influence_changed"));
    state = receipt.after_state;
    assert_eq!(number(field(&state.value, "influence")), 15);
    assert_eq!(
        string(field(conflict(&state, "practice_gate"), "winner_id")),
        "mara_declaration"
    );
}

#[test]
fn closing_a_scene_reviews_beliefs_awards_influence_and_recovers_harm() {
    let (preview, registry, mut state) = state_after_harm_boost();
    let before_influence = number(field(&state.value, "influence"));
    assert_eq!(before_influence, 16);
    assert_eq!(number(field(attribute_state(&state, "mind"), "harm")), 2);
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_close",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "after_the_flood",
            "After the Flood",
            vec![
                review_input(
                    "long_archive",
                    "The gate held long enough to seal the oldest stacks.",
                    "confirmed",
                ),
                review_input(
                    "first_warning",
                    "The warning came too late to count as honest.",
                    "refuted",
                ),
            ],
            "award",
            3,
            "The evacuees trusted the marked route.",
            vec![recovery_input("mind", 2)],
        ),
    );
    let kinds = event_kinds(&receipt);
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "belief_reviewed")
            .count(),
        2
    );
    assert!(kinds.contains(&"harm_changed"));
    assert!(kinds.contains(&"influence_changed"));
    assert!(kinds.contains(&"scene_closed"));
    state = receipt.after_state;
    assert_eq!(number(field(&state.value, "influence")), 19);
    assert_eq!(number(field(attribute_state(&state, "mind"), "harm")), 0);
    let beliefs = as_object(field(&state.value, "beliefs"));
    assert_eq!(
        symbol(field(&beliefs["long_archive"], "status")),
        SparkBeliefStatus::Confirmed.symbol()
    );
    assert_eq!(
        field(&beliefs["first_warning"], "rationale"),
        &DomainValue::String("The warning came too late to count as honest.".to_owned())
    );
    assert_eq!(
        symbol(field(&beliefs["first_warning"], "status")),
        SparkBeliefStatus::Refuted.symbol()
    );
    let scenes = as_object(field(&state.value, "scenes"));
    let scene = &scenes["after_the_flood"];
    assert_eq!(
        field(scene, "influence_rationale"),
        &DomainValue::String("The evacuees trusted the marked route.".to_owned())
    );
    let DomainValue::List(reviews) = field(scene, "belief_reviews") else {
        panic!("expected stored reviews");
    };
    assert_eq!(reviews.len(), 2);

    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_refresh",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "rest_after",
            "Rest After",
            Vec::new(),
            "refresh",
            30,
            "Refresh Influence to the reviewed floor after a quiet watch.",
            Vec::new(),
        ),
    );
    assert!(event_kinds(&receipt).contains(&"influence_changed"));
    state = receipt.after_state;
    assert_eq!(number(field(&state.value, "influence")), 30);
}

#[test]
fn invalid_artifacts_and_ill_ordered_operations_fail_closed() {
    let mut duplicate_beliefs = creation_request();
    duplicate_beliefs.beliefs[2].id = duplicate_beliefs.beliefs[0].id.clone();
    assert!(create_spark_character(&duplicate_beliefs).is_err());

    let mut duplicate_attribute = creation_request();
    duplicate_attribute.attributes[3].attribute = SparkAttribute::Body;
    assert!(create_spark_character(&duplicate_attribute).is_err());

    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        open_input("bad_conflict", 0, "influence", 0, ""),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "bad_conflict",
            "mara_declaration",
            "org.weave.character.mara_vey",
            "mind",
            "Mara listens for the tide.",
            None,
        ),
    );
    state = receipt.after_state;
    let duplicate_actor = try_execute(
        &registry,
        &preview,
        &state,
        "request_duplicate_actor",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "bad_conflict",
            "second_declaration",
            "org.weave.character.mara_vey",
            "heart",
            "Mara also listens closely.",
            None,
        ),
    );
    assert!(duplicate_actor.is_err());
    let boost_before_roll = try_execute(
        &registry,
        &preview,
        &state,
        "request_early_boost",
        "boost_roll",
        TabletopCapability::ChecksAndConflicts,
        boost_input(
            "bad_conflict",
            "mara_declaration",
            "influence",
            1,
            "",
            "Too early.",
        ),
    );
    assert!(boost_before_roll.is_err());

    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_opponent",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "bad_conflict",
            "opponent_declaration",
            "juno_vale",
            "body",
            "Juno closes the outer door.",
            None,
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_roll",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("bad_conflict"),
    );
    state = receipt.after_state;
    let unknown_belief = try_execute(
        &registry,
        &preview,
        &state,
        "request_bad_close",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "bad_scene",
            "Bad Scene",
            vec![review_input(
                "missing_belief",
                "This Belief does not exist.",
                "confirmed",
            )],
            "award",
            1,
            "Invalid review.",
            Vec::new(),
        ),
    );
    assert!(unknown_belief.is_err());
    assert_eq!(
        symbol(field(conflict(&state, "bad_conflict"), "status")),
        SparkConflictStatus::Rolled.symbol()
    );
    assert_eq!(SparkBeliefVerdict::Confirmed.symbol(), "confirmed");
    assert_eq!(SparkInfluenceMode::Refresh.symbol(), "refresh");
    assert_eq!(preview.adapter.id, SPARK_ADAPTER_ID);
}

#[test]
fn preview_owner_must_match_the_definition_character() {
    let mut preview = create_spark_character(&creation_request()).expect("creation succeeds");
    preview.initial_state.owner_id = "org.weave.character.other_actor".to_owned();
    assert!(validate_spark_creation_preview(&preview).is_err());
    preview.initial_state.owner_id = "org.weave.character.mara_vey".to_owned();
    validate_spark_creation_preview(&preview).expect("matching owner is accepted");
}

#[test]
fn supporters_are_restricted_to_the_owning_character() {
    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        open_input("owner_gate", 0, "influence", 0, ""),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_mara",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "owner_gate",
            "mara_declaration",
            "org.weave.character.mara_vey",
            "mind",
            "Mara holds the owner's gate.",
            Some("archive_sense"),
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_declare_juno",
        "declare_action",
        TabletopCapability::ChecksAndConflicts,
        declare_input(
            "owner_gate",
            "juno_declaration",
            "juno_vale",
            "body",
            "Juno contests the owner's gate.",
            None,
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_support_owner",
        "add_supporter",
        TabletopCapability::ChecksAndConflicts,
        support_input("owner_gate", "mara_declaration", "sable"),
    );
    let payload = receipt
        .events
        .iter()
        .find(|event| event.kind == "supporter_added")
        .and_then(|event| event.payload.as_ref())
        .expect("supporter event payload");
    assert_eq!(
        string(field(payload, "actor_id")),
        "org.weave.character.mara_vey"
    );
    state = receipt.after_state;
    let rejected = try_execute(
        &registry,
        &preview,
        &state,
        "request_support_foreign",
        "add_supporter",
        TabletopCapability::ChecksAndConflicts,
        support_input("owner_gate", "juno_declaration", "old_rook"),
    );
    assert!(rejected.is_err());
}

fn setting_fixture() -> SparkSetting {
    let namespace = "spark".to_owned();
    SparkSetting {
        format_version: SPARK_SETTING_FORMAT_VERSION,
        id: "org.weave.setting.flooded_archive".to_owned(),
        namespace: namespace.clone(),
        title: "The Flooded Archive".to_owned(),
        inspirations: BTreeSet::from([
            "A tidal city stores its records below the waterline.".to_owned()
        ]),
        genres: BTreeSet::from(["municipal fantasy".to_owned(), "quiet disaster".to_owned()]),
        facts: BTreeSet::from([
            "The lower archive floods on the ninth bell.".to_owned(),
            "Every district keeps a lantern signal.".to_owned(),
        ]),
        setting_beliefs: BTreeMap::from([
            (
                "long_archive".to_owned(),
                SparkSettingBelief {
                    id: "long_archive".to_owned(),
                    namespace: namespace.clone(),
                    owner_id: "org.weave.participant.lumen".to_owned(),
                    statement: "The oldest records must survive the flood.".to_owned(),
                },
            ),
            (
                "honest_signal".to_owned(),
                SparkSettingBelief {
                    id: "honest_signal".to_owned(),
                    namespace: namespace.clone(),
                    owner_id: "org.weave.participant.sable".to_owned(),
                    statement: "A warning must reach every district.".to_owned(),
                },
            ),
            (
                "shared_route".to_owned(),
                SparkSettingBelief {
                    id: "shared_route".to_owned(),
                    namespace: namespace.clone(),
                    owner_id: "org.weave.participant.orin".to_owned(),
                    statement: "Safe routes belong to everyone who uses them.".to_owned(),
                },
            ),
        ]),
        gm_attributes: BTreeMap::from([(
            "flood_pressure".to_owned(),
            SparkGmAttribute {
                id: "flood_pressure".to_owned(),
                namespace: namespace.clone(),
                owner_id: "org.weave.participant.pell".to_owned(),
                name: "Flood pressure".to_owned(),
                value: 3,
                description: "How close the water is to the reading room.".to_owned(),
            },
        )]),
        factions: BTreeMap::from([
            (
                "gate_wardens".to_owned(),
                SparkFaction {
                    id: "gate_wardens".to_owned(),
                    namespace: namespace.clone(),
                    owner_id: "org.weave.participant.lumen".to_owned(),
                    name: "Gate Wardens".to_owned(),
                    description: "They keep the lower doors open for as long as they can."
                        .to_owned(),
                },
            ),
            (
                "signal_runners".to_owned(),
                SparkFaction {
                    id: "signal_runners".to_owned(),
                    namespace: namespace.clone(),
                    owner_id: "org.weave.participant.sable".to_owned(),
                    name: "Signal Runners".to_owned(),
                    description: "They carry warnings between districts when the bells fail."
                        .to_owned(),
                },
            ),
        ]),
        ties: BTreeMap::from([(
            "shared_watch".to_owned(),
            SparkTie {
                id: "shared_watch".to_owned(),
                namespace: namespace.clone(),
                owner_id: "org.weave.participant.orin".to_owned(),
                from_faction: "gate_wardens".to_owned(),
                to_faction: "signal_runners".to_owned(),
                description: "The two groups share a night watch over the lower stairs.".to_owned(),
            },
        )]),
        faces: BTreeMap::from([(
            "lumen_vey".to_owned(),
            SparkFace {
                id: "lumen_vey".to_owned(),
                namespace: namespace.clone(),
                owner_id: "org.weave.participant.lumen".to_owned(),
                name: "Lumen Vey".to_owned(),
                role: "Lantern keeper".to_owned(),
                description: "Keeps the reading-room lamp above the waterline.".to_owned(),
                faction_id: Some("gate_wardens".to_owned()),
            },
        )]),
        agendas: BTreeMap::from([(
            "keep_the_doors".to_owned(),
            SparkAgenda {
                id: "keep_the_doors".to_owned(),
                namespace: namespace.clone(),
                owner_id: "org.weave.participant.lumen".to_owned(),
                name: "Keep the lower doors open".to_owned(),
                description: "Delay the flood closure until the last record is carried out."
                    .to_owned(),
                faction_id: Some("gate_wardens".to_owned()),
            },
        )]),
        world_references: BTreeMap::from([(
            "world_gate".to_owned(),
            SparkWorldReference {
                id: "world_gate".to_owned(),
                namespace,
                owner_id: "org.weave.participant.pell".to_owned(),
                target_kind: SparkWorldTargetKind::Location,
                target_id: "org.weave.world.lower_gate".to_owned(),
                description: "Optional stable reference to a Weave World location.".to_owned(),
            },
        )]),
    }
}

#[test]
fn setting_is_typed_reference_safe_portable_and_visible_to_story_expressions() {
    let setting = setting_fixture();
    validate_spark_setting(&setting).expect("setting validates");
    let json = setting.to_json().expect("setting JSON");
    let ron = setting.to_ron().expect("setting RON");
    assert!(equivalent_json_and_ron::<SparkSetting>(&json, &ron).expect("setting equivalence"));
    assert_eq!(
        SparkSetting::from_json(&json).expect("setting JSON load"),
        setting
    );
    assert_eq!(
        SparkSetting::from_ron(&ron).expect("setting RON load"),
        setting
    );

    let values = spark_setting_story_values(&setting);
    assert!(values.contains_key("setting_id"));
    assert!(values.contains_key("setting_beliefs"));
    assert!(values.contains_key("gm_attributes"));
    assert!(values.contains_key("factions"));
    assert!(values.contains_key("ties"));
    assert!(values.contains_key("faces"));
    assert!(values.contains_key("agendas"));
    assert!(values.contains_key("world_references"));
    assert_eq!(
        setting.world_references["world_gate"].target_id,
        "org.weave.world.lower_gate"
    );

    let keys = setting.factions.keys().collect::<Vec<_>>();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(setting.setting_beliefs.len(), 3);

    let mut broken = setting.clone();
    broken
        .ties
        .get_mut("shared_watch")
        .expect("tie fixture")
        .to_faction = "missing_faction".to_owned();
    assert!(validate_spark_setting(&broken).is_err());

    let mut wrong_key = setting.clone();
    let faction = wrong_key
        .factions
        .remove("gate_wardens")
        .expect("faction fixture");
    wrong_key.factions.insert("renamed_key".to_owned(), faction);
    assert!(validate_spark_setting(&wrong_key).is_err());
}

fn setting_participants() -> BTreeMap<String, SparkSettingParticipant> {
    BTreeMap::from([
        (
            "org.weave.participant.lumen".to_owned(),
            SparkSettingParticipant {
                id: "org.weave.participant.lumen".to_owned(),
                display_name: "Lumen".to_owned(),
                role: SparkParticipantRole::Owner,
            },
        ),
        (
            "org.weave.participant.sable".to_owned(),
            SparkSettingParticipant {
                id: "org.weave.participant.sable".to_owned(),
                display_name: "Sable".to_owned(),
                role: SparkParticipantRole::Player,
            },
        ),
        (
            "org.weave.participant.orin".to_owned(),
            SparkSettingParticipant {
                id: "org.weave.participant.orin".to_owned(),
                display_name: "Orin".to_owned(),
                role: SparkParticipantRole::Player,
            },
        ),
        (
            "org.weave.participant.pell".to_owned(),
            SparkSettingParticipant {
                id: "org.weave.participant.pell".to_owned(),
                display_name: "Pell".to_owned(),
                role: SparkParticipantRole::Gm,
            },
        ),
    ])
}

fn contribution(
    contribution_id: &str,
    participant_id: &str,
    path: &str,
    revision: u32,
    consent: bool,
    value: SparkSettingContributionValue,
) -> SparkSettingContribution {
    SparkSettingContribution {
        contribution_id: contribution_id.to_owned(),
        participant_id: participant_id.to_owned(),
        path: path.to_owned(),
        revision,
        consent,
        value,
    }
}

fn text_value(text: &str) -> SparkSettingContributionValue {
    SparkSettingContributionValue::Text {
        text: text.to_owned(),
    }
}

fn setting_creation() -> SparkSettingCreation {
    SparkSettingCreation {
        format_version: SPARK_SETTING_CREATION_FORMAT_VERSION,
        setting_id: "org.weave.setting.flooded_archive".to_owned(),
        namespace: "spark".to_owned(),
        participants: setting_participants(),
        contributions: vec![
            contribution(
                "title_lumen",
                "org.weave.participant.lumen",
                "title",
                1,
                true,
                text_value("The Flooded Archive"),
            ),
            contribution(
                "inspiration_lumen",
                "org.weave.participant.lumen",
                "inspiration.archive",
                1,
                true,
                text_value("A tidal city stores its records below the waterline."),
            ),
            contribution(
                "genre_sable",
                "org.weave.participant.sable",
                "genre.municipal",
                1,
                true,
                text_value("municipal fantasy"),
            ),
            contribution(
                "genre_orin",
                "org.weave.participant.orin",
                "genre.disaster",
                1,
                true,
                text_value("quiet disaster"),
            ),
            contribution(
                "fact_pell",
                "org.weave.participant.pell",
                "fact.ninth_bell",
                1,
                true,
                text_value("The lower archive floods on the ninth bell."),
            ),
            contribution(
                "fact_sable",
                "org.weave.participant.sable",
                "fact.lantern_signal",
                1,
                true,
                text_value("Every district keeps a lantern signal."),
            ),
            contribution(
                "belief_long_archive",
                "org.weave.participant.lumen",
                "belief.long_archive",
                1,
                true,
                SparkSettingContributionValue::Belief {
                    statement: "The oldest records must survive the flood.".to_owned(),
                },
            ),
            contribution(
                "belief_honest_signal",
                "org.weave.participant.sable",
                "belief.honest_signal",
                1,
                true,
                SparkSettingContributionValue::Belief {
                    statement: "A warning must reach every district.".to_owned(),
                },
            ),
            contribution(
                "belief_shared_route",
                "org.weave.participant.orin",
                "belief.shared_route",
                1,
                true,
                SparkSettingContributionValue::Belief {
                    statement: "Safe routes belong to everyone who uses them.".to_owned(),
                },
            ),
            contribution(
                "gm_flood_pressure",
                "org.weave.participant.pell",
                "gm_attribute.flood_pressure",
                1,
                true,
                SparkSettingContributionValue::GmAttribute {
                    name: "Flood pressure".to_owned(),
                    value: 3,
                    description: "How close the water is to the reading room.".to_owned(),
                },
            ),
            contribution(
                "faction_gate_wardens",
                "org.weave.participant.lumen",
                "faction.gate_wardens",
                1,
                true,
                SparkSettingContributionValue::Faction {
                    name: "Gate Wardens".to_owned(),
                    description: "They keep the lower doors open for as long as they can."
                        .to_owned(),
                },
            ),
            contribution(
                "faction_signal_runners",
                "org.weave.participant.sable",
                "faction.signal_runners",
                1,
                true,
                SparkSettingContributionValue::Faction {
                    name: "Signal Runners".to_owned(),
                    description: "They carry warnings between districts when the bells fail."
                        .to_owned(),
                },
            ),
            contribution(
                "tie_shared_watch",
                "org.weave.participant.orin",
                "tie.shared_watch",
                1,
                true,
                SparkSettingContributionValue::Tie {
                    from_faction: "gate_wardens".to_owned(),
                    to_faction: "signal_runners".to_owned(),
                    description: "The two groups share a night watch over the lower stairs."
                        .to_owned(),
                },
            ),
            contribution(
                "face_lumen_vey",
                "org.weave.participant.lumen",
                "face.lumen_vey",
                1,
                true,
                SparkSettingContributionValue::Face {
                    name: "Lumen Vey".to_owned(),
                    role: "Lantern keeper".to_owned(),
                    description: "Keeps the reading-room lamp above the waterline.".to_owned(),
                    faction_id: Some("gate_wardens".to_owned()),
                },
            ),
            contribution(
                "agenda_keep_the_doors",
                "org.weave.participant.lumen",
                "agenda.keep_the_doors",
                1,
                true,
                SparkSettingContributionValue::Agenda {
                    name: "Keep the lower doors open".to_owned(),
                    description: "Delay the flood closure until the last record is carried out."
                        .to_owned(),
                    faction_id: Some("gate_wardens".to_owned()),
                },
            ),
            contribution(
                "world_gate_pell",
                "org.weave.participant.pell",
                "world_reference.world_gate",
                1,
                true,
                SparkSettingContributionValue::WorldReference {
                    target_kind: SparkWorldTargetKind::Location,
                    target_id: "org.weave.world.lower_gate".to_owned(),
                    description: "Optional stable reference to a Weave World location.".to_owned(),
                },
            ),
        ],
    }
}

fn scene_frame_request() -> SparkSceneFrameRequest {
    SparkSceneFrameRequest {
        format_version: SPARK_SCENE_FRAME_FORMAT_VERSION,
        scene_id: "after_the_flood".to_owned(),
        platform: "The lower archive at the ninth bell".to_owned(),
        tilt: "The water is rising faster than the records can move".to_owned(),
        question: "Can the wardens keep the lower doors open until the last record is carried out?"
            .to_owned(),
        end_condition:
            "The scene ends when the last record is above the waterline or the doors close."
                .to_owned(),
        contributors: BTreeMap::from([
            (
                "platform".to_owned(),
                "org.weave.participant.lumen".to_owned(),
            ),
            ("tilt".to_owned(), "org.weave.participant.sable".to_owned()),
            (
                "question".to_owned(),
                "org.weave.participant.orin".to_owned(),
            ),
            (
                "end_condition".to_owned(),
                "org.weave.participant.pell".to_owned(),
            ),
        ]),
    }
}

fn refuted_character_state() -> (SparkCreationPreview, ResolverRegistry, TabletopState) {
    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_refute",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "refute_scene",
            "Refute Scene",
            vec![review_input(
                "long_archive",
                "The flood proved the old promise wrong.",
                "refuted",
            )],
            "award",
            0,
            "No Influence review.",
            Vec::new(),
        ),
    );
    (preview, registry, receipt.after_state)
}

#[test]
fn collaborative_setting_creation_is_order_independent_and_resumable() {
    let creation = setting_creation();
    let result =
        resolve_spark_setting_creation(&creation).expect("collaborative creation succeeds");
    assert!(result.completed);
    assert!(!result.canonical_write_back);
    assert!(result.missing_paths.is_empty());
    assert!(result.next_responsibilities.is_empty());
    assert_eq!(result.setting, Some(setting_fixture()));
    let mut reversed = creation.clone();
    reversed.contributions.reverse();
    let reordered = resolve_spark_setting_creation(&reversed).expect("reordered creation succeeds");
    assert_eq!(result, reordered);

    let mut partial = creation.clone();
    partial
        .contributions
        .retain(|item| item.path == "title" || item.path == "belief.long_archive");
    let partial_result =
        resolve_spark_setting_creation(&partial).expect("partial creation resolves");
    assert!(!partial_result.completed);
    assert!(partial_result.setting.is_none());
    assert_eq!(partial_result.missing_paths, vec!["belief".to_owned()]);
    assert_eq!(partial_result.next_responsibilities.len(), 1);
    assert_eq!(
        partial_result.next_responsibilities[0].participant_id,
        "org.weave.participant.lumen"
    );
    let resumed = resolve_spark_setting_creation(&creation).expect("resumed creation succeeds");
    assert!(resumed.completed);
}

#[test]
fn concurrent_edits_retain_both_revisions_and_choose_deterministically() {
    let mut creation = setting_creation();
    creation.contributions.push(contribution(
        "title_sable",
        "org.weave.participant.sable",
        "title",
        2,
        true,
        text_value("The Archive Under the Ninth Bell"),
    ));
    creation.contributions.push(contribution(
        "title_orin_pending",
        "org.weave.participant.orin",
        "title",
        3,
        false,
        text_value("A Rejected Title"),
    ));
    let result = resolve_spark_setting_creation(&creation).expect("concurrent creation resolves");
    let setting = result.setting.clone().expect("setting completes");
    assert_eq!(setting.title, "The Archive Under the Ninth Bell");
    let history = &result.history["title"];
    assert_eq!(history.len(), 3);
    assert_eq!(history.iter().filter(|entry| entry.applied).count(), 1);
    assert!(
        history
            .iter()
            .any(|entry| entry.contribution_id == "title_sable" && entry.applied)
    );
    assert!(
        history
            .iter()
            .any(|entry| entry.contribution_id == "title_orin_pending" && !entry.applied)
    );
    let mut reversed = creation.clone();
    reversed.contributions.reverse();
    assert_eq!(
        result,
        resolve_spark_setting_creation(&reversed).expect("reordered creation resolves")
    );
}

#[test]
fn scene_framing_validates_and_opens_the_existing_conflict_loop() {
    let participants = setting_participants();
    let request = scene_frame_request();
    let frame = frame_spark_scene(&request, &participants).expect("scene frame validates");
    assert_eq!(frame.next_actor_id, "org.weave.participant.lumen");

    let mut missing_question = request.clone();
    missing_question.question.clear();
    let failure = frame_spark_scene(&missing_question, &participants).expect_err("question fails");
    assert!(failure.to_string().contains("question"));
    let mut missing_end = request.clone();
    missing_end.end_condition.clear();
    let failure = frame_spark_scene(&missing_end, &participants).expect_err("end condition fails");
    assert!(failure.to_string().contains("end_condition"));
    let mut missing_contributor = request.clone();
    missing_contributor.contributors.remove("question");
    let failure = frame_spark_scene(&missing_contributor, &participants)
        .expect_err("missing contributor fails");
    assert!(failure.to_string().contains("contributors.question"));

    let preview = create_spark_character(&creation_request()).expect("creation succeeds");
    let registry = registry();
    let state = preview.initial_state.clone();
    let input = frame.open_conflict_input("framed_gate", 0, SparkCostKind::Influence, 0, "");
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_framed_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        input,
    );
    let conflict = conflict(&receipt.after_state, "framed_gate");
    assert_eq!(string(field(conflict, "stakes")), frame.question);
}

#[test]
fn belief_replacement_requires_confirmation_retains_lineage_and_grants_growth() {
    let (preview, registry, mut state) = refuted_character_state();
    let original_statement = "The archive's oldest passages deserve protection.".to_owned();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_replace",
        "replace_belief",
        TabletopCapability::Advancement,
        object([
            ("belief_id", DomainValue::String("long_archive".to_owned())),
            (
                "confirmed_by",
                DomainValue::List(vec![
                    DomainValue::String("org.weave.participant.lumen".to_owned()),
                    DomainValue::String("org.weave.participant.sable".to_owned()),
                ]),
            ),
            (
                "inspiration",
                DomainValue::String("The waterline map shows a higher route.".to_owned()),
            ),
            (
                "new_statement",
                DomainValue::String("The archive must move above the flood line.".to_owned()),
            ),
            (
                "rationale",
                DomainValue::String(
                    "The group confirmed the Inspiration and the new statement.".to_owned(),
                ),
            ),
            (
                "replacement_id",
                DomainValue::String("first_replacement".to_owned()),
            ),
        ]),
    );
    let kinds = event_kinds(&receipt);
    assert!(kinds.contains(&"belief_replaced"));
    assert!(kinds.contains(&"growth_changed"));
    state = receipt.after_state;
    assert_eq!(number(field(&state.value, "growth")), 1);
    let lineage =
        spark_belief_lineage(&preview.definition.definition, &state.value, "long_archive")
            .expect("lineage reads");
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].old_statement, original_statement);
    assert_eq!(
        lineage[0].new_statement,
        "The archive must move above the flood line."
    );
    assert!(lineage[0].growth_awarded);
    assert_eq!(
        spark_effective_belief_statement(
            &preview.definition.definition,
            &state.value,
            "long_archive"
        )
        .expect("effective statement reads"),
        "The archive must move above the flood line."
    );
    let definition_beliefs = as_object(field(&preview.definition.definition, "beliefs"));
    assert_eq!(
        string(field(&definition_beliefs["long_archive"], "statement")),
        original_statement
    );

    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "request_replace_held",
        "replace_belief",
        TabletopCapability::Advancement,
        object([
            ("belief_id", DomainValue::String("kept_promise".to_owned())),
            (
                "confirmed_by",
                DomainValue::List(vec![DomainValue::String(
                    "org.weave.participant.lumen".to_owned(),
                )]),
            ),
            (
                "inspiration",
                DomainValue::String("A quiet watch kept the promise visible.".to_owned()),
            ),
            (
                "new_statement",
                DomainValue::String("A promise kept in calm weather still binds.".to_owned()),
            ),
            (
                "rationale",
                DomainValue::String("The group replaced a held Belief.".to_owned()),
            ),
            (
                "replacement_id",
                DomainValue::String("second_replacement".to_owned()),
            ),
        ]),
    );
    assert!(!event_kinds(&receipt).contains(&"growth_changed"));
    let state = receipt.after_state;
    assert_eq!(number(field(&state.value, "growth")), 1);

    let duplicate_confirmation = try_execute(
        &registry,
        &preview,
        &state,
        "request_duplicate_confirmation",
        "replace_belief",
        TabletopCapability::Advancement,
        object([
            ("belief_id", DomainValue::String("first_warning".to_owned())),
            (
                "confirmed_by",
                DomainValue::List(vec![
                    DomainValue::String("org.weave.participant.lumen".to_owned()),
                    DomainValue::String("org.weave.participant.lumen".to_owned()),
                ]),
            ),
            (
                "inspiration",
                DomainValue::String("A duplicate confirmation is not consent.".to_owned()),
            ),
            (
                "new_statement",
                DomainValue::String("A warning must name its source.".to_owned()),
            ),
            (
                "rationale",
                DomainValue::String("Duplicate confirmation probe.".to_owned()),
            ),
            (
                "replacement_id",
                DomainValue::String("third_replacement".to_owned()),
            ),
        ]),
    );
    assert!(duplicate_confirmation.is_err());
}

#[test]
fn evolution_requires_conditions_and_records_reviewable_changes() {
    let setting = setting_fixture();
    let context = SparkEvolutionContext {
        refuted_beliefs: BTreeSet::from(["long_archive".to_owned()]),
        growth: 1,
        faction_ids: BTreeSet::from(["gate_wardens".to_owned(), "signal_runners".to_owned()]),
        closed_scenes: BTreeSet::from(["after_the_flood".to_owned()]),
    };
    let request = SparkEvolutionRequest {
        format_version: SPARK_EVOLUTION_FORMAT_VERSION,
        evolution_id: "first_evolution".to_owned(),
        setting_id: setting.id.clone(),
        conditions: vec![
            SparkEvolutionCondition::BeliefRefuted {
                belief_id: "long_archive".to_owned(),
            },
            SparkEvolutionCondition::GrowthAtLeast { amount: 1 },
            SparkEvolutionCondition::SceneClosed {
                scene_id: "after_the_flood".to_owned(),
            },
        ],
        changes: vec![
            SparkEvolutionChange::SetSettingBelief {
                belief_id: "long_archive".to_owned(),
                statement: "The archive must move above the flood line.".to_owned(),
            },
            SparkEvolutionChange::SetGmAttribute {
                attribute_id: "flood_pressure".to_owned(),
                value: 5,
            },
            SparkEvolutionChange::SetFactionName {
                faction_id: "gate_wardens".to_owned(),
                name: "Upper Gate Wardens".to_owned(),
            },
        ],
        rationale: "The refuted Belief and confirmed replacement changed the setting.".to_owned(),
        accepted: true,
        accepted_by: vec![
            "org.weave.participant.lumen".to_owned(),
            "org.weave.participant.sable".to_owned(),
        ],
    };
    let result = evolve_spark_setting(&setting, &request, &context).expect("Evolution applies");
    assert!(result.applied);
    assert!(!result.canonical_write_back);
    assert_eq!(result.changes.len(), 3);
    let evolved = result.setting.clone().expect("evolved setting");
    assert_eq!(
        evolved.setting_beliefs["long_archive"].statement,
        "The archive must move above the flood line."
    );
    assert_eq!(evolved.gm_attributes["flood_pressure"].value, 5);
    assert_eq!(evolved.factions["gate_wardens"].name, "Upper Gate Wardens");
    assert_eq!(
        setting.setting_beliefs["long_archive"].statement,
        "The oldest records must survive the flood."
    );

    let mut refused = request.clone();
    refused.evolution_id = "refused_evolution".to_owned();
    refused
        .conditions
        .push(SparkEvolutionCondition::SceneClosed {
            scene_id: "missing_scene".to_owned(),
        });
    let refused_result =
        evolve_spark_setting(&setting, &refused, &context).expect("refused Evolution resolves");
    assert!(!refused_result.applied);
    assert!(
        refused_result
            .failed_conditions
            .contains(&"scene_closed.missing_scene".to_owned())
    );
    assert!(refused_result.setting.is_none());

    let mut unaccepted = request.clone();
    unaccepted.evolution_id = "unaccepted_evolution".to_owned();
    unaccepted.accepted = false;
    let unaccepted_result = evolve_spark_setting(&setting, &unaccepted, &context)
        .expect("unaccepted Evolution resolves");
    assert!(!unaccepted_result.applied);
    assert!(!unaccepted_result.accepted);

    let json = result.to_json().expect("Evolution JSON");
    let ron = result.to_ron().expect("Evolution RON");
    assert!(
        equivalent_json_and_ron::<SparkEvolutionResult>(&json, &ron)
            .expect("Evolution equivalence")
    );
}

#[test]
fn synthetic_campaign_fixture_replays_across_json_ron_and_host_projections() {
    let creation = setting_creation();
    let collaborative =
        resolve_spark_setting_creation(&creation).expect("collaborative setup succeeds");
    assert!(collaborative.completed);
    let setting = collaborative.setting.clone().expect("setting");
    let frame =
        frame_spark_scene(&scene_frame_request(), &setting_participants()).expect("scene frame");
    let preview = create_spark_character(&creation_request()).expect("character creation");
    let registry = registry();
    let mut state = preview.initial_state.clone();
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_open",
        "open_conflict",
        TabletopCapability::ChecksAndConflicts,
        frame.open_conflict_input("fixture_gate", 0, SparkCostKind::Influence, 0, ""),
    );
    state = receipt.after_state;
    for (request_id, declaration_id, actor_id, attribute) in [
        (
            "fixture_declare_owner",
            "fixture_owner",
            "org.weave.character.mara_vey",
            "mind",
        ),
        (
            "fixture_declare_opponent",
            "fixture_opponent",
            "juno_vale",
            "body",
        ),
    ] {
        let (_, receipt) = execute(
            &registry,
            &preview,
            &state,
            request_id,
            "declare_action",
            TabletopCapability::ChecksAndConflicts,
            declare_input(
                "fixture_gate",
                declaration_id,
                actor_id,
                attribute,
                "The conformance campaign tests the framed scene.",
                None,
            ),
        );
        state = receipt.after_state;
    }
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_roll",
        "roll_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("fixture_gate"),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_resolve",
        "resolve_conflict",
        TabletopCapability::ChecksAndConflicts,
        conflict_input("fixture_gate"),
    );
    state = receipt.after_state;

    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_close_first",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "fixture_scene_one",
            "Fixture Scene One",
            vec![review_input(
                "long_archive",
                "The framed scene refuted the old archive promise.",
                "refuted",
            )],
            "award",
            2,
            "The framed scene earned Influence.",
            Vec::new(),
        ),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_replace_belief",
        "replace_belief",
        TabletopCapability::Advancement,
        object([
            ("belief_id", DomainValue::String("long_archive".to_owned())),
            (
                "confirmed_by",
                DomainValue::List(vec![DomainValue::String(
                    "org.weave.participant.lumen".to_owned(),
                )]),
            ),
            (
                "inspiration",
                DomainValue::String("The frame revealed a higher route.".to_owned()),
            ),
            (
                "new_statement",
                DomainValue::String("The archive must move above the flood line.".to_owned()),
            ),
            (
                "rationale",
                DomainValue::String(
                    "The single-user fallback confirmed the replacement.".to_owned(),
                ),
            ),
            (
                "replacement_id",
                DomainValue::String("fixture_replacement".to_owned()),
            ),
        ]),
    );
    state = receipt.after_state;
    let (_, receipt) = execute(
        &registry,
        &preview,
        &state,
        "fixture_close_second",
        "close_scene",
        TabletopCapability::Scenes,
        close_scene_input(
            "fixture_scene_two",
            "Fixture Scene Two",
            vec![review_input(
                "kept_promise",
                "The second scene confirmed the promise.",
                "confirmed",
            )],
            "refresh",
            10,
            "The second scene refreshed Influence.",
            Vec::new(),
        ),
    );
    state = receipt.after_state;

    let context = SparkEvolutionContext {
        refuted_beliefs: BTreeSet::from(["long_archive".to_owned()]),
        growth: number(field(&state.value, "growth")),
        faction_ids: setting.factions.keys().cloned().collect(),
        closed_scenes: BTreeSet::from([
            "fixture_scene_one".to_owned(),
            "fixture_scene_two".to_owned(),
        ]),
    };
    let evolution = SparkEvolutionRequest {
        format_version: SPARK_EVOLUTION_FORMAT_VERSION,
        evolution_id: "fixture_evolution".to_owned(),
        setting_id: setting.id.clone(),
        conditions: vec![
            SparkEvolutionCondition::BeliefRefuted {
                belief_id: "long_archive".to_owned(),
            },
            SparkEvolutionCondition::GrowthAtLeast { amount: 1 },
            SparkEvolutionCondition::SceneClosed {
                scene_id: "fixture_scene_two".to_owned(),
            },
        ],
        changes: vec![SparkEvolutionChange::SetSettingBelief {
            belief_id: "long_archive".to_owned(),
            statement: "The archive must move above the flood line.".to_owned(),
        }],
        rationale: "The campaign's confirmed replacement changed the setting.".to_owned(),
        accepted: true,
        accepted_by: vec!["org.weave.participant.lumen".to_owned()],
    };
    let evolved = evolve_spark_setting(&setting, &evolution, &context).expect("Evolution applies");
    assert!(evolved.applied);
    assert!(!evolved.canonical_write_back);

    let state_json = state.to_json().expect("state JSON");
    let state_ron = state.to_ron().expect("state RON");
    assert!(
        equivalent_json_and_ron::<TabletopState>(&state_json, &state_ron)
            .expect("state equivalence")
    );
    let setting_json = setting.to_json().expect("setting JSON");
    let setting_ron = setting.to_ron().expect("setting RON");
    assert!(
        equivalent_json_and_ron::<SparkSetting>(&setting_json, &setting_ron)
            .expect("setting equivalence")
    );
    let creation_json = collaborative.to_json().expect("creation JSON");
    let creation_ron = collaborative.to_ron().expect("creation RON");
    assert!(
        equivalent_json_and_ron::<SparkSettingCreationResult>(&creation_json, &creation_ron)
            .expect("creation equivalence")
    );
    assert!(!setting_json.is_empty());
    assert!(!setting_ron.is_empty());
    assert_eq!(
        spark_setting_story_values(&setting)["setting_id"],
        DomainValue::String(setting.id.clone())
    );
}

#[test]
fn collaboration_notes_document_the_cc0_core_and_optional_world_boundary() {
    assert!(SPARK_COLLABORATION_NOTES.len() >= 4);
    let notes = SPARK_COLLABORATION_NOTES.join(" ");
    assert!(notes.contains("CC0-1.0"));
    assert!(notes.contains("Weave-authored"));
    assert!(notes.contains("optional stable identifiers"));
    assert!(notes.contains("not reproduced"));
}

#[test]
fn single_user_fallback_covers_setting_scene_and_evolution() {
    let base = setting_creation();
    let lumen = "org.weave.participant.lumen".to_owned();
    let participants = BTreeMap::from([(
        lumen.clone(),
        SparkSettingParticipant {
            id: lumen.clone(),
            display_name: "Lumen".to_owned(),
            role: SparkParticipantRole::Owner,
        },
    )]);
    let single = SparkSettingCreation {
        participants: participants.clone(),
        contributions: base
            .contributions
            .iter()
            .map(|item| SparkSettingContribution {
                participant_id: lumen.clone(),
                ..item.clone()
            })
            .collect(),
        ..base
    };
    let created = resolve_spark_setting_creation(&single).expect("single-user creation");
    assert!(created.completed);
    let setting = created.setting.clone().expect("single-user setting");
    assert!(
        setting
            .setting_beliefs
            .values()
            .all(|belief| belief.owner_id == lumen)
    );

    let mut frame_request = scene_frame_request();
    for contributor in frame_request.contributors.values_mut() {
        *contributor = lumen.clone();
    }
    let frame = frame_spark_scene(&frame_request, &participants).expect("single-user frame");
    assert_eq!(frame.next_actor_id, lumen);

    let context = SparkEvolutionContext {
        refuted_beliefs: BTreeSet::new(),
        growth: 0,
        faction_ids: setting.factions.keys().cloned().collect(),
        closed_scenes: BTreeSet::new(),
    };
    let evolution = SparkEvolutionRequest {
        format_version: SPARK_EVOLUTION_FORMAT_VERSION,
        evolution_id: "single_user_evolution".to_owned(),
        setting_id: setting.id.clone(),
        conditions: Vec::new(),
        changes: vec![SparkEvolutionChange::SetFactionName {
            faction_id: "gate_wardens".to_owned(),
            name: "The Upper Gate Wardens".to_owned(),
        }],
        rationale: "The single-user fallback reviewed one setting change.".to_owned(),
        accepted: true,
        accepted_by: vec![lumen],
    };
    let evolved =
        evolve_spark_setting(&setting, &evolution, &context).expect("single-user Evolution");
    assert!(evolved.applied);
    assert!(evolved.setting.is_some());
}
