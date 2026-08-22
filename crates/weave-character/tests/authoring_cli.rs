use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/domain-modules/weave-character/authoring")
        .join(name)
}

fn run(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_weave-character"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_exact(actual: &Path, expected: &Path) {
    assert_eq!(
        std::fs::read(actual).unwrap(),
        std::fs::read(expected).unwrap(),
        "{} differs from {}",
        actual.display(),
        expected.display()
    );
}

#[test]
fn text_authoring_commands_match_checked_editor_contract_bytes() {
    let temporary = tempdir().unwrap();
    let created = temporary.path().join("created.json");
    run(&[
        "authoring-create",
        fixture("workspace.empty.authoring-workspace.json")
            .to_str()
            .unwrap(),
        fixture("blank.authoring-overlay.json").to_str().unwrap(),
        "--format",
        "json",
        "--output",
        created.to_str().unwrap(),
    ]);
    assert_exact(
        &created,
        &fixture("workspace.created.authoring-workspace.json"),
    );

    let list = temporary.path().join("list.json");
    run(&[
        "authoring-list",
        created.to_str().unwrap(),
        "--output",
        list.to_str().unwrap(),
    ]);
    let summaries: Vec<weave_character::CharacterAuthoringDraftSummary> =
        weave_domain::parse_strict_json(&std::fs::read_to_string(&list).unwrap()).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "org.weave.character.lumen_reed");

    let shown = temporary.path().join("shown.json");
    run(&[
        "authoring-show",
        created.to_str().unwrap(),
        "org.weave.character.lumen_reed",
        "--output",
        shown.to_str().unwrap(),
    ]);
    let _: weave_character::CharacterAuthoringDraft =
        weave_domain::parse_strict_json(&std::fs::read_to_string(&shown).unwrap()).unwrap();

    let proposal = temporary.path().join("proposal.json");
    run(&[
        "questionnaire-propose",
        fixture("input.character.json").to_str().unwrap(),
        fixture("lantern_choices.questionnaire-pack.json")
            .to_str()
            .unwrap(),
        fixture("answers.questionnaire-answers.json")
            .to_str()
            .unwrap(),
        "--seed",
        "20260822",
        "--output",
        proposal.to_str().unwrap(),
    ]);
    assert_exact(&proposal, &fixture("proposal.questionnaire-proposal.json"));

    let review = temporary.path().join("review.json");
    run(&[
        "questionnaire-review",
        proposal.to_str().unwrap(),
        fixture("facet-decisions.questionnaire-review.json")
            .to_str()
            .unwrap(),
        fixture("conflict-decisions.questionnaire-review.json")
            .to_str()
            .unwrap(),
        "--reviewer",
        "org.weave.reviewer.fixture",
        "--rationale",
        "Review every proposed facet and retain one intentional narrative tension.",
        "--output",
        review.to_str().unwrap(),
    ]);
    assert_exact(&review, &fixture("review.questionnaire-review.json"));

    let receipt = temporary.path().join("receipt.json");
    run(&[
        "questionnaire-apply",
        proposal.to_str().unwrap(),
        review.to_str().unwrap(),
        "--output",
        receipt.to_str().unwrap(),
    ]);
    assert_exact(&receipt, &fixture("receipt.questionnaire-receipt.json"));

    let revision = temporary.path().join("revision.json");
    run(&[
        "questionnaire-revision",
        created.to_str().unwrap(),
        "org.weave.character.lumen_reed",
        receipt.to_str().unwrap(),
        "--id",
        "org.weave.character.revision.lumen_questionnaire",
        "--rationale",
        "Apply the fully reviewed original narrative questionnaire.",
        "--output",
        revision.to_str().unwrap(),
    ]);
    assert_exact(&revision, &fixture("questionnaire.authoring-revision.json"));

    let preview = temporary.path().join("preview.json");
    run(&[
        "authoring-preview",
        created.to_str().unwrap(),
        revision.to_str().unwrap(),
        "--output",
        preview.to_str().unwrap(),
    ]);
    assert_exact(&preview, &fixture("questionnaire.authoring-preview.json"));

    let revised = temporary.path().join("revised.json");
    run(&[
        "authoring-revise",
        created.to_str().unwrap(),
        revision.to_str().unwrap(),
        "--format",
        "json",
        "--output",
        revised.to_str().unwrap(),
    ]);
    assert_exact(
        &revised,
        &fixture("workspace.revised.authoring-workspace.json"),
    );

    let reviewed = temporary.path().join("reviewed.json");
    run(&[
        "authoring-review",
        revised.to_str().unwrap(),
        "org.weave.character.lumen_reed",
        "--decision",
        "accepted",
        "--reviewer",
        "org.weave.reviewer.final",
        "--rationale",
        "Canonical fields, derived views, context, provenance, and diagnostics are ready.",
        "--format",
        "json",
        "--output",
        reviewed.to_str().unwrap(),
    ]);
    assert_exact(
        &reviewed,
        &fixture("workspace.reviewed.authoring-workspace.json"),
    );

    let exported = temporary.path().join("exported.json");
    run(&[
        "authoring-export",
        reviewed.to_str().unwrap(),
        "org.weave.character.lumen_reed",
        "--output",
        exported.to_str().unwrap(),
    ]);
    assert_exact(&exported, &fixture("exported.character.json"));

    let reopened = temporary.path().join("reopened.json");
    run(&[
        "authoring-reopen",
        reviewed.to_str().unwrap(),
        "--output",
        reopened.to_str().unwrap(),
    ]);
    assert_exact(&reopened, &reviewed);
    run(&["authoring-validate", reopened.to_str().unwrap()]);

    let cloned = temporary.path().join("cloned.json");
    run(&[
        "authoring-clone",
        created.to_str().unwrap(),
        "org.weave.character.lumen_reed",
        "--new-character-id",
        "org.weave.character.lumen_reed_clone",
        "--new-overlay-id",
        "org.weave.character.overlay.lumen_reed_clone",
        "--format",
        "json",
        "--output",
        cloned.to_str().unwrap(),
    ]);
    let cloned = weave_character::CharacterAuthoringWorkspace::from_json(
        &std::fs::read_to_string(cloned).unwrap(),
    )
    .unwrap();
    assert_eq!(cloned.drafts.len(), 2);
}

#[test]
fn text_authoring_rejects_invalid_source_before_persistence() {
    for (kind, file) in [
        (
            "authoring-revision",
            "invalid/unknown-placeholder.authoring-revision.json",
        ),
        (
            "authoring-revision",
            "invalid/protected-extension.authoring-revision.json",
        ),
        (
            "authoring-revision",
            "invalid/unknown-field.authoring-revision.json",
        ),
        (
            "authoring-workspace",
            "invalid/broken-template.authoring-workspace.json",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_weave-character"))
            .args(["validate", kind, fixture(file).to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "invalid fixture unexpectedly passed"
        );
    }
}
