use weave_character::{
    CHARACTER_OPERATION_REQUEST_FORMAT_VERSION, CharacterCollection, CharacterCorpusAction,
    CharacterDiagnosticCode, CharacterExtension, CharacterJobProgress, CharacterOperationRequest,
    CharacterOverlay, CharacterProfile, CharacterProposal, CharacterProposalReview,
    CharacterReviewDecision, CharacterScope, LockState, apply_reviewed_character_proposal,
    character_collection_schema, character_operation_request_schema, character_progress_schema,
    character_proposal_schema, character_review_schema, collection_fingerprint, list_characters,
    propose_character_operation, resume_character_operation, review_character_proposal,
    show_character,
};

const COLLECTION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/collection.character-collection.json"
);
const COLLECTION_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/collection.character-collection.ron"
);
const REQUEST_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-request.json"
);
const REQUEST_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-request.ron"
);
const PROPOSAL_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-proposal.json"
);
const PROPOSAL_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-proposal.ron"
);
const REVIEW_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-review.json"
);
const REVIEW_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-review.ron"
);
const PROGRESS_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-progress.json"
);
const PROGRESS_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/rename.character-progress.ron"
);
const RENAMED_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/renamed.character-collection.json"
);
const RENAMED_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/renamed.character-collection.ron"
);
const PROFILE_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.json");
const PROFILE_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.ron");
const OVERLAY_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/overlay.character.json");
const STALE_REVIEW_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/stale-character-review.json"
);
const STALE_PROGRESS_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/stale-character-progress.json"
);
const MALFORMED_PROPOSAL_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/malformed-character-proposal.json"
);

const COLLECTION_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-collection-v1.schema.json");
const REQUEST_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-operation-request-v1.schema.json");
const PROPOSAL_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-proposal-v1.schema.json");
const REVIEW_SCHEMA: &str = include_str!("../../../schemas/weave-character-review-v1.schema.json");
const PROGRESS_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-progress-v1.schema.json");

fn fixture_collection() -> CharacterCollection {
    CharacterCollection::from_json(COLLECTION_JSON).expect("collection fixture")
}

fn request_for(
    collection: &CharacterCollection,
    id: &str,
    scope: CharacterScope,
    action: CharacterCorpusAction,
) -> CharacterOperationRequest {
    let mut request = CharacterOperationRequest::from_json(REQUEST_JSON).expect("request fixture");
    request.id = format!("org.weave.character.operation.{id}");
    request.expected_input_sha256 = collection_fingerprint(collection).expect("collection hash");
    request.scope = scope;
    request.action = action;
    request
}

fn retarget_profile(
    mut profile: CharacterProfile,
    id: &str,
    display_name: &str,
) -> CharacterProfile {
    let old_id = profile.id.clone();
    profile.id = id.to_owned();
    profile.canon.identity.display_name.value = display_name.to_owned();
    for extension in profile.extensions.values_mut() {
        let CharacterExtension::Relationships(relationships) = extension else {
            continue;
        };
        for edge in relationships.value.edges.values_mut() {
            if edge.source_character_id == old_id {
                edge.source_character_id = id.to_owned();
            }
        }
    }
    profile
}

#[test]
fn operation_artifacts_round_trip_and_match_canonical_schemas() {
    let collection = fixture_collection();
    assert_eq!(
        CharacterCollection::from_ron(COLLECTION_RON).unwrap(),
        collection
    );
    assert_eq!(collection.to_json().unwrap(), COLLECTION_JSON);
    assert_eq!(collection.to_ron().unwrap(), COLLECTION_RON);

    let request = CharacterOperationRequest::from_json(REQUEST_JSON).unwrap();
    assert_eq!(
        CharacterOperationRequest::from_ron(REQUEST_RON).unwrap(),
        request
    );
    assert_eq!(request.to_json().unwrap(), REQUEST_JSON);
    assert_eq!(request.to_ron().unwrap(), REQUEST_RON);

    let proposal = CharacterProposal::from_json(PROPOSAL_JSON).unwrap();
    assert_eq!(CharacterProposal::from_ron(PROPOSAL_RON).unwrap(), proposal);
    assert_eq!(proposal.to_json().unwrap(), PROPOSAL_JSON);
    assert_eq!(proposal.to_ron().unwrap(), PROPOSAL_RON);
    assert_eq!(
        propose_character_operation(&collection, &request).unwrap(),
        proposal
    );

    let review = CharacterProposalReview::from_json(REVIEW_JSON).unwrap();
    assert_eq!(
        CharacterProposalReview::from_ron(REVIEW_RON).unwrap(),
        review
    );
    assert_eq!(review.to_json().unwrap(), REVIEW_JSON);
    assert_eq!(review.to_ron().unwrap(), REVIEW_RON);

    let progress = CharacterJobProgress::from_json(PROGRESS_JSON).unwrap();
    assert_eq!(
        CharacterJobProgress::from_ron(PROGRESS_RON).unwrap(),
        progress
    );
    assert_eq!(progress.to_json().unwrap(), PROGRESS_JSON);
    assert_eq!(progress.to_ron().unwrap(), PROGRESS_RON);

    let renamed = apply_reviewed_character_proposal(&collection, &proposal, &review).unwrap();
    assert_eq!(
        CharacterCollection::from_json(RENAMED_JSON).unwrap(),
        renamed
    );
    assert_eq!(CharacterCollection::from_ron(RENAMED_RON).unwrap(), renamed);

    assert_eq!(character_collection_schema().unwrap(), COLLECTION_SCHEMA);
    assert_eq!(
        character_operation_request_schema().unwrap(),
        REQUEST_SCHEMA
    );
    assert_eq!(character_proposal_schema().unwrap(), PROPOSAL_SCHEMA);
    assert_eq!(character_review_schema().unwrap(), REVIEW_SCHEMA);
    assert_eq!(character_progress_schema().unwrap(), PROGRESS_SCHEMA);
}

