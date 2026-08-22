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
const COLLECTION_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/collection.character-collection.json";
const COLLECTION_RON_PATH: &str =
    "examples/domain-modules/weave-character/operations/collection.character-collection.ron";
const REQUEST_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-request.json";
const REQUEST_RON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-request.ron";
const PROPOSAL_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-proposal.json";
const PROPOSAL_RON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-proposal.ron";
const REVIEW_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-review.json";
const REVIEW_RON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-review.ron";
const PROGRESS_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/rename.character-progress.json";
const RENAMED_JSON_PATH: &str =
    "examples/domain-modules/weave-character/operations/renamed.character-collection.json";
const RENAMED_RON_PATH: &str =
    "examples/domain-modules/weave-character/operations/renamed.character-collection.ron";

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

#[test]
fn validates_and_writes_every_collection_contract_schema_exactly() {
    let temporary = tempdir().expect("temporary output directory");
    for (kind, input, schema) in [
        (
            "collection",
            COLLECTION_JSON_PATH,
            "schemas/weave-character-collection-v1.schema.json",
        ),
        (
            "request",
            REQUEST_JSON_PATH,
            "schemas/weave-character-operation-request-v1.schema.json",
        ),
        (
            "proposal",
            PROPOSAL_JSON_PATH,
            "schemas/weave-character-proposal-v1.schema.json",
        ),
        (
            "review",
            REVIEW_JSON_PATH,
            "schemas/weave-character-review-v1.schema.json",
        ),
        (
            "progress",
            PROGRESS_JSON_PATH,
            "schemas/weave-character-progress-v1.schema.json",
        ),
    ] {
        let validation = run(&["validate", kind, input]);
        assert!(
            validation.status.success(),
            "{}",
            String::from_utf8_lossy(&validation.stderr)
        );
        let output_path = temporary.path().join(format!("{kind}.schema.json"));
        let schema_output = run(&["schema", kind, "--output", output_path.to_str().unwrap()]);
        assert!(
            schema_output.status.success(),
            "{}",
            String::from_utf8_lossy(&schema_output.stderr)
        );
        assert_eq!(
            fs::read(&output_path).unwrap(),
            fs::read(repository_root().join(schema)).unwrap()
        );
    }
}

