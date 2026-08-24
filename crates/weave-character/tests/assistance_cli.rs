use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;
use weave_character::AssistanceDecisionReview;

const ROOT: &str = "examples/domain-modules/weave-character/assistance";

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

fn fixture(stem: &str, format: &str) -> String {
    format!("{ROOT}/{stem}.{format}")
}

fn assert_file_eq(actual: &Path, expected: &str) {
    assert_eq!(
        fs::read(actual).unwrap(),
        fs::read(repository_root().join(expected)).unwrap()
    );
}

#[test]
fn assistance_schemas_and_strict_documents_match_checked_artifacts() {
    let temporary = tempdir().unwrap();
    for (kind, schema, stem) in [
        (
            "assistance-template",
            "schemas/weave-character-assistance-template-v1.schema.json",
            "glasswind.assistance-template",
        ),
        (
            "assistance-request",
            "schemas/weave-character-assistance-request-v1.schema.json",
            "single.assistance-request",
        ),
        (
            "assistance-preview",
            "schemas/weave-character-assistance-preview-v1.schema.json",
            "single.assistance-preview",
        ),
        (
            "assistance-approval",
            "schemas/weave-character-assistance-approval-v1.schema.json",
            "single.assistance-approval",
        ),
        (
            "assistance-provider-response",
            "schemas/weave-character-assistance-provider-response-v1.schema.json",
            "single.assistance-provider-response",
        ),
        (
            "assistance-candidate-set",
            "schemas/weave-character-assistance-candidate-set-v1.schema.json",
            "single.assistance-candidate-set",
        ),
        (
            "assistance-advisory-review",
            "schemas/weave-character-assistance-advisory-review-v1.schema.json",
            "single.assistance-advisory-review",
        ),
        (
            "assistance-decision-review",
            "schemas/weave-character-assistance-decision-review-v1.schema.json",
            "single.assistance-decision-review",
        ),
        (
            "assistance-receipt",
            "schemas/weave-character-assistance-receipt-v1.schema.json",
            "single.assistance-receipt",
        ),
        (
            "assistance-batch-request",
            "schemas/weave-character-assistance-batch-request-v1.schema.json",
            "batch.assistance-request",
        ),
        (
            "assistance-batch-preview",
            "schemas/weave-character-assistance-batch-preview-v1.schema.json",
            "batch.assistance-preview",
        ),
        (
            "assistance-job",
            "schemas/weave-character-assistance-job-v1.schema.json",
            "batch.assistance-job",
        ),
        (
            "assistance-batch-receipt",
            "schemas/weave-character-assistance-batch-receipt-v1.schema.json",
            "batch.assistance-receipt",
        ),
        (
            "assistance-comparison",
            "schemas/weave-character-assistance-comparison-v1.schema.json",
            "providers.assistance-comparison",
        ),
    ] {
        let output_path = temporary.path().join(format!("{kind}.schema.json"));
        let output = run(&["schema", kind, "--output", output_path.to_str().unwrap()]);
        assert_success(&output);
        assert_file_eq(&output_path, schema);
        for format in ["json", "ron"] {
            assert_success(&run(&["validate", kind, &fixture(stem, format)]));
        }
    }
}

