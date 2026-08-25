use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root()
        .join("examples/tabletop-adapters/contract")
        .join(name)
}

fn plug_and_play_fixture(name: &str) -> PathBuf {
    root()
        .join("examples/tabletop-adapters/plug-and-play")
        .join(name)
}

fn dungeonpunk_fixture(name: &str) -> PathBuf {
    root()
        .join("examples/tabletop-adapters/dungeonpunk")
        .join(name)
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weave-tabletop"))
        .args(arguments)
        .current_dir(root())
        .output()
        .expect("run weave-tabletop")
}

fn path(value: &Path) -> &str {
    value.to_str().expect("UTF-8 fixture path")
}

#[test]
fn validates_all_canonical_artifacts_and_exact_source_policy() {
    let manifest = fixture("synthetic.tabletop-adapter.json");
    let state = fixture("state.tabletop-state.json");
    let selection = fixture("selection.tabletop-selection.json");
    let projection = fixture("character.tabletop-projection.ron");
    let request = fixture("request.tabletop-request.json");
    let receipt = fixture("receipt.tabletop-receipt.ron");
    let cases = [
        vec!["validate", "manifest", path(&manifest)],
        vec![
            "validate",
            "selection",
            path(&selection),
            "--manifest",
            path(&manifest),
        ],
        vec![
            "validate",
            "projection",
            path(&projection),
            "--manifest",
            path(&manifest),
        ],
        vec![
            "validate",
            "state",
            path(&state),
            "--manifest",
            path(&manifest),
        ],
        vec![
            "validate",
            "request",
            path(&request),
            "--manifest",
            path(&manifest),
            "--state",
            path(&state),
        ],
        vec![
            "validate",
            "receipt",
            path(&receipt),
            "--manifest",
            path(&manifest),
            "--request",
            path(&request),
        ],
    ];
    for arguments in cases {
        let output = run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = run(&[
        "license-gate",
        path(&manifest),
        "--source-artifact",
        path(&fixture("source/synthetic-rules.txt")),
        "--license-text",
        path(&fixture("LICENSE")),
    ]);
    assert!(output.status.success());
    let fingerprint = run(&["fingerprint", path(&manifest)]);
    assert!(fingerprint.status.success());
    assert_eq!(
        String::from_utf8(fingerprint.stdout).unwrap().trim().len(),
        64
    );
}

#[test]
fn schemas_and_runtime_projection_match_checked_bytes() {
    let directory = tempdir().unwrap();
    let schema = directory.path().join("manifest.schema.json");
    let output = run(&["schema", "manifest", "--output", path(&schema)]);
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(schema).unwrap(),
        fs::read_to_string(root().join("schemas/weave-tabletop-adapter-manifest-v1.schema.json"))
            .unwrap()
    );

    let projected = directory.path().join("runtime.json");
    let output = run(&[
        "project-events",
        path(&fixture("receipt.tabletop-receipt.json")),
        "--manifest",
        path(&fixture("synthetic.tabletop-adapter.json")),
        "--audience",
        "runtime",
        "--output",
        path(&projected),
    ]);
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(projected).unwrap(),
        fs::read_to_string(fixture("runtime.tabletop-receipt.json")).unwrap()
    );
}

