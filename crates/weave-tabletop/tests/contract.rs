use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_core::ir::DomainValueIr;
use weave_domain::{DomainCatalog, DomainPack, DomainValue, ModuleManifest};
use weave_tabletop::{
    AdapterManifest, AdapterSelection, AdapterSourceClass, EntropyStream, HostAudience,
    ResolutionReceipt, ResolutionRequest, ResolverOutput, ResolverRegistry,
    TabletopCharacterProjection, TabletopError, TabletopResolver, TabletopState,
    equivalent_json_and_ron, preview_adapter_switch, project_receipt_for_audience,
    validate_adapter_manifest, validate_adapter_selection, validate_character_projection,
    validate_resolution_receipt, validate_resolution_receipt_for_request,
    validate_resolution_request, validate_tabletop_state, verify_adapter_source,
};

const MANIFEST_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json");
const MANIFEST_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/synthetic.tabletop-adapter.ron");
const SELECTION_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/selection.tabletop-selection.json");
const SELECTION_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/selection.tabletop-selection.ron");
const PROJECTION_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/character.tabletop-projection.json");
const PROJECTION_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/character.tabletop-projection.ron");
const STATE_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/state.tabletop-state.json");
const STATE_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/state.tabletop-state.ron");
const REQUEST_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/request.tabletop-request.json");
const REQUEST_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/request.tabletop-request.ron");
const RECEIPT_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/receipt.tabletop-receipt.json");
const RECEIPT_RON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/receipt.tabletop-receipt.ron");
const RUNTIME_JSON: &str =
    include_str!("../../../examples/tabletop-adapters/contract/runtime.tabletop-receipt.json");
const SOURCE: &[u8] =
    include_bytes!("../../../examples/tabletop-adapters/contract/source/synthetic-rules.txt");
const LICENSE: &[u8] = include_bytes!("../../../examples/tabletop-adapters/contract/LICENSE");

fn manifest() -> AdapterManifest {
    AdapterManifest::from_json(MANIFEST_JSON).unwrap()
}

struct InvalidStateResolver;

impl TabletopResolver for InvalidStateResolver {
    fn adapter_id(&self) -> &str {
        "org.weave.tabletop.lantern_trail"
    }

    fn adapter_version(&self) -> &str {
        "1.0.0"
    }

    fn resolve(
        &self,
        _operation: &str,
        _definition: &DomainValue,
        _request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        let mut fields = match state {
            DomainValue::Object(fields) => fields.clone(),
            _ => return Err(TabletopError::SchemaMismatch { path: "resolver" }),
        };
        fields.insert("momentum".to_owned(), DomainValue::Number(999.0));
        let _ = entropy.draw_bounded(6)?;
        Ok(ResolverOutput {
            state: DomainValue::Object(fields),
            events: Vec::new(),
        })
    }
}

#[test]
fn invalid_resolver_output_is_atomic_and_exact_request_lineage_is_checked() {
    let manifest = manifest();
    let state = TabletopState::from_json(STATE_JSON).unwrap();
    let request = ResolutionRequest::from_json(REQUEST_JSON).unwrap();
    let before = state.clone();
    let mut registry = ResolverRegistry::new();
    registry.register(&manifest, InvalidStateResolver).unwrap();
    assert!(matches!(
        registry.execute(&manifest, &request, &state),
        Err(TabletopError::SchemaMismatch {
            path: "resolver.state"
        })
    ));
    assert_eq!(state, before);

    let receipt = ResolutionReceipt::from_json(RECEIPT_JSON).unwrap();
    let mut changed_request = request;
    changed_request.request_id = "different_attempt".to_owned();
    assert!(matches!(
        validate_resolution_receipt_for_request(&receipt, &manifest, &changed_request),
        Err(TabletopError::ContentHashMismatch {
            path: "request_sha256"
        })
    ));
}

