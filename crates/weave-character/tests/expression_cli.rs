use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

const ROOT: &str = "examples/domain-modules/weave-character/expression";

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

#[test]
fn expression_schema_and_strict_document_validation_cover_every_public_artifact() {
    let temporary = tempdir().unwrap();
    for (kind, schema, fixture) in [
        (
            "expression-pack",
            "schemas/weave-character-expression-pack-v1.schema.json",
            format!("{ROOT}/glasswind.expression-pack.json"),
        ),
        (
            "expression-revision",
            "schemas/weave-character-expression-revision-v1.schema.json",
            format!("{ROOT}/normalized.expression-revision.json"),
        ),
        (
            "expression-assignment-request",
            "schemas/weave-character-expression-assignment-request-v1.schema.json",
            format!("{ROOT}/assignment.expression-request.json"),
        ),
        (
            "expression-assignment-receipt",
            "schemas/weave-character-expression-assignment-receipt-v1.schema.json",
            format!("{ROOT}/assignment.expression-receipt.json"),
        ),
        (
            "expression-resolution-request",
            "schemas/weave-character-expression-resolution-request-v1.schema.json",
            format!("{ROOT}/contextual.expression-resolution-request.json"),
        ),
        (
            "expression-resolution",
            "schemas/weave-character-expression-resolution-v1.schema.json",
            format!("{ROOT}/contextual.expression-resolution.json"),
        ),
        (
            "expression-lint",
            "schemas/weave-character-expression-lint-v1.schema.json",
            format!("{ROOT}/lint.expression-lint.json"),
        ),
        (
            "expression-coverage",
            "schemas/weave-character-expression-coverage-v1.schema.json",
            format!("{ROOT}/coverage.expression-coverage.json"),
        ),
    ] {
        let output_path = temporary.path().join(format!("{kind}.schema.json"));
        let output = run(&["schema", kind, "--output", output_path.to_str().unwrap()]);
        assert_success(&output);
        assert_eq!(
            fs::read(&output_path).unwrap(),
            fs::read(repository_root().join(schema)).unwrap()
        );

        let output = run(&["validate", kind, &fixture]);
        assert_success(&output);
    }
}

#[test]
fn expression_normalize_revise_list_and_show_match_checked_fixtures() {
    let temporary = tempdir().unwrap();
    let normalized = temporary.path().join("normalized.json");
    let output = run(&[
        "expression-normalize",
        &format!("{ROOT}/raw.expression-revision.json"),
        "--output",
        normalized.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&normalized).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/normalized.expression-revision.json")))
            .unwrap()
    );

    let revised = temporary.path().join("revised.json");
    let output = run(&[
        "expression-revise",
        "examples/domain-modules/weave-character/omitted-extensions.character.json",
        normalized.to_str().unwrap(),
        "--output",
        revised.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&revised).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/revised.character.json"))).unwrap()
    );

    let list = run(&[
        "expression-list",
        &format!("{ROOT}/applied.character.json"),
        "--kind",
        "term",
        "--origin",
        "pack-assigned",
    ]);
    assert_success(&list);
    let records: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 2);
    assert!(records.as_array().unwrap().iter().all(|record| {
        record["kind"] == "term" && record["value"]["origin"] == "pack_assigned"
    }));

    let shown = run(&[
        "expression-show",
        &format!("{ROOT}/applied.character.json"),
        "term",
        "trailmark",
    ]);
    assert_success(&shown);
    let record: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(record["value"]["surface"], "trailmark");
}

#[test]
fn expression_assign_lint_coverage_validate_and_resolve_are_reproducible() {
    let temporary = tempdir().unwrap();
    let receipt = temporary.path().join("receipt.json");
    let profile = temporary.path().join("profile.json");
    let pack = format!("{ROOT}/glasswind.expression-pack.json");
    let output = run(&[
        "expression-assign",
        &format!("{ROOT}/revised.character.json"),
        &pack,
        &format!("{ROOT}/assignment.expression-request.json"),
        "--receipt-output",
        receipt.to_str().unwrap(),
        "--profile-output",
        profile.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&receipt).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/assignment.expression-receipt.json")))
            .unwrap()
    );
    assert_eq!(
        fs::read(&profile).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/applied.character.json"))).unwrap()
    );

    let lint = temporary.path().join("lint.json");
    let output = run(&[
        "expression-lint",
        profile.to_str().unwrap(),
        "--pack",
        &pack,
        "--output",
        lint.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&lint).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/lint.expression-lint.json"))).unwrap()
    );

    let coverage = temporary.path().join("coverage.json");
    let output = run(&[
        "expression-coverage",
        profile.to_str().unwrap(),
        "--pack",
        &pack,
        "--output",
        coverage.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&coverage).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/coverage.expression-coverage.json")))
            .unwrap()
    );

    assert_success(&run(&[
        "expression-validate",
        profile.to_str().unwrap(),
        "--pack",
        &pack,
    ]));

    let resolution = temporary.path().join("resolution.json");
    let output = run(&[
        "expression-resolve",
        profile.to_str().unwrap(),
        &pack,
        &format!("{ROOT}/contextual.expression-resolution-request.json"),
        "--output",
        resolution.to_str().unwrap(),
    ]);
    assert_success(&output);
    assert_eq!(
        fs::read(&resolution).unwrap(),
        fs::read(repository_root().join(format!("{ROOT}/contextual.expression-resolution.json")))
            .unwrap()
    );
}
