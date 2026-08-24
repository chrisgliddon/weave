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
const CONTEXT_INPUT: &str = "examples/domain-modules/weave-character/context/input.character.json";
const CONTEXT_APOLLO_PACK: &str =
    "examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json";
const CONTEXT_CALENDAR_PACK: &str =
    "examples/domain-modules/weave-character/context/calendar.temporal-pack.json";
const CONTEXT_WORLD_PACK: &str =
    "examples/domain-modules/weave-character/context/world.temporal-pack.json";
const CONTEXT_CONFIG: &str =
    "examples/domain-modules/weave-character/context/ranking.temporal-config.json";
const CONTEXT_PROPOSAL: &str =
    "examples/domain-modules/weave-character/context/proposal.temporal-proposal.json";
const CONTEXT_DECISIONS: &str =
    "examples/domain-modules/weave-character/context/decisions.temporal-review.json";
const CONTEXT_REVIEW: &str =
    "examples/domain-modules/weave-character/context/review.temporal-review.json";
const CONTEXT_RECEIPT: &str =
    "examples/domain-modules/weave-character/context/receipt.temporal-receipt.json";
const ALIGNMENT_INPUT: &str =
    "examples/domain-modules/weave-character/alignment/input.character.json";
const ALIGNMENT_PACK: &str =
    "examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json";
const ALIGNMENT_CONFIG: &str =
    "examples/domain-modules/weave-character/alignment/selection.alignment-config.json";
const ALIGNMENT_PROPOSAL: &str =
    "examples/domain-modules/weave-character/alignment/proposal.alignment-proposal.json";
const ALIGNMENT_DECISIONS: &str =
    "examples/domain-modules/weave-character/alignment/decisions.alignment-review.json";
const ALIGNMENT_REVIEW: &str =
    "examples/domain-modules/weave-character/alignment/review.alignment-review.json";
const ALIGNMENT_RECEIPT: &str =
    "examples/domain-modules/weave-character/alignment/receipt.alignment-receipt.json";
const PRESENTATION_INPUT_JSON: &str =
    "examples/domain-modules/weave-character/presentation/input.character-collection.json";
const PRESENTATION_INPUT_RON: &str =
    "examples/domain-modules/weave-character/presentation/input.character-collection.ron";
const PRESENTATION_CATALOG_JSON: &str =
    "examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.json";
const PRESENTATION_CATALOG_RON: &str =
    "examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.ron";
const PRESENTATION_REQUEST_JSON: &str =
    "examples/domain-modules/weave-character/presentation/allocation.presentation-request.json";
const PRESENTATION_REQUEST_RON: &str =
    "examples/domain-modules/weave-character/presentation/allocation.presentation-request.ron";
const PRESENTATION_PROPOSAL_JSON: &str =
    "examples/domain-modules/weave-character/presentation/proposal.presentation-proposal.json";
const PRESENTATION_PROPOSAL_RON: &str =
    "examples/domain-modules/weave-character/presentation/proposal.presentation-proposal.ron";
const PRESENTATION_DECISIONS_JSON: &str =
    "examples/domain-modules/weave-character/presentation/decisions.presentation-review.json";
const PRESENTATION_DECISIONS_RON: &str =
    "examples/domain-modules/weave-character/presentation/decisions.presentation-review.ron";
const PRESENTATION_REVIEW_JSON: &str =
    "examples/domain-modules/weave-character/presentation/review.presentation-review.json";
const PRESENTATION_REVIEW_RON: &str =
    "examples/domain-modules/weave-character/presentation/review.presentation-review.ron";
const PRESENTATION_RECEIPT_JSON: &str =
    "examples/domain-modules/weave-character/presentation/receipt.presentation-receipt.json";
const PRESENTATION_RECEIPT_RON: &str =
    "examples/domain-modules/weave-character/presentation/receipt.presentation-receipt.ron";
const PRESENTATION_APPLIED_JSON: &str =
    "examples/domain-modules/weave-character/presentation/applied.character-collection.json";
const PRESENTATION_APPLIED_RON: &str =
    "examples/domain-modules/weave-character/presentation/applied.character-collection.ron";
const PRESENTATION_LOCK_REVISION_JSON: &str = "examples/domain-modules/weave-character/presentation/\
unlock-avatar.presentation-lock-revision.json";
const PRESENTATION_UNLOCKED_JSON: &str =
    "examples/domain-modules/weave-character/presentation/unlocked.character-collection.json";
