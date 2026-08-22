use std::fs;

use semver::Version;
use tempfile::tempdir;
use weave_domain::{
    DOMAIN_CONTRACT_VERSION, DomainCatalog, DomainPack, DomainPackageError, DomainProjectConfig,
    DomainRegistry, ModuleManifest, load_domain_project,
};

const MANIFEST_JSON: &str =
    include_str!("../../../examples/domain-modules/contract/module.weave-module.json");
const PACK_JSON: &str =
    include_str!("../../../examples/domain-modules/contract/pack.weave-domain.json");

fn current_weave() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace package version")
}

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST_JSON).expect("canonical manifest")
}

fn pack() -> DomainPack {
    DomainPack::from_json(PACK_JSON).expect("canonical pack")
}

fn write_release(
    directory: &std::path::Path,
    manifest: &ModuleManifest,
    pack: &DomainPack,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let manifest_path = directory.join(format!("module-{}.json", manifest.version));
    let pack_path = directory.join(format!("pack-{}.json", pack.version));
    fs::write(&manifest_path, manifest.to_json().expect("manifest JSON")).expect("write manifest");
    fs::write(&pack_path, pack.to_json().expect("pack JSON")).expect("write pack");
    (manifest_path, pack_path)
}

#[test]
fn registry_install_discovery_and_index_are_deterministic() {
    let directory = tempdir().expect("temporary directory");
    let source = directory.path().join("source");
    fs::create_dir(&source).expect("create source directory");
    let (manifest_path, pack_path) = write_release(&source, &manifest(), &pack());
    let registry = DomainRegistry::new(directory.path().join("registry"));

    let first = registry
        .install(
            &manifest_path,
            std::slice::from_ref(&pack_path),
            &current_weave(),
        )
        .expect("install release");
    let second = registry
        .install(
            &manifest_path,
            std::slice::from_ref(&pack_path),
            &current_weave(),
        )
        .expect("idempotent install");
    assert_eq!(first, second);

    let snapshot = registry
        .discover(&current_weave())
        .expect("discover registry");
    assert_eq!(snapshot.catalog.manifests().count(), 1);
    assert_eq!(snapshot.catalog.packs().count(), 1);
    let first_index = snapshot.index.to_json().expect("registry index");
    let second_index = registry
        .discover(&current_weave())
        .expect("repeat discovery")
        .index
        .to_json()
        .expect("repeat registry index");
    assert_eq!(first_index, second_index);
    assert!(first_index.contains(&first.manifest_sha256));

    fs::write(&first.manifest, b"{}").expect("tamper installed manifest");
    assert!(matches!(
        registry.discover(&current_weave()),
        Err(DomainPackageError::Checksum)
    ));
}

#[test]
fn project_discovery_selects_supported_upgrades_and_locks_exact_bytes() {
    let directory = tempdir().expect("temporary directory");
    let source = directory.path().join("source");
    fs::create_dir(&source).expect("create source directory");
    let registry_path = directory.path().join("registry");
    let registry = DomainRegistry::new(&registry_path);

    let first_manifest = manifest();
    let first_pack = pack();
    let (manifest_path, pack_path) = write_release(&source, &first_manifest, &first_pack);
    registry
        .install(&manifest_path, &[pack_path], &current_weave())
        .expect("install first release");

    let mut next_manifest = first_manifest.clone();
    next_manifest.version = "1.1.0".into();
    let mut next_pack = first_pack.clone();
    next_pack.version = "1.1.0".into();
    next_pack.module.version = "^1.1".into();
    let (manifest_path, pack_path) = write_release(&source, &next_manifest, &next_pack);
    registry
        .install(&manifest_path, &[pack_path], &current_weave())
        .expect("install compatible upgrade");

    let config_path = directory.path().join("weave.modules.json");
    fs::write(
        &config_path,
        DomainProjectConfig {
            schema_version: 1,
            registries: vec!["registry".into()],
            manifests: Vec::new(),
            packs: Vec::new(),
        }
        .to_json()
        .expect("project JSON"),
    )
    .expect("write project");
    let project = load_domain_project(&config_path, &current_weave()).expect("load project");
    let root = project
        .catalog()
        .resolve(
            "org.weave.synthetic_constellation",
            "^1",
            "glasswing_sky",
            "^1",
            &current_weave(),
        )
        .expect("resolve latest compatible release");
    assert_eq!(root.manifest.version, "1.1.0");
    assert_eq!(root.pack.version, "1.1.0");

    let graph = project
        .catalog()
        .resolve_graph([&root], &current_weave())
        .expect("resolve closure");
    let lock = project.lock_for(&graph).expect("build exact lock");
    project.write_lock(&lock).expect("write lock");
    project.verify_lock(&lock).expect("verify lock");
    assert_eq!(lock.modules[0].version, "1.1.0");
    assert_eq!(lock.packs[0].version, "1.1.0");

    let mut changed = lock.clone();
    changed.packs[0].sha256 = "0".repeat(64);
    assert!(matches!(
        project.verify_lock(&changed),
        Err(DomainPackageError::LockMismatch)
    ));
}

