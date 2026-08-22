use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

const PROFILE: &str = "examples/domain-modules/weave-character/profile.character.json";
const MANIFEST_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/module.weave-module.json");
const MANIFEST_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/module.weave-module.ron");
const PACK_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari_vale.weave-domain.json");
const PACK_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari_vale.weave-domain.ron");

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weave-character"))
        .current_dir(repository_root())
        .args(arguments)
        .output()
        .expect("run weave-character")
}

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
}

#[test]
fn writes_exact_module_manifest_and_profile_pack_in_both_formats() {
    let temporary = tempdir().expect("temporary output directory");
    for (format, checked_manifest, checked_pack) in [
        ("json", MANIFEST_JSON, PACK_JSON),
        ("ron", MANIFEST_RON, PACK_RON),
    ] {
        let manifest = temporary.path().join(format!("module.{format}"));
        let output = run(&[
            "module-manifest",
            "--format",
            format,
            "--output",
            manifest.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(manifest).unwrap(), checked_manifest);

        let pack = temporary.path().join(format!("pack.{format}"));
        let output = run(&[
            "domain-pack",
            PROFILE,
            "--id",
            "ari_vale",
            "--version",
            "1.0.0",
            "--title",
            "Ari Vale Synthetic Character",
            "--format",
            format,
            "--output",
            pack.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(pack).unwrap(), checked_pack);
    }
}

#[test]
fn invalid_profile_fails_without_a_partial_pack_or_value_disclosure() {
    let temporary = tempdir().expect("temporary output directory");
    let output_path = temporary.path().join("rejected.json");
    let output = run(&[
        "domain-pack",
        "examples/domain-modules/weave-character/invalid/derived-canonical-evidence.character.json",
        "--id",
        "rejected_profile",
        "--version",
        "1.0.0",
        "--title",
        "Rejected Profile",
        "--output",
        output_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(!output_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("C109") || stderr.contains("ForbiddenWriteBack"));
    assert!(!stderr.contains("Ari Vale"));
}
