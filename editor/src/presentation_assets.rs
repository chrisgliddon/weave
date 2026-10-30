//! Project-relative presentation images staged by the Play surface.
//!
//! A story declares background and character images as paths relative to the
//! project directory. The compiler checks that those paths are relative, but a
//! compiled story can also arrive from a file, a module, or a test, so this
//! module validates every path again before it touches the filesystem:
//! absolute paths, parent traversal, and symlinks that leave the project root
//! are all refused.
//!
//! Decoded frames are cached by path and file stamp. [`PresentationAssets::refresh`]
//! is the only method that stats cached files, so the editor's existing 100 ms
//! poll can pick up a replaced image without a second file watcher.
//!
//! The fallback is deliberate: when a declared file is missing, unreadable,
//! undecodable, or outside the project, the loader returns `None`, the Play
//! surface keeps the scene's synthetic tone and silhouette, and a small muted
//! note names the declaration and the reason under the stage.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use gpui::RenderImage;
use image::{Frame, ImageFormat};

/// Why a declared presentation asset could not be staged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssetError {
    /// No project directory is bound, so no relative path can be resolved.
    #[error("no project directory is open")]
    NoProject,
    /// The declaration is an empty string.
    #[error("the asset path is empty")]
    EmptyPath,
    /// The declaration is rooted, so it does not live in the project.
    #[error("absolute asset paths are not allowed")]
    AbsolutePath,
    /// The declaration traverses or links outside the project directory.
    #[error("the asset path escapes the project directory")]
    EscapesProject,
    /// The file does not exist or cannot be reached.
    #[error("the file was not found")]
    Missing,
    /// The file exists but cannot be read.
    #[error("the file could not be read: {0}")]
    Unreadable(String),
    /// The bytes are not a PNG or JPEG image, or the image is malformed.
    #[error("the file is not a decodable PNG or JPEG: {0}")]
    Undecodable(String),
}

/// Length and modification time used to notice a replaced file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: Option<SystemTime>,
}

/// One resolved or failed declaration, kept until its file stamp changes.
#[derive(Debug)]
struct CachedAsset {
    /// File whose stamp drives invalidation; `None` when the path cannot be watched.
    watched: Option<PathBuf>,
    /// Stamp observed when this entry was populated.
    stamp: Option<FileStamp>,
    /// Decoded frame, when the file was staged successfully.
    image: Option<Arc<RenderImage>>,
    /// Human-readable fallback note naming the declaration and the reason.
    failure: Option<String>,
}

/// Image loader and cache bound to one project directory.
#[derive(Debug, Default)]
pub struct PresentationAssets {
    root: Option<PathBuf>,
    cache: HashMap<String, CachedAsset>,
}

