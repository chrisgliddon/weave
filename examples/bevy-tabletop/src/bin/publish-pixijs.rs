//! Publish the public PixiJS projection from fully replayed adapter fixtures.
//!
//! The generator verifies every checked-in fixture tree and canonical project, re-derives public
//! events through `project_receipt_for_audience`, and writes only public event payloads and public
//! state. Run with:
//!
//! ```text
//! cargo run -p weave-example-bevy-tabletop --bin publish-pixijs
//! ```

use std::fs;

use weave_example_bevy_tabletop::{fixture_root, publish_pixijs_projection};

fn main() {
    let root = fixture_root();
    let output = root
        .parent()
        .expect("fixtures live under examples")
        .join("domain-module-pixijs")
        .join("src")
        .join("tabletop-fixtures.generated.json");
    match publish_pixijs_projection(&root) {
        Ok(json) => match fs::write(&output, json) {
            Ok(()) => println!(
                "Published public tabletop projection to {}",
                output.display()
            ),
            Err(error) => {
                eprintln!("failed to write {}: {error}", output.display());
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("failed to build the public tabletop projection: {error}");
            std::process::exit(1);
        }
    }
}
