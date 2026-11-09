//! Load the optional World and Character presentation inputs beside a project.
//!
//! Play resolution is host-provided but never host-called: the surface reloads
//! this configuration during its normal compile and preview cycle. The single
//! adjacent file, [`PRESENTATION_INPUT_FILE_NAME`], is optional. A project with
//! no file still plays with the documented fallbacks already in
//! [`crate::play_view::resolve_presentation`]: no scene or stage is resolved and
//! the drawer explains that nothing is configured.
//!
//! The file names a World selector and the world value-to-scene map that
//! [`crate::play_view::WorldPresentation`] already models. Module selectors are
//! resolved at presentation time through the public compiled-story module map
//! and [`weave_core::ir::DomainModuleIr::value`], so the loader never guesses a
//! module schema.
//!
//! The Character section may name a project-relative Character collection. It
//! is read through the public [`weave_character::CharacterCollection::from_json`]
//! API, and a projection is derived for every collection character that the
//! story stage map covers: the stable character id becomes the projection stage
//! identity, the canonical display name becomes the author-facing name, and the
//! first portrait, else avatar, asset becomes the image. Explicit inline
//! projections always win over derived values.
//!
//! Loading is deterministic: collections use sorted maps and the derived
//! projection order follows the sorted character map. A missing, unreadable,
//! malformed, or unsafe input is reported as a [`PresentationInputIssue`] and
//! never panics or aborts playback. Messages name project-relative declarations
//! only, never an absolute path.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

/// Largest presentation input the editor will read.
const MAX_PRESENTATION_INPUT_BYTES: u64 = 1024 * 1024;

/// Read one project-local file through an opened handle.
///
/// Refuses symlinks and special files on Unix, and bounds the read so a large
/// or blocking file cannot stall the UI thread.
fn read_bounded_text(path: &Path) -> std::io::Result<String> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?
    };
    #[cfg(not(unix))]
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PRESENTATION_INPUT_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "not a regular file within the presentation input size limit",
        ));
    }
    let mut text = String::new();
    file.take(MAX_PRESENTATION_INPUT_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_PRESENTATION_INPUT_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "presentation input exceeds the size limit",
        ));
    }
    Ok(text)
}

use serde::Deserialize;
use weave_character::{
    CharacterCollection, CharacterExtension, CharacterProfile,
    IDENTITY_PRESENTATION_EXTENSION_NAMESPACE, PresentationAssetKind,
};

use crate::play_view::{CharacterPresentation, CharacterProjection, ValueRef, WorldPresentation};

/// Adjacent file that supplies optional World and Character presentation inputs.
pub const PRESENTATION_INPUT_FILE_NAME: &str = "weave.presentation.json";

/// Current serialized presentation-input format version.
pub const PRESENTATION_INPUT_FORMAT_VERSION: u32 = 1;

/// Why an optional presentation input could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationInputIssueKind {
    /// The optional file or a file it declared is absent.
    Missing,
    /// The file exists but cannot be read, parsed, or validated.
    Invalid,
}

impl PresentationInputIssueKind {
    /// Short author-facing name for the issue category.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Invalid => "invalid",
        }
    }
}

/// One redaction-safe problem found while loading presentation inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationInputIssue {
    /// Issue category.
    pub kind: PresentationInputIssueKind,
    /// Project-relative declaration or adjacent file name, never an absolute path.
    pub source: String,
    /// Redaction-safe explanation.
    pub detail: String,
}

/// Optional World and Character inputs plus any load problems.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresentationInputs {
    /// World-driven scene selection, when configured.
    pub world: Option<WorldPresentation>,
    /// Character-driven stage selection, when configured.
    pub character: Option<CharacterPresentation>,
    /// Load problems in deterministic order; empty when the file is valid.
    pub issues: Vec<PresentationInputIssue>,
}

impl PresentationInputs {
    /// Whether any World or Character presentation is configured.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.world.is_some() || self.character.is_some()
    }
}

