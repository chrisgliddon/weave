use weave_tabletop::{
    ADAPTER_SWITCH_FORMAT_VERSION, AdapterCharacterDefinition, AdapterManifest,
    AdapterSwitchPreview, AdapterSwitchRequest, SwitchDecisionKind, SwitchDiagnosticSeverity,
    SwitchInitiator, SwitchOutgoingPolicy, TabletopCharacterProjection, TabletopState,
    canonical_fingerprint, dungeonpunk_manifest, freehack_manifest, plug_and_play_manifest,
    resolved_adapter, validate_character_projection, validate_tabletop_state,
};

const PNP_PROJECTION: &str = include_str!(
    "../../../examples/tabletop-adapters/plug-and-play/character.tabletop-projection.json"
);
const PNP_STATE: &str =
    include_str!("../../../examples/tabletop-adapters/plug-and-play/state.tabletop-state.json");
const DUNGEONPUNK_PROJECTION: &str = include_str!(
    "../../../examples/tabletop-adapters/dungeonpunk/character.tabletop-projection.json"
);
const DUNGEONPUNK_STATE: &str =
    include_str!("../../../examples/tabletop-adapters/dungeonpunk/state.tabletop-state.json");
const FREEHACK_PROJECTION: &str =
    include_str!("../../../examples/tabletop-adapters/freehack/character.tabletop-projection.json");
const FREEHACK_STATE: &str = include_str!(
    "../../../examples/tabletop-adapters/freehack/authority-state.tabletop-state.json"
);

const CHARACTER_ID: &str = "org.weave.character.switch_fixture";
const CANONICAL_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn installed() -> Vec<AdapterManifest> {
    vec![
        plug_and_play_manifest(),
        dungeonpunk_manifest(),
        freehack_manifest(),
    ]
}

fn fixture_projection(source: &str) -> TabletopCharacterProjection {
    let mut projection = TabletopCharacterProjection::from_json(source).unwrap();
    projection.character_id = CHARACTER_ID.to_owned();
    projection.canonical_profile_sha256 = CANONICAL_SHA256.to_owned();
    projection.canonical_character_write_back = false;
    projection
}

fn fixture_definition(source: &str) -> AdapterCharacterDefinition {
    fixture_projection(source).active.unwrap()
}

fn fixture_state(source: &str) -> TabletopState {
    TabletopState::from_json(source).unwrap()
}

fn request(
    target: Option<weave_tabletop::ResolvedAdapter>,
    target_definition: Option<AdapterCharacterDefinition>,
) -> AdapterSwitchRequest {
    AdapterSwitchRequest {
        switch_format_version: ADAPTER_SWITCH_FORMAT_VERSION,
        target,
        initiator: SwitchInitiator::Author,
        initiator_id: "org.weave.test.author".to_owned(),
        outgoing_policy: SwitchOutgoingPolicy::Archive,
        discard_inactive: Vec::new(),
        accepted_decisions: Vec::new(),
        target_definition,
    }
}

fn accept_all(preview: &AdapterSwitchPreview, request: &mut AdapterSwitchRequest) {
    request.accepted_decisions = preview
        .required_decisions
        .iter()
        .map(|decision| decision.id.clone())
        .collect();
    request.accepted_decisions.sort();
    request.accepted_decisions.dedup();
}

fn declared_capabilities(manifest: &AdapterManifest) -> Vec<weave_tabletop::TabletopCapability> {
    let mut capabilities = manifest
        .capabilities
        .iter()
        .map(|declaration| declaration.capability)
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities
}