#[test]
fn unsupported_releases_and_unsafe_project_paths_fail_closed() {
    let directory = tempdir().expect("temporary directory");
    let (manifest_path, pack_path) = write_release(directory.path(), &manifest(), &pack());
    let registry = DomainRegistry::new(directory.path().join("registry"));

    let mut future_contract = manifest();
    future_contract.contract_version = DOMAIN_CONTRACT_VERSION + 1;
    fs::write(
        &manifest_path,
        future_contract.to_json().expect("future manifest JSON"),
    )
    .expect("write future manifest");
    assert!(matches!(
        registry.install(
            &manifest_path,
            std::slice::from_ref(&pack_path),
            &current_weave()
        ),
        Err(DomainPackageError::Contract(_))
    ));

    let mut incompatible_weave = manifest();
    incompatible_weave.weave_version = ">=99.0.0".into();
    fs::write(
        &manifest_path,
        incompatible_weave
            .to_json()
            .expect("incompatible manifest JSON"),
    )
    .expect("write incompatible manifest");
    assert!(matches!(
        registry.install(
            &manifest_path,
            std::slice::from_ref(&pack_path),
            &current_weave()
        ),
        Err(DomainPackageError::Contract(_))
    ));

    fs::write(&manifest_path, manifest().to_json().expect("manifest JSON"))
        .expect("restore manifest");
    let mut future_pack = pack();
    future_pack.pack_format_version += 1;
    fs::write(&pack_path, future_pack.to_json().expect("future pack JSON"))
        .expect("write future pack");
    assert!(matches!(
        registry.install(
            &manifest_path,
            std::slice::from_ref(&pack_path),
            &current_weave()
        ),
        Err(DomainPackageError::Contract(_))
    ));

    let unsafe_config = DomainProjectConfig {
        schema_version: 1,
        registries: vec!["../outside".into()],
        manifests: Vec::new(),
        packs: Vec::new(),
    };
    assert!(matches!(
        unsafe_config.to_json(),
        Err(DomainPackageError::InvalidProjectPath { .. })
    ));

    let credential_shaped = DomainProjectConfig {
        schema_version: 1,
        registries: Vec::new(),
        manifests: vec![format!("artifacts/ghp_{}", "A".repeat(32))],
        packs: Vec::new(),
    };
    let error = credential_shaped
        .to_json()
        .expect_err("credential-shaped path must fail");
    assert!(!error.to_string().contains(&"A".repeat(32)));
}

#[test]
fn dependency_closure_is_ordered_before_dependents() {
    let foundation_manifest = manifest();
    let foundation_pack = pack();
    let mut dependent_manifest = foundation_manifest.clone();
    dependent_manifest.id = "org.weave.synthetic_orbit".into();
    dependent_manifest.namespace = "orbit".into();
    dependent_manifest.title = "Synthetic Orbit".into();
    dependent_manifest.dependencies = vec![weave_domain::ModuleDependency {
        id: foundation_manifest.id.clone(),
        version: "^1".into(),
    }];
    let mut dependent_pack = foundation_pack.clone();
    dependent_pack.id = "orbit_map".into();
    dependent_pack.title = "Synthetic Orbit Map".into();
    dependent_pack.module.id = dependent_manifest.id.clone();
    dependent_pack.dependencies = vec![weave_domain::PackDependency {
        module_id: foundation_manifest.id.clone(),
        pack_id: foundation_pack.id.clone(),
        version: "^1".into(),
    }];

    let catalog = DomainCatalog::from_artifacts(
        [dependent_manifest, foundation_manifest],
        [dependent_pack, foundation_pack],
    )
    .expect("catalog");
    let root = catalog
        .resolve(
            "org.weave.synthetic_orbit",
            "^1",
            "orbit_map",
            "^1",
            &current_weave(),
        )
        .expect("resolve root");
    let graph = catalog
        .resolve_graph([&root], &current_weave())
        .expect("resolve dependency closure");
    assert_eq!(
        graph.module_order,
        [
            "org.weave.synthetic_constellation",
            "org.weave.synthetic_orbit"
        ]
    );
    assert_eq!(
        graph.pack_order,
        [
            (
                "org.weave.synthetic_constellation".to_owned(),
                "glasswing_sky".to_owned()
            ),
            (
                "org.weave.synthetic_orbit".to_owned(),
                "orbit_map".to_owned()
            )
        ]
    );
}
