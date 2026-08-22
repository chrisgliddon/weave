use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_weave-pattern")
}

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../patterns/community/ember-omens/package.weave-pattern.json")
}

#[test]
fn validates_publishes_installs_and_indexes_the_sample() {
    let directory = tempdir().expect("temporary directory");
    let publication = directory.path().join("publication");
    let registry = directory.path().join("registry");

    let validation = Command::new(binary())
        .args(["validate"])
        .arg(sample())
        .output()
        .expect("run validate");
    assert!(validation.status.success());
    assert!(String::from_utf8_lossy(&validation.stdout).contains("valid ember_omens@1.0.0"));

    let publish = Command::new(binary())
        .args(["publish"])
        .arg(sample())
        .args(["--output"])
        .arg(&publication)
        .output()
        .expect("run publish");
    assert!(publish.status.success());
    let artifact = publication.join("ember_omens-1.0.0.weave-pattern.json");
    assert!(artifact.is_file());
    assert!(
        publication
            .join("ember_omens-1.0.0.weave-pattern.json.sha256")
            .is_file()
    );

    let install = Command::new(binary())
        .args(["install"])
        .arg(&artifact)
        .args(["--registry"])
        .arg(&registry)
        .output()
        .expect("run install");
    assert!(install.status.success());

    let list = Command::new(binary())
        .args(["list", "--registry"])
        .arg(&registry)
        .output()
        .expect("run list");
    assert!(list.status.success());
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(listing.contains("ember_omens@1.0.0"));
    assert!(listing.contains("MIT"));

    let index = Command::new(binary())
        .args(["index", "--registry"])
        .arg(&registry)
        .output()
        .expect("run index");
    assert!(index.status.success());
    let index: serde_json::Value = serde_json::from_slice(&index.stdout).expect("index is JSON");
    assert_eq!(index["schema_version"], 1);
    assert_eq!(index["packages"][0]["id"], "ember_omens");
}

#[test]
fn invalid_package_fails_without_publishing() {
    let directory = tempdir().expect("temporary directory");
    let invalid = directory.path().join("invalid.json");
    let publication = directory.path().join("publication");
    fs::write(&invalid, "{\"schema_version\":99}\n").expect("write invalid package");

    let output = Command::new(binary())
        .args(["publish"])
        .arg(&invalid)
        .args(["--output"])
        .arg(&publication)
        .output()
        .expect("run publish");
    assert!(!output.status.success());
    assert!(!publication.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid community pattern"));
}

#[test]
fn emits_the_versioned_package_schema() {
    let output = Command::new(binary())
        .arg("schema")
        .output()
        .expect("run schema");
    assert!(output.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).expect("schema is JSON");
    assert_eq!(
        schema["$id"],
        "urn:weave:schema:community-pattern-package:1"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
}