const RELATIONSHIP_BLANK_JSON: &str =
    "examples/domain-modules/weave-character/relationships/blank.character-collection.json";
const RELATIONSHIP_BLANK_RON: &str =
    "examples/domain-modules/weave-character/relationships/blank.character-collection.ron";
const RELATIONSHIP_PACK_JSON: &str =
    "examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json";
const RELATIONSHIP_PACK_RON: &str =
    "examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.ron";
const RELATIONSHIP_POLICY_JSON: &str =
    "examples/domain-modules/weave-character/relationships/project.relationship-policy.json";
const RELATIONSHIP_REVISION_JSON: &str =
    "examples/domain-modules/weave-character/relationships/authored.relationship-revision.json";
const RELATIONSHIP_REVISION_RON: &str =
    "examples/domain-modules/weave-character/relationships/authored.relationship-revision.ron";
const RELATIONSHIP_INPUT_JSON: &str =
    "examples/domain-modules/weave-character/relationships/input.character-collection.json";
const RELATIONSHIP_INPUT_RON: &str =
    "examples/domain-modules/weave-character/relationships/input.character-collection.ron";
const RELATIONSHIP_CONFIG_JSON: &str =
    "examples/domain-modules/weave-character/relationships/scoring.relationship-config.json";
const RELATIONSHIP_CONFIG_RON: &str =
    "examples/domain-modules/weave-character/relationships/scoring.relationship-config.ron";
const RELATIONSHIP_PROPOSAL_JSON: &str =
    "examples/domain-modules/weave-character/relationships/proposal.relationship-proposal.json";
const RELATIONSHIP_PROPOSAL_RON: &str =
    "examples/domain-modules/weave-character/relationships/proposal.relationship-proposal.ron";
const RELATIONSHIP_DECISIONS_JSON: &str =
    "examples/domain-modules/weave-character/relationships/decisions.relationship-review.json";
const RELATIONSHIP_DECISIONS_RON: &str =
    "examples/domain-modules/weave-character/relationships/decisions.relationship-review.ron";
const RELATIONSHIP_REVIEW_JSON: &str =
    "examples/domain-modules/weave-character/relationships/review.relationship-review.json";
const RELATIONSHIP_REVIEW_RON: &str =
    "examples/domain-modules/weave-character/relationships/review.relationship-review.ron";
const RELATIONSHIP_RECEIPT_JSON: &str =
    "examples/domain-modules/weave-character/relationships/receipt.relationship-receipt.json";
const RELATIONSHIP_RECEIPT_RON: &str =
    "examples/domain-modules/weave-character/relationships/receipt.relationship-receipt.ron";
const RELATIONSHIP_APPLIED_JSON: &str =
    "examples/domain-modules/weave-character/relationships/applied.character-collection.json";
const RELATIONSHIP_APPLIED_RON: &str =
    "examples/domain-modules/weave-character/relationships/applied.character-collection.ron";
const RELATIONSHIP_CONFLICTED_JSON: &str =
    "examples/domain-modules/weave-character/relationships/conflicted.character-collection.json";