#[test]
fn fixtures_match_the_installed_adapter_releases() {
    let installed = installed();
    let pnp = resolved_adapter(&installed[0]).unwrap();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let freehack = resolved_adapter(&installed[2]).unwrap();
    assert_eq!(
        fixture_projection(PNP_PROJECTION).active.unwrap().adapter,
        pnp
    );
    assert_eq!(
        fixture_projection(DUNGEONPUNK_PROJECTION)
            .active
            .unwrap()
            .adapter,
        dungeonpunk
    );
    assert_eq!(
        fixture_projection(FREEHACK_PROJECTION)
            .active
            .unwrap()
            .adapter,
        freehack
    );
    validate_tabletop_state(&fixture_state(PNP_STATE), &installed[0]).unwrap();
    validate_tabletop_state(&fixture_state(DUNGEONPUNK_STATE), &installed[1]).unwrap();
    validate_tabletop_state(&fixture_state(FREEHACK_STATE), &installed[2]).unwrap();
}

#[test]
fn switches_across_all_three_adapters_without_touching_canon() {
    let installed = installed();
    let pnp = resolved_adapter(&installed[0]).unwrap();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let freehack = resolved_adapter(&installed[2]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);
    let canonical_before = projection.canonical_profile_sha256.clone();
    let suggestions_before = projection.suggestions.clone();

    let pnp_state = fixture_state(PNP_STATE);
    let mut first = request(
        Some(dungeonpunk.clone()),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &first, Some(&pnp_state))
        .unwrap();
    assert_eq!(preview.from, Some(pnp.clone()));
    assert_eq!(preview.to, Some(dungeonpunk.clone()));
    assert!(preview.archive_current);
    assert_eq!(
        preview.archive.as_ref().unwrap().namespace,
        pnp.namespace_key()
    );
    assert!(!preview.automatic_conversion);
    let pnp_capabilities = declared_capabilities(&installed[0])
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let dungeonpunk_capabilities = declared_capabilities(&installed[1])
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        preview.capabilities_gained,
        dungeonpunk_capabilities
            .difference(&pnp_capabilities)
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        preview.capabilities_lost,
        pnp_capabilities
            .difference(&dungeonpunk_capabilities)
            .copied()
            .collect::<Vec<_>>()
    );
    assert!(!preview.non_transferable_fields.is_empty());
    assert!(
        preview
            .non_transferable_fields
            .iter()
            .all(|field| field.starts_with("definition."))
    );
    assert!(!preview.incompatible_active_resources.is_empty());
    assert!(preview.active_state_sha256.is_some());
    assert!(preview.state_archive_required);
    assert!(
        preview
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("never converted"))
    );
    accept_all(&preview, &mut first);
    let first_audit = projection
        .apply_switch(&installed, &first, Some(&pnp_state))
        .unwrap();
    assert_eq!(first_audit.from, Some(pnp.clone()));
    assert_eq!(first_audit.to, Some(dungeonpunk.clone()));
    assert_eq!(first_audit.initiator, SwitchInitiator::Author);
    assert_eq!(first_audit.initiator_id, "org.weave.test.author");
    assert_eq!(first_audit.canonical_profile_sha256, canonical_before);
    assert!(!first_audit.canonical_character_write_back);
    assert_eq!(
        first_audit.result_projection_sha256,
        canonical_fingerprint(&projection).unwrap()
    );
    assert_eq!(
        projection.active.as_ref().unwrap().adapter,
        dungeonpunk.clone()
    );
    assert_eq!(projection.canonical_profile_sha256, canonical_before);
    assert_eq!(projection.suggestions, suggestions_before);
    assert!(projection.archived_projection(&pnp).is_some());
    assert!(projection.active_definition().is_some());
    assert!(
        projection
            .projection_for_namespace(&pnp.namespace_key())
            .is_none(),
        "an inactive archive must never resolve for evaluation"
    );
    assert_eq!(
        projection.active_capabilities(&installed).unwrap(),
        declared_capabilities(&installed[1])
    );

    let dungeonpunk_state = fixture_state(DUNGEONPUNK_STATE);
    let mut second = request(
        Some(freehack.clone()),
        Some(fixture_definition(FREEHACK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &second, Some(&dungeonpunk_state))
        .unwrap();
    accept_all(&preview, &mut second);
    let second_audit = projection
        .apply_switch(&installed, &second, Some(&dungeonpunk_state))
        .unwrap();
    assert_eq!(second_audit.from, Some(dungeonpunk.clone()));
    assert_eq!(second_audit.to, Some(freehack.clone()));
    assert_eq!(
        projection.active.as_ref().unwrap().adapter,
        freehack.clone()
    );
    assert_eq!(projection.inactive.len(), 2);
    assert!(projection.archived_projection(&pnp).is_some());
    assert!(projection.archived_projection(&dungeonpunk).is_some());
    assert_eq!(projection.canonical_profile_sha256, canonical_before);
    assert_eq!(
        projection.active_capabilities(&installed).unwrap(),
        declared_capabilities(&installed[2])
    );
    assert!(
        projection
            .projection_for_namespace(&dungeonpunk.namespace_key())
            .is_none()
    );
    assert!(
        projection
            .projection_for_namespace(&freehack.namespace_key())
            .is_some()
    );
}

#[test]
fn switch_during_invalid_active_play_state_requires_a_decision_and_rolls_back() {
    let installed = installed();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);
    let before = projection.clone();
    let mut invalid_state = fixture_state(PNP_STATE);
    invalid_state.entropy.algorithm = "unsupported_entropy".to_owned();
    let state_before = invalid_state.clone();

    let mut switch = request(
        Some(dungeonpunk.clone()),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &switch, Some(&invalid_state))
        .unwrap();
    assert!(
        preview
            .pending_conflicts
            .iter()
            .any(|conflict| conflict.contains("fails validation"))
    );
    assert!(
        preview
            .required_decisions
            .iter()
            .any(|decision| decision.kind == SwitchDecisionKind::ResolvePendingConflicts)
    );
    assert!(preview.active_state_sha256.is_some());

    let error = projection
        .apply_switch(&installed, &switch, Some(&invalid_state))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("required author decision is missing")
    );
    assert_eq!(projection, before, "a failed switch must roll back exactly");

    accept_all(&preview, &mut switch);
    projection
        .apply_switch(&installed, &switch, Some(&invalid_state))
        .unwrap();
    assert_eq!(
        projection.active.as_ref().unwrap().adapter,
        dungeonpunk.clone()
    );
    assert_eq!(
        invalid_state, state_before,
        "switch never mutates play state"
    );
}

