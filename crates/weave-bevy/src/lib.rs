//! Bevy 0.18 asset-pipeline and ECS integration for Weave.
//!
//! The plugin keeps the standalone runtime free of ECS types while exposing one observable
//! resource, command extensions, and observer events to a Bevy application.

use std::ops::Deref;

use bevy::asset::{
    Asset, AssetApp, AssetEvent, AssetLoadFailedEvent, AssetLoader, AssetPath, AssetServer, Assets,
    Handle, LoadContext, io::Reader,
};
use bevy::ecs::message::MessageReader;
use bevy::ecs::system::Command;
use bevy::prelude::{
    App, Commands, Event, IntoScheduleConfigs, Plugin, Res, ResMut, Resource, SystemSet, Update,
    World,
};
use bevy::reflect::TypePath;
use weave_compiler::{CompileOptions, compile};
use weave_core::ir::StoryIr;
use weave_runtime::{ChoiceView, RuntimeError, Story, StoryEvent, StoryState, VariableState};

/// Bevy version targeted by this integration.
pub const BEVY_VERSION: &str = "0.18";

/// A compiled story managed by Bevy's asset pipeline.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct WeaveAsset {
    /// Immutable versioned runtime data.
    pub story: StoryIr,
}

/// Source `.weave` asset loader.
#[derive(Default, TypePath)]
pub struct WeaveSourceLoader;

/// Compiled `.ron` asset loader.
#[derive(Default, TypePath)]
pub struct WeaveRonLoader;

/// Failure while loading or compiling a Weave asset.
#[derive(Debug, thiserror::Error)]
pub enum WeaveAssetLoaderError {
    /// Asset bytes could not be read.
    #[error("could not read Weave asset: {0}")]
    Io(#[from] std::io::Error),
    /// Source was not valid UTF-8.
    #[error("Weave source is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// Source compilation failed.
    #[error("Weave compilation failed:\n{0}")]
    Compile(String),
    /// Compiled RON could not be decoded.
    #[error("could not decode compiled Weave RON: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for WeaveSourceLoader {
    type Asset = WeaveAsset;
    type Settings = ();
    type Error = WeaveAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = std::str::from_utf8(&bytes)?;
        let options = CompileOptions {
            source_name: Some(load_context.path().to_string()),
        };
        let compiled = compile(source, &options)
            .map_err(|error| WeaveAssetLoaderError::Compile(error.to_string()))?;
        Ok(WeaveAsset {
            story: compiled.story,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["weave"]
    }
}

impl AssetLoader for WeaveRonLoader {
    type Asset = WeaveAsset;
    type Settings = ();
    type Error = WeaveAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(WeaveAsset {
            story: ron::de::from_bytes(&bytes)?,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

/// ECS-owned host state for one story session.
#[derive(Resource, Debug)]
pub struct WeaveStory {
    path: AssetPath<'static>,
    handle: Option<Handle<WeaveAsset>>,
    runtime: Option<Story>,
    seed: u64,
    loaded_once: bool,
}

impl WeaveStory {
    /// Describe a source or compiled story to load through [`AssetServer`].
    #[must_use]
    pub fn new(path: impl Into<AssetPath<'static>>) -> Self {
        Self {
            path: path.into(),
            handle: None,
            runtime: None,
            seed: 0,
            loaded_once: false,
        }
    }

    /// Set the deterministic grammar seed used on initial load and reload fallback.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Requested asset path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath<'static> {
        &self.path
    }

    /// Current runtime, once the asset is ready.
    #[must_use]
    pub const fn runtime(&self) -> Option<&Story> {
        self.runtime.as_ref()
    }

    /// Current saveable runtime state.
    #[must_use]
    pub fn state(&self) -> Option<&StoryState> {
        self.runtime.as_ref().map(Story::state)
    }

    /// Current observable story variables.
    #[must_use]
    pub fn variables(&self) -> Option<&std::collections::BTreeMap<String, VariableState>> {
        self.state().map(StoryState::variables)
    }
}

/// Emitted after a story asset creates a valid runtime.
#[derive(Event, Debug, Clone, Copy)]
pub struct StoryReady;

/// Emitted after an already-loaded asset is hot-reloaded.
#[derive(Event, Debug, Clone, Copy)]
pub struct StoryReloaded;

/// One narrative line delivered by the runtime.
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub struct DeliverLine {
    /// Fully rendered text.
    pub text: String,
}

/// A host-selectable choice set delivered by the runtime.
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub struct DeliverChoices {
    /// Eligible choices in source order.
    pub choices: Vec<ChoiceView>,
}

impl Deref for DeliverChoices {
    type Target = [ChoiceView];

    fn deref(&self) -> &Self::Target {
        &self.choices
    }
}

/// Emitted when the story reaches its end boundary.
#[derive(Event, Debug, Clone, Copy)]
pub struct StoryEnded;

/// Emitted for an asset-load or structured runtime failure.
#[derive(Event, Debug, Clone)]
pub struct StoryFailed {
    /// Human-readable context safe to present in development UI.
    pub message: String,
    /// Structured runtime failure, absent for asset-pipeline failures.
    pub runtime_error: Option<RuntimeError>,
}

/// Ordered plugin stages. Applications may order their systems around this set.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaveSet {
    /// Request a configured asset handle.
    RequestAsset,
    /// Apply asset lifecycle and hot-reload messages.
    SyncAsset,
}

/// First-class Bevy wrapper around the standalone Weave runtime.
#[derive(Default)]
pub struct WeavePlugin;

impl Plugin for WeavePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<WeaveAsset>()
            .init_asset_loader::<WeaveSourceLoader>()
            .init_asset_loader::<WeaveRonLoader>()
            .configure_sets(
                Update,
                (WeaveSet::RequestAsset, WeaveSet::SyncAsset).chain(),
            )
            .add_systems(Update, request_asset.in_set(WeaveSet::RequestAsset))
            .add_systems(
                Update,
                (sync_asset_events, sync_asset_failures)
                    .chain()
                    .in_set(WeaveSet::SyncAsset),
            );
    }
}

fn request_asset(asset_server: Res<AssetServer>, story: Option<ResMut<WeaveStory>>) {
    let Some(mut story) = story else {
        return;
    };
    if story.handle.is_none() {
        story.handle = Some(asset_server.load(story.path.clone()));
    }
}

fn sync_asset_events(
    mut events: MessageReader<AssetEvent<WeaveAsset>>,
    assets: Res<Assets<WeaveAsset>>,
    story: Option<ResMut<WeaveStory>>,
    mut commands: Commands,
) {
    let Some(mut story) = story else {
        events.clear();
        return;
    };
    let Some(handle) = story.handle.clone() else {
        return;
    };
    for event in events.read() {
        let relevant = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => *id == handle.id(),
            AssetEvent::Removed { id } if *id == handle.id() => {
                story.runtime = None;
                commands.trigger(StoryFailed {
                    message: "the active Weave asset was removed".to_owned(),
                    runtime_error: None,
                });
                false
            }
            _ => false,
        };
        if !relevant {
            continue;
        }
        let Some(asset) = assets.get(handle.id()) else {
            continue;
        };
        let replacement = story
            .runtime
            .as_ref()
            .and_then(|runtime| Story::restore(asset.story.clone(), runtime.state().clone()).ok());
        let runtime =
            replacement.or_else(|| Story::with_seed(asset.story.clone(), story.seed).ok());
        match runtime {
            Some(runtime) => {
                let reloaded = story.loaded_once;
                story.runtime = Some(runtime);
                story.loaded_once = true;
                if reloaded {
                    commands.trigger(StoryReloaded);
                } else {
                    commands.trigger(StoryReady);
                }
            }
            None => commands.trigger(StoryFailed {
                message: "compiled story could not initialize".to_owned(),
                runtime_error: Story::with_seed(asset.story.clone(), story.seed).err(),
            }),
        }
    }
}