#[test]
fn single_assistance_cli_flow_is_reproducible_and_review_gated() {
    let temporary = tempdir().unwrap();
    let preview = temporary.path().join("preview.json");
    assert_success(&run(&[
        "assistance-preview",
        &fixture("input.character-collection", "json"),
        &fixture("glasswind.assistance-template", "json"),
        &fixture("single.assistance-request", "json"),
        "--output",
        preview.to_str().unwrap(),
    ]));
    assert_file_eq(&preview, &fixture("single.assistance-preview", "json"));

    let approval = temporary.path().join("approval.json");
    assert_success(&run(&[
        "assistance-approve",
        preview.to_str().unwrap(),
        "--author",
        "org.weave.reviewer.fixture",
        "--rationale",
        "Approve the exact visible offline scope for this original synthetic fixture.",
        "--output",
        approval.to_str().unwrap(),
    ]));
    assert_file_eq(&approval, &fixture("single.assistance-approval", "json"));

    let candidates = temporary.path().join("candidates.json");
    assert_success(&run(&[
        "assistance-generate-offline",
        &fixture("input.character-collection", "json"),
        &fixture("glasswind.assistance-template", "json"),
        preview.to_str().unwrap(),
        approval.to_str().unwrap(),
        "--output",
        candidates.to_str().unwrap(),
    ]));
    assert_file_eq(
        &candidates,
        &fixture("single.assistance-candidate-set", "json"),
    );

    let set: serde_json::Value = serde_json::from_slice(&fs::read(&candidates).unwrap()).unwrap();
    let candidate_id = set["candidates"]
        .as_object()
        .and_then(|values| values.keys().next())
        .unwrap();
    let inspected = run(&[
        "assistance-inspect",
        candidates.to_str().unwrap(),
        candidate_id,
    ]);
    assert_success(&inspected);
    let inspected: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected["id"], candidate_id.as_str());
    assert!(!inspected["evidence"].as_array().unwrap().is_empty());

    let advisory = temporary.path().join("advisory.json");
    assert_success(&run(&[
        "assistance-advise",
        candidates.to_str().unwrap(),
        "--reviewer",
        "org.weave.reviewer.advisory",
        "--output",
        advisory.to_str().unwrap(),
    ]));
    assert_file_eq(
        &advisory,
        &fixture("single.assistance-advisory-review", "json"),
    );

    let checked_review = AssistanceDecisionReview::from_json(
        &fs::read_to_string(
            repository_root().join(fixture("single.assistance-decision-review", "json")),
        )
        .unwrap(),
    )
    .unwrap();
    let decisions = temporary.path().join("decisions.json");
    fs::write(
        &decisions,
        weave_domain::to_pretty_json(&checked_review.decisions).unwrap(),
    )
    .unwrap();
    let review = temporary.path().join("review.json");
    assert_success(&run(&[
        "assistance-review",
        candidates.to_str().unwrap(),
        decisions.to_str().unwrap(),
        "--author",
        "org.weave.reviewer.fixture",
        "--rationale",
        "Exercise accept, edit, reject, defer, and regenerate as explicit fixture decisions.",
        "--output",
        review.to_str().unwrap(),
    ]));
    assert_file_eq(
        &review,
        &fixture("single.assistance-decision-review", "json"),
    );

    let dry_receipt = temporary.path().join("dry-receipt.json");
    let absent_profile = temporary.path().join("dry-profile.json");
    assert_success(&run(&[
        "assistance-apply",
        "examples/domain-modules/weave-character/profile.character.json",
        candidates.to_str().unwrap(),
        review.to_str().unwrap(),
        "--advisory",
        advisory.to_str().unwrap(),
        "--dry-run",
        "--receipt-output",
        dry_receipt.to_str().unwrap(),
        "--profile-output",
        absent_profile.to_str().unwrap(),
    ]));
    assert_file_eq(&dry_receipt, &fixture("single.assistance-receipt", "json"));
    assert!(!absent_profile.exists());

    let receipt = temporary.path().join("receipt.json");
    let applied = temporary.path().join("applied.json");
    assert_success(&run(&[
        "assistance-apply",
        "examples/domain-modules/weave-character/profile.character.json",
        candidates.to_str().unwrap(),
        review.to_str().unwrap(),
        "--advisory",
        advisory.to_str().unwrap(),
        "--receipt-output",
        receipt.to_str().unwrap(),
        "--profile-output",
        applied.to_str().unwrap(),
    ]));
    assert_file_eq(&receipt, &fixture("single.assistance-receipt", "json"));
    assert_file_eq(&applied, &fixture("single.applied-character", "json"));

    let comparison = temporary.path().join("comparison.json");
    assert_success(&run(&[
        "assistance-compare",
        "--candidates",
        candidates.to_str().unwrap(),
        "--candidates",
        &fixture("alternate.assistance-candidate-set", "json"),
        "--advisory",
        advisory.to_str().unwrap(),
        "--advisory",
        &fixture("alternate.assistance-advisory-review", "json"),
        "--output",
        comparison.to_str().unwrap(),
    ]));
    assert_file_eq(
        &comparison,
        &fixture("providers.assistance-comparison", "json"),
    );
}

