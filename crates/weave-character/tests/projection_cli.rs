use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

const ROOT: &str = "examples/domain-modules/weave-character/projections";

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
fn projection_schemas_and_documents_match_checked_artifacts() {
    let temporary = tempdir().unwrap();
    for (kind, schema, fixture) in [
        (
            "projection-pack",
            "schemas/weave-character-projection-pack-v1.schema.json",
            format!("{ROOT}/glasswind.projection-pack.json"),
        ),
        (
            "projection-config",
            "schemas/weave-character-projection-config-v1.schema.json",
            format!("{ROOT}/selection.projection-config.json"),
        ),
        (
            "projection-proposal",
            "schemas/weave-character-projection-proposal-v1.schema.json",
            format!("{ROOT}/proposal.projection-proposal.json"),
        ),
        (
            "projection-review",
            "schemas/weave-character-projection-review-v1.schema.json",
            format!("{ROOT}/review.projection-review.json"),
        ),
        (
            "projection-receipt",
            "schemas/weave-character-projection-receipt-v1.schema.json",
            format!("{ROOT}/receipt.projection-receipt.json"),
        ),
        (
            "projection-lock-revision",
            "schemas/weave-character-projection-lock-revision-v1.schema.json",
            format!("{ROOT}/unlock.projection-lock-revision.json"),
        ),
    ] {
        let output_path = temporary.path().join(format!("{kind}.schema.json"));
        let output = run(&["schema", kind, "--output", output_path.to_str().unwrap()]);
        assert_success(&output);
        assert_eq!(
            fs::read(&output_path).unwrap(),
            fs::read(repository_root().join(schema)).unwrap()
        );
        assert_success(&run(&["validate", kind, &fixture]));
    }
}

#[test]
fn projection_propose_inspect_review_apply_and_lock_are_reproducible() {
    let temporary = tempdir().unwrap();

    for format in ["json", "ron"] {
        let proposal = temporary.path().join(format!("proposal.{format}"));
        assert_success(&run(&[
            "projection-propose",
            &format!("{ROOT}/input.character-collection.{format}"),
            &format!("{ROOT}/glasswind.projection-pack.{format}"),
            &format!("{ROOT}/selection.projection-config.{format}"),
            "--seed",
            "20260824",
            "--format",
            format,
            "--output",
            proposal.to_str().unwrap(),
        ]));
        assert_eq!(
            fs::read(&proposal).unwrap(),
            fs::read(
                repository_root().join(format!("{ROOT}/proposal.projection-proposal.{format}"))
            )
            .unwrap()
        );

        let review = temporary.path().join(format!("review.{format}"));
        assert_success(&run(&[
            "projection-review",
            proposal.to_str().unwrap(),
            &format!("{ROOT}/decisions.projection-review.{format}"),
            "--reviewer",
            "org.weave.reviewer.fixture",
            "--rationale",
            "Review every original Glasswind Lenses display and role independently; retain only explicit fictional editorial choices outside canon.",
            "--format",
            format,
            "--output",
            review.to_str().unwrap(),
        ]));
        assert_eq!(
            fs::read(&review).unwrap(),
            fs::read(repository_root().join(format!("{ROOT}/review.projection-review.{format}")))
                .unwrap()
        );

        let receipt = temporary.path().join(format!("receipt.{format}"));
        let applied = temporary.path().join(format!("applied.{format}"));
        assert_success(&run(&[
            "projection-apply",
            &format!("{ROOT}/input.character-collection.{format}"),
            proposal.to_str().unwrap(),
            review.to_str().unwrap(),
            "--receipt-output",
            receipt.to_str().unwrap(),
            "--collection-output",
            applied.to_str().unwrap(),
            "--format",
            format,
        ]));
        assert_eq!(
            fs::read(&receipt).unwrap(),
            fs::read(repository_root().join(format!("{ROOT}/receipt.projection-receipt.{format}")))
                .unwrap()
        );
        assert_eq!(
            fs::read(&applied).unwrap(),
            fs::read(
                repository_root().join(format!("{ROOT}/applied.character-collection.{format}"))
            )
            .unwrap()
        );

        let unlocked = temporary.path().join(format!("unlocked.{format}"));
        assert_success(&run(&[
            "projection-lock",
            applied.to_str().unwrap(),
            &format!("{ROOT}/unlock.projection-lock-revision.{format}"),
            "--output",
            unlocked.to_str().unwrap(),
            "--format",
            format,
        ]));
        assert_eq!(
            fs::read(&unlocked).unwrap(),
            fs::read(
                repository_root().join(format!("{ROOT}/unlocked.character-collection.{format}"))
            )
            .unwrap()
        );
    }

    let inspected = run(&[
        "projection-inspect",
        &format!("{ROOT}/proposal.projection-proposal.json"),
        "org.weave.character.ari_vale",
        "org.weave.projection.glasswind_lenses.narrative_role",
    ]);
    assert_success(&inspected);
    let value: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(value["character_id"], "org.weave.character.ari_vale");
    assert_eq!(value["disposition"], "proposed");
    assert!(
        value["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|candidate| {
                !candidate["seeded_sha256"].as_str().unwrap().is_empty()
                    && !candidate["ordered_evidence"].as_array().unwrap().is_empty()
                    && !candidate["explanation"].as_str().unwrap().is_empty()
            })
    );
}
