use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/projection.rs"]
mod projection_support;

use weave_character::{
    CHARACTER_COLLECTION_FORMAT_VERSION, CharacterCollection, CharacterProfile, LockState,
    PROJECTION_CONFIG_FORMAT_VERSION, PROJECTION_LOCK_REVISION_FORMAT_VERSION,
    ProjectionAssignmentMode, ProjectionConfig, ProjectionLockRevision, ProjectionLockTarget,
    ProjectionProposal, ProjectionReservation, ProjectionReviewDecision,
    apply_projection_lock_revision, apply_projection_review, collection_fingerprint,
    create_projection_review, projection_config_schema, projection_lock_revision_schema,
    projection_pack_schema, projection_proposal_schema, projection_receipt_schema,
    projection_review_schema, propose_projections, validate_character_collection,
    validate_projection_pack,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--check".to_owned());
    let write = match mode.as_str() {
        "--write" => true,
        "--check" => false,
        _ => return Err("expected --write or --check".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root")?
        .to_path_buf();
    let fixture = root.join("examples/domain-modules/weave-character/projections");
    let schemas = root.join("schemas");

    let profile = CharacterProfile::from_json(&fs::read_to_string(
        root.join("examples/domain-modules/weave-character/omitted-extensions.character.json"),
    )?)?;
    let input = projection_collection(&profile)?;
    let pack = projection_support::reference_projection_pack();
    validate_projection_pack(&pack)?;
    let config = projection_config(&input, &pack);
    let proposal = propose_projections(&input, &pack, &config, 20_260_824)?;
    let review = create_projection_review(
        &proposal,
        "org.weave.reviewer.fixture",
        "Review every original Glasswind Lenses display and role independently; retain only explicit fictional editorial choices outside canon.",
        review_decisions(&proposal),
    )?;
    let receipt = apply_projection_review(&input, &proposal, &review)?;
    let lock_revision = ProjectionLockRevision {
        revision_format_version: PROJECTION_LOCK_REVISION_FORMAT_VERSION,
        id: "org.weave.character.projection.unlock_narrative_role".to_owned(),
        expected_input_sha256: collection_fingerprint(&receipt.output_collection)?,
        targets: vec![ProjectionLockTarget {
            character_id: "org.weave.character.ari_vale".to_owned(),
            projection_id: "narrative_role".to_owned(),
        }],
        lock: LockState::Unlocked,
        rationale: "Unlock the reviewed narrative role for a later explicit rebalance preview."
            .to_owned(),
        provenance: pack.provenance.clone(),
    };
    let unlocked = apply_projection_lock_revision(&receipt.output_collection, &lock_revision)?;
    let mut rebalance_config = config.clone();
    rebalance_config.id = "org.weave.character.projection.glasswind_rebalance".to_owned();
    rebalance_config.mode = ProjectionAssignmentMode::Rebalance;
    let rebalance = propose_projections(&unlocked, &pack, &rebalance_config, 20_260_825)?;

    write_pair(write, &fixture, "input.character-collection", &input)?;
    write_pair(write, &fixture, "glasswind.projection-pack", &pack)?;
    write_pair(write, &fixture, "selection.projection-config", &config)?;
    write_pair(write, &fixture, "proposal.projection-proposal", &proposal)?;
    write_pair(
        write,
        &fixture,
        "decisions.projection-review",
        &review.decisions,
    )?;
    write_pair(write, &fixture, "review.projection-review", &review)?;
    write_pair(write, &fixture, "receipt.projection-receipt", &receipt)?;
    write_pair(
        write,
        &fixture,
        "applied.character-collection",
        &receipt.output_collection,
    )?;
    write_pair(
        write,
        &fixture,
        "unlock.projection-lock-revision",
        &lock_revision,
    )?;
    write_pair(write, &fixture, "unlocked.character-collection", &unlocked)?;
    write_pair(
        write,
        &fixture,
        "rebalance.projection-config",
        &rebalance_config,
    )?;
    write_pair(write, &fixture, "rebalance.projection-proposal", &rebalance)?;

    write_text(
        write,
        &schemas.join("weave-character-projection-pack-v1.schema.json"),
        &projection_pack_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-projection-config-v1.schema.json"),
        &projection_config_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-projection-proposal-v1.schema.json"),
        &projection_proposal_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-projection-review-v1.schema.json"),
        &projection_review_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-projection-receipt-v1.schema.json"),
        &projection_receipt_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-projection-lock-revision-v1.schema.json"),
        &projection_lock_revision_schema()?,
    )?;
    Ok(())
}

fn projection_collection(
    source: &CharacterProfile,
) -> Result<CharacterCollection, Box<dyn std::error::Error>> {
    let mut characters = BTreeMap::new();
    for (id, display_name) in [
        ("org.weave.character.ari_vale", "Ari Vale"),
        ("org.weave.character.sable_reed", "Sable Reed"),
        ("org.weave.character.tavi_quill", "Tavi Quill"),
    ] {
        let mut profile = source.clone();
        profile.id = id.to_owned();
        profile.canon.identity.display_name.value = display_name.to_owned();
        profile.canon.identity.aliases = None;
        characters.insert(id.to_owned(), profile);
    }
    let collection = CharacterCollection {
        collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
        id: "org.weave.character.projection_fixture".to_owned(),
        revision: 0,
        characters,
    };
    validate_character_collection(&collection)?;
    Ok(collection)
}

fn projection_config(
    collection: &CharacterCollection,
    pack: &weave_character::ProjectionPack,
) -> ProjectionConfig {
    let capacity_overrides = pack
        .taxonomies
        .iter()
        .map(|(taxonomy_id, taxonomy)| {
            (
                taxonomy_id.clone(),
                taxonomy
                    .entries
                    .keys()
                    .map(|entry_id| (entry_id.clone(), 1))
                    .collect(),
            )
        })
        .collect();
    ProjectionConfig {
        config_format_version: PROJECTION_CONFIG_FORMAT_VERSION,
        id: "org.weave.character.projection.glasswind_selection".to_owned(),
        selected_taxonomies: projection_support::taxonomy_ids(),
        eligible_character_ids: collection.characters.keys().cloned().collect(),
        minimum_coverage_micros: 800_000,
        mode: ProjectionAssignmentMode::FillMissing,
        capacity_overrides,
        reservations: vec![ProjectionReservation {
            character_id: "org.weave.character.sable_reed".to_owned(),
            taxonomy_id: "org.weave.projection.glasswind_lenses.social_role".to_owned(),
            entry_id: "circle_anchor".to_owned(),
        }],
    }
}

fn review_decisions(
    proposal: &ProjectionProposal,
) -> BTreeMap<String, BTreeMap<String, ProjectionReviewDecision>> {
    let mut decisions = BTreeMap::new();
    for (character_id, taxonomies) in &proposal.review_manifest {
        let mut character_decisions = BTreeMap::new();
        for taxonomy_id in taxonomies {
            let decision = match (character_id.as_str(), taxonomy_id.rsplit('.').next()) {
                ("org.weave.character.ari_vale", Some("narrative_role")) => {
                    ProjectionReviewDecision::Accept {
                        lock: LockState::Locked,
                        rationale: Some(
                            "Retain this reviewed fictional story function across ordinary rebalance previews."
                                .to_owned(),
                        ),
                    }
                }
                ("org.weave.character.ari_vale", Some("social_role")) => {
                    ProjectionReviewDecision::Edit {
                        entry_id: proposed_entry(
                            proposal,
                            "org.weave.character.tavi_quill",
                            taxonomy_id,
                        ),
                        lock: LockState::Unlocked,
                        rationale: "Select another eligible group prompt after editorial review."
                            .to_owned(),
                    }
                }
                ("org.weave.character.ari_vale", Some("vocation")) => {
                    ProjectionReviewDecision::Reject {
                        rationale: "Do not publish a vocation prompt for this fixture character."
                            .to_owned(),
                    }
                }
                ("org.weave.character.sable_reed", Some("narrative_role")) => {
                    ProjectionReviewDecision::Override {
                        entry_id: "turning_compass".to_owned(),
                        label: "Turning Compass".to_owned(),
                        lock: LockState::Unlocked,
                        rationale: "Use an explicit original story function outside the ranked labels."
                            .to_owned(),
                    }
                }
                ("org.weave.character.sable_reed", Some("personality")) => {
                    ProjectionReviewDecision::Withhold {
                        rationale: "Keep the lossy categorical display out of the published profile."
                            .to_owned(),
                    }
                }
                ("org.weave.character.tavi_quill", Some("vocation")) => {
                    ProjectionReviewDecision::Edit {
                        entry_id: proposed_entry(
                            proposal,
                            "org.weave.character.ari_vale",
                            taxonomy_id,
                        ),
                        lock: LockState::Unlocked,
                        rationale: "Choose the alternate eligible vocation as an explicit editorial decision."
                            .to_owned(),
                    }
                }
                ("org.weave.character.tavi_quill", Some("social_role")) => {
                    ProjectionReviewDecision::Reject {
                        rationale: "Leave this group-role slot open after the edited allocation."
                            .to_owned(),
                    }
                }
                _ => ProjectionReviewDecision::Accept {
                    lock: LockState::Unlocked,
                    rationale: None,
                },
            };
            character_decisions.insert(taxonomy_id.clone(), decision);
        }
        decisions.insert(character_id.clone(), character_decisions);
    }
    decisions
}

fn proposed_entry(proposal: &ProjectionProposal, character_id: &str, taxonomy_id: &str) -> String {
    proposal.assignments[character_id][taxonomy_id]
        .proposed_entry_id
        .clone()
        .expect("reference fixture allocates every selected taxonomy")
}

fn write_pair<T>(
    write: bool,
    directory: &Path,
    stem: &str,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::Serialize,
{
    let json = weave_domain::to_pretty_json(value)?;
    let ron = weave_domain::to_pretty_ron(value)?;
    write_text(write, &directory.join(format!("{stem}.json")), &json)?;
    write_text(write, &directory.join(format!("{stem}.ron")), &ron)
}

fn write_text(write: bool, path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        return Ok(());
    }
    let existing = fs::read_to_string(path)?;
    if existing != content {
        return Err(format!("checked fixture is stale: {}", path.display()).into());
    }
    Ok(())
}
