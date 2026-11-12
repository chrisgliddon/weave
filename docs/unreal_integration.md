# Unreal Engine integration

Weave runs in Unreal Engine 5.9 and later through two pieces:

- `crates/weave-ffi`, a C ABI over the same `Player` the browser runtime uses. Every call takes and
  returns UTF-8 JSON, catches panics, and reports errors as `{"ok": false, "error": "..."}`.
- `integrations/unreal/Weave`, an engine plugin that loads that library at runtime and exposes it to
  C++ and Blueprints.

## Build and stage the runtime library

```bash
integrations/unreal/stage-ffi.sh /path/to/YourProject/Plugins/Weave
```

The script builds `weave-ffi` in release mode and copies `libweave_ffi.so`, `libweave_ffi.dylib`
or `weave_ffi.dll` to `Binaries/ThirdParty/Weave/<Platform>/` inside the plugin. Copy
`integrations/unreal/Weave` into your project's `Plugins/` folder first, then build the project's
editor target as usual. The plugin declares the library as a runtime dependency, so packaged builds
carry it.

## Story assets

`UWeaveStory` holds compiled story JSON (`weavec <file> --format json`). The editor module imports
`.weave` files directly by running `weavec`; it looks for the compiler in the `WEAVEC` environment
variable, then on `PATH`, then in `~/.cargo/bin`. Scripts can also set the JSON themselves:

```python
story = unreal.AssetToolsHelpers.get_asset_tools().create_asset(
    "WS_Intro", "/Game/Stories", unreal.WeaveStory, unreal.WeaveStoryFactoryNew())
story.set_compiled_json(open("intro.json").read(), "intro.json")
unreal.EditorAssetLibrary.save_loaded_asset(story)
```

## Playing a story

`UWeaveStoryPlayer` mirrors the runtime API:

| Call | Result |
|---|---|
| `CreateWeaveStoryPlayer(Story, Seed, Error)` | a player, or null with `Error` set |
| `Continue()` | an `FWeaveFrame`: `Line` (text), `Choices`, `Ended` or `Error` |
| `GetChoices()`, `Choose(Index, Error)` | the open choices; pick one by position |
| `CanContinue()`, `Restart(Seed)` | flow control |
| `SaveState()`, `RestoreState(State, Error)` | serialized runtime state as a string |

Frames also carry the raw presentation and pattern-draw JSON for games that use those features.

## A ready-made reader

`AWeaveStoryTextActor` plays a story as world-space text: each line types on, the choices appear,
and the picked choice is highlighted. `ChoicePlan` lists which choice to take at each branch
(entries past its end choose by seed), so a playthrough is repeatable. Two command-line switches
turn it into a recorder:

- `-WeaveCapture=<dir>` saves every frame as `frame_00000.png` and onwards.
- `-WeaveQuitWhenDone` exits when the story ends, or shortly after showing an error.

Run with a fixed time step and no window, then encode:

```bash
UnrealEditor YourProject.uproject /Game/Maps/StoryStage -game -RenderOffscreen \
  -ResX=1280 -ResY=720 -benchmark -fps=30 -nosound -unattended \
  -WeaveCapture=/tmp/frames -WeaveQuitWhenDone
ffmpeg -framerate 30 -i /tmp/frames/frame_%05d.png -c:v libx264 -pix_fmt yuv420p story.mp4
```

The first run on a fresh machine compiles shaders; render twice if early frames look unfinished.

## ABI versioning

`weave_ffi_abi_version()` returns `WEAVE_FFI_ABI_VERSION` (currently 1). The plugin refuses a
library whose version differs. Strings returned by the library are freed with
`weave_string_free`, players with `weave_player_free`.