#[test]
fn list_show_and_filter_are_stable_and_read_only() {
    let collection = fixture_collection();
    let original = collection.clone();
    let all = list_characters(&collection, &CharacterScope::All).unwrap();
    assert_eq!(
        all.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        [
            "org.weave.character.ari_vale",
            "org.weave.character.sable_reed"
        ]
    );
    let filtered = list_characters(
        &collection,
        &CharacterScope::Filter {
            id_prefix: Some("org.weave.character.s".to_owned()),
            extension_namespace: Some("org.weave.character.relationships".to_owned()),
        },
    )
    .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].display_name, "Sable Reed");
    assert_eq!(
        show_character(&collection, "org.weave.character.ari_vale")
            .unwrap()
            .canon
            .identity
            .display_name
            .value,
        "Ari Vale"
    );
    assert_eq!(
        show_character(&collection, "org.weave.character.absent")
            .expect_err("missing profile")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::InvalidReference
    );
    assert_eq!(collection, original);
}

#[test]
fn dry_run_is_byte_stable_and_resumption_is_payload_free() {
    let collection = fixture_collection();
    let request = CharacterOperationRequest::from_json(REQUEST_JSON).unwrap();
    let original = collection.clone();
    let first = propose_character_operation(&collection, &request).unwrap();
    let second = propose_character_operation(&collection, &request).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.to_json().unwrap(), second.to_json().unwrap());
    assert_eq!(collection, original);

    let first_step = resume_character_operation(&collection, &request, None, 1).unwrap();
    assert!(first_step.proposal.is_none());
    assert_eq!(first_step.progress.next_index, 1);
    assert_eq!(
        first_step.progress,
        CharacterJobProgress::from_json(PROGRESS_JSON).unwrap()
    );
    let progress_json = first_step.progress.to_json().unwrap();
    for forbidden in ["canon", "display_name", "Ari Vale", "source_character_id"] {
        assert!(!progress_json.contains(forbidden));
    }
    let final_step =
        resume_character_operation(&collection, &request, Some(&first_step.progress), 1).unwrap();
    assert_eq!(final_step.proposal.unwrap(), first);

    let stale = CharacterJobProgress::from_json(STALE_PROGRESS_JSON).unwrap();
    assert_eq!(
        resume_character_operation(&collection, &request, Some(&stale), 1)
            .expect_err("stale cursor")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::StaleInput
    );
}

#[test]
fn accepted_apply_is_atomic_and_every_other_manifest_fails_closed() {
    let collection = fixture_collection();
    let proposal = CharacterProposal::from_json(PROPOSAL_JSON).unwrap();
    let review = CharacterProposalReview::from_json(REVIEW_JSON).unwrap();
    let original = collection.clone();
    let applied = apply_reviewed_character_proposal(&collection, &proposal, &review).unwrap();
    assert_eq!(applied.revision, collection.revision + 1);
    assert_eq!(collection, original);

    let rejected = review_character_proposal(
        &proposal,
        CharacterReviewDecision::Rejected,
        "org.weave.reviewer.fixture",
        "Reject the complete proposal without applying any subset.",
    )
    .unwrap();
    assert!(apply_reviewed_character_proposal(&collection, &proposal, &rejected).is_err());
    assert_eq!(collection, original);

    let stale_review = CharacterProposalReview::from_json(STALE_REVIEW_JSON).unwrap();
    assert_eq!(
        apply_reviewed_character_proposal(&collection, &proposal, &stale_review)
            .expect_err("stale review")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::StaleInput
    );

    let mut changed_collection = collection.clone();
    changed_collection.revision += 1;
    assert_eq!(
        apply_reviewed_character_proposal(&changed_collection, &proposal, &review)
            .expect_err("stale input")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::StaleInput
    );
    assert!(CharacterProposal::from_json(MALFORMED_PROPOSAL_JSON).is_err());
    assert_eq!(collection, original);
}