#[test]
fn invalid_inputs_report_stable_codes_without_partial_output() {
    let manifest = fixture("synthetic.tabletop-adapter.json");
    let output = run(&[
        "validate",
        "selection",
        path(&fixture(
            "invalid/conflicting-primary.tabletop-selection.json",
        )),
        "--manifest",
        path(&manifest),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("TT102"));

    let output = run(&[
        "validate",
        "request",
        path(&fixture(
            "invalid/undeclared-capability.tabletop-request.json",
        )),
        "--manifest",
        path(&manifest),
        "--state",
        path(&fixture("state.tabletop-state.json")),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("TT106"));

    let directory = tempdir().unwrap();
    let destination = directory.path().join("forbidden.json");
    let output = run(&[
        "project-events",
        path(&fixture(
            "invalid/undeclared-capability.tabletop-request.json",
        )),
        "--manifest",
        path(&manifest),
        "--audience",
        "runtime",
        "--output",
        path(&destination),
    ]);
    assert!(!output.status.success());
    assert!(!destination.exists());
}

#[test]
fn plug_and_play_create_validate_resolve_and_schemas_match_goldens() {
    let directory = tempdir().unwrap();
    let preview = directory.path().join("preview.ron");
    let output = run(&[
        "plug-and-play-create",
        path(&plug_and_play_fixture("creation.tabletop-creation.json")),
        "--output",
        path(&preview),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&preview).unwrap(),
        fs::read_to_string(plug_and_play_fixture(
            "preview.tabletop-creation-preview.ron"
        ))
        .unwrap()
    );

    for (kind, artifact) in [
        (
            "plug-and-play-creation-request",
            "creation.tabletop-creation.json",
        ),
        (
            "plug-and-play-creation-preview",
            "preview.tabletop-creation-preview.ron",
        ),
    ] {
        let output = run(&["validate", kind, path(&plug_and_play_fixture(artifact))]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let receipt = directory.path().join("receipt.json");
    let output = run(&[
        "plug-and-play-resolve",
        path(&plug_and_play_fixture("request.tabletop-request.json")),
        "--state",
        path(&plug_and_play_fixture("state.tabletop-state.ron")),
        "--output",
        path(&receipt),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&receipt).unwrap(),
        fs::read_to_string(plug_and_play_fixture("receipt.tabletop-receipt.json")).unwrap()
    );

    for (kind, checked) in [
        (
            "plug-and-play-creation-request",
            "weave-tabletop-plug-and-play-creation-request-v1.schema.json",
        ),
        (
            "plug-and-play-creation-preview",
            "weave-tabletop-plug-and-play-creation-preview-v1.schema.json",
        ),
    ] {
        let generated = directory.path().join(checked);
        let output = run(&["schema", kind, "--output", path(&generated)]);
        assert!(output.status.success());
        assert_eq!(
            fs::read_to_string(generated).unwrap(),
            fs::read_to_string(root().join("schemas").join(checked)).unwrap()
        );
    }
}

#[test]
fn dungeonpunk_create_validate_resolve_and_schemas_match_goldens() {
    let directory = tempdir().unwrap();
    let preview = directory.path().join("preview.ron");
    let output = run(&[
        "dungeonpunk-create",
        path(&dungeonpunk_fixture("creation.tabletop-creation.json")),
        "--output",
        path(&preview),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&preview).unwrap(),
        fs::read_to_string(dungeonpunk_fixture("preview.tabletop-creation-preview.ron")).unwrap()
    );

    for (kind, artifact) in [
        (
            "dungeonpunk-creation-request",
            "creation.tabletop-creation.json",
        ),
        (
            "dungeonpunk-creation-preview",
            "preview.tabletop-creation-preview.ron",
        ),
    ] {
        let output = run(&["validate", kind, path(&dungeonpunk_fixture(artifact))]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let receipt = directory.path().join("receipt.json");
    let output = run(&[
        "dungeonpunk-resolve",
        path(&dungeonpunk_fixture("request.tabletop-request.json")),
        "--state",
        path(&dungeonpunk_fixture("state.tabletop-state.ron")),
        "--output",
        path(&receipt),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&receipt).unwrap(),
        fs::read_to_string(dungeonpunk_fixture("receipt.tabletop-receipt.json")).unwrap()
    );

    for (kind, checked) in [
        (
            "dungeonpunk-creation-request",
            "weave-tabletop-dungeonpunk-creation-request-v1.schema.json",
        ),
        (
            "dungeonpunk-creation-preview",
            "weave-tabletop-dungeonpunk-creation-preview-v1.schema.json",
        ),
    ] {
        let generated = directory.path().join(checked);
        let output = run(&["schema", kind, "--output", path(&generated)]);
        assert!(output.status.success());
        assert_eq!(
            fs::read_to_string(generated).unwrap(),
            fs::read_to_string(root().join("schemas").join(checked)).unwrap()
        );
    }
}