#[test]
fn archived_projection_is_restored_exactly_and_never_evaluated_while_inactive() {
    let installed = installed();
    let pnp = resolved_adapter(&installed[0]).unwrap();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);
    let original = projection.active.clone().unwrap();

    let mut away = request(
        Some(dungeonpunk.clone()),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &away, Some(&fixture_state(PNP_STATE)))
        .unwrap();
    accept_all(&preview, &mut away);
    projection
        .apply_switch(&installed, &away, Some(&fixture_state(PNP_STATE)))
        .unwrap();
    assert!(
        projection
            .projection_for_namespace(&pnp.namespace_key())
            .is_none()
    );

    let mut back = request(Some(pnp.clone()), None);
    let dungeonpunk_state = fixture_state(DUNGEONPUNK_STATE);
    let preview = projection
        .preview_switch(&installed, &back, Some(&dungeonpunk_state))
        .unwrap();
    assert!(preview.restore_existing_archive);
    assert!(preview.restore.is_some());
    assert_eq!(
        preview.restore.as_ref().unwrap().definition_sha256,
        original.definition_sha256
    );
    accept_all(&preview, &mut back);
    let audit = projection
        .apply_switch(&installed, &back, Some(&dungeonpunk_state))
        .unwrap();
    let restored = projection.active.as_ref().unwrap();
    assert_eq!(restored.adapter, pnp.clone());
    assert_eq!(restored.definition, original.definition);
    assert_eq!(restored.definition_sha256, original.definition_sha256);
    assert!(restored.completed_creation_steps.is_empty());
    assert!(projection.archived_projection(&pnp).is_none());
    assert!(projection.archived_projection(&dungeonpunk).is_some());
    assert!(audit.restored_projection.is_some());
    assert_eq!(
        projection
            .projection_for_namespace(&pnp.namespace_key())
            .unwrap()
            .definition_sha256,
        original.definition_sha256
    );
    assert!(
        projection
            .projection_for_namespace(&dungeonpunk.namespace_key())
            .is_none()
    );
}