#[test]
fn batch_assistance_cli_flow_resumes_cancels_and_applies_atomically() {
    let temporary = tempdir().unwrap();
    let preview = temporary.path().join("batch-preview.json");
    assert_success(&run(&[
        "assistance-batch-preview",
        &fixture("input.character-collection", "json"),
        &fixture("glasswind.assistance-template", "json"),
        &fixture("batch.assistance-request", "json"),
        "--output",
        preview.to_str().unwrap(),
    ]));
    assert_file_eq(&preview, &fixture("batch.assistance-preview", "json"));

    let approval = temporary.path().join("batch-approval.json");
    assert_success(&run(&[
        "assistance-batch-approve",
        preview.to_str().unwrap(),
        "--author",
        "org.weave.reviewer.fixture",
        "--rationale",
        "Approve every exact per-character disclosure in the deterministic offline fixture batch.",
        "--output",
        approval.to_str().unwrap(),
    ]));
    assert_file_eq(&approval, &fixture("batch.assistance-approval", "json"));

    let initial = temporary.path().join("batch-initial.json");
    assert_success(&run(&[
        "assistance-batch-start",
        &fixture("input.character-collection", "json"),
        &fixture("glasswind.assistance-template", "json"),
        preview.to_str().unwrap(),
        approval.to_str().unwrap(),
        "--output",
        initial.to_str().unwrap(),
    ]));

    let cancelled = temporary.path().join("batch-cancelled.json");
    assert_success(&run(&[
        "assistance-batch-cancel",
        initial.to_str().unwrap(),
        "--rationale",
        "Stop this reviewable synthetic batch before its first provider call.",
        "--output",
        cancelled.to_str().unwrap(),
    ]));
    let cancelled: serde_json::Value =
        serde_json::from_slice(&fs::read(&cancelled).unwrap()).unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["provider_calls"], 0);

    let first = temporary.path().join("batch-first.json");
    assert_success(&run(&[
        "assistance-batch-resume-offline",
        &fixture("input.character-collection", "json"),
        initial.to_str().unwrap(),
        "--max-attempts",
        "64",
        "--output",
        first.to_str().unwrap(),
    ]));
    let complete = temporary.path().join("batch-complete.json");
    assert_success(&run(&[
        "assistance-batch-resume-offline",
        &fixture("input.character-collection", "json"),
        first.to_str().unwrap(),
        "--max-attempts",
        "64",
        "--output",
        complete.to_str().unwrap(),
    ]));
    assert_file_eq(&complete, &fixture("batch.assistance-job", "json"));

    let receipt = temporary.path().join("batch-receipt.json");
    let applied = temporary.path().join("batch-applied.json");
    assert_success(&run(&[
        "assistance-batch-apply",
        &fixture("input.character-collection", "json"),
        complete.to_str().unwrap(),
        "--review",
        &fixture("batch.ari_vale.assistance-decision-review", "json"),
        "--review",
        &fixture("batch.sable_reed.assistance-decision-review", "json"),
        "--advisory",
        &fixture("batch.ari_vale.0.assistance-advisory-review", "json"),
        "--advisory",
        &fixture("batch.sable_reed.0.assistance-advisory-review", "json"),
        "--receipt-output",
        receipt.to_str().unwrap(),
        "--collection-output",
        applied.to_str().unwrap(),
    ]));
    assert_file_eq(&receipt, &fixture("batch.assistance-receipt", "json"));
    assert_file_eq(
        &applied,
        &fixture("batch.applied-character-collection", "json"),
    );
}