impl PresentationAssets {
    /// Bind the loader to a project directory.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            cache: HashMap::new(),
        }
    }

    /// Create a loader with no project directory yet.
    #[must_use]
    pub fn unbound() -> Self {
        Self::default()
    }

    /// Project directory currently used to resolve relative paths.
    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Point the loader at a project directory, discarding the old cache.
    ///
    /// Returns whether the directory changed, so the caller can repaint only
    /// when the binding actually moved.
    pub fn rebind(&mut self, root: Option<PathBuf>) -> bool {
        if self.root == root {
            return false;
        }
        self.root = root;
        self.cache.clear();
        true
    }

    /// Drop the project binding and every cached frame.
    pub fn clear(&mut self) {
        self.root = None;
        self.cache.clear();
    }

    /// Resolve a project-relative declaration to a file under the project root.
    ///
    /// The path is re-joined under the root and canonicalised, so `..`,
    /// absolute paths, and symlinks that point outside the project are refused
    /// even when the compiler already accepted the declaration.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, AssetError> {
        let root = self.root.as_deref().ok_or(AssetError::NoProject)?;
        if relative.trim().is_empty() {
            return Err(AssetError::EmptyPath);
        }
        let relative_path = Path::new(relative);
        for component in relative_path.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => {
                    return Err(AssetError::AbsolutePath);
                }
                Component::ParentDir => return Err(AssetError::EscapesProject),
                Component::CurDir | Component::Normal(_) => {}
            }
        }
        let canonical_root = root.canonicalize().map_err(|error| {
            AssetError::Unreadable(format!("the project directory is unavailable: {error}"))
        })?;
        let resolved = canonical_root
            .join(relative_path)
            .canonicalize()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => AssetError::Missing,
                _ => AssetError::Unreadable(error.to_string()),
            })?;
        if !resolved.starts_with(&canonical_root) {
            return Err(AssetError::EscapesProject);
        }
        if !resolved.is_file() {
            return Err(AssetError::Unreadable(
                "the path is not a regular file".to_owned(),
            ));
        }
        Ok(resolved)
    }

    /// Stage the background image declared by one scene.
    pub fn background(&mut self, scene_id: &str, path: &str) -> Option<Arc<RenderImage>> {
        self.load("scene", scene_id, path)
    }

    /// Stage the character image declared by one stage.
    pub fn stage(&mut self, stage_id: &str, path: &str) -> Option<Arc<RenderImage>> {
        self.load("stage", stage_id, path)
    }

    /// Fallback note for a declaration that could not be staged.
    ///
    /// The note names the declaration and the reason, and remains available
    /// until a later lookup replaces it.
    #[must_use]
    pub fn failure(&self, relative: &str) -> Option<&str> {
        self.cache.get(relative)?.failure.as_deref()
    }

    /// Re-stat cached files and invalidate entries whose stamp changed.
    ///
    /// Returns whether anything changed, so the caller can repaint. A replaced,
    /// truncated, created, or removed file invalidates its entry; the next
    /// lookup decodes the new bytes or records the new failure.
    pub fn refresh(&mut self) -> bool {
        let mut changed = false;
        self.cache.retain(|_, entry| {
            let stamp = entry.watched.as_deref().and_then(file_stamp);
            if stamp == entry.stamp {
                true
            } else {
                changed = true;
                false
            }
        });
        changed
    }

    fn load(&mut self, owner: &str, id: &str, relative: &str) -> Option<Arc<RenderImage>> {
        if let Some(cached) = self.cache.get(relative) {
            return cached.image.as_ref().map(Arc::clone);
        }
        let (watched, stamp, image, error) = match self.resolve(relative) {
            Ok(resolved) => {
                let stamp = file_stamp(&resolved);
                match decode(&resolved) {
                    Ok(image) => (Some(resolved), stamp, Some(image), None),
                    Err(error) => (Some(resolved), stamp, None, Some(error)),
                }
            }
            Err(error) => {
                // Watch the intended file even when it is missing, so a file
                // that later appears invalidates the cached failure.
                let watched = self.watched_path(relative);
                let stamp = watched.as_deref().and_then(file_stamp);
                (watched, stamp, None, Some(error))
            }
        };
        let entry = CachedAsset {
            failure: error.map(|error| format!("{owner} \"{id}\" · {relative} — {error}")),
            watched,
            stamp,
            image,
        };
        let staged = entry.image.as_ref().map(Arc::clone);
        self.cache.insert(relative.to_owned(), entry);
        staged
    }

    /// Best-effort path to stat when resolution failed, but only for paths that
    /// are structurally allowed to live under the root.
    fn watched_path(&self, relative: &str) -> Option<PathBuf> {
        let root = self.root.as_deref()?;
        if relative.trim().is_empty() {
            return None;
        }
        let path = Path::new(relative);
        let stays_relative = path
            .components()
            .all(|component| matches!(component, Component::CurDir | Component::Normal(_)));
        stays_relative.then(|| root.join(path))
    }
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// Decode a PNG or JPEG into a GPUI render frame.
/// Largest side, and largest total, a staged image may declare.
///
/// Decoding is synchronous on the render thread, so an image that claims a
/// huge canvas would allocate before anyone could cancel it. The declared
/// dimensions are checked before any pixel buffer is created.
const MAX_ASSET_SIDE: u32 = 8192;
const MAX_ASSET_PIXELS: u64 = 33_554_432;