/// Load the optional presentation inputs from one project directory.
///
/// A missing file is reported as [`PresentationInputIssueKind::Missing`] so the
/// author sees why Play has no resolved references. Any parse, version, read,
/// or path-safety failure is reported as [`PresentationInputIssueKind::Invalid`]
/// and yields no inputs; the caller keeps playing.
#[must_use]
pub fn load_presentation_inputs(directory: &Path) -> PresentationInputs {
    let source = PRESENTATION_INPUT_FILE_NAME.to_owned();
    // Resolve through the project-relative gate so a symlink cannot point outside the project on
    // any platform, then read the resolved regular file through a bounded handle.
    let path = match resolve_project_file(directory, PRESENTATION_INPUT_FILE_NAME) {
        Ok(path) => path,
        Err(PresentationInputIssueKind::Missing) => {
            return PresentationInputs {
                world: None,
                character: None,
                issues: vec![PresentationInputIssue {
                    kind: PresentationInputIssueKind::Missing,
                    source,
                    detail:
                        "no World or Character presentation configuration is present; Play uses \
                         explicit or fallback presentation"
                            .to_owned(),
                }],
            };
        }
        Err(kind) => {
            return PresentationInputs {
                world: None,
                character: None,
                issues: vec![PresentationInputIssue {
                    kind,
                    source,
                    detail:
                        "the presentation configuration path must stay inside the project directory"
                            .to_owned(),
                }],
            };
        }
    };
    let text = match read_bounded_text(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return PresentationInputs {
                world: None,
                character: None,
                issues: vec![PresentationInputIssue {
                    kind: PresentationInputIssueKind::Missing,
                    source,
                    detail:
                        "no World or Character presentation configuration is present; Play uses \
                         explicit or fallback presentation"
                            .to_owned(),
                }],
            };
        }
        Err(error) => {
            return invalid_inputs(
                source,
                format!("the presentation configuration could not be read: {error}"),
            );
        }
    };

    let file = match serde_json::from_str::<PresentationFile>(&text) {
        Ok(file) => file,
        Err(error) => {
            return invalid_inputs(
                source,
                format!("the presentation configuration is not valid JSON: {error}"),
            );
        }
    };
    if file.format_version != PRESENTATION_INPUT_FORMAT_VERSION {
        return invalid_inputs(
            source,
            format!(
                "unsupported presentation configuration version {}; expected {}",
                file.format_version, PRESENTATION_INPUT_FORMAT_VERSION
            ),
        );
    }

    let mut issues = Vec::new();
    let character = file
        .character
        .map(|character| character.into_presentation(directory, &mut issues));
    PresentationInputs {
        world: file.world,
        character,
        issues,
    }
}