#[test]
fn deactivation_keeps_canon_and_can_reactivate_from_the_archive() {
    let installed = installed();
    let pnp = resolved_adapter(&installed[0]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);
    let canonical = projection.canonical_profile_sha256.clone();
    let original = projection.active.clone().unwrap();

    let mut deactivate = request(None, None);
    let preview = projection
        .preview_switch(&installed, &deactivate, Some(&fixture_state(PNP_STATE)))
        .unwrap();
    assert_eq!(preview.to, None);
    assert!(preview.archive_current);
    assert_eq!(
        preview.capabilities_lost,
        declared_capabilities(&installed[0])
    );
    accept_all(&preview, &mut deactivate);
    let audit = projection
        .apply_switch(&installed, &deactivate, Some(&fixture_state(PNP_STATE)))
        .unwrap();
    assert!(projection.active.is_none());
    assert!(projection.active_definition().is_none());
    assert_eq!(projection.canonical_profile_sha256, canonical);
    assert!(projection.archived_projection(&pnp).is_some());
    assert_eq!(audit.to, None);
    assert!(audit.archived_projection.is_some());
    assert!(
        projection
            .active_capabilities(&installed)
            .unwrap()
            .is_empty()
    );

    let mut reactivate = request(Some(pnp.clone()), None);
    let preview = projection
        .preview_switch(&installed, &reactivate, None)
        .unwrap();
    assert!(preview.restore_existing_archive);
    accept_all(&preview, &mut reactivate);
    projection
        .apply_switch(&installed, &reactivate, None)
        .unwrap();
    assert_eq!(
        projection.active.as_ref().unwrap().definition,
        original.definition
    );
    assert_eq!(projection.canonical_profile_sha256, canonical);

    let invalid = request(None, Some(fixture_definition(PNP_PROJECTION)));
    assert!(
        projection
            .preview_switch(&installed, &invalid, None)
            .is_err()
    );
}

