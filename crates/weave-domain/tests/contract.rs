use semver::Version;
use weave_domain::{
    DOMAIN_CONTRACT_VERSION, DomainError, DomainPack, DomainValue, ModuleDependency,
    ModuleManifest, ProvenanceKind, domain_pack_schema, module_manifest_schema,
    resolve_module_order, validate_manifest, validate_pack,
};

const MANIFEST_JSON: &str =
    include_str!("../../../examples/domain-modules/contract/module.weave-module.json");
const MANIFEST_RON: &str =
    include_str!("../../../examples/domain-modules/contract/module.weave-module.ron");
const PACK_JSON: &str =
    include_str!("../../../examples/domain-modules/contract/pack.weave-domain.json");
const PACK_RON: &str =
    include_str!("../../../examples/domain-modules/contract/pack.weave-domain.ron");

fn current_weave() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace package version")
}

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST_JSON).expect("canonical manifest JSON")
}

fn pack() -> DomainPack {
    DomainPack::from_json(PACK_JSON).expect("canonical pack JSON")
}

#[test]
fn canonical_json_and_ron_are_semantically_equal_and_stable() {
    let manifest_json = manifest();
    let manifest_ron = ModuleManifest::from_ron(MANIFEST_RON).expect("canonical manifest RON");
    let pack_json = pack();
    let pack_ron = DomainPack::from_ron(PACK_RON).expect("canonical pack RON");

    assert_eq!(manifest_json, manifest_ron);
    assert_eq!(pack_json, pack_ron);
    assert_eq!(
        manifest_json.to_json().expect("manifest JSON"),
        MANIFEST_JSON
    );
    assert_eq!(manifest_json.to_ron().expect("manifest RON"), MANIFEST_RON);
    assert_eq!(pack_json.to_json().expect("pack JSON"), PACK_JSON);
    assert_eq!(pack_json.to_ron().expect("pack RON"), PACK_RON);

    validate_manifest(&manifest_json, &current_weave()).expect("valid manifest");
    validate_pack(&pack_json, &manifest_json, &current_weave()).expect("valid pack");
}

#[test]
fn checked_schemas_are_generated_from_the_contract_types() {
    assert_eq!(
        module_manifest_schema().expect("manifest schema"),
        include_str!("../../../schemas/domain-module-manifest-v1.schema.json")
    );
    assert_eq!(
        domain_pack_schema().expect("pack schema"),
        include_str!("../../../schemas/domain-pack-v1.schema.json")
    );
}

#[test]
fn version_negotiation_fails_closed() {
    let mut future_contract = manifest();
    future_contract.contract_version = DOMAIN_CONTRACT_VERSION + 1;
    assert!(matches!(
        validate_manifest(&future_contract, &current_weave()),
        Err(DomainError::UnsupportedContract { .. })
    ));

    let mut incompatible_weave = manifest();
    incompatible_weave.weave_version = ">=99.0.0".into();
    assert!(matches!(
        validate_manifest(&incompatible_weave, &current_weave()),
        Err(DomainError::IncompatibleWeave { .. })
    ));

    let mut incompatible_pack = pack();
    incompatible_pack.module.version = "=2.0.0".into();
    assert!(matches!(
        validate_pack(&incompatible_pack, &manifest(), &current_weave()),
        Err(DomainError::IncompatibleModule { .. })
    ));
}

#[test]
fn resolution_is_order_independent_and_rejects_namespace_collisions() {
    let foundation = manifest();
    let mut dependent = foundation.clone();
    dependent.id = "org.weave.synthetic_orbit".into();
    dependent.namespace = "orbit".into();
    dependent.title = "Synthetic Orbit".into();
    dependent.dependencies = vec![ModuleDependency {
        id: foundation.id.clone(),
        version: "=1.0.0".into(),
    }];

    let forward = resolve_module_order(&[foundation.clone(), dependent.clone()], &current_weave())
        .expect("forward order");
    let reverse = resolve_module_order(&[dependent.clone(), foundation.clone()], &current_weave())
        .expect("reverse discovery order");
    assert_eq!(forward, reverse);
    assert_eq!(
        forward,
        vec![
            "org.weave.synthetic_constellation",
            "org.weave.synthetic_orbit"
        ]
    );

    dependent.namespace = foundation.namespace.clone();
    assert!(matches!(
        resolve_module_order(&[dependent, foundation], &current_weave()),
        Err(DomainError::NamespaceCollision { .. })
    ));
}

