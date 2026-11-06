use std::collections::BTreeMap;

use weave_domain::DomainValue;
use weave_tabletop::{
    ExcludedMaterial, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolverRegistry, SPARK_ADAPTER_ID, SPARK_CREATION_FORMAT_VERSION, SPARK_SOURCE_RECORD,
    SPARK_SOURCE_RECORD_SHA256, SPARK_SOURCE_URL, SparkAttribute, SparkAttributeSeed,
    SparkBeliefSeed, SparkBeliefStatus, SparkBeliefVerdict, SparkConflictStatus,
    SparkCreationPreview, SparkCreationRequest, SparkInfluenceMode, SparkRelationshipSeed,
    SparkResolver, SparkStep, SparkTalentSeed, SparkTalentTier, TabletopCapability, TabletopError,
    TabletopState, canonical_fingerprint, create_spark_character, equivalent_json_and_ron,
    resolved_adapter, sha256_bytes, spark_manifest, validate_adapter_manifest,
    validate_resolution_receipt, validate_spark_creation_preview,
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