#[test]
fn save_reload_and_json_ron_round_trips_are_exact() {
    let installed = installed();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let freehack = resolved_adapter(&installed[2]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);

    let mut switch = request(
        Some(dungeonpunk.clone()),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let pnp_state = fixture_state(PNP_STATE);
    let preview = projection
        .preview_switch(&installed, &switch, Some(&pnp_state))
        .unwrap();
    assert_eq!(
        AdapterSwitchPreview::from_json(&preview.to_json().unwrap()).unwrap(),
        preview
    );
    assert_eq!(
        AdapterSwitchPreview::from_ron(&preview.to_ron().unwrap()).unwrap(),
        preview
    );
    accept_all(&preview, &mut switch);
    assert_eq!(
        AdapterSwitchRequest::from_json(&switch.to_json().unwrap()).unwrap(),
        switch
    );
    assert_eq!(
        AdapterSwitchRequest::from_ron(&switch.to_ron().unwrap()).unwrap(),
        switch
    );
    let audit = projection
        .apply_switch(&installed, &switch, Some(&pnp_state))
        .unwrap();
    assert_eq!(
        weave_tabletop::AdapterSwitchAudit::from_json(&audit.to_json().unwrap()).unwrap(),
        audit
    );
    assert_eq!(
        weave_tabletop::AdapterSwitchAudit::from_ron(&audit.to_ron().unwrap()).unwrap(),
        audit
    );

    let saved_json = projection.to_json().unwrap();
    let saved_ron = projection.to_ron().unwrap();
    let from_json = TabletopCharacterProjection::from_json(&saved_json).unwrap();
    let from_ron = TabletopCharacterProjection::from_ron(&saved_ron).unwrap();
    assert_eq!(from_json, projection);
    assert_eq!(from_ron, projection);
    validate_character_projection(&from_json, &installed).unwrap();

    let mut reloaded = from_json;
    let mut next = request(
        Some(freehack.clone()),
        Some(fixture_definition(FREEHACK_PROJECTION)),
    );
    let dungeonpunk_state = fixture_state(DUNGEONPUNK_STATE);
    let preview = reloaded
        .preview_switch(&installed, &next, Some(&dungeonpunk_state))
        .unwrap();
    accept_all(&preview, &mut next);
    reloaded
        .apply_switch(&installed, &next, Some(&dungeonpunk_state))
        .unwrap();
    assert_eq!(reloaded.active.as_ref().unwrap().adapter, freehack.clone());
    assert_eq!(reloaded.inactive.len(), 2);
}

#[test]
fn same_adapter_upgrade_uses_reviewed_migration_dry_run_and_rolls_back() {
    let base = plug_and_play_manifest();
    let upgraded_coordinate = {
        let mut upgraded = base.clone();
        upgraded.version = "2.0.0".to_owned();
        upgraded.schema_version = 2;
        upgraded
            .migrations
            .push(weave_tabletop::AdapterMigrationDeclaration {
                id: "plug_and_play_schema_two".to_owned(),
                from_adapter_id: base.id.clone(),
                from_version: ">=1.0.0, <2.0.0".to_owned(),
                from_schema_version: 1,
                to_schema_version: 2,
                reviewed: true,
                preserves_canonical_character: true,
                description: "Synthetic reviewed same-adapter schema upgrade.".to_owned(),
            });
        let coordinate = resolved_adapter(&upgraded).unwrap();
        (vec![upgraded], coordinate)
    };
    let (upgraded_installed, upgraded) = upgraded_coordinate;
    let mut projection = fixture_projection(PNP_PROJECTION);
    let source_definition_sha256 = projection
        .active
        .as_ref()
        .unwrap()
        .definition_sha256
        .clone();
    let before = projection.clone();

    let mut migrated = projection.active.clone().unwrap();
    migrated.adapter = upgraded.clone();
    let mut switch = request(Some(upgraded.clone()), Some(migrated));
    let preview = projection
        .preview_switch(&upgraded_installed, &switch, None)
        .unwrap();
    assert_eq!(
        preview.reviewed_migration_id.as_deref(),
        Some("plug_and_play_schema_two")
    );
    assert!(!preview.archive_current, "an upgrade migrates in place");
    assert!(preview.archive.is_none());
    assert!(
        preview
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Migration dry run"))
    );
    assert!(
        preview
            .required_decisions
            .iter()
            .any(|decision| decision.kind == SwitchDecisionKind::ApplyReviewedMigration)
    );
    accept_all(&preview, &mut switch);
    let audit = projection
        .apply_switch(&upgraded_installed, &switch, None)
        .unwrap();
    assert_eq!(
        projection.active.as_ref().unwrap().adapter,
        upgraded.clone()
    );
    assert!(projection.inactive.is_empty());
    assert_eq!(
        audit.migrated_from_definition_sha256,
        Some(source_definition_sha256)
    );
    assert_eq!(
        audit.migration_id.as_deref(),
        Some("plug_and_play_schema_two")
    );
    assert!(audit.discarded_current);

    let mut broken = request(
        Some(upgraded.clone()),
        Some(AdapterCharacterDefinition {
            completed_creation_steps: vec!["undeclared_step".to_owned()],
            ..projection.active.clone().unwrap()
        }),
    );
    let mut reloaded = before.clone();
    let preview = reloaded
        .preview_switch(&upgraded_installed, &broken, None)
        .unwrap();
    assert!(
        preview
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == SwitchDiagnosticSeverity::Error)
    );
    accept_all(&preview, &mut broken);
    assert!(
        reloaded
            .apply_switch(&upgraded_installed, &broken, None)
            .is_err()
    );
    assert_eq!(
        reloaded, before,
        "a failed migration must roll back exactly"
    );
}

