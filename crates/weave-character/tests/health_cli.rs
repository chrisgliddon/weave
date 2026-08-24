use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;

const ROOT: &str = "examples/domain-modules/weave-character/health";
const SCENARIOS: [&str; 6] = [
    "healthy",
    "incomplete",
    "stale",
    "unsafe",
    "malformed",
    "migration-required",
];

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weave-character"))
        .current_dir(repository_root())
        .args(arguments)
        .output()
        .expect("run weave-character")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture(scenario: &str, file: &str) -> String {
    format!("{ROOT}/{scenario}/{file}")
}

fn assert_file_eq(actual: &Path, expected: &str) {
    assert_eq!(
        fs::read(actual).unwrap(),
        fs::read(repository_root().join(expected)).unwrap()
    );
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, current: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, snapshot);
            } else {
                snapshot.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[test]
fn health_schemas_and_all_golden_documents_match_strict_cli_contracts() {
    let temporary = tempdir().unwrap();
    for (kind, schema) in [
        (
            "health-manifest",
            "schemas/weave-character-health-manifest-v1.schema.json",
        ),
        (
            "health-report",
            "schemas/weave-character-health-report-v1.schema.json",
        ),
    ] {
        let output_path = temporary.path().join(format!("{kind}.schema.json"));
        let output = run(&["schema", kind, "--output", output_path.to_str().unwrap()]);
        assert_success(&output);
        assert_file_eq(&output_path, schema);
    }

    for scenario in SCENARIOS {
        for format in ["json", "ron"] {
            assert_success(&run(&[
                "validate",
                "health-manifest",
                &fixture(scenario, &format!("project.health-manifest.{format}")),
            ]));
            assert_success(&run(&[
                "validate",
                "health-report",
                &fixture(scenario, &format!("report.health-report.{format}")),
            ]));
        }
    }
}

#[test]
fn health_audit_outputs_are_exact_read_only_filterable_and_ci_thresholded() {
    let fixture_root = repository_root().join(ROOT);
    let before = snapshot_tree(&fixture_root);
    let temporary = tempdir().unwrap();
    let healthy_manifest = fixture("healthy", "project.health-manifest.json");

    for format in ["text", "json", "ron"] {
        let suffix = if format == "text" { "txt" } else { format };
        let output_path = temporary.path().join(format!("healthy-report.{suffix}"));
        assert_success(&run(&[
            "health-audit",
            &healthy_manifest,
            "--format",
            format,
            "--output",
            output_path.to_str().unwrap(),
        ]));
        assert_file_eq(
            &output_path,
            &fixture("healthy", &format!("report.health-report.{suffix}")),
        );
    }

    let healthy_ci = run(&["health-audit", &healthy_manifest, "--ci"]);
    assert_success(&healthy_ci);
    let incomplete_ci = run(&[
        "health-audit",
        &fixture("incomplete", "project.health-manifest.json"),
        "--ci",
    ]);
    assert_success(&incomplete_ci);

    for scenario in ["stale", "unsafe", "malformed", "migration-required"] {
        let output = run(&[
            "health-audit",
            &fixture(scenario, "project.health-manifest.json"),
            "--format",
            "json",
            "--ci",
        ]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let filtered_but_failing = run(&[
        "health-audit",
        &fixture("unsafe", "project.health-manifest.json"),
        "--format",
        "json",
        "--code",
        "H104",
        "--ci",
    ]);
    assert_eq!(filtered_but_failing.status.code(), Some(2));
    let filtered: serde_json::Value = serde_json::from_slice(&filtered_but_failing.stdout).unwrap();
    assert!(filtered["diagnostics"].as_array().unwrap().is_empty());

    let unsafe_filtered = run(&[
        "health-audit",
        &fixture("unsafe", "project.health-manifest.ron"),
        "--format",
        "json",
        "--code",
        "H304",
        "--minimum-severity",
        "error",
    ]);
    assert_success(&unsafe_filtered);
    let unsafe_filtered: serde_json::Value =
        serde_json::from_slice(&unsafe_filtered.stdout).unwrap();
    assert_eq!(unsafe_filtered["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(unsafe_filtered["diagnostics"][0]["code"], "H304");

    assert_eq!(snapshot_tree(&fixture_root), before);
}

#[test]
fn health_audit_refuses_to_replace_any_input() {
    let manifest = fixture("unsafe", "project.health-manifest.json");
    let source = fixture("unsafe", "unsafe.expression-pack.json");
    for output_path in [&manifest, &source] {
        let before = fs::read(repository_root().join(output_path)).unwrap();
        let output = run(&[
            "health-audit",
            &manifest,
            "--format",
            "json",
            "--output",
            output_path,
        ]);
        assert!(!output.status.success());
        assert_eq!(
            fs::read(repository_root().join(output_path)).unwrap(),
            before
        );
    }
}