#[test]
fn canonical_json_and_ron_are_semantically_equal_and_valid() {
    assert!(equivalent_json_and_ron::<AdapterManifest>(MANIFEST_JSON, MANIFEST_RON).unwrap());
    assert!(equivalent_json_and_ron::<AdapterSelection>(SELECTION_JSON, SELECTION_RON).unwrap());
    assert!(
        equivalent_json_and_ron::<TabletopCharacterProjection>(PROJECTION_JSON, PROJECTION_RON)
            .unwrap()
    );
    assert!(equivalent_json_and_ron::<TabletopState>(STATE_JSON, STATE_RON).unwrap());
    assert!(equivalent_json_and_ron::<ResolutionRequest>(REQUEST_JSON, REQUEST_RON).unwrap());
    assert!(equivalent_json_and_ron::<ResolutionReceipt>(RECEIPT_JSON, RECEIPT_RON).unwrap());

    let manifest = manifest();
    let selection = AdapterSelection::from_json(SELECTION_JSON).unwrap();
    let projection = TabletopCharacterProjection::from_json(PROJECTION_JSON).unwrap();
    let state = TabletopState::from_json(STATE_JSON).unwrap();
    let request = ResolutionRequest::from_json(REQUEST_JSON).unwrap();
    let receipt = ResolutionReceipt::from_json(RECEIPT_JSON).unwrap();
    validate_adapter_manifest(&manifest).unwrap();
    validate_adapter_selection(&selection, std::slice::from_ref(&manifest)).unwrap();
    validate_character_projection(&projection, std::slice::from_ref(&manifest)).unwrap();
    validate_tabletop_state(&state, &manifest).unwrap();
    validate_resolution_request(&request, &state, &manifest).unwrap();
    validate_resolution_receipt(&receipt, &manifest).unwrap();
    validate_resolution_receipt_for_request(&receipt, &manifest, &request).unwrap();
    verify_adapter_source(&manifest, SOURCE, LICENSE).unwrap();

    let mut other = manifest.clone();
    other.id = "org.weave.tabletop.other_fixture".to_owned();
    other.namespace = "other_fixture".to_owned();
    let mut installed = vec![
        weave_tabletop::resolved_adapter(&manifest).unwrap(),
        weave_tabletop::resolved_adapter(&other).unwrap(),
    ];
    installed.sort_by(|left, right| left.id.cmp(&right.id));
    let no_primary = AdapterSelection {
        selection_format_version: weave_tabletop::ADAPTER_SELECTION_FORMAT_VERSION,
        installed,
        primary: Vec::new(),
    };
    validate_adapter_selection(&no_primary, &[manifest, other]).unwrap();
}

#[test]
fn visibility_projection_retains_audit_hashes_and_hides_payloads() {
    let manifest = manifest();
    let receipt = ResolutionReceipt::from_json(RECEIPT_JSON).unwrap();
    let runtime = project_receipt_for_audience(&receipt, &manifest, HostAudience::Runtime).unwrap();
    assert_eq!(runtime.to_json().unwrap(), RUNTIME_JSON);
    assert_eq!(runtime.events.len(), 3);
    assert!(runtime.events[0].payload.is_some());
    assert!(runtime.events[1].payload.is_none());
    assert!(runtime.events[2].payload.is_none());
    assert!(
        runtime
            .events
            .iter()
            .all(|event| event.payload_sha256.len() == 64)
    );

    let authoring =
        project_receipt_for_audience(&receipt, &manifest, HostAudience::Authoring).unwrap();
    assert!(authoring.events[0].payload.is_some());
    assert!(authoring.events[1].payload.is_none());
    assert!(authoring.events[2].payload.is_some());
    let host =
        project_receipt_for_audience(&receipt, &manifest, HostAudience::AuthorityHost).unwrap();
    assert!(host.events.iter().all(|event| event.payload.is_some()));
}

#[test]
fn invalid_versions_primary_conflicts_and_capabilities_fail_closed() {
    let invalid_manifest = AdapterManifest::from_json(include_str!(
        "../../../examples/tabletop-adapters/contract/invalid/unsupported-version.tabletop-adapter.json"
    ))
    .unwrap();
    assert_eq!(
        validate_adapter_manifest(&invalid_manifest),
        Err(TabletopError::UnsupportedVersion {
            path: "manifest_format_version"
        })
    );
    let conflict = AdapterSelection::from_json(include_str!(
        "../../../examples/tabletop-adapters/contract/invalid/conflicting-primary.tabletop-selection.json"
    ))
    .unwrap();
    assert_eq!(
        validate_adapter_selection(&conflict, &[manifest()]),
        Err(TabletopError::PrimaryConflict)
    );
    let request = ResolutionRequest::from_json(include_str!(
        "../../../examples/tabletop-adapters/contract/invalid/undeclared-capability.tabletop-request.json"
    ))
    .unwrap();
    assert_eq!(
        validate_resolution_request(
            &request,
            &TabletopState::from_json(STATE_JSON).unwrap(),
            &manifest()
        ),
        Err(TabletopError::UndeclaredCapability {
            capability: weave_tabletop::TabletopCapability::Advancement
        })
    );
}

#[test]
fn license_gate_is_explicit_for_original_cc0_and_apache_only() {
    let original = manifest();
    validate_adapter_manifest(&original).unwrap();

    let mut cc0 = original.clone();
    cc0.provenance.source_class = AdapterSourceClass::VerifiedCc0;
    cc0.provenance.license = "CC0-1.0".to_owned();
    validate_adapter_manifest(&cc0).unwrap();

    let mut apache = original.clone();
    apache.provenance.source_class = AdapterSourceClass::SeparatelyLicensedApache2;
    apache.provenance.license = "Apache-2.0".to_owned();
    apache.provenance.notices = vec!["Required Apache notice retained.".to_owned()];
    validate_adapter_manifest(&apache).unwrap();

    for license in [
        "GPL-3.0-only",
        "CC-BY-SA-4.0",
        "CC-BY-NC-4.0",
        "CC-BY-ND-4.0",
        "LicenseRef-Unknown",
        "CC0-1.0 OR GPL-3.0-only",
    ] {
        let mut rejected = original.clone();
        rejected.provenance.license = license.to_owned();
        assert_eq!(
            validate_adapter_manifest(&rejected),
            Err(TabletopError::LicenseRejected),
            "{license} unexpectedly passed"
        );
    }
}