#[test]
fn missing_same_adapter_migration_blocks_without_mutation() {
    let base = plug_and_play_manifest();
    let mut upgraded = base.clone();
    upgraded.version = "2.0.0".to_owned();
    upgraded.schema_version = 2;
    let installed = vec![upgraded];
    let upgraded_coordinate = resolved_adapter(&installed[0]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);
    let before = projection.clone();
    let mut migrated = projection.active.clone().unwrap();
    migrated.adapter = upgraded_coordinate.clone();
    let mut switch = request(Some(upgraded_coordinate), Some(migrated));
    let preview = projection
        .preview_switch(&installed, &switch, None)
        .unwrap();
    assert!(preview.reviewed_migration_id.is_none());
    assert!(
        preview
            .pending_conflicts
            .iter()
            .any(|conflict| { conflict.contains("No reviewed migration") })
    );
    accept_all(&preview, &mut switch);
    assert!(projection.apply_switch(&installed, &switch, None).is_err());
    assert_eq!(projection, before);
}

#[test]
fn a_cross_system_migration_declaration_is_never_used() {
    let pnp = plug_and_play_manifest();
    let mut dungeonpunk = dungeonpunk_manifest();
    dungeonpunk
        .migrations
        .push(weave_tabletop::AdapterMigrationDeclaration {
            id: "never_cross_system".to_owned(),
            from_adapter_id: pnp.id.clone(),
            from_version: ">=1.0.0, <2.0.0".to_owned(),
            from_schema_version: 1,
            to_schema_version: 1,
            reviewed: true,
            preserves_canonical_character: true,
            description: "Synthetic declaration that must not authorize conversion.".to_owned(),
        });
    let installed = vec![pnp, dungeonpunk.clone()];
    let target = resolved_adapter(&dungeonpunk).unwrap();
    let projection = fixture_projection(PNP_PROJECTION);
    let switch = request(
        Some(target),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &switch, None)
        .unwrap();
    assert!(preview.reviewed_migration_id.is_none());
    assert!(!preview.automatic_conversion);
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.contains("cross-system conversion is forbidden"))
    );
}

#[test]
fn explicit_discard_is_deterministic_and_unknown_namespaces_fail_closed() {
    let installed = installed();
    let pnp = resolved_adapter(&installed[0]).unwrap();
    let dungeonpunk = resolved_adapter(&installed[1]).unwrap();
    let freehack = resolved_adapter(&installed[2]).unwrap();
    let mut projection = fixture_projection(PNP_PROJECTION);

    let mut away = request(
        Some(dungeonpunk.clone()),
        Some(fixture_definition(DUNGEONPUNK_PROJECTION)),
    );
    let preview = projection
        .preview_switch(&installed, &away, Some(&fixture_state(PNP_STATE)))
        .unwrap();
    accept_all(&preview, &mut away);
    projection
        .apply_switch(&installed, &away, Some(&fixture_state(PNP_STATE)))
        .unwrap();

    let mut discard = request(
        Some(freehack.clone()),
        Some(fixture_definition(FREEHACK_PROJECTION)),
    );
    discard.discard_inactive = vec![pnp.namespace_key()];
    let preview = projection
        .preview_switch(
            &installed,
            &discard,
            Some(&fixture_state(DUNGEONPUNK_STATE)),
        )
        .unwrap();
    assert_eq!(preview.discard_inactive, vec![pnp.namespace_key()]);
    assert!(
        preview
            .retained_inactive
            .iter()
            .all(|namespace| namespace != &pnp.namespace_key())
    );
    accept_all(&preview, &mut discard);
    let audit = projection
        .apply_switch(
            &installed,
            &discard,
            Some(&fixture_state(DUNGEONPUNK_STATE)),
        )
        .unwrap();
    assert!(projection.archived_projection(&pnp).is_none());
    assert_eq!(audit.discarded_inactive, vec![pnp.namespace_key()]);

    let mut unknown = request(
        Some(freehack.clone()),
        Some(fixture_definition(FREEHACK_PROJECTION)),
    );
    unknown.discard_inactive = vec!["org_weave_missing_v1_0_0".to_owned()];
    assert!(
        projection
            .preview_switch(&installed, &unknown, None)
            .is_err()
    );
}
