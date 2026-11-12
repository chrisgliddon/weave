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
| `GetString(Table, Key, Error)` | an `FWeaveString` from a `STRINGS` table |
| `GetChannels()` | every declared channel as `FWeaveChannel`, keyed by name |

A `Line` frame carries the line's stable `Id`, its `Channel`, the current `Speaker` (empty while
narrating), bare `Tags` and `key:value` `Meta`, plain `Text`, `RichText` ready for a Rich Text
Block, styled `Spans`, and timed `Beats`. Span and beat offsets are UTF-16 indexes into `Text`, so
they line up with `FString`. Choices carry their `Tags` and `Meta` too. Frames also carry the raw
presentation and pattern-draw JSON for games that use those features.

`RichText` uses `<style>text</>`: add a row named after each style (`em`, `b`, `title`...) to the
Rich Text Block's text style set. Text and punctuation that would read as markup arrive escaped as
`&amp;`, `&lt;`, `&gt;`, and `&quot;`.

## Exporting text for localization and voice

```bash
weavec ids --stamp story.weave          # pin every default line id into the source
weavec export --unreal Content/Story story.weave
```

The export writes:

- `StringTable_<table>.csv` for each `STRINGS` table, with `Key`, `SourceString`, and `Comment`
  columns, ready for String Table import. `SourceString` is Rich Text markup when the entry has
  styles; `Comment` holds the entry's tags.
- `Lines.csv` and `Lines.json`, one row per narrative line, ready to import as a DataTable with the
  `FWeaveLineRow` row struct. Row names are line ids; columns are `Knot`, `Channel`, `Speaker`,
  `Text`, `Rich`, `Tags`, `Meta` (`key=value` separated by `;`), and `VoFile`.
- `vo/<speaker>.md`, a voice script per speaker (`narration` for unvoiced lines) with each line's
  id, text with `[pause N s]` markers, tag direction, and suggested file name
  (`VO_<speaker>_<id with dots as underscores>`).

Speakers and channels in the export come from reading the source in order, because the runtime
decides them only while playing. The opening knot starts from narration; every other knot, and any
line after a thread or a branch that changes them, is uncertain. The export prints one warning per
knot with uncertain lines; starting each knot with `@speaker` and `@to` makes every row exact. It
also warns when two lines would share a voice file, or when names differ only in case.

Choice labels are not exported. They have no `^id:`, so they are not yet part of string tables or
voice scripts.

## A ready-made reader

`AWeaveStoryTextActor` plays a story as world-space text: each line types on, the choices appear,
and the picked choice is highlighted. Typing holds on every `<pause N>`, uses the channel's `cps`
when it declares one (else `CharactersPerSecond`), and shows lines on `animated: false` channels
all at once, holding them for their pauses. `bShowSpeaker` prefixes lines with the speaker, and
`ChannelColors` tints lines per channel. `ChoicePlan` lists which choice to take at each branch
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

`weave_ffi_abi_version()` returns `WEAVE_FFI_ABI_VERSION` (currently 2). The plugin refuses a
library whose version differs; rebuild and restage the library after updating the plugin. ABI 2
adds `weave_player_string(handle, table, key)` and `weave_player_channels(handle)`, and line frames
carry the structured fields described in [`json_format.md`](json_format.md#line-frames). Strings returned by the library are freed with
`weave_string_free`, players with `weave_player_free`.
