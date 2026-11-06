//! Reference Bevy 0.18 consumer boundary for the Open Tale fixture.
//!
//! GAP: this adapter fixture tree is not a Cargo workspace member. The task that added it
//! excluded Cargo changes, so the workspace does not compile this file. To host it, add a Bevy
//! 0.18 crate that depends on `weave-tabletop` (and `serde_json` for the index), or move the
//! loader into an existing Bevy host.
//!
//! The loader reads the same RON artifacts as the standalone test and exposes them as a Bevy
//! `Resource` during `Startup`. It does not reimplement any rules.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use weave_tabletop::{OpenTaleCreationPreview, ResolutionReceipt, TabletopState};

#[derive(Resource, Debug)]
pub struct OpenTaleFixture {
    pub preview: OpenTaleCreationPreview,
    pub receipts: Vec<ResolutionReceipt>,
    pub final_state: TabletopState,
}

pub fn load_fixture(root: &Path) -> Result<OpenTaleFixture, Box<dyn std::error::Error>> {
    let preview = OpenTaleCreationPreview::from_ron(&fs::read_to_string(
        root.join("preview.tabletop-creation-preview.ron"),
    )?)?;
    let final_state = TabletopState::from_ron(&fs::read_to_string(
        root.join("final-state.tabletop-state.ron"),
    )?)?;
    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("playthrough/index.json"))?)?;

    let mut receipts = Vec::new();
    for step in index.as_array().ok_or("step index is not an array")? {
        let path = step["receipt_ron"]
            .as_str()
            .ok_or("step has no receipt path")?;
        receipts.push(ResolutionReceipt::from_ron(&fs::read_to_string(
            root.join(path),
        )?)?);
    }

    Ok(OpenTaleFixture {
        preview,
        receipts,
        final_state,
    })
}

fn load_at_startup(mut commands: Commands) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    match load_fixture(&root) {
        Ok(fixture) => {
            let kinds = fixture
                .receipts
                .iter()
                .flat_map(|receipt| receipt.events.iter().map(|event| event.kind.as_str()))
                .collect::<Vec<_>>();
            info!("Open Tale fixture loaded with {} events", kinds.len());
            commands.insert_resource(fixture);
        }
        Err(error) => error!("Open Tale fixture failed to load: {error}"),
    }
}

pub struct OpenTaleFixturePlugin;

impl Plugin for OpenTaleFixturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_at_startup);
    }
}
