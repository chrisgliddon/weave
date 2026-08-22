use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;
use weave_character::{
    CHARACTER_OVERLAY_FORMAT_VERSION, CHARACTER_TEMPLATE_FORMAT_VERSION, CharacterOverlay,
    CharacterTemplate, CharacterTemplateRef, PresentationReceipt, apply_authoring_revision,
    create_authoring_draft, new_authoring_workspace, presentation_authoring_revision,
    template_fingerprint,
};

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

#[test]
fn presentation_receipt_revision_matches_library_and_text_authoring_bytes() {
    let temporary = tempdir().unwrap();
    let receipt_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../examples/domain-modules/weave-character/presentation/receipt.presentation-receipt.json",
    );
    let receipt = PresentationReceipt::from_json(
        &std::fs::read_to_string(&receipt_path).expect("checked presentation receipt"),
    )
    .unwrap();
    let character_id = "org.weave.character.ari_vale";
    let profile = receipt.proposal.input_collection.characters[character_id].clone();
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.presentation_cli".to_owned(),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.presentation_cli".to_owned(),
        character_id: character_id.to_owned(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template).unwrap(),
        }),
        operations: Vec::new(),
        provenance: profile.provenance.clone(),
    };
    let workspace = new_authoring_workspace(
        "org.weave.character.authoring.presentation_cli",
        profile.provenance.clone(),
    )
    .and_then(|workspace| create_authoring_draft(&workspace, Some(template), overlay))
    .unwrap();
    let workspace_path = temporary.path().join("workspace.json");
    std::fs::write(&workspace_path, workspace.to_json().unwrap()).unwrap();

    let revision_id = "org.weave.character.revision.presentation_cli";
    let rationale = "Adopt the complete reviewed presentation allocation.";
    let expected_revision = presentation_authoring_revision(
        &workspace.drafts[character_id],
        revision_id,
        rationale,
        receipt,
    )
    .unwrap();
    let revision_path = temporary.path().join("revision.json");
    run(&[
        "presentation-revision",
        workspace_path.to_str().unwrap(),
        character_id,
        receipt_path.to_str().unwrap(),
        "--id",
        revision_id,
        "--rationale",
        rationale,
        "--output",
        revision_path.to_str().unwrap(),
    ]);
    assert_eq!(
        std::fs::read_to_string(&revision_path).unwrap(),
        expected_revision.to_json().unwrap()
    );

    let expected_workspace = apply_authoring_revision(&workspace, &expected_revision).unwrap();
    let revised_path = temporary.path().join("revised.json");
    run(&[
        "authoring-revise",
        workspace_path.to_str().unwrap(),
        revision_path.to_str().unwrap(),
        "--output",
        revised_path.to_str().unwrap(),
    ]);
    assert_eq!(
        std::fs::read_to_string(revised_path).unwrap(),
        expected_workspace.to_json().unwrap()
    );
}