#[test]
fn reference_safe_rename_previews_every_migration_and_respects_locks() {
    let collection = fixture_collection();
    let request = CharacterOperationRequest::from_json(REQUEST_JSON).unwrap();
    let proposal = propose_character_operation(&collection, &request).unwrap();
    assert_eq!(proposal.changes.len(), 2);
    assert_eq!(proposal.changes[0].affected_references.len(), 1);
    assert_eq!(proposal.changes[1].affected_references.len(), 1);
    assert!(
        proposal.changes[0].affected_references[0]
            .path
            .ends_with("source_character_id")
    );
    assert!(
        proposal.changes[1].affected_references[0]
            .path
            .ends_with("target_character_id")
    );

    let renamed = &proposal.resulting_collection;
    assert!(
        !renamed
            .characters
            .contains_key("org.weave.character.ari_vale")
    );
    let renamed_ari = &renamed.characters["org.weave.character.ari_vale_wayfinder"];
    let CharacterExtension::Relationships(ari_relationships) =
        &renamed_ari.extensions["org.weave.character.relationships"]
    else {
        panic!("relationship kind changed")
    };
    assert!(
        ari_relationships
            .value
            .edges
            .values()
            .all(|edge| { edge.source_character_id == "org.weave.character.ari_vale_wayfinder" })
    );
    let CharacterExtension::Relationships(sable_relationships) =
        &renamed.characters["org.weave.character.sable_reed"].extensions["org.weave.character.relationships"]
    else {
        panic!("relationship kind changed")
    };
    assert!(
        sable_relationships
            .value
            .edges
            .values()
            .all(|edge| { edge.target_character_id == "org.weave.character.ari_vale_wayfinder" })
    );

    let mut locked_collection = fixture_collection();
    let CharacterExtension::Relationships(relationships) = locked_collection
        .characters
        .get_mut("org.weave.character.sable_reed")
        .unwrap()
        .extensions
        .get_mut("org.weave.character.relationships")
        .unwrap()
    else {
        panic!("relationship kind changed")
    };
    relationships.header.lock = LockState::Locked;
    let locked_request = request_for(
        &locked_collection,
        "locked_rename",
        request.scope.clone(),
        request.action.clone(),
    );
    assert_eq!(
        propose_character_operation(&locked_collection, &locked_request)
            .expect_err("locked reference")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::LockedField
    );

    let CharacterCorpusAction::Rename {
        override_locked, ..
    } = &locked_request.action
    else {
        panic!("rename fixture changed")
    };
    assert!(!override_locked);
    let mut override_request = locked_request;
    let CharacterCorpusAction::Rename {
        override_locked, ..
    } = &mut override_request.action
    else {
        panic!("rename fixture changed")
    };
    *override_locked = true;
    let overridden = propose_character_operation(&locked_collection, &override_request).unwrap();
    let CharacterExtension::Relationships(relationships) = &overridden
        .resulting_collection
        .characters["org.weave.character.sable_reed"]
        .extensions["org.weave.character.relationships"]
    else {
        panic!("relationship kind changed")
    };
    assert_eq!(relationships.header.lock, LockState::Locked);
    assert_eq!(
        relationships.header.rationale,
        "Migrate the stable identifier and every relationship reference together."
    );
}