#[test]
fn list_show_propose_and_review_match_the_shared_contract() {
    let list = run(&[
        "collection-list",
        COLLECTION_JSON_PATH,
        "--id-prefix",
        "org.weave.character.s",
        "--extension-namespace",
        "org.weave.character.relationships",
    ]);
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let summaries: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(summaries.as_array().unwrap().len(), 1);
    assert_eq!(summaries[0]["id"], "org.weave.character.sable_reed");

    let temporary = tempdir().expect("temporary output directory");
    let shown = temporary.path().join("ari.character.json");
    let show = run(&[
        "collection-show",
        COLLECTION_JSON_PATH,
        "org.weave.character.ari_vale",
        "--output",
        shown.to_str().unwrap(),
    ]);
    assert!(
        show.status.success(),
        "{}",
        String::from_utf8_lossy(&show.stderr)
    );
    assert_eq!(
        fs::read(shown).unwrap(),
        fs::read(repository_root().join(PROFILE)).unwrap()
    );

    for (format, collection, request, checked_proposal) in [
        (
            "json",
            COLLECTION_JSON_PATH,
            REQUEST_JSON_PATH,
            PROPOSAL_JSON_PATH,
        ),
        (
            "ron",
            COLLECTION_RON_PATH,
            REQUEST_RON_PATH,
            PROPOSAL_RON_PATH,
        ),
    ] {
        let proposal = temporary.path().join(format!("proposal.{format}"));
        let output = run(&[
            "collection-propose",
            collection,
            request,
            "--format",
            format,
            "--output",
            proposal.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(proposal).unwrap(),
            fs::read(repository_root().join(checked_proposal)).unwrap()
        );
    }

    let review = temporary.path().join("review.json");
    let output = run(&[
        "collection-review",
        PROPOSAL_JSON_PATH,
        "--decision",
        "accepted",
        "--reviewer",
        "org.weave.reviewer.fixture",
        "--rationale",
        "Approve the complete synthetic reference-safe rename.",
        "--output",
        review.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(review).unwrap(),
        fs::read(repository_root().join(REVIEW_JSON_PATH)).unwrap()
    );
}

#[test]
fn resume_and_atomic_apply_are_deterministic_and_dry_run_writes_nothing() {
    let temporary = tempdir().expect("temporary output directory");
    let progress = temporary.path().join("progress.json");
    let first = run(&[
        "collection-resume",
        COLLECTION_JSON_PATH,
        REQUEST_JSON_PATH,
        "--max-items",
        "1",
        "--progress-output",
        progress.to_str().unwrap(),
    ]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        fs::read(&progress).unwrap(),
        fs::read(repository_root().join(PROGRESS_JSON_PATH)).unwrap()
    );

    let ready_progress = temporary.path().join("ready-progress.json");
    let proposal = temporary.path().join("proposal.json");
    let second = run(&[
        "collection-resume",
        COLLECTION_JSON_PATH,
        REQUEST_JSON_PATH,
        "--progress",
        progress.to_str().unwrap(),
        "--max-items",
        "1",
        "--progress-output",
        ready_progress.to_str().unwrap(),
        "--proposal-output",
        proposal.to_str().unwrap(),
    ]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        fs::read(proposal).unwrap(),
        fs::read(repository_root().join(PROPOSAL_JSON_PATH)).unwrap()
    );
    let ready: serde_json::Value =
        serde_json::from_slice(&fs::read(ready_progress).unwrap()).unwrap();
    assert_eq!(ready["state"], "ready_for_review");

    let copied_collection = temporary.path().join("collection.json");
    fs::copy(
        repository_root().join(COLLECTION_JSON_PATH),
        &copied_collection,
    )
    .unwrap();
    let original = fs::read(&copied_collection).unwrap();
    let forbidden_output = temporary.path().join("dry-run-output.json");
    let dry_run = run(&[
        "collection-apply",
        copied_collection.to_str().unwrap(),
        PROPOSAL_JSON_PATH,
        REVIEW_JSON_PATH,
        "--dry-run",
        "--output",
        forbidden_output.to_str().unwrap(),
    ]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert_eq!(fs::read(&copied_collection).unwrap(), original);
    assert!(!forbidden_output.exists());

    for (format, collection, proposal, review, expected) in [
        (
            "json",
            COLLECTION_JSON_PATH,
            PROPOSAL_JSON_PATH,
            REVIEW_JSON_PATH,
            RENAMED_JSON_PATH,
        ),
        (
            "ron",
            COLLECTION_RON_PATH,
            PROPOSAL_RON_PATH,
            REVIEW_RON_PATH,
            RENAMED_RON_PATH,
        ),
    ] {
        let applied = temporary.path().join(format!("applied.{format}"));
        let output = run(&[
            "collection-apply",
            collection,
            proposal,
            review,
            "--format",
            format,
            "--output",
            applied.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(applied).unwrap(),
            fs::read(repository_root().join(expected)).unwrap()
        );
    }
}

#[test]
fn stale_or_malformed_collection_inputs_never_create_partial_outputs() {
    let temporary = tempdir().expect("temporary output directory");
    for (proposal, review, name) in [
        (
            PROPOSAL_JSON_PATH,
            "examples/domain-modules/weave-character/invalid/stale-character-review.json",
            "stale.json",
        ),
        (
            "examples/domain-modules/weave-character/invalid/malformed-character-proposal.json",
            REVIEW_JSON_PATH,
            "malformed.json",
        ),
    ] {
        let output_path = temporary.path().join(name);
        let output = run(&[
            "collection-apply",
            COLLECTION_JSON_PATH,
            proposal,
            review,
            "--output",
            output_path.to_str().unwrap(),
        ]);
        assert!(!output.status.success());
        assert!(!output_path.exists());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Ari Vale"));
    }
}