const RELATIONSHIP_RECONCILIATION_JSON: &str = "examples/domain-modules/weave-character/relationships/\
reconciliation.relationship-reconciliation.json";

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
        (
            "temporal-pack",
            CONTEXT_APOLLO_PACK,
            "schemas/weave-character-temporal-pack-v1.schema.json",
        ),
        (
            "temporal-config",
            CONTEXT_CONFIG,
            "schemas/weave-character-temporal-config-v1.schema.json",
        ),
        (
            "temporal-proposal",
            CONTEXT_PROPOSAL,
            "schemas/weave-character-temporal-proposal-v1.schema.json",
        ),
        (
            "temporal-review",
            CONTEXT_REVIEW,
            "schemas/weave-character-temporal-review-v1.schema.json",
        ),
        (
            "temporal-receipt",
            CONTEXT_RECEIPT,
            "schemas/weave-character-temporal-receipt-v1.schema.json",
        ),
        (
            "alignment-pack",
            ALIGNMENT_PACK,
            "schemas/weave-character-alignment-pack-v1.schema.json",
        ),
        (
            "alignment-config",
            ALIGNMENT_CONFIG,
            "schemas/weave-character-alignment-config-v1.schema.json",
        ),
        (
            "alignment-proposal",
            ALIGNMENT_PROPOSAL,
            "schemas/weave-character-alignment-proposal-v1.schema.json",
        ),
        (
            "alignment-review",
            ALIGNMENT_REVIEW,
            "schemas/weave-character-alignment-review-v1.schema.json",
        ),
        (
            "alignment-receipt",
            ALIGNMENT_RECEIPT,
            "schemas/weave-character-alignment-receipt-v1.schema.json",
        ),
        (
            "relationship-kind-pack",
            RELATIONSHIP_PACK_JSON,
            "schemas/weave-character-relationship-kind-pack-v1.schema.json",
        ),
        (
            "relationship-policy",
            RELATIONSHIP_POLICY_JSON,
            "schemas/weave-character-relationship-policy-v1.schema.json",
        ),
        (
            "relationship-config",
            RELATIONSHIP_CONFIG_JSON,
            "schemas/weave-character-relationship-config-v1.schema.json",
        ),
        (
            "relationship-proposal",
            RELATIONSHIP_PROPOSAL_JSON,
            "schemas/weave-character-relationship-proposal-v1.schema.json",
        ),
        (
            "relationship-review",
            RELATIONSHIP_REVIEW_JSON,
            "schemas/weave-character-relationship-review-v1.schema.json",
        ),
        (
            "relationship-receipt",
            RELATIONSHIP_RECEIPT_JSON,
            "schemas/weave-character-relationship-receipt-v1.schema.json",
        ),
        (
            "relationship-revision",
            RELATIONSHIP_REVISION_JSON,
            "schemas/weave-character-relationship-revision-v1.schema.json",
        ),
        (
            "relationship-reconciliation",
            RELATIONSHIP_RECONCILIATION_JSON,
            "schemas/weave-character-relationship-reconciliation-v1.schema.json",
        ),
        (
            "presentation-catalog",
            PRESENTATION_CATALOG_JSON,
            "schemas/weave-character-presentation-catalog-v1.schema.json",
        ),
        (
            "presentation-request",
            PRESENTATION_REQUEST_JSON,
            "schemas/weave-character-presentation-request-v1.schema.json",
        ),
        (
            "presentation-proposal",
            PRESENTATION_PROPOSAL_JSON,
            "schemas/weave-character-presentation-proposal-v1.schema.json",
        ),
        (
            "presentation-review",
            PRESENTATION_REVIEW_JSON,
            "schemas/weave-character-presentation-review-v1.schema.json",
        ),
        (
            "presentation-receipt",
            PRESENTATION_RECEIPT_JSON,
            "schemas/weave-character-presentation-receipt-v1.schema.json",
        ),
        (
            "presentation-lock-revision",
            PRESENTATION_LOCK_REVISION_JSON,
            "schemas/weave-character-presentation-lock-revision-v1.schema.json",
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

#[test]
fn temporal_propose_review_and_apply_match_shared_fixture_bytes() {
    let temporary = tempdir().expect("temporary output directory");
    let proposal = temporary.path().join("proposal.json");
    let proposed = run(&[
        "context-propose",
        CONTEXT_INPUT,
        "--pack",
        CONTEXT_WORLD_PACK,
        "--pack",
        CONTEXT_CALENDAR_PACK,
        "--pack",
        CONTEXT_APOLLO_PACK,
        "--config",
        CONTEXT_CONFIG,
        "--seed",
        "19690720",
        "--output",
        proposal.to_str().unwrap(),
    ]);
    assert!(
        proposed.status.success(),
        "{}",
        String::from_utf8_lossy(&proposed.stderr)
    );
    assert_eq!(
        fs::read(&proposal).unwrap(),
        fs::read(repository_root().join(CONTEXT_PROPOSAL)).unwrap()
    );

    let expected_review: serde_json::Value =
        serde_json::from_slice(&fs::read(repository_root().join(CONTEXT_REVIEW)).unwrap()).unwrap();
    let reviewer = expected_review["reviewer"].as_str().unwrap();
    let rationale = expected_review["rationale"].as_str().unwrap();
    let review = temporary.path().join("review.json");
    let reviewed = run(&[
        "context-review",
        proposal.to_str().unwrap(),
        CONTEXT_DECISIONS,
        "--reviewer",
        reviewer,
        "--rationale",
        rationale,
        "--output",
        review.to_str().unwrap(),
    ]);
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    assert_eq!(
        fs::read(&review).unwrap(),
        fs::read(repository_root().join(CONTEXT_REVIEW)).unwrap()
    );

    let receipt = temporary.path().join("receipt.json");
    let applied = run(&[
        "context-apply",
        CONTEXT_INPUT,
        "--pack",
        CONTEXT_APOLLO_PACK,
        "--pack",
        CONTEXT_CALENDAR_PACK,
        "--pack",
        CONTEXT_WORLD_PACK,
        proposal.to_str().unwrap(),
        review.to_str().unwrap(),
        "--output",
        receipt.to_str().unwrap(),
    ]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(
        fs::read(receipt).unwrap(),
        fs::read(repository_root().join(CONTEXT_RECEIPT)).unwrap()
    );
}

#[test]
fn temporal_dry_run_and_stale_inputs_never_write_partial_receipts() {
    let temporary = tempdir().expect("temporary output directory");
    let forbidden = temporary.path().join("dry-run-receipt.json");
    let dry_run = run(&[
        "context-apply",
        CONTEXT_INPUT,
        "--pack",
        CONTEXT_APOLLO_PACK,
        "--pack",
        CONTEXT_CALENDAR_PACK,
        "--pack",
        CONTEXT_WORLD_PACK,
        CONTEXT_PROPOSAL,
        CONTEXT_REVIEW,
        "--dry-run",
        "--output",
        forbidden.to_str().unwrap(),
    ]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!forbidden.exists());

    let stale_output = temporary.path().join("stale-receipt.json");
    let stale = run(&[
        "context-apply",
        CONTEXT_INPUT,
        "--pack",
        CONTEXT_APOLLO_PACK,
        "--pack",
        CONTEXT_CALENDAR_PACK,
        "--pack",
        CONTEXT_WORLD_PACK,
        "examples/domain-modules/weave-character/invalid/stale-temporal-proposal.json",
        CONTEXT_REVIEW,
        "--output",
        stale_output.to_str().unwrap(),
    ]);
    assert!(!stale.status.success());
    assert!(!stale_output.exists());
    assert!(!String::from_utf8_lossy(&stale.stderr).contains("Ari Vale"));
}

#[test]
fn alignment_propose_review_and_apply_match_shared_fixture_bytes() {
    let temporary = tempdir().expect("temporary output directory");
    let proposal = temporary.path().join("proposal.json");
    let proposed = run(&[
        "alignment-propose",
        ALIGNMENT_INPUT,
        "--pack",
        ALIGNMENT_PACK,
        "--config",
        ALIGNMENT_CONFIG,
        "--seed",
        "20260822",
        "--output",
        proposal.to_str().unwrap(),
    ]);
    assert!(
        proposed.status.success(),
        "{}",
        String::from_utf8_lossy(&proposed.stderr)
    );
    assert_eq!(
        fs::read(&proposal).unwrap(),
        fs::read(repository_root().join(ALIGNMENT_PROPOSAL)).unwrap()
    );

    let expected_review: serde_json::Value =
        serde_json::from_slice(&fs::read(repository_root().join(ALIGNMENT_REVIEW)).unwrap())
            .unwrap();
    let review = temporary.path().join("review.json");
    let reviewed = run(&[
        "alignment-review",
        proposal.to_str().unwrap(),
        "--pack",
        ALIGNMENT_PACK,
        ALIGNMENT_DECISIONS,
        "--reviewer",
        expected_review["reviewer"].as_str().unwrap(),
        "--rationale",
        expected_review["rationale"].as_str().unwrap(),
        "--output",
        review.to_str().unwrap(),
    ]);
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    assert_eq!(
        fs::read(&review).unwrap(),
        fs::read(repository_root().join(ALIGNMENT_REVIEW)).unwrap()
    );

    let receipt = temporary.path().join("receipt.json");
    let applied = run(&[
        "alignment-apply",
        ALIGNMENT_INPUT,
        "--pack",
        ALIGNMENT_PACK,
        proposal.to_str().unwrap(),
        review.to_str().unwrap(),
        "--output",
        receipt.to_str().unwrap(),
    ]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(
        fs::read(receipt).unwrap(),
        fs::read(repository_root().join(ALIGNMENT_RECEIPT)).unwrap()
    );
}

#[test]
fn alignment_dry_run_and_stale_inputs_never_write_partial_receipts() {
    let temporary = tempdir().expect("temporary output directory");
    let forbidden = temporary.path().join("dry-run-receipt.json");
    let dry_run = run(&[
        "alignment-apply",
        ALIGNMENT_INPUT,
        "--pack",
        ALIGNMENT_PACK,
        ALIGNMENT_PROPOSAL,
        ALIGNMENT_REVIEW,
        "--dry-run",
        "--output",
        forbidden.to_str().unwrap(),
    ]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!forbidden.exists());

    let stale_output = temporary.path().join("stale-receipt.json");
    let stale = run(&[
        "alignment-apply",
        ALIGNMENT_INPUT,
        "--pack",
        ALIGNMENT_PACK,
        "examples/domain-modules/weave-character/invalid/stale-alignment-proposal.json",
        ALIGNMENT_REVIEW,
        "--output",
        stale_output.to_str().unwrap(),
    ]);
    assert!(!stale.status.success());
    assert!(!stale_output.exists());
    assert!(!String::from_utf8_lossy(&stale.stderr).contains("Ari Vale"));
}

#[test]
fn presentation_propose_review_and_apply_match_shared_fixture_bytes() {
    let temporary = tempdir().expect("temporary output directory");
    let expected_review: serde_json::Value = serde_json::from_slice(
        &fs::read(repository_root().join(PRESENTATION_REVIEW_JSON)).unwrap(),
    )
    .unwrap();

    for (
        format,
        collection,
        catalog,
        request,
        decisions,
        checked_proposal,
        checked_review,
        checked_receipt,
        checked_collection,
    ) in [
        (
            "json",
            PRESENTATION_INPUT_JSON,
            PRESENTATION_CATALOG_JSON,
            PRESENTATION_REQUEST_JSON,
            PRESENTATION_DECISIONS_JSON,
            PRESENTATION_PROPOSAL_JSON,
            PRESENTATION_REVIEW_JSON,
            PRESENTATION_RECEIPT_JSON,
            PRESENTATION_APPLIED_JSON,
        ),
        (
            "ron",
            PRESENTATION_INPUT_RON,
            PRESENTATION_CATALOG_RON,
            PRESENTATION_REQUEST_RON,
            PRESENTATION_DECISIONS_RON,
            PRESENTATION_PROPOSAL_RON,
            PRESENTATION_REVIEW_RON,
            PRESENTATION_RECEIPT_RON,
            PRESENTATION_APPLIED_RON,
        ),
    ] {
        let proposal = temporary.path().join(format!("proposal.{format}"));
        let proposed = run(&[
            "presentation-propose",
            collection,
            catalog,
            request,
            "--format",
            format,
            "--output",
            proposal.to_str().unwrap(),
        ]);
        assert!(
            proposed.status.success(),
            "{}",
            String::from_utf8_lossy(&proposed.stderr)
        );
        assert_eq!(
            fs::read(&proposal).unwrap(),
            fs::read(repository_root().join(checked_proposal)).unwrap()
        );

        let review = temporary.path().join(format!("review.{format}"));
        let reviewed = run(&[
            "presentation-review",
            proposal.to_str().unwrap(),
            decisions,
            "--reviewer",
            expected_review["reviewer"].as_str().unwrap(),
            "--rationale",
            expected_review["rationale"].as_str().unwrap(),
            "--format",
            format,
            "--output",
            review.to_str().unwrap(),
        ]);
        assert!(
            reviewed.status.success(),
            "{}",
            String::from_utf8_lossy(&reviewed.stderr)
        );
        assert_eq!(
            fs::read(&review).unwrap(),
            fs::read(repository_root().join(checked_review)).unwrap()
        );

        let receipt = temporary.path().join(format!("receipt.{format}"));
        let applied_collection = temporary.path().join(format!("applied.{format}"));
        let applied = run(&[
            "presentation-apply",
            collection,
            proposal.to_str().unwrap(),
            review.to_str().unwrap(),
            "--receipt-output",
            receipt.to_str().unwrap(),
            "--collection-output",
            applied_collection.to_str().unwrap(),
            "--format",
            format,
        ]);
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        assert_eq!(
            fs::read(&receipt).unwrap(),
            fs::read(repository_root().join(checked_receipt)).unwrap()
        );
        assert_eq!(
            fs::read(&applied_collection).unwrap(),
            fs::read(repository_root().join(checked_collection)).unwrap()
        );
    }
}

#[test]
fn presentation_dry_run_and_lock_revision_preserve_atomicity() {
    let temporary = tempdir().expect("temporary output directory");
    let copied_input = temporary.path().join("input.character-collection.json");
    fs::copy(
        repository_root().join(PRESENTATION_INPUT_JSON),
        &copied_input,
    )
    .unwrap();
    let original = fs::read(&copied_input).unwrap();
    let receipt = temporary.path().join("dry-run.presentation-receipt.json");
    let forbidden_collection = temporary.path().join("forbidden.character-collection.json");
    let dry_run = run(&[
        "presentation-apply",
        copied_input.to_str().unwrap(),
        PRESENTATION_PROPOSAL_JSON,
        PRESENTATION_REVIEW_JSON,
        "--dry-run",
        "--receipt-output",
        receipt.to_str().unwrap(),
        "--collection-output",
        forbidden_collection.to_str().unwrap(),
    ]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert_eq!(fs::read(&copied_input).unwrap(), original);
    assert!(!forbidden_collection.exists());
    assert_eq!(
        fs::read(&receipt).unwrap(),
        fs::read(repository_root().join(PRESENTATION_RECEIPT_JSON)).unwrap()
    );

    let copied_applied = temporary.path().join("applied.character-collection.json");
    fs::copy(
        repository_root().join(PRESENTATION_APPLIED_JSON),
        &copied_applied,
    )
    .unwrap();
    let locked_original = fs::read(&copied_applied).unwrap();
    let forbidden_lock_output = temporary.path().join("forbidden-lock-output.json");
    let lock_dry_run = run(&[
        "presentation-lock",
        copied_applied.to_str().unwrap(),
        PRESENTATION_LOCK_REVISION_JSON,
        "--dry-run",
        "--output",
        forbidden_lock_output.to_str().unwrap(),
    ]);
    assert!(
        lock_dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&lock_dry_run.stderr)
    );
    assert_eq!(fs::read(&copied_applied).unwrap(), locked_original);
    assert!(!forbidden_lock_output.exists());

    let unlocked_output = temporary.path().join("unlocked.character-collection.json");
    let lock_apply = run(&[
        "presentation-lock",
        copied_applied.to_str().unwrap(),
        PRESENTATION_LOCK_REVISION_JSON,
        "--output",
        unlocked_output.to_str().unwrap(),
    ]);
    assert!(
        lock_apply.status.success(),
        "{}",
        String::from_utf8_lossy(&lock_apply.stderr)
    );
    assert_eq!(
        fs::read(unlocked_output).unwrap(),
        fs::read(repository_root().join(PRESENTATION_UNLOCKED_JSON)).unwrap()
    );
}

#[test]
fn invalid_presentation_inputs_fail_before_any_output() {
    let temporary = tempdir().expect("temporary output directory");
    let attempts = [
        vec![
            "presentation-propose",
            PRESENTATION_INPUT_JSON,
            PRESENTATION_CATALOG_JSON,
            "examples/domain-modules/weave-character/invalid/missing-presentation-asset.presentation-request.json",
        ],
        vec![
            "presentation-review",
            PRESENTATION_PROPOSAL_JSON,
            "examples/domain-modules/weave-character/invalid/incompatible-presentation-override.json",
            "--reviewer",
            "org.weave.reviewer.invalid_fixture",
            "--rationale",
            "Exercise the incompatible presentation override boundary.",
        ],
        vec![
            "presentation-apply",
            PRESENTATION_INPUT_JSON,
            "examples/domain-modules/weave-character/invalid/stale-presentation-proposal.json",
            PRESENTATION_REVIEW_JSON,
            "--receipt-output",
        ],
    ];
    for (index, mut arguments) in attempts.into_iter().enumerate() {
        let output = temporary.path().join(format!("forbidden-{index}.json"));
        if arguments.last() == Some(&"--receipt-output") {
            arguments.push(output.to_str().unwrap());
        } else {
            arguments.extend(["--output", output.to_str().unwrap()]);
        }
        let result = run(&arguments);
        assert!(!result.status.success());
        assert!(!output.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("Ari Vale"));
    }

    for (kind, path) in [
        (
            "presentation-catalog",
            "examples/domain-modules/weave-character/invalid/invalid-palette-slot.presentation-catalog.json",
        ),
        (
            "profile",
            "examples/domain-modules/weave-character/invalid/duplicate-alias.character.json",
        ),
    ] {
        let result = run(&["validate", kind, path]);
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("Ari Vale"));
    }
}

#[test]
fn relationship_revision_list_inspect_validate_and_csv_share_one_graph_contract() {
    let temporary = tempdir().expect("temporary output directory");
    for (format, collection, pack, revision, expected) in [
        (
            "json",
            RELATIONSHIP_BLANK_JSON,
            RELATIONSHIP_PACK_JSON,
            RELATIONSHIP_REVISION_JSON,
            RELATIONSHIP_INPUT_JSON,
        ),
        (
            "ron",
            RELATIONSHIP_BLANK_RON,
            RELATIONSHIP_PACK_RON,
            RELATIONSHIP_REVISION_RON,
            RELATIONSHIP_INPUT_RON,
        ),
    ] {
        let output_path = temporary.path().join(format!("revised.{format}"));
        let revised = run(&[
            "relationship-revise",
            collection,
            pack,
            revision,
            "--format",
            format,
            "--output",
            output_path.to_str().unwrap(),
        ]);
        assert!(
            revised.status.success(),
            "{}",
            String::from_utf8_lossy(&revised.stderr)
        );
        assert_eq!(
            fs::read(output_path).unwrap(),
            fs::read(repository_root().join(expected)).unwrap()
        );
    }

    let validated = run(&[
        "relationship-validate",
        RELATIONSHIP_INPUT_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
    ]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let listed = run(&[
        "relationship-list",
        RELATIONSHIP_INPUT_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
        "--origin",
        "imported",
    ]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let values: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(values.as_array().unwrap().len(), 1);
    assert_eq!(values[0]["edge"]["id"], "mentor_ari_sable");
    assert_eq!(values[0]["directionality"]["mode"], "directed");

    let signed_date = run(&[
        "relationship-list",
        RELATIONSHIP_INPUT_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
        "--active-on=-0001-01-01",
    ]);
    assert!(
        signed_date.status.success(),
        "{}",
        String::from_utf8_lossy(&signed_date.stderr)
    );

    let inspected = run(&[
        "relationship-inspect",
        RELATIONSHIP_INPUT_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
        "org.weave.character.ari_vale",
        "mentor_ari_sable",
    ]);
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(value["family"], "mentorship");
    assert_eq!(value["edge"]["origin"], "imported");

    for (format, checked) in [
        (
            "edge-csv",
            "examples/domain-modules/weave-character/relationships/edges.review.csv",
        ),
        (
            "matrix-csv",
            "examples/domain-modules/weave-character/relationships/matrix.review.csv",
        ),
    ] {
        let output_path = temporary.path().join(format!("{format}.csv"));
        let exported = run(&[
            "relationship-list",
            RELATIONSHIP_APPLIED_JSON,
            RELATIONSHIP_PACK_JSON,
            RELATIONSHIP_POLICY_JSON,
            "--format",
            format,
            "--output",
            output_path.to_str().unwrap(),
        ]);
        assert!(
            exported.status.success(),
            "{}",
            String::from_utf8_lossy(&exported.stderr)
        );
        assert_eq!(
            fs::read(output_path).unwrap(),
            fs::read(repository_root().join(checked)).unwrap()
        );
    }
}

#[test]
fn relationship_propose_review_and_atomic_apply_match_fixture_bytes() {
    let temporary = tempdir().expect("temporary output directory");
    let expected_review: serde_json::Value = serde_json::from_slice(
        &fs::read(repository_root().join(RELATIONSHIP_REVIEW_JSON)).unwrap(),
    )
    .unwrap();
    for (
        format,
        collection,
        pack,
        config,
        decisions,
        checked_proposal,
        checked_review,
        checked_receipt,
        checked_collection,
    ) in [
        (
            "json",
            RELATIONSHIP_INPUT_JSON,
            RELATIONSHIP_PACK_JSON,
            RELATIONSHIP_CONFIG_JSON,
            RELATIONSHIP_DECISIONS_JSON,
            RELATIONSHIP_PROPOSAL_JSON,
            RELATIONSHIP_REVIEW_JSON,
            RELATIONSHIP_RECEIPT_JSON,
            RELATIONSHIP_APPLIED_JSON,
        ),
        (
            "ron",
            RELATIONSHIP_INPUT_RON,
            RELATIONSHIP_PACK_RON,
            RELATIONSHIP_CONFIG_RON,
            RELATIONSHIP_DECISIONS_RON,
            RELATIONSHIP_PROPOSAL_RON,
            RELATIONSHIP_REVIEW_RON,
            RELATIONSHIP_RECEIPT_RON,
            RELATIONSHIP_APPLIED_RON,
        ),
    ] {
        let proposal = temporary.path().join(format!("proposal.{format}"));
        let proposed = run(&[
            "relationship-propose",
            collection,
            pack,
            config,
            "--format",
            format,
            "--output",
            proposal.to_str().unwrap(),
        ]);
        assert!(
            proposed.status.success(),
            "{}",
            String::from_utf8_lossy(&proposed.stderr)
        );
        assert_eq!(
            fs::read(&proposal).unwrap(),
            fs::read(repository_root().join(checked_proposal)).unwrap()
        );

        let review = temporary.path().join(format!("review.{format}"));
        let reviewed = run(&[
            "relationship-review",
            proposal.to_str().unwrap(),
            decisions,
            "--reviewer",
            expected_review["reviewer"].as_str().unwrap(),
            "--rationale",
            expected_review["rationale"].as_str().unwrap(),
            "--format",
            format,
            "--output",
            review.to_str().unwrap(),
        ]);
        assert!(
            reviewed.status.success(),
            "{}",
            String::from_utf8_lossy(&reviewed.stderr)
        );
        assert_eq!(
            fs::read(&review).unwrap(),
            fs::read(repository_root().join(checked_review)).unwrap()
        );

        let receipt = temporary.path().join(format!("receipt.{format}"));
        let applied_collection = temporary.path().join(format!("applied.{format}"));
        let applied = run(&[
            "relationship-apply",
            collection,
            proposal.to_str().unwrap(),
            review.to_str().unwrap(),
            "--receipt-output",
            receipt.to_str().unwrap(),
            "--collection-output",
            applied_collection.to_str().unwrap(),
            "--format",
            format,
        ]);
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        assert_eq!(
            fs::read(receipt).unwrap(),
            fs::read(repository_root().join(checked_receipt)).unwrap()
        );
        assert_eq!(
            fs::read(applied_collection).unwrap(),
            fs::read(repository_root().join(checked_collection)).unwrap()
        );
    }

    let dry_receipt = temporary.path().join("forbidden-receipt.json");
    let dry_collection = temporary.path().join("forbidden-collection.json");
    let dry_run = run(&[
        "relationship-apply",
        RELATIONSHIP_INPUT_JSON,
        RELATIONSHIP_PROPOSAL_JSON,
        RELATIONSHIP_REVIEW_JSON,
        "--dry-run",
        "--receipt-output",
        dry_receipt.to_str().unwrap(),
        "--collection-output",
        dry_collection.to_str().unwrap(),
    ]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!dry_receipt.exists());
    assert!(!dry_collection.exists());
}

#[test]
fn relationship_reconciliation_and_invalid_inputs_are_redaction_safe() {
    let temporary = tempdir().expect("temporary output directory");
    let report = temporary.path().join("reconciliation.json");
    let reconciled = run(&[
        "relationship-reconcile",
        RELATIONSHIP_CONFLICTED_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
        "--output",
        report.to_str().unwrap(),
    ]);
    assert!(
        reconciled.status.success(),
        "{}",
        String::from_utf8_lossy(&reconciled.stderr)
    );
    assert_eq!(
        fs::read(report).unwrap(),
        fs::read(repository_root().join(RELATIONSHIP_RECONCILIATION_JSON)).unwrap()
    );

    let invalid_graph = run(&[
        "relationship-validate",
        RELATIONSHIP_CONFLICTED_JSON,
        RELATIONSHIP_PACK_JSON,
        RELATIONSHIP_POLICY_JSON,
    ]);
    assert!(!invalid_graph.status.success());
    let graph_stderr = String::from_utf8_lossy(&invalid_graph.stderr);
    assert!(graph_stderr.contains("R106"));
    assert!(!graph_stderr.contains("Ari Vale"));

    for (proposal, review, stem) in [
        (
            "examples/domain-modules/weave-character/invalid/stale-relationship-proposal.json",
            RELATIONSHIP_REVIEW_JSON,
            "stale",
        ),
        (
            RELATIONSHIP_PROPOSAL_JSON,
            "examples/domain-modules/weave-character/invalid/incomplete-relationship-review.json",
            "incomplete",
        ),
    ] {
        let receipt = temporary.path().join(format!("{stem}-receipt.json"));
        let collection = temporary.path().join(format!("{stem}-collection.json"));
        let result = run(&[
            "relationship-apply",
            RELATIONSHIP_INPUT_JSON,
            proposal,
            review,
            "--receipt-output",
            receipt.to_str().unwrap(),
            "--collection-output",
            collection.to_str().unwrap(),
        ]);
        assert!(!result.status.success());
        assert!(!receipt.exists());
        assert!(!collection.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("Ari Vale"));
    }
}