/// Load from an optional project directory.
///
/// An untitled project has no directory, so there is nothing adjacent to read
/// and no diagnostic is raised; the drawer note explains the fallback.
#[must_use]
pub fn load_adjacent_presentation_inputs(directory: Option<&Path>) -> PresentationInputs {
    directory.map_or_else(PresentationInputs::default, load_presentation_inputs)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationFile {
    format_version: u32,
    #[serde(default)]
    world: Option<WorldPresentation>,
    #[serde(default)]
    character: Option<CharacterPresentationFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterPresentationFile {
    /// Optional project-relative Character collection with presentation data.
    #[serde(default)]
    collection: Option<String>,
    selector: ValueRef,
    #[serde(default)]
    projections: BTreeMap<String, CharacterProjection>,
    #[serde(default)]
    stage_ids: BTreeMap<String, String>,
    #[serde(default)]
    expressions: BTreeMap<String, String>,
    #[serde(default)]
    fallback_stage: Option<String>,
}

impl CharacterPresentationFile {
    fn into_presentation(
        self,
        directory: &Path,
        issues: &mut Vec<PresentationInputIssue>,
    ) -> CharacterPresentation {
        let mut projections = self.projections;
        if let Some(relative) = self.collection.as_deref() {
            match load_character_collection(directory, relative) {
                Ok(collection) => {
                    derive_projections(&collection, &self.stage_ids, &mut projections);
                }
                Err(issue) => issues.push(issue),
            }
        }
        CharacterPresentation {
            selector: self.selector,
            projections,
            stage_ids: self.stage_ids,
            expressions: self.expressions,
            fallback_stage: self.fallback_stage,
        }
    }
}

fn invalid_inputs(source: String, detail: String) -> PresentationInputs {
    PresentationInputs {
        world: None,
        character: None,
        issues: vec![PresentationInputIssue {
            kind: PresentationInputIssueKind::Invalid,
            source,
            detail,
        }],
    }
}

/// Read one declared Character collection through the public Character API.
fn load_character_collection(
    directory: &Path,
    relative: &str,
) -> Result<CharacterCollection, PresentationInputIssue> {
    let path =
        resolve_project_file(directory, relative).map_err(|kind| PresentationInputIssue {
            kind,
            source: relative.to_owned(),
            detail: "the Character collection path must stay inside the project directory"
                .to_owned(),
        })?;
    let text = read_bounded_text(&path).map_err(|error| PresentationInputIssue {
        kind: if error.kind() == ErrorKind::NotFound {
            PresentationInputIssueKind::Missing
        } else {
            PresentationInputIssueKind::Invalid
        },
        source: relative.to_owned(),
        detail: match error.kind() {
            ErrorKind::NotFound => {
                "the declared Character collection file was not found".to_owned()
            }
            _ => format!("the declared Character collection could not be read: {error}"),
        },
    })?;
    CharacterCollection::from_json(&text).map_err(|error| PresentationInputIssue {
        kind: PresentationInputIssueKind::Invalid,
        source: relative.to_owned(),
        detail: format!("the Character collection is invalid: {error}"),
    })
}

/// Resolve a declared path only when it is relative and stays under the root.
fn resolve_project_file(
    directory: &Path,
    relative: &str,
) -> Result<PathBuf, PresentationInputIssueKind> {
    if relative.trim().is_empty() {
        return Err(PresentationInputIssueKind::Invalid);
    }
    let declared = Path::new(relative);
    for component in declared.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(PresentationInputIssueKind::Invalid);
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    let root = directory
        .canonicalize()
        .map_err(|_| PresentationInputIssueKind::Invalid)?;
    match root.join(declared).canonicalize() {
        Ok(resolved) if resolved.starts_with(&root) && resolved.is_file() => Ok(resolved),
        Ok(_) => Err(PresentationInputIssueKind::Invalid),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(PresentationInputIssueKind::Missing)
        }
        Err(_) => Err(PresentationInputIssueKind::Invalid),
    }
}

/// Add a deterministic projection for every collection character not supplied inline.
fn derive_projections(
    collection: &CharacterCollection,
    stage_ids: &BTreeMap<String, String>,
    projections: &mut BTreeMap<String, CharacterProjection>,
) {
    for (id, profile) in &collection.characters {
        if projections.contains_key(id) {
            continue;
        }
        projections.insert(
            id.clone(),
            CharacterProjection {
                display_name: Some(profile.canon.identity.display_name.value.clone()),
                stage: stage_ids.contains_key(id).then_some(id.clone()),
                expression: None,
                portrait: portrait_path(profile),
            },
        );
    }
}

/// First portrait, else avatar, asset in sorted asset order.
fn portrait_path(profile: &CharacterProfile) -> Option<String> {
    let CharacterExtension::IdentityPresentation(extension) = profile
        .extensions
        .get(IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)?
    else {
        return None;
    };
    for kind in [
        PresentationAssetKind::Portrait,
        PresentationAssetKind::Avatar,
    ] {
        if let Some(asset) = extension
            .value
            .assets
            .values()
            .find(|asset| asset.value.kind == kind)
        {
            return Some(asset.value.path.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use weave_core::ir::{
        AnchorIr, DomainExportIr, DomainExportSourceIr, DomainModuleIr, DomainValueIr, Fit,
        SceneIr, Slot, StageIr, StoryIr,
    };
    use weave_runtime::Value;

    use super::*;
    use crate::play_view::{
        PresentationDiagnosticKind, ResolvedPresentation, resolve_presentation,
    };

    const CHARACTER_COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/presentation/input.character-collection.json"
    );

    fn story() -> StoryIr {
        let mut story = StoryIr::new("start");
        story.presentation.scenes.insert(
            "harbour".to_owned(),
            SceneIr {
                background: Some("art/harbour.png".to_owned()),
                fit: Fit::Cover,
                anchor: AnchorIr::default(),
            },
        );
        story.presentation.scenes.insert(
            "cabin".to_owned(),
            SceneIr {
                background: Some("art/cabin.png".to_owned()),
                fit: Fit::Contain,
                anchor: AnchorIr::default(),
            },
        );
        story.presentation.stages.insert(
            "mara".to_owned(),
            StageIr {
                image: Some("art/mara_neutral.png".to_owned()),
                name: Some("Mara".to_owned()),
                slot: Slot::Left,
                layer: 1,
                variants: BTreeMap::from([(
                    "surprised".to_owned(),
                    "art/mara_surprised.png".to_owned(),
                )]),
            },
        );
        story.presentation.stages.insert(
            "keeper".to_owned(),
            StageIr {
                image: Some("art/keeper_neutral.png".to_owned()),
                name: Some("The Keeper".to_owned()),
                slot: Slot::Right,
                layer: 2,
                variants: BTreeMap::new(),
            },
        );
        story.modules.insert(
            "world".to_owned(),
            DomainModuleIr {
                id: "org.weave.world".to_owned(),
                version: "1.0.0".to_owned(),
                pack_id: "example.world".to_owned(),
                pack_version: "1.0.0".to_owned(),
                exports: BTreeMap::from([(
                    "seed".to_owned(),
                    DomainExportIr {
                        source: DomainExportSourceIr::Pack,
                        value: DomainValueIr::Object(BTreeMap::from([(
                            "primary_biome".to_owned(),
                            DomainValueIr::Symbol("coastal".to_owned()),
                        )])),
                    },
                )]),
                authored_overrides: Vec::new(),
            },
        );
        story
    }

    fn snapshot(entries: [(&str, Value); 1]) -> BTreeMap<String, Value> {
        entries
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect()
    }

    fn project_with(contents: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("temp dir");
        fs::write(
            directory.path().join(PRESENTATION_INPUT_FILE_NAME),
            contents,
        )
        .expect("write presentation config");
        directory
    }

    const WORLD_CONFIG: &str = r#"{
        "format_version": 1,
        "world": {
            "selector": { "kind": "variable", "name": "place" },
            "scenes": { "coastal": "harbour", "forest": "cabin" },
            "fallback": "cabin"
        }
    }"#;

    const CHARACTER_CONFIG: &str = r#"{
        "format_version": 1,
        "character": {
            "selector": { "kind": "variable", "name": "speaker" },
            "projections": {
                "mara": {
                    "display_name": "Mara Vale",
                    "stage": "mara",
                    "expression": "surprised"
                }
            },
            "stage_ids": { "mara": "mara" },
            "expressions": { "surprised": "surprised" },
            "fallback_stage": "keeper"
        }
    }"#;

    #[test]
    fn world_driven_inputs_select_a_declared_scene() {
        let directory = project_with(WORLD_CONFIG);
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.issues.is_empty(), "{:?}", inputs.issues);
        let world = inputs.world.as_ref().expect("world input");

        let harbour = resolve_presentation(
            &story(),
            Some(world),
            None,
            Some(&snapshot([("place", Value::String("coastal".to_owned()))])),
        );
        assert_eq!(
            harbour.scene.as_ref().map(|scene| scene.id.as_str()),
            Some("harbour")
        );

        // Editing the supported value changes the presented scene.
        let cabin = resolve_presentation(
            &story(),
            Some(world),
            None,
            Some(&snapshot([("place", Value::String("forest".to_owned()))])),
        );
        assert_eq!(
            cabin.scene.as_ref().map(|scene| scene.id.as_str()),
            Some("cabin")
        );
        assert_ne!(harbour, cabin);
    }

    #[test]
    fn character_driven_inputs_apply_the_projection_and_expression() {
        let directory = project_with(CHARACTER_CONFIG);
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.issues.is_empty(), "{:?}", inputs.issues);
        let character = inputs.character.as_ref().expect("character input");

        let plan = resolve_presentation(
            &story(),
            None,
            Some(character),
            Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
        );
        let stage = plan.stage.as_ref().expect("projected stage");
        assert_eq!(stage.id, "mara");
        assert_eq!(stage.name, "Mara Vale");
        assert_eq!(stage.variant.as_deref(), Some("surprised"));
        assert_eq!(stage.image.as_deref(), Some("art/mara_surprised.png"));
    }

    #[test]
    fn a_missing_configuration_keeps_play_on_documented_fallbacks() {
        let directory = tempfile::tempdir().expect("temp dir");
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.world.is_none());
        assert!(inputs.character.is_none());
        assert!(!inputs.is_configured());
        assert_eq!(
            inputs.issues.first().map(|issue| issue.kind),
            Some(PresentationInputIssueKind::Missing)
        );

        let plan = resolve_presentation(
            &story(),
            inputs.world.as_ref(),
            inputs.character.as_ref(),
            None,
        );
        assert_eq!(plan, ResolvedPresentation::empty());
    }

    #[test]
    fn a_missing_module_alias_is_a_resolution_diagnostic_not_a_load_failure() {
        let config = r#"{
            "format_version": 1,
            "world": {
                "selector": { "kind": "module", "alias": "absent", "path": ["seed"] },
                "scenes": { "coastal": "harbour" },
                "fallback": "cabin"
            }
        }"#;
        let directory = project_with(config);
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.issues.is_empty(), "{:?}", inputs.issues);

        let plan = resolve_presentation(&story(), inputs.world.as_ref(), None, None);
        let kinds: Vec<_> = plan
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                PresentationDiagnosticKind::UnknownModule,
                PresentationDiagnosticKind::Fallback
            ]
        );
        assert_eq!(
            plan.scene.as_ref().map(|scene| scene.id.as_str()),
            Some("cabin")
        );
    }

    #[test]
    fn invalid_configuration_is_reported_without_a_crash() {
        let directory = project_with("{ this is not json");
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.world.is_none());
        assert!(inputs.character.is_none());
        let issue = inputs.issues.first().expect("invalid issue");
        assert_eq!(issue.kind, PresentationInputIssueKind::Invalid);
        assert!(issue.detail.contains("JSON"), "{}", issue.detail);
        assert!(
            !issue
                .detail
                .contains(directory.path().to_str().unwrap_or_default())
        );
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let directory = project_with(r#"{ "format_version": 99 }"#);
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.world.is_none());
        assert_eq!(
            inputs.issues.first().map(|issue| issue.kind),
            Some(PresentationInputIssueKind::Invalid)
        );
    }

    #[test]
    fn an_unknown_selected_value_uses_the_configured_fallback() {
        let directory = project_with(WORLD_CONFIG);
        let inputs = load_presentation_inputs(directory.path());
        let plan = resolve_presentation(
            &story(),
            inputs.world.as_ref(),
            None,
            Some(&snapshot([(
                "place",
                Value::Symbol("elsewhere".to_owned()),
            )])),
        );
        assert_eq!(
            plan.scene.as_ref().map(|scene| scene.id.as_str()),
            Some("cabin")
        );
        assert!(plan.scene.as_ref().is_some_and(|scene| scene.fallback));
    }

    #[test]
    fn the_same_files_produce_the_same_inputs_and_plan() {
        let directory = project_with(WORLD_CONFIG);
        let first = load_presentation_inputs(directory.path());
        let second = load_presentation_inputs(directory.path());
        assert_eq!(first, second);

        let variables = snapshot([("place", Value::String("coastal".to_owned()))]);
        let first_plan =
            resolve_presentation(&story(), first.world.as_ref(), None, Some(&variables));
        let second_plan =
            resolve_presentation(&story(), second.world.as_ref(), None, Some(&variables));
        assert_eq!(first_plan, second_plan);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_presentation_file_outside_the_project_is_refused() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().expect("project dir");
        let outside = tempfile::tempdir().expect("outside dir");
        let target = outside.path().join("conf.json");
        fs::write(
            &target,
            r#"{"format_version":1,"world":{"selector":{"kind":"variable","name":"place"},"scenes":{},"fallback":null}}"#,
        )
        .expect("write outside file");
        symlink(&target, directory.path().join(PRESENTATION_INPUT_FILE_NAME)).expect("symlink");
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.world.is_none());
        assert_eq!(inputs.issues.len(), 1);
        assert_eq!(inputs.issues[0].kind, PresentationInputIssueKind::Invalid);
    }

    #[test]
    fn an_oversized_presentation_file_is_refused() {
        let directory = tempfile::tempdir().expect("project dir");
        let oversized = vec![b' '; MAX_PRESENTATION_INPUT_BYTES as usize + 1];
        fs::write(
            directory.path().join(PRESENTATION_INPUT_FILE_NAME),
            oversized,
        )
        .expect("write oversized file");
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.world.is_none());
        assert_eq!(inputs.issues.len(), 1);
        assert_eq!(inputs.issues[0].kind, PresentationInputIssueKind::Invalid);
    }

    #[test]
    fn a_character_collection_derives_projections_through_the_public_api() {
        let directory = tempfile::tempdir().expect("temp dir");
        fs::write(
            directory.path().join("characters.json"),
            CHARACTER_COLLECTION,
        )
        .expect("write collection");
        fs::write(
            directory.path().join(PRESENTATION_INPUT_FILE_NAME),
            r#"{
                "format_version": 1,
                "character": {
                    "collection": "characters.json",
                    "selector": { "kind": "variable", "name": "speaker" },
                    "stage_ids": {
                        "org.weave.character.ari_vale": "mara"
                    },
                    "fallback_stage": "keeper"
                }
            }"#,
        )
        .expect("write config");

        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.issues.is_empty(), "{:?}", inputs.issues);
        let character = inputs.character.as_ref().expect("character input");
        let projection = character
            .projections
            .get("org.weave.character.ari_vale")
            .expect("derived projection");
        assert_eq!(projection.display_name.as_deref(), Some("Ari Vale"));
        assert_eq!(
            projection.stage.as_deref(),
            Some("org.weave.character.ari_vale")
        );
        assert_eq!(
            projection.portrait.as_deref(),
            Some("presentation/assets/ari-vale-avatar.svg")
        );

        let plan = resolve_presentation(
            &story(),
            None,
            Some(character),
            Some(&snapshot([(
                "speaker",
                Value::String("org.weave.character.ari_vale".to_owned()),
            )])),
        );
        let stage = plan.stage.as_ref().expect("resolved stage");
        assert_eq!(stage.id, "mara");
        assert_eq!(stage.name, "Ari Vale");
        assert_eq!(
            stage.image.as_deref(),
            Some("presentation/assets/ari-vale-avatar.svg")
        );
    }

    #[test]
    fn an_invalid_collection_keeps_inline_projections_and_reports_the_failure() {
        let directory = tempfile::tempdir().expect("temp dir");
        fs::write(directory.path().join("characters.json"), "{ broken")
            .expect("write invalid collection");
        fs::write(
            directory.path().join(PRESENTATION_INPUT_FILE_NAME),
            r#"{
                "format_version": 1,
                "character": {
                    "collection": "characters.json",
                    "selector": { "kind": "variable", "name": "speaker" },
                    "projections": {
                        "mara": { "display_name": "Mara Vale", "stage": "mara" }
                    },
                    "stage_ids": { "mara": "mara" }
                }
            }"#,
        )
        .expect("write config");

        let inputs = load_presentation_inputs(directory.path());
        let character = inputs.character.as_ref().expect("inline character input");
        assert!(character.projections.contains_key("mara"));
        let issue = inputs.issues.first().expect("collection issue");
        assert_eq!(issue.kind, PresentationInputIssueKind::Invalid);
        assert_eq!(issue.source, "characters.json");
    }

    #[test]
    fn a_collection_path_that_escapes_the_project_is_rejected() {
        let directory = project_with(
            r#"{
                "format_version": 1,
                "character": {
                    "collection": "../outside.json",
                    "selector": { "kind": "variable", "name": "speaker" }
                }
            }"#,
        );
        let inputs = load_presentation_inputs(directory.path());
        assert!(inputs.character.is_some());
        let issue = inputs.issues.first().expect("path issue");
        assert_eq!(issue.kind, PresentationInputIssueKind::Invalid);
        assert_eq!(issue.source, "../outside.json");
        assert!(
            !issue
                .detail
                .contains(directory.path().to_str().unwrap_or_default())
        );
    }
}