#[test]
fn create_clone_revise_import_and_recompute_share_one_contract() {
    let collection = fixture_collection();
    let ari = CharacterProfile::from_json(PROFILE_JSON).unwrap();
    assert_eq!(CharacterProfile::from_ron(PROFILE_RON).unwrap(), ari);

    let created_profile = retarget_profile(
        ari.clone(),
        "org.weave.character.lumen_quill",
        "Lumen Quill",
    );
    let create = request_for(
        &collection,
        "create_lumen",
        CharacterScope::All,
        CharacterCorpusAction::Create {
            profile: Box::new(created_profile),
        },
    );
    let created = propose_character_operation(&collection, &create).unwrap();
    assert!(
        created
            .resulting_collection
            .characters
            .contains_key("org.weave.character.lumen_quill")
    );

    let clone = request_for(
        &collection,
        "clone_ari",
        CharacterScope::All,
        CharacterCorpusAction::Clone {
            source_id: "org.weave.character.ari_vale".to_owned(),
            new_id: "org.weave.character.tess_waymark".to_owned(),
            display_name: Some("Tess Waymark".to_owned()),
            rationale: "Create an explicit synthetic branch for further authoring.".to_owned(),
        },
    );
    let cloned = propose_character_operation(&collection, &clone).unwrap();
    assert_eq!(
        cloned.resulting_collection.characters["org.weave.character.tess_waymark"]
            .canon
            .identity
            .display_name
            .value,
        "Tess Waymark"
    );

    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).unwrap();
    let mut revise = request_for(
        &collection,
        "revise_ari",
        CharacterScope::Characters {
            ids: vec!["org.weave.character.ari_vale".to_owned()],
        },
        CharacterCorpusAction::Revise {
            character_id: "org.weave.character.ari_vale".to_owned(),
            operations: overlay.operations,
        },
    );
    revise.provenance = overlay.provenance;
    let revised = propose_character_operation(&collection, &revise).unwrap();
    assert_eq!(
        revised.resulting_collection.characters["org.weave.character.ari_vale"]
            .canon
            .identity
            .display_name
            .value,
        "Ari Vale of Glasswind"
    );

    let exact_profiles = collection.characters.values().cloned().collect::<Vec<_>>();
    let import = request_for(
        &collection,
        "idempotent_import",
        CharacterScope::All,
        CharacterCorpusAction::Import {
            profiles: exact_profiles,
            force: false,
            rationale: None,
        },
    );
    let first_import = propose_character_operation(&collection, &import).unwrap();
    let second_import = propose_character_operation(&collection, &import).unwrap();
    assert!(first_import.changes.is_empty());
    assert_eq!(first_import, second_import);
    assert_eq!(first_import.resulting_collection, collection);

    let mut replacement = ari;
    replacement.canon.identity.display_name.value = "Ari Vale Revised".to_owned();
    let conflicting_import = request_for(
        &collection,
        "conflicting_import",
        CharacterScope::All,
        CharacterCorpusAction::Import {
            profiles: vec![replacement.clone()],
            force: false,
            rationale: None,
        },
    );
    assert!(propose_character_operation(&collection, &conflicting_import).is_err());
    let forced_import = request_for(
        &collection,
        "forced_import",
        CharacterScope::All,
        CharacterCorpusAction::Import {
            profiles: vec![replacement],
            force: true,
            rationale: Some(
                "Replace locked authored data only as this complete reviewed proposal.".to_owned(),
            ),
        },
    );
    assert_eq!(
        propose_character_operation(&collection, &forced_import)
            .unwrap()
            .resulting_collection
            .characters["org.weave.character.ari_vale"]
            .canon
            .identity
            .display_name
            .value,
        "Ari Vale Revised"
    );

    let mut stale = collection;
    stale
        .characters
        .get_mut("org.weave.character.ari_vale")
        .unwrap()
        .derived
        .ocean
        .openness
        .as_mut()
        .unwrap()
        .score = 0.0;
    let locked_prudence = stale.characters["org.weave.character.ari_vale"]
        .canon
        .personality
        .conscientiousness
        .prudence
        .clone();
    let recompute = request_for(
        &stale,
        "recompute_ari",
        CharacterScope::Characters {
            ids: vec!["org.weave.character.ari_vale".to_owned()],
        },
        CharacterCorpusAction::Recompute,
    );
    let recomputed = propose_character_operation(&stale, &recompute).unwrap();
    let repaired = &recomputed.resulting_collection.characters["org.weave.character.ari_vale"];
    assert_eq!(
        repaired.derived.ocean.openness.as_ref().unwrap().score,
        0.83
    );
    assert_eq!(
        repaired.canon.personality.conscientiousness.prudence,
        locked_prudence
    );
}

#[test]
fn operation_validation_rejects_secret_shaped_text_without_echoing_it() {
    let collection = fixture_collection();
    let request = request_for(
        &collection,
        "secret_rejection",
        CharacterScope::All,
        CharacterCorpusAction::Clone {
            source_id: "org.weave.character.ari_vale".to_owned(),
            new_id: "org.weave.character.redacted_clone".to_owned(),
            display_name: None,
            rationale: "api_key=[REDACTED]".to_owned(),
        },
    );
    assert_eq!(
        request.request_format_version,
        CHARACTER_OPERATION_REQUEST_FORMAT_VERSION
    );
    let error = propose_character_operation(&collection, &request).expect_err("secret-shaped text");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(!error.to_string().contains("api_key"));
}
