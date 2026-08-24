use weave_character::{
    CharacterCollection, CharacterExtension, LockState, ProjectionAssignmentDisposition,
    ProjectionConfig, ProjectionKind, ProjectionLockRevision, ProjectionPack, ProjectionProposal,
    ProjectionPublicDecision, ProjectionReceipt, ProjectionReview, ProjectionReviewDecision,
    ROLE_PROJECTION_EXTENSION_NAMESPACE, apply_projection_lock_revision, apply_projection_review,
    projection_pack_fingerprint, projection_pack_schema, propose_projections, recompute_derived,
    validate_projection_pack,
};

const INPUT_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/input.character-collection.json"
);
const PACK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/glasswind.projection-pack.json"
);
const PACK_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/glasswind.projection-pack.ron"
);
const CONFIG_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/selection.projection-config.json"
);
const PROPOSAL_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/proposal.projection-proposal.json"
);
const REVIEW_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/review.projection-review.json"
);
const RECEIPT_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/receipt.projection-receipt.json"
);
const RECEIPT_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/receipt.projection-receipt.ron"
);
const LOCK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/unlock.projection-lock-revision.json"
);
const UNLOCKED_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/unlocked.character-collection.json"
);
const REBALANCE_CONFIG_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/rebalance.projection-config.json"
);
const REBALANCE_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/rebalance.projection-proposal.json"
);

#[test]
fn pack_round_trips_and_calibrations_pin_the_fixed_point_method() {
    let json = ProjectionPack::from_json(PACK_JSON).unwrap();
    let ron = ProjectionPack::from_ron(PACK_RON).unwrap();
    assert_eq!(json, ron);
    assert_eq!(json.to_json().unwrap(), PACK_JSON);
    assert_eq!(json.to_ron().unwrap(), PACK_RON);
    assert_eq!(
        projection_pack_fingerprint(&json).unwrap(),
        "41f7ef23f3d141644c95226b3fac2bc2f0455537b7b930170fcb188954f62746"
    );
    assert_eq!(json.inputs.len(), 12);
    assert_eq!(json.taxonomies.len(), 4);
    assert_eq!(json.calibrations.len(), 2);

    let schema: serde_json::Value =
        serde_json::from_str(&projection_pack_schema().unwrap()).unwrap();
    assert_eq!(
        schema["$id"],
        "urn:weave:schema:character-projection-pack:1"
    );

    let mut invalid = json;
    invalid
        .taxonomies
        .values_mut()
        .next()
        .unwrap()
        .independent_evidence = true;
    assert!(validate_projection_pack(&invalid).is_err());

    let mut secret_shaped = ProjectionPack::from_json(PACK_JSON).unwrap();
    let rejected = "api_key=example_credential_value";
    secret_shaped.title = rejected.to_owned();
    let error = validate_projection_pack(&secret_shaped).expect_err("credential-shaped text");
    assert!(!error.to_string().contains(rejected));
}