#[test]
fn switching_never_converts_or_mutates_canonical_character_data() {
    let projection = TabletopCharacterProjection::from_json(PROJECTION_JSON).unwrap();
    let before = projection.canonical_profile_sha256.clone();
    let mut target = manifest();
    target.id = "org.weave.tabletop.other_fixture".to_owned();
    target.namespace = "other_fixture".to_owned();
    target.version = "2.0.0".to_owned();
    let preview = preview_adapter_switch(&projection, Some(&target)).unwrap();
    assert_eq!(preview.canonical_profile_sha256, before);
    assert!(preview.archive_current);
    assert!(!preview.automatic_conversion);
    assert!(preview.reviewed_migration_id.is_none());
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.contains("conversion is forbidden"))
    );

    let mut writeback = projection;
    writeback.canonical_character_write_back = true;
    assert_eq!(
        validate_character_projection(&writeback, &[manifest()]),
        Err(TabletopError::CanonicalWriteBack)
    );
}

#[test]
fn duplicate_json_keys_and_changed_hashes_are_rejected() {
    let duplicate = MANIFEST_JSON.replacen(
        "\"manifest_format_version\": 1,",
        "\"manifest_format_version\": 1,\n  \"manifest_format_version\": 1,",
        1,
    );
    assert_eq!(
        AdapterManifest::from_json(&duplicate),
        Err(TabletopError::Artifact)
    );
    let manifest = manifest();
    assert!(matches!(
        verify_adapter_source(&manifest, b"changed", LICENSE),
        Err(TabletopError::ContentHashMismatch {
            path: "provenance.sha256"
        })
    ));
}

#[test]
fn ordinary_source_and_compiler_are_capability_driven_without_adapter_branches() {
    let source =
        include_str!("../../../examples/tabletop-adapters/contract/runtime/lantern-trail.weave");
    let domain_manifest = ModuleManifest::from_json(include_str!(
        "../../../examples/tabletop-adapters/contract/runtime/module.weave-module.json"
    ))
    .unwrap();
    let domain_pack = DomainPack::from_json(include_str!(
        "../../../examples/tabletop-adapters/contract/runtime/lumen_reed.weave-domain.json"
    ))
    .unwrap();
    let catalog = DomainCatalog::from_artifacts([domain_manifest], [domain_pack]).unwrap();
    let options = CompileOptions {
        source_name: Some(
            "examples/tabletop-adapters/contract/runtime/lantern-trail.weave".to_owned(),
        ),
    };
    let first = compile_with_modules(source, &options, &catalog).unwrap();
    let second = compile_with_modules(source, &options, &catalog).unwrap();
    assert_eq!(
        to_json(&first.story).unwrap(),
        to_json(&second.story).unwrap()
    );
    assert_eq!(
        to_ron(&first.story).unwrap(),
        to_ron(&second.story).unwrap()
    );
    assert_eq!(
        to_json(&first.story).unwrap(),
        include_str!(
            "../../../examples/tabletop-adapters/contract/runtime/lantern-trail.story.json"
        )
    );
    assert_eq!(
        to_ron(&first.story).unwrap(),
        include_str!(
            "../../../examples/tabletop-adapters/contract/runtime/lantern-trail.story.ron"
        )
    );
    let rules = &first.story.modules["rules"];
    assert_eq!(
        rules.value(&["definition", "focus"]),
        Some(&DomainValueIr::Number(3.0))
    );
    assert_eq!(
        rules.value(&["state", "momentum"]),
        Some(&DomainValueIr::Number(2.0))
    );
    assert_eq!(
        rules.value(&["capabilities", "checks_and_conflicts"]),
        Some(&DomainValueIr::Bool(true))
    );
    let selection = AdapterSelection::from_json(SELECTION_JSON).unwrap();
    assert_eq!(
        rules.value(&["adapter", "content_sha256"]),
        Some(&DomainValueIr::String(
            selection.primary[0].content_sha256.clone()
        ))
    );

    let absent = source.replace(
        "rules.capabilities.checks_and_conflicts",
        "rules.capabilities.advancement",
    );
    let error = compile_with_modules(&absent, &options, &catalog).unwrap_err();
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "D143")
    );

    let forbidden = source.replace(
        "    pack: \"lumen_reed@=1.0.0\"",
        "    pack: \"lumen_reed@=1.0.0\"\n    override definition.focus: 4",
    );
    let error = compile_with_modules(&forbidden, &options, &catalog).unwrap_err();
    assert!(error.diagnostics.iter().any(|diagnostic| {
        diagnostic.code.0 == "D140" && diagnostic.message.contains("read-only path")
    }));

    let authored_state = source.replace(
        "    pack: \"lumen_reed@=1.0.0\"",
        "    pack: \"lumen_reed@=1.0.0\"\n    override state.momentum: 4",
    );
    let compiled = compile_with_modules(&authored_state, &options, &catalog).unwrap();
    assert_eq!(
        compiled.story.modules["rules"].value(&["state", "momentum"]),
        Some(&DomainValueIr::Number(4.0))
    );
}
