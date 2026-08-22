use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};
use weave_world::{
    WorldCompositionPlan, compact_world_schema, compose_world_pack, export_world,
    full_world_schema, world_composition_schema,
};

const FIXTURE: &str = "examples/domain-modules/weave-world";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("--write") => Mode::Write,
        Some("--check") => Mode::Check,
        _ => return Err("expected `--write` or `--check`".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join(FIXTURE);
    let manifest = ModuleManifest::from_json(&read(&fixture.join("module.weave-module.json"))?)?;
    let input_paths = [
        "pack.weave-domain.json",
        "packs/british_columbia_temperate_forest.weave-domain.json",
        "packs/hokkaido_japan.weave-domain.json",
    ];
    let inputs = input_paths
        .iter()
        .map(|path| -> Result<DomainPack, Box<dyn std::error::Error>> {
            Ok(DomainPack::from_json(&read(&fixture.join(path))?)?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let plan =
        WorldCompositionPlan::from_json(&read(&fixture.join("composition.weave-world.json"))?)?;
    let composed = compose_world_pack(
        &plan,
        &manifest,
        inputs.clone(),
        &Version::parse(env!("CARGO_PKG_VERSION"))?,
    )?;
    let catalog = DomainCatalog::from_artifacts(
        [manifest],
        inputs.into_iter().chain([composed.pack.clone()]),
    )?;
    let compiled = compile_with_modules(
        &read(&fixture.join("composed-setting.weave"))?,
        &CompileOptions {
            source_name: Some(format!("{FIXTURE}/composed-setting.weave")),
        },
        &catalog,
    )?;
    let (full, compact) = export_world(&compiled.domain_modules["world"], &composed.receipt)?;

    let artifacts = BTreeMap::from([
        (
            fixture.join("packs/glasswind_composed.weave-domain.json"),
            composed.pack.to_json()?,
        ),
        (
            fixture.join("composition.receipt.json"),
            composed.receipt.to_json()?,
        ),
        (
            fixture.join("composition.receipt.ron"),
            composed.receipt.to_ron()?,
        ),
        (
            fixture.join("composed-setting.story.json"),
            to_json(&compiled.story)?,
        ),
        (
            fixture.join("composed-setting.story.ron"),
            to_ron(&compiled.story)?,
        ),
        (fixture.join("composed-world.full.json"), full.to_json()?),
        (fixture.join("composed-world.full.ron"), full.to_ron()?),
        (
            fixture.join("composed-world.compact.json"),
            compact.to_json()?,
        ),
        (
            fixture.join("composed-world.compact.ron"),
            compact.to_ron()?,
        ),
        (
            root.join("schemas/weave-world-composition-v1.schema.json"),
            world_composition_schema()?,
        ),
        (
            root.join("schemas/weave-world-full-v1.schema.json"),
            full_world_schema()?,
        ),
        (
            root.join("schemas/weave-world-compact-v1.schema.json"),
            compact_world_schema()?,
        ),
    ]);
    for (path, expected) in artifacts {
        match mode {
            Mode::Write => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, expected)?;
            }
            Mode::Check => {
                if read(&path)? != expected {
                    return Err(format!(
                        "checked World artifact differs: {}",
                        public_path(&root, &path)
                    )
                    .into());
                }
            }
        }
    }
    println!("{} portable World fixture artifacts", mode.verb());
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Write,
    Check,
}

impl Mode {
    fn verb(self) -> &'static str {
        match self {
            Self::Write => "wrote",
            Self::Check => "checked",
        }
    }
}

fn read(path: &Path) -> Result<String, std::io::Error> {
    fs::read_to_string(path)
}

fn public_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