#[test]
fn proposal_is_seeded_capacity_limited_reserved_and_fully_explained() {
    let collection = CharacterCollection::from_json(INPUT_JSON).unwrap();
    let pack = ProjectionPack::from_json(PACK_JSON).unwrap();
    let config = ProjectionConfig::from_json(CONFIG_JSON).unwrap();
    let expected = ProjectionProposal::from_json(PROPOSAL_JSON).unwrap();
    let actual = propose_projections(&collection, &pack, &config, 20_260_824).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.review_manifest.len(), 3);
    assert!(actual.distribution.unavailable.is_empty());
    assert_eq!(
        actual.distribution.reserved["org.weave.character.sable_reed"],
        ["org.weave.projection.glasswind_lenses.social_role"]
    );
    for entries in actual.distribution.counts.values() {
        assert_eq!(entries.values().copied().sum::<u32>(), 3);
        assert!(entries.values().all(|count| *count == 1));
    }
    for assignments in actual.assignments.values() {
        for assignment in assignments.values() {
            assert!(matches!(
                assignment.disposition,
                ProjectionAssignmentDisposition::Proposed
                    | ProjectionAssignmentDisposition::Reserved
            ));
            assert!(assignment.candidates.iter().all(|candidate| {
                candidate.coverage_micros == 1_000_000
                    && candidate.score_micros.is_some()
                    && candidate.ordered_evidence.iter().all(|evidence| {
                        evidence.profile_path.starts_with("canon.personality.")
                            && evidence.profile_micros.is_some()
                    })
            }));
        }
    }

    let changed_seed = propose_projections(&collection, &pack, &config, 20_260_825).unwrap();
    assert_ne!(changed_seed, actual);
    for (character_id, taxonomies) in &actual.assignments {
        for (taxonomy_id, assignment) in taxonomies {
            for candidate in &assignment.candidates {
                let changed = changed_seed.assignments[character_id][taxonomy_id]
                    .candidates
                    .iter()
                    .find(|value| value.entry_id == candidate.entry_id)
                    .unwrap();
                assert_eq!(candidate.score_micros, changed.score_micros);
                assert_ne!(candidate.seeded_sha256, changed.seeded_sha256);
            }
        }
    }
}

#[test]
fn complete_review_replays_without_crossing_any_authority_boundary() {
    let input = CharacterCollection::from_json(INPUT_JSON).unwrap();
    let proposal = ProjectionProposal::from_json(PROPOSAL_JSON).unwrap();
    let review = ProjectionReview::from_json(REVIEW_JSON).unwrap();
    let expected = ProjectionReceipt::from_json(RECEIPT_JSON).unwrap();
    assert_eq!(expected, ProjectionReceipt::from_ron(RECEIPT_RON).unwrap());
    let actual = apply_projection_review(&input, &proposal, &review).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.output_collection.revision, input.revision + 1);
    assert!(
        review
            .decisions
            .values()
            .flat_map(|values| values.values())
            .any(|decision| matches!(decision, ProjectionReviewDecision::Reject { .. }))
    );
    assert!(
        review
            .decisions
            .values()
            .flat_map(|values| values.values())
            .any(|decision| matches!(decision, ProjectionReviewDecision::Withhold { .. }))
    );
    assert!(
        review
            .decisions
            .values()
            .flat_map(|values| values.values())
            .any(|decision| matches!(decision, ProjectionReviewDecision::Edit { .. }))
    );
    assert!(
        review
            .decisions
            .values()
            .flat_map(|values| values.values())
            .any(|decision| matches!(decision, ProjectionReviewDecision::Override { .. }))
    );

    for (character_id, before) in &input.characters {
        let after = &actual.output_collection.characters[character_id];
        assert_eq!(before.canon, after.canon);
        assert_eq!(before.derived, after.derived);
        assert_eq!(before.suggestions, after.suggestions);
        for (namespace, extension) in &before.extensions {
            if namespace != ROLE_PROJECTION_EXTENSION_NAMESPACE {
                assert_eq!(after.extensions.get(namespace), Some(extension));
            }
        }
    }
    let ari = &actual.output_collection.characters["org.weave.character.ari_vale"];
    let CharacterExtension::RoleProjections(roles) =
        &ari.extensions[ROLE_PROJECTION_EXTENSION_NAMESPACE]
    else {
        panic!("reserved namespace contains typed projections")
    };
    assert_eq!(roles.value.roles["narrative_role"].lock, LockState::Locked);
    assert!(
        roles.value.roles["personality_lens"].lossy
            && !roles.value.roles["personality_lens"].independent_evidence
            && matches!(
                roles.value.roles["personality_lens"].decision,
                ProjectionPublicDecision::Derived
            )
    );
    assert!(!roles.value.roles.contains_key("vocation"));
    assert!(roles.value.roles.values().all(|value| {
        value.pack.as_ref() == Some(&proposal.pack_ref)
            && value.proposal_sha256.as_deref() == Some(actual.proposal_sha256.as_str())
            && value.review_sha256.as_deref() == Some(actual.review_sha256.as_str())
    }));
}