fn sync_asset_failures(
    mut failures: MessageReader<AssetLoadFailedEvent<WeaveAsset>>,
    story: Option<Res<WeaveStory>>,
    mut commands: Commands,
) {
    let Some(story) = story else {
        failures.clear();
        return;
    };
    let active_id = story.handle.as_ref().map(Handle::id);
    for failure in failures.read() {
        if active_id == Some(failure.id) {
            commands.trigger(StoryFailed {
                message: format!("could not load {}: {}", failure.path, failure.error),
                runtime_error: None,
            });
        }
    }
}

/// Extensions that queue safe Weave actions through Bevy's deferred command buffer.
pub trait WeaveCommandsExt {
    /// Advance to the next line, choices, or end boundary.
    fn weave_continue(&mut self);
    /// Select one current choice, then advance to the next boundary.
    fn weave_choose(&mut self, index: usize);
    /// Jump to a knot, then advance to the next boundary.
    fn weave_jump_to(&mut self, knot: impl Into<String>);
}

impl WeaveCommandsExt for Commands<'_, '_> {
    fn weave_continue(&mut self) {
        self.queue(WeaveAction::Continue);
    }

    fn weave_choose(&mut self, index: usize) {
        self.queue(WeaveAction::Choose(index));
    }

    fn weave_jump_to(&mut self, knot: impl Into<String>) {
        self.queue(WeaveAction::Jump(knot.into()));
    }
}

enum WeaveAction {
    Continue,
    Choose(usize),
    Jump(String),
}

impl Command for WeaveAction {
    fn apply(self, world: &mut World) {
        let Some(mut story_resource) = world.remove_resource::<WeaveStory>() else {
            world.trigger(StoryFailed {
                message: "WeaveStory resource is not installed".to_owned(),
                runtime_error: None,
            });
            return;
        };
        let result = match story_resource.runtime.as_mut() {
            Some(runtime) => apply_action(runtime, self),
            None => Err(None),
        };
        world.insert_resource(story_resource);
        match result {
            Ok(event) => trigger_story_event(world, event),
            Err(Some(error)) => world.trigger(StoryFailed {
                message: error.to_string(),
                runtime_error: Some(error),
            }),
            Err(None) => world.trigger(StoryFailed {
                message: "Weave story is not ready".to_owned(),
                runtime_error: None,
            }),
        }
    }
}