/// Whether declared dimensions are too large to stage.
fn asset_size_problem(width: u32, height: u32) -> Option<String> {
    if width == 0 || height == 0 {
        return Some("the image declares an empty canvas".to_owned());
    }
    if width > MAX_ASSET_SIDE
        || height > MAX_ASSET_SIDE
        || u64::from(width) * u64::from(height) > MAX_ASSET_PIXELS
    {
        return Some(format!(
            "{width}x{height} exceeds the {MAX_ASSET_SIDE}px / {MAX_ASSET_PIXELS}-pixel limit"
        ));
    }
    None
}

fn decode(path: &Path) -> Result<Arc<RenderImage>, AssetError> {
    let bytes = fs::read(path).map_err(|error| AssetError::Unreadable(error.to_string()))?;
    let format = image::guess_format(&bytes).map_err(|error| {
        AssetError::Undecodable(format!("the header is not a recognised image ({error})"))
    })?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(AssetError::Undecodable(format!(
            "{format:?} is not staged; only PNG and JPEG are supported"
        )));
    }
    let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| AssetError::Undecodable(error.to_string()))?
        .into_dimensions()
        .map_err(|error| AssetError::Undecodable(error.to_string()))?;
    if let Some(problem) = asset_size_problem(dimensions.0, dimensions.1) {
        return Err(AssetError::Undecodable(problem));
    }
    let mut rgba = image::load_from_memory_with_format(&bytes, format)
        .map_err(|error| AssetError::Undecodable(error.to_string()))?
        .into_rgba8();
    // GPUI's renderer consumes BGRA, and its own decoder swaps the red and
    // blue channels after decoding to RGBA.
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new([Frame::new(rgba)])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_view::{scene_from_session, scene_tone};
    use crate::preview::PreviewSession;

    #[test]
    fn oversized_and_empty_canvases_are_refused_before_decoding() {
        assert!(asset_size_problem(0, 10).is_some());
        assert!(asset_size_problem(10, 0).is_some());
        assert!(asset_size_problem(MAX_ASSET_SIDE + 1, 1).is_some());
        assert!(asset_size_problem(8000, 8000).is_some());
        assert!(asset_size_problem(640, 480).is_none());
    }

    fn write_png(path: &Path, colour: [u8; 4], width: u32, height: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("asset directory");
        }
        image::RgbaImage::from_pixel(width, height, image::Rgba(colour))
            .save_with_format(path, ImageFormat::Png)
            .expect("png fixture");
    }

    fn pixel(image: &RenderImage, index: usize) -> [u8; 4] {
        let bytes = image.as_bytes(0).expect("one frame");
        let start = index * 4;
        [
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ]
    }

    #[test]
    fn resolve_accepts_nested_paths_and_rejects_escapes_and_absolute_paths() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().join("project");
        write_png(&root.join("art/scenes/ferry.png"), [10, 20, 30, 255], 1, 1);
        let assets = PresentationAssets::new(root.clone());

        let resolved = assets
            .resolve("art/scenes/ferry.png")
            .expect("nested asset resolves");
        assert!(resolved.ends_with("art/scenes/ferry.png"));
        assert!(resolved.starts_with(root.canonicalize().expect("canonical root")));

        assert_eq!(
            assets.resolve("../outside.png"),
            Err(AssetError::EscapesProject)
        );
        assert_eq!(assets.resolve("/etc/passwd"), Err(AssetError::AbsolutePath));
        assert_eq!(assets.resolve(""), Err(AssetError::EmptyPath));
        assert_eq!(assets.resolve("art/missing.png"), Err(AssetError::Missing));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_a_symlink_that_leaves_the_project() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().join("project");
        let outside = directory.path().join("outside.png");
        write_png(&outside, [255, 0, 0, 255], 1, 1);
        fs::create_dir_all(root.join("art")).expect("project directory");
        std::os::unix::fs::symlink(&outside, root.join("art/link.png")).expect("symlink fixture");
        let assets = PresentationAssets::new(root);

        assert_eq!(
            assets.resolve("art/link.png"),
            Err(AssetError::EscapesProject)
        );
    }

    #[test]
    fn cache_decodes_once_and_invalidates_a_replaced_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().to_path_buf();
        let path = root.join("art/tone.png");
        write_png(&path, [255, 0, 0, 255], 1, 1);
        let mut assets = PresentationAssets::new(root);

        let first = assets
            .background("tone", "art/tone.png")
            .expect("first decode");
        let cached = assets
            .background("tone", "art/tone.png")
            .expect("cached decode");
        assert!(Arc::ptr_eq(&first, &cached));
        assert_eq!(first.size(0).width.0, 1);
        assert_eq!(pixel(&first, 0), [0, 0, 255, 255]);
        assert!(!assets.refresh());

        write_png(&path, [0, 0, 255, 255], 2, 2);
        assert!(assets.refresh(), "a rewritten file invalidates the entry");
        let second = assets
            .background("tone", "art/tone.png")
            .expect("second decode");
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.size(0).width.0, 2);
        assert_eq!(pixel(&second, 0), [255, 0, 0, 255]);
        assert!(assets.failure("art/tone.png").is_none());
    }

    #[test]
    fn missing_and_undecodable_files_report_a_reason() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join("art")).expect("asset directory");
        fs::write(root.join("art/broken.png"), b"not an image").expect("broken fixture");
        let mut assets = PresentationAssets::new(root);

        assert!(assets.background("missing", "art/missing.png").is_none());
        let missing = assets.failure("art/missing.png").expect("missing note");
        assert!(missing.contains("art/missing.png"));
        assert!(missing.contains("not found"));

        assert!(assets.background("broken", "art/broken.png").is_none());
        let broken = assets.failure("art/broken.png").expect("broken note");
        assert!(broken.contains("art/broken.png"));
        assert!(broken.contains("decodable"));
    }

    #[test]
    fn a_file_that_appears_after_a_missing_failure_is_staged() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join("art")).expect("asset directory");
        let mut assets = PresentationAssets::new(root.clone());

        assert!(assets.stage("mara", "art/mara.png").is_none());
        assert!(assets.failure("art/mara.png").is_some());

        write_png(&root.join("art/mara.png"), [1, 2, 3, 255], 1, 1);
        assert!(assets.refresh(), "a created file invalidates the failure");
        assert!(assets.stage("mara", "art/mara.png").is_some());
        assert!(assets.failure("art/mara.png").is_none());
    }

    fn cued_story() -> weave_core::ir::StoryIr {
        let source = r#"
scene ferry_dusk {
    background: "art/ferry_dusk.png"
}

stage mara {
    image: "art/mara.png"
    name: "Mara"
}

=== start ===
@scene ferry_dusk
@show mara
Mara: The ferry leaves at dusk.
-> END
"#;
        weave_compiler::compile(source, &weave_compiler::CompileOptions::default())
            .expect("fixture compiles")
            .story
    }

    #[test]
    fn the_play_scene_prefers_a_decoded_image_and_falls_back_to_the_tone() {
        let directory = tempfile::tempdir().expect("tempdir");
        let root = directory.path().to_path_buf();
        write_png(&root.join("art/ferry_dusk.png"), [40, 60, 80, 255], 3, 2);
        write_png(&root.join("art/mara.png"), [90, 70, 50, 255], 2, 3);
        let mut session = PreviewSession::from_story(cued_story(), 3, 1);
        session.run();
        let scene = scene_from_session(&session);
        let background = scene.background.as_deref().expect("scene cue");
        let stage = scene.stage.as_deref().expect("stage cue");
        let background_path = session
            .scene_background(background)
            .expect("declared background");
        let stage_path = session.stage_image(stage).expect("declared image");

        let mut assets = PresentationAssets::new(root.clone());
        let decoded = assets
            .background(background, background_path)
            .expect("background decodes");
        assert_eq!(decoded.size(0).width.0, 3);
        assert_eq!(
            assets
                .stage(stage, stage_path)
                .expect("stage decodes")
                .size(0)
                .height
                .0,
            3
        );

        fs::remove_file(root.join(background_path)).expect("remove background");
        assert!(assets.refresh());
        assert!(assets.background(background, background_path).is_none());
        assert!(
            assets
                .failure(background_path)
                .is_some_and(|note| note.contains("not found"))
        );
        assert_ne!(scene_tone(Some(background)), 0x0e1118);
    }
}