#[test]
fn locks_and_authored_overrides_survive_rebalance_while_unlocked_values_move() {
    let receipt = ProjectionReceipt::from_json(RECEIPT_JSON).unwrap();
    let revision = ProjectionLockRevision::from_json(LOCK_JSON).unwrap();
    let unlocked = apply_projection_lock_revision(&receipt.output_collection, &revision).unwrap();
    assert_eq!(
        unlocked,
        CharacterCollection::from_json(UNLOCKED_JSON).unwrap()
    );

    let pack = ProjectionPack::from_json(PACK_JSON).unwrap();
    let config = ProjectionConfig::from_json(REBALANCE_CONFIG_JSON).unwrap();
    let rebalance = propose_projections(&unlocked, &pack, &config, 20_260_825).unwrap();
    assert_eq!(
        rebalance,
        ProjectionProposal::from_json(REBALANCE_JSON).unwrap()
    );
    let retained = &rebalance.assignments["org.weave.character.sable_reed"]["org.weave.projection.glasswind_lenses.narrative_role"];
    assert_eq!(
        retained.disposition,
        ProjectionAssignmentDisposition::Retained
    );
    assert!(matches!(
        retained.prior.as_ref().unwrap().decision,
        ProjectionPublicDecision::Overridden
    ));
    assert!(
        !rebalance.review_manifest["org.weave.character.sable_reed"]
            .contains(&"org.weave.projection.glasswind_lenses.narrative_role".to_owned())
    );
}

#[test]
fn missing_inputs_reduce_coverage_and_never_become_fabricated_scores() {
    let mut collection = CharacterCollection::from_json(INPUT_JSON).unwrap();
    collection
        .characters
        .retain(|id, _| id == "org.weave.character.ari_vale");
    let profile = collection.characters.values_mut().next().unwrap();
    profile.canon.personality.openness.factor = None;
    profile.canon.personality.openness.creativity = None;
    profile.canon.personality.openness.inquisitiveness = None;
    profile.canon.personality.conscientiousness.organization = None;
    profile.canon.personality.conscientiousness.diligence = None;
    profile.canon.personality.extraversion.factor = None;
    profile.canon.personality.extraversion.sociability = None;
    profile.canon.personality.agreeableness.patience = None;
    recompute_derived(profile);

    let pack = ProjectionPack::from_json(PACK_JSON).unwrap();
    let mut config = ProjectionConfig::from_json(CONFIG_JSON).unwrap();
    config.eligible_character_ids = collection.characters.keys().cloned().collect();
    config.selected_taxonomies =
        vec!["org.weave.projection.glasswind_lenses.personality".to_owned()];
    config
        .capacity_overrides
        .retain(|taxonomy, _| taxonomy == "org.weave.projection.glasswind_lenses.personality");
    config.reservations.clear();
    config.minimum_coverage_micros = 1_000_000;
    let proposal = propose_projections(&collection, &pack, &config, 7).unwrap();
    let assignment = &proposal.assignments["org.weave.character.ari_vale"]["org.weave.projection.glasswind_lenses.personality"];
    assert_eq!(
        assignment.disposition,
        ProjectionAssignmentDisposition::Unavailable
    );
    assert!(assignment.candidates.iter().all(|candidate| {
        candidate.coverage_micros < 1_000_000
            && !candidate.qualified
            && candidate
                .ordered_evidence
                .iter()
                .any(|evidence| evidence.profile_micros.is_none())
    }));
    assert!(proposal.review_manifest.is_empty());
    assert!(
        proposal
            .distribution
            .unavailable
            .contains_key("org.weave.character.ari_vale")
    );
    assert!(pack.taxonomies.values().any(|value| {
        matches!(value.kind, ProjectionKind::CategoricalPersonality) && value.lossy
    }));
}