fn apply_action(
    runtime: &mut Story,
    action: WeaveAction,
) -> Result<StoryEvent, Option<RuntimeError>> {
    match action {
        WeaveAction::Continue => runtime.advance().map_err(Some),
        WeaveAction::Choose(index) => {
            runtime.choose(index).map_err(Some)?;
            runtime.advance().map_err(Some)
        }
        WeaveAction::Jump(knot) => {
            runtime.jump(&knot).map_err(Some)?;
            runtime.advance().map_err(Some)
        }
    }
}

fn trigger_story_event(world: &mut World, event: StoryEvent) {
    match event {
        StoryEvent::Line(text) => world.trigger(DeliverLine { text }),
        StoryEvent::Choices(choices) => world.trigger(DeliverChoices { choices }),
        StoryEvent::Ended => world.trigger(StoryEnded),
    }
}

/// Commonly used integration types.
pub mod prelude {
    pub use crate::{
        DeliverChoices, DeliverLine, StoryEnded, StoryFailed, StoryReady, StoryReloaded,
        WeaveAsset, WeaveCommandsExt, WeavePlugin, WeaveSet, WeaveStory,
    };
}

#[cfg(test)]
mod tests {
    use bevy::asset::{AssetId, AssetPlugin, Assets};
    use bevy::prelude::{App, Commands, MinimalPlugins, On, ResMut, Resource};
    use weave_compiler::{CompileOptions, compile};

    use super::*;

    #[derive(Resource, Default)]
    struct EventLog(Vec<String>);

    fn ready(_: On<StoryReady>, mut log: ResMut<EventLog>, mut commands: Commands) {
        log.0.push("ready".to_owned());
        commands.weave_continue();
    }

    fn line(line: On<DeliverLine>, mut log: ResMut<EventLog>) {
        log.0.push(format!("line:{}", line.text));
    }

    fn reloaded(_: On<StoryReloaded>, mut log: ResMut<EventLog>) {
        log.0.push("reloaded".to_owned());
    }

    fn failed(error: On<StoryFailed>, mut log: ResMut<EventLog>) {
        log.0.push(format!("failed:{}", error.message));
    }

    fn test_asset(source: &str) -> WeaveAsset {
        WeaveAsset {
            story: compile(source, &CompileOptions::default())
                .expect("fixture should compile")
                .story,
        }
    }

    #[test]
    fn plugin_emits_ready_before_line_and_preserves_hot_reload_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), WeavePlugin))
            .init_resource::<EventLog>()
            .add_observer(ready)
            .add_observer(line)
            .add_observer(reloaded)
            .add_observer(failed);

        let handle = app
            .world_mut()
            .resource_mut::<Assets<WeaveAsset>>()
            .add(test_asset(
                "VAR count = 1\n=== start ===\nFirst {count}.\n-> END\n",
            ));
        let mut story = WeaveStory::new("test.weave");
        story.handle = Some(handle.clone());
        app.insert_resource(story);
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<EventLog>().0,
            ["ready", "line:First 1."]
        );

        app.world_mut()
            .resource_mut::<Assets<WeaveAsset>>()
            .insert(
                handle.id(),
                test_asset("VAR count = 1\n=== start ===\nChanged {count}.\n-> END\n"),
            )
            .expect("active asset generation should remain valid");
        app.world_mut()
            .write_message(AssetEvent::Modified { id: handle.id() });
        app.update();
        assert!(
            app.world()
                .resource::<EventLog>()
                .0
                .contains(&"reloaded".to_owned())
        );
    }

    #[test]
    fn commands_surface_runtime_failures_as_observer_events() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), WeavePlugin))
            .init_resource::<EventLog>()
            .add_observer(failed)
            .add_systems(Update, |mut commands: Commands| {
                commands.weave_continue();
            });
        app.insert_resource(WeaveStory::new("missing.weave"));
        app.update();
        assert!(
            app.world()
                .resource::<EventLog>()
                .0
                .iter()
                .any(|entry| entry.contains("not ready"))
        );
    }

    #[test]
    fn deliver_choices_dereferences_to_choice_slice() {
        let event = DeliverChoices {
            choices: vec![ChoiceView {
                id: "choice".to_owned(),
                text: "Choose".to_owned(),
                once: true,
            }],
        };
        assert_eq!(event.len(), 1);
        assert_eq!(event[0].text, "Choose");
    }

    #[test]
    fn asset_id_type_is_stable_for_active_handle_comparison() {
        fn accepts_id(_: AssetId<WeaveAsset>) {}
        let handle = Handle::<WeaveAsset>::default();
        accepts_id(handle.id());
    }
}
