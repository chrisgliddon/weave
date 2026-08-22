use std::fs;
use std::process::Command;

use tempfile::tempdir;

const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/contract/module.weave-module.json");
const PACK: &str = include_str!("../../../examples/domain-modules/contract/pack.weave-domain.json");

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_weave-module")
}

#[test]
fn publishes_lists_indexes_and_validates_a_project() {
    let directory = tempdir().expect("temporary directory");
    let manifest = directory.path().join("module.json");
    let pack = directory.path().join("pack.json");
    let registry = directory.path().join("registry");
    fs::write(&manifest, MANIFEST).expect("write manifest");
    fs::write(&pack, PACK).expect("write pack");

    let publish = Command::new(binary())
        .arg("publish")
        .arg(&manifest)
        .arg("--pack")
        .arg(&pack)
        .arg("--output")
        .arg(&registry)
        .output()
        .expect("publish module");
    assert!(
        publish.status.success(),
        "{}",
        String::from_utf8_lossy(&publish.stderr)
    );
    let message = String::from_utf8(publish.stdout).expect("UTF-8 publication message");
    assert!(message.contains("published domain module org.weave.synthetic_constellation@1.0.0"));
    assert!(!message.contains(directory.path().to_string_lossy().as_ref()));

    let list = Command::new(binary())
        .arg("list")
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("list registry");
    assert!(list.status.success());
    let listed: serde_json::Value =
        serde_json::from_slice(&list.stdout).expect("registry list JSON");
    assert_eq!(listed["schema_version"], 1);
    assert_eq!(
        listed["modules"][0]["id"],
        "org.weave.synthetic_constellation"
    );

    let index = directory.path().join("registry-index.json");
    let indexed = Command::new(binary())
        .arg("index")
        .arg("--registry")
        .arg(&registry)
        .arg("--output")
        .arg(&index)
        .output()
        .expect("write index");
    assert!(indexed.status.success());
    assert_eq!(fs::read(&index).expect("index bytes"), list.stdout);

    let project = directory.path().join("weave.modules.json");
    fs::write(
        &project,
        concat!(
            "{\n",
            "  \"schema_version\": 1,\n",
            "  \"registries\": [\"registry\"],\n",
            "  \"manifests\": [],\n",
            "  \"packs\": []\n",
            "}\n"
        ),
    )
    .expect("write project");
    let validated = Command::new(binary())
        .arg("validate-project")
        .arg(&project)
        .output()
        .expect("validate project");
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    assert!(
        String::from_utf8_lossy(&validated.stdout)
            .contains("valid domain project (1 module release(s), 1 pack release(s))")
    );
}

#[test]
fn install_refuses_to_replace_an_immutable_coordinate() {
    let directory = tempdir().expect("temporary directory");
    let manifest = directory.path().join("module.json");
    let pack = directory.path().join("pack.json");
    let registry = directory.path().join("registry");
    fs::write(&manifest, MANIFEST).expect("write manifest");
    fs::write(&pack, PACK).expect("write pack");

    let first = Command::new(binary())
        .arg("install")
        .arg(&manifest)
        .arg("--pack")
        .arg(&pack)
        .arg("--registry")
        .arg(&registry)
        .status()
        .expect("install release");
    assert!(first.success());

    fs::write(
        &manifest,
        MANIFEST.replace("Synthetic Constellation", "Changed Constellation"),
    )
    .expect("write conflicting manifest");
    let conflict = Command::new(binary())
        .arg("install")
        .arg(&manifest)
        .arg("--pack")
        .arg(&pack)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("reject conflicting release");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("refusing to overwrite"));
}