#[test]
fn packs_reject_unknown_or_mistyped_values_without_echoing_them() {
    let manifest = manifest();
    let mut unknown = pack();
    unknown
        .values
        .insert("undeclared".into(), DomainValue::Bool(true));
    assert!(matches!(
        validate_pack(&unknown, &manifest, &current_weave()),
        Err(DomainError::UnknownExport { .. })
    ));

    let mut mistyped = pack();
    mistyped
        .values
        .insert("phase".into(), DomainValue::String("not-a-symbol".into()));
    let error = validate_pack(&mistyped, &manifest, &current_weave())
        .expect_err("mistyped value must fail");
    assert!(matches!(error, DomainError::TypeMismatch { .. }));
    assert!(!error.to_string().contains("not-a-symbol"));
}

#[test]
fn parsers_reject_unknown_and_duplicate_json_keys() {
    let unknown = MANIFEST_JSON.replacen(
        "\"contract_version\": 1,",
        "\"contract_version\": 1,\n  \"unexpected\": true,",
        1,
    );
    assert!(ModuleManifest::from_json(&unknown).is_err());

    let duplicate = MANIFEST_JSON.replacen(
        "\"contract_version\": 1,",
        "\"contract_version\": 1,\n  \"contract_version\": 1,",
        1,
    );
    assert!(matches!(
        ModuleManifest::from_json(&duplicate),
        Err(DomainError::InvalidJson { .. })
    ));
}

#[test]
fn dependency_cycles_and_unsupported_capabilities_fail_closed() {
    let mut first = manifest();
    let mut second = first.clone();
    second.id = "org.weave.synthetic_orbit".into();
    second.namespace = "orbit".into();
    second.title = "Synthetic Orbit".into();
    first.dependencies = vec![ModuleDependency {
        id: second.id.clone(),
        version: "=1.0.0".into(),
    }];
    second.dependencies = vec![ModuleDependency {
        id: first.id.clone(),
        version: "=1.0.0".into(),
    }];
    assert!(matches!(
        resolve_module_order(&[first, second], &current_weave()),
        Err(DomainError::DependencyCycle)
    ));

    let mut unsupported = manifest();
    unsupported.capabilities[0].version += 1;
    assert!(matches!(
        validate_manifest(&unsupported, &current_weave()),
        Err(DomainError::UnsupportedCapability { .. })
    ));
}

#[test]
fn provenance_and_sensitive_values_are_validated_without_value_disclosure() {
    let mut missing_hash = manifest();
    missing_hash.provenance.sources[0].kind = ProvenanceKind::PublicSource;
    let error = validate_manifest(&missing_hash, &current_weave())
        .expect_err("public source without a hash must fail");
    assert!(matches!(error, DomainError::InvalidField { .. }));

    let mut invalid_license = manifest();
    invalid_license.license = "MIT OR".into();
    assert!(matches!(
        validate_manifest(&invalid_license, &current_weave()),
        Err(DomainError::InvalidField { .. })
    ));

    let mut sensitive = pack();
    let synthetic = ["ghp_", &"A".repeat(32)].concat();
    let DomainValue::Object(observation) = sensitive
        .values
        .get_mut("observation")
        .expect("observation export")
    else {
        panic!("observation must be an object");
    };
    observation.insert("label".into(), DomainValue::String(synthetic.clone()));
    let error = validate_pack(&sensitive, &manifest(), &current_weave())
        .expect_err("credential-like data must fail");
    assert!(!error.to_string().contains(&synthetic));
}
