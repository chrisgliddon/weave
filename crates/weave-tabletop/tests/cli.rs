use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;
use weave_tabletop::FreehackAuthorityReceipt;

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

fn freehack_fixture(name: &str) -> PathBuf {
    root()
        .join("examples/tabletop-adapters/freehack")
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

#[test]
fn freehack_create_probability_private_resolution_and_schemas_match_goldens() {
    let directory = tempdir().unwrap();
    let preview = directory.path().join("preview.ron");
    let output = run(&[
        "freehack-create",
        path(&freehack_fixture("creation.tabletop-creation.json")),
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
        fs::read_to_string(freehack_fixture("preview.tabletop-creation-preview.ron")).unwrap()
    );

    let probability = directory.path().join("probability.json");
    let output = run(&[
        "freehack-probability",
        path(&freehack_fixture("probability.freehack-probability.ron")),
        "--output",
        path(&probability),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&probability).unwrap(),
        fs::read_to_string(freehack_fixture(
            "probability-preview.freehack-probability-preview.json"
        ))
        .unwrap()
    );

    for (kind, artifact) in [
        (
            "freehack-creation-request",
            "creation.tabletop-creation.json",
        ),
        (
            "freehack-creation-preview",
            "preview.tabletop-creation-preview.ron",
        ),
        (
            "freehack-probability-request",
            "probability.freehack-probability.ron",
        ),
        (
            "freehack-probability-preview",
            "probability-preview.freehack-probability-preview.json",
        ),
        (
            "freehack-public-state",
            "public-state.freehack-public-state.ron",
        ),
        (
            "freehack-public-receipt",
            "public-receipt.freehack-public-receipt.json",
        ),
        (
            "freehack-authority-receipt",
            "authority-receipt.freehack-authority-receipt.ron",
        ),
    ] {
        let output = run(&["validate", kind, path(&freehack_fixture(artifact))]);
        assert!(
            output.status.success(),
            "{}: {}",
            artifact,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let preview_authority = directory.path().join("preview-authority.json");
    let preview_public = directory.path().join("preview-public.ron");
    let output = run(&[
        "freehack-resolve",
        path(&freehack_fixture(
            "playthrough/preview_crossing_probability.tabletop-request.json",
        )),
        "--state",
        path(&freehack_fixture("authority-state.tabletop-state.ron")),
        "--authority-output",
        path(&preview_authority),
        "--public-output",
        path(&preview_public),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&preview_authority).unwrap(),
        fs::read_to_string(freehack_fixture(
            "playthrough/preview_crossing_probability.authority-receipt.json"
        ))
        .unwrap()
    );
    assert_eq!(
        fs::read_to_string(&preview_public).unwrap(),
        fs::read_to_string(freehack_fixture(
            "playthrough/preview_crossing_probability.public-receipt.ron"
        ))
        .unwrap()
    );

    let preview_receipt =
        FreehackAuthorityReceipt::from_json(&fs::read_to_string(&preview_authority).unwrap())
            .unwrap();
    let visible_before = directory.path().join("visible-before.json");
    fs::write(
        &visible_before,
        preview_receipt.receipt.after_state.to_json().unwrap(),
    )
    .unwrap();
    let visible_authority = directory.path().join("visible-authority.ron");
    let visible_public = directory.path().join("visible-public.json");
    let output = run(&[
        "freehack-resolve",
        path(&freehack_fixture("request.tabletop-request.json")),
        "--state",
        path(&visible_before),
        "--authority-output",
        path(&visible_authority),
        "--public-output",
        path(&visible_public),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&visible_authority).unwrap(),
        fs::read_to_string(freehack_fixture(
            "authority-receipt.freehack-authority-receipt.ron"
        ))
        .unwrap()
    );
    assert_eq!(
        fs::read_to_string(&visible_public).unwrap(),
        fs::read_to_string(freehack_fixture(
            "public-receipt.freehack-public-receipt.json"
        ))
        .unwrap()
    );

    let visible_receipt =
        FreehackAuthorityReceipt::from_ron(&fs::read_to_string(&visible_authority).unwrap())
            .unwrap();
    let hidden_before = directory.path().join("hidden-before.ron");
    fs::write(
        &hidden_before,
        visible_receipt.receipt.after_state.to_ron().unwrap(),
    )
    .unwrap();
    let hidden_authority = directory.path().join("hidden-authority.json");
    let forbidden_public = directory.path().join("hidden-public.json");
    let output = run(&[
        "freehack-resolve",
        path(&freehack_fixture(
            "playthrough/notice_hidden_signal.tabletop-request.ron",
        )),
        "--state",
        path(&hidden_before),
        "--authority-output",
        path(&hidden_authority),
        "--public-output",
        path(&forbidden_public),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&hidden_authority).unwrap(),
        fs::read_to_string(freehack_fixture(
            "hidden-authority-receipt.freehack-authority-receipt.json"
        ))
        .unwrap()
    );
    assert!(!forbidden_public.exists());

    for (kind, checked) in [
        (
            "freehack-creation-request",
            "weave-tabletop-freehack-creation-request-v1.schema.json",
        ),
        (
            "freehack-creation-preview",
            "weave-tabletop-freehack-creation-preview-v1.schema.json",
        ),
        (
            "freehack-probability-request",
            "weave-tabletop-freehack-probability-request-v1.schema.json",
        ),
        (
            "freehack-probability-preview",
            "weave-tabletop-freehack-probability-preview-v1.schema.json",
        ),
        (
            "freehack-public-state",
            "weave-tabletop-freehack-public-state-v1.schema.json",
        ),
        (
            "freehack-public-receipt",
            "weave-tabletop-freehack-public-receipt-v1.schema.json",
        ),
        (
            "freehack-authority-receipt",
            "weave-tabletop-freehack-authority-receipt-v1.schema.json",
        ),
    ] {
        let generated = directory.path().join(checked);
        let output = run(&["schema", kind, "--output", path(&generated)]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(generated).unwrap(),
            fs::read_to_string(root().join("schemas").join(checked)).unwrap()
        );
    }
}
