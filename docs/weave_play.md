# Weave Play presentation contract

Status: **implemented slice landed; full contract deferred.** The first vertical
slice is compiled into story IR version 5 and drained by the runtime. The
remaining presentation document — layout, fit and anchor, slots, transitions,
the asset manifest, conditions, and step keys — is still a later contract. The
[Decisions](#decisions) log records the change and why.

Weave Play renders a compiled story as a staged scene instead of a diagnostic
transcript. This document defines the portable presentation contract that the
editor, Bevy, and browser renderers share. It defines what a scene looks like
and when, never what a story does.

## Scope

In scope for the implemented slice:

- background scenes and character stages declared in source;
- speaker selection and nameplate data;
- ordered presentation cues delivered alongside story events; and
- project-relative, safe asset path validation.

Deferred to the later presentation document:

- named stage slots, stage geometry, layering, fit, and anchor;
- transitions and their timing;
- dialogue framing, text reveal, and choice presentation;
- presentation conditions over story variables;
- an asset manifest with hashes, licences, and provenance; and
- step keys and a story-hash binding.

Out of scope, and deliberately not modelled:

- player-controlled movement, camera control, collision, and physics;
- inventory, character-sheet, or map interfaces;
- save-game management, settings screens, and menus;
- audio, voice, and localized rendering; and
- any decision that changes story state, control flow, choice order, or text.

A host that needs free-roaming movement renders it outside this contract. Weave
Play is a staged scene: the camera does not move, and the author advances the
scene by advancing the story.

## What compiles today

Presentation is not a sibling `weave.play` document yet. Scene and stage
declarations, and the `@scene`, `@show`, `@hide`, and `@speaker` directives,
compile into `StoryIr` version 5:

- `scene` and `stage` declarations lower into the sorted `presentation` map;
- each directive lowers into a `Cue` instruction in its knot, in source order;
- the runtime executes cue instructions without ending the current advance and
  queues a `PresentationEvent` for each one; and
- a host drains those events with `Story::take_presentation()` before rendering
  the line or choice set that follows.

Naming an undeclared scene or stage is a compile error. Asset paths are
validated at compile time and must be relative and safe.

This slice changes the IR contract, so IR version moved from 4 to 5. It does not
yet carry any renderer vocabulary beyond scene and stage identity, a background
or image path, an optional display name, and the four cue kinds. Layout, fit,
anchor, slots, transitions, conditions, and hashes remain a separate contract so
a visual adjustment does not force another story-format bump.

## Example

This is a complete scene using only implemented syntax:

```weave
scene ferry_dusk {
    background: "art/ferry_dusk.png"
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
}

stage keeper {
    image: "art/keeper_neutral.png"
    name: "The Keeper"
}

=== start ===
@scene ferry_dusk
@show mara
@speaker mara
The ferry leaves at dusk.

+ [Board the ferry]
    -> deck

+ [Ask about the crossing]
    -> ask

=== ask ===
@show keeper
@speaker keeper
The water remembers every passenger.
-> deck

=== deck ===
@hide keeper
@hide mara
@speaker
Then we had better give it something to remember.
-> END
```

The compiled `StoryIr` excerpt is:

```ron
StoryIr(
    version: 5,
    entry: "start",
    // ...
    presentation: PresentationIr(
        scenes: {
            "ferry_dusk": SceneIr(
                background: Some("art/ferry_dusk.png"),
            ),
        },
        stages: {
            "keeper": StageIr(
                image: Some("art/keeper_neutral.png"),
                name: Some("The Keeper"),
            ),
            "mara": StageIr(
                image: Some("art/mara_neutral.png"),
                name: Some("Mara"),
            ),
        },
    ),
    knots: {
        "start": KnotIr(
            content: [
                Instruction(
                    span: Span(
                        start: 217,
                        end: 234,
                        line: 16,
                        column: 1,
                    ),
                    kind: InstructionKind(
                        kind: cue,
                        value: CueIr(
                            kind: scene,
                            value: "ferry_dusk",
                        ),
                    ),
                ),
                // ...
            ],
        ),
        // ...
    },
)
```

That excerpt was produced by compiling the example above. Instruction and cue
values are adjacently tagged: `InstructionKind` names the instruction family and
`CueIr` carries the cue kind and target. The span values are the compiler's byte
offsets into that source.

The same presentation definitions in JSON:

```json
{
  "version": 5,
  "presentation": {
    "scenes": {
      "ferry_dusk": { "background": "art/ferry_dusk.png" }
    },
    "stages": {
      "keeper": { "image": "art/keeper_neutral.png", "name": "The Keeper" },
      "mara": { "image": "art/mara_neutral.png", "name": "Mara" }
    }
  }
}
```

## Cues

A cue is one runtime event with a kind and an optional target.

| Kind | Target | Effect |
|---|---|---|
| `scene` | scene id | Replace the background. |
| `show` | stage id | Make a declared stage visible. |
| `hide` | stage id | Remove a stage. |
| `speaker` | stage id or none | Select the nameplate source; none is narration. |

Cues apply in source order. A host applies the cues drained for one advance
before presenting that advance's line or choices. The runtime never reorders,
relabels, filters, or disables a choice, and presentation never feeds back into
story state.

Consequences a host may rely on:

- Replaying the same compiled story with the same seed and choices emits the
  same ordered cue sequence.
- Presentation timing never feeds back into story state.
- Two renderers may interpolate transitions differently and still agree on cue
  sequence and final staging.

## Deferred syntax

The following source syntax and document members are part of the later contract
and are **not accepted by the current compiler**. They are listed so hosts and
authors do not mistake them for shipped features.

| Deferred construct | Why it is deferred |
|---|---|
| `@move <stage> to <slot>` | Needs the named-slot geometry contract. |
| `fit:` on a scene | Needs a fit/anchor contract common to every renderer. |
| `slot:` on a stage | Needs the same slot contract as `@move`. |
| `@swap <stage> <variant>` | Needs an image-variant model. |
| `transition:` on a cue | Needs transition timing and reduced-motion rules. |
| `when:` conditions | Needs condition evaluation and typed path resolution. |
| `weave.play.ron` / `weave.play.json` | Needs the full sibling document and story-hash binding. |
| Asset manifest with hashes and licences | Needs the publication and provenance gate below. |

## Assets

Every asset reference is a normalized project-relative path using `/`
separators. The checker rejects absolute paths, drive letters, URL schemes,
percent encoding, backslashes, control characters, `.` and `..` segments, empty
segments, trailing `/`, Windows reserved device names, and non-ASCII characters.
Only ASCII letters, digits, `_`, `-`, `.`, and `/` are permitted. A rejected
path produces a source-spanned diagnostic on the offending string literal.

Resolution happens against an explicit project asset root. A host receives the
root from the project, never from the document. The document cannot name a
location outside the project, and a reference that escapes the root is a
diagnostic, not a fallback.

The later manifest will declare each asset with a kind, a SHA-256 hash, a
licence expression, and a provenance value:

- `Original` — authored for this project, with no third-party lineage.
- `Bundled` — a redistributed third-party artifact; requires an allowlist entry
  in the repository that also names the exact upstream artifact and revision.

A hash mismatch, a missing entry, or an undeclared reference is a diagnostic. A
referenced file that is absent at load time is an error state, never a silent
skip.

### Fallbacks

The later contract assigns each scene and stage a fallback, used when the asset
is missing, unreadable, unsupported, or fails its hash check:

- `Ink` — the scene renders as a flat ink field with a 1px paper rule frame.
- `Silhouette` — the stage renders as a flat ink figure carrying the stage's
  display name.

Fallbacks are always drawable without assets, so a scene never renders an empty
frame. A fallback replaces only the missing element; the rest of the staging
stands.

## Visual language

The Weave Play look is original, high-contrast, and monochrome. It has no hue:
emphasis comes from weight, size, inverse blocks, and rules, never colour.

| Role | Foreground | Background |
|---|---|---|
| Body text | `#101010` | `#F2F0EA` |
| Inverse text | `#F2F0EA` | `#101010` |
| Rules and frames | `#101010` | `#F2F0EA` |

Ink on paper measures approximately 16.7:1, comfortably above the 7:1 required
for body text. Requirements:

- Body text meets at least 7:1; large text and interface boundaries at least
  3:1.
- The focus indicator is a 2px inverse outline with a 1px offset, legible
  against both the paper and ink fields.
- No gradients, textures, drop shadows, or photographic character art.
- Characters are flat ink silhouettes or line figures until an author supplies
  original artwork.
- Transitions are at most 240ms and collapse to `cut` under reduced motion.
- Type is set in one bundled family, with a bundled monospaced companion for
  counts, identifiers, and debug values. The family is fixed by the project, not
  inherited from the host, so the scene renders identically on every platform.

The nameplate is a solid ink block with inverse text, sitting above the dialogue
measure. Narration uses no nameplate.

## Layout states

The renderer selects a layout state from the available width in logical pixels:

| State | Width |
|---|---|
| `compact` | below 640 |
| `regular` | 640 to 1023 |
| `wide` | 1024 and above |

`margin` is the horizontal inset. `measure` is the maximum line length in
characters, not pixels, so reflow is stable across font sizes. `dialogue_scale`
and `choice_scale` multiply the base type size.

Regardless of state:

- dialogue never clips; it wraps within the measure and grows upward from the
  bottom safe area;
- every choice is reachable, and a choice set longer than `max_visible` scrolls
  with a visible affordance rather than shrinking type;
- stage positions occupy the upper two thirds and never overlap the dialogue
  measure; and
- the scene scales to the viewport without letterboxing the dialogue.

Stage slots are named positions on a normalized grid: `far_left`, `left`,
`center`, `right`, `far_right`. A slot's geometry is the renderer's choice; the
name is the later contract's vocabulary. Until that contract lands, a host may
place stages with its own policy as long as cue order is preserved.

## Renderer responsibilities

| Guarantee | Owner |
|---|---|
| Cue sequence and final staging for a given story, seed, and choices | Story runtime plus this contract |
| Scene and stage identity and declared asset paths | Story IR v5 |
| Dialogue and choice text, order, and identifiers | Story runtime |
| Layout state selection thresholds | Contract |
| Fallback selection when an asset is unavailable | Later contract |
| Font rasterization, hinting, and exact pixel metrics | Renderer |
| Easing curves and interpolation between transition frames | Renderer |
| Texture filtering, scaling filters, and colour management | Renderer |
| Window chrome, menus, and author controls | Renderer |
| Audio, voice, and haptics | Out of scope |

Two renderers are conformant when, for the same compiled story, seed, and
choices, they emit the same cue sequence and reach the same final staging. They
are not required to produce identical pixels.

## Diagnostics

Implemented presentation diagnostics use the shared `W` code space with a
severity, a source span, and a remediation sentence.

| Code | Meaning |
|---|---|
| `W1040` | Unknown presentation directive. |
| `W1041` | Scene repeats its `background` setting. |
| `W1042` | Stage repeats an `image` or `name` setting. |
| `W1043` | Presentation directive is missing its cue target name. |
| `W2200` | Duplicate scene declaration. |
| `W2201` | Duplicate stage declaration. |
| `W2202` | Unknown scene reference. |
| `W2203` | Unknown stage reference. |
| `W2204` | Invalid or unsafe asset path. |

`W1040` through `W1043` are reported while parsing presentation syntax;
`W2200` through `W2204` are reported while checking it. All nine are compile
errors and prevent output.

The later presentation document will use stable `P` codes:

| Code | Meaning |
|---|---|
| `P001` | Referenced asset is missing. |
| `P002` | Asset path escapes the project root. |
| `P003` | Asset kind is unsupported by this renderer. |
| `P004` | Unknown scene, stage, slot, or variant reference. |
| `P005` | Condition names an unknown or wrongly typed path. |
| `P006` | Duplicate or missing cue order within a step. |
| `P007` | Unsupported contract version. |
| `P008` | Asset hash mismatch or duplicate manifest entry. |
| `P009` | Missing fallback for a scene or stage. |
| `P010` | Presentation compiled against a different story hash. |

`P001`, `P002`, `P003`, `P008`, `P009`, and `P010` are load-time failures for
the affected element. The element falls back, the frame is retained, and the
diagnostic is surfaced without obscuring the scene. `P004` through `P007` are
compile-time failures.

## Empty and error states

| State | Behaviour |
|---|---|
| Story with no presentation definitions | The story runs with an ink field, no stages, and the default layout. Lines and choices render normally. |
| Scene with no stages | Background only; dialogue anchored to the bottom safe area. |
| Advance with no cues | The previous presentation persists. |
| Stage declared but never shown | Nothing renders; the definition is inert. |
| Missing asset | Fallback for that element once the fallback contract lands; until then the renderer decides. |
| Runtime failure during an advance | Every cue the runtime produced before the failure is applied in source order, then the failure is surfaced. A host never discards those cues and never defers them to a later frame. |
| Compile failure while playing | The last valid frame is retained and a non-blocking diagnostic banner appears. Playback resumes on the next successful compile. |
| Story hash mismatch | Deferred until the sibling document exists. |

## Compatibility

- Any added, removed, renamed, or redefined serialized field or enum variant
  requires a new IR version and a new schema. Adding scenes, stages, and cue
  instructions therefore moved story IR from 4 to 5.
- RON and JSON encode one model and must decode to equal values.
- Map member order carries no meaning; the compiler emits sorted maps so diffs
  and reviews stay stable.
- Unknown members and unknown enum variants are rejected, not ignored. A host
  must not treat a tolerant decoder as version negotiation.
- When the sibling document lands, `story.hash` will bind it to one compiled
  story; recompiling a story produces a new presentation document and a stale
  one is refused.
- Playtest records are a separate format with a separate compatibility
  guarantee (`bs6g7xs8`). A playtest record refers to a contract version but
  never embeds one.

## Licence and provenance gate

Before an asset or dependency ships with the repository:

- Every manifest entry carries a licence expression and a provenance value.
- Original assets name their author and record that they are synthetic.
- Bundled third-party assets require an allowlist entry naming the exact
  upstream artifact, revision, and hash, and their licence text and notices
  ship beside the project.
- A typeface is a bundled dependency carrying its own licence. Weave Play ships
  IBM Plex Sans with IBM Plex Mono for numerals and identifiers, both under SIL
  OFL 1.1. The licence text ships beside the binaries, the family is recorded in
  the asset allowlist with a hash, and the font files are never presented as
  MIT-only.
- A font may ship only when its licence permits redistribution and its licence
  text and notices ship with it. A family that cannot meet that bar is not used,
  and the renderer does not silently fall back to a host font.
- No third-party interface assets, icons, illustrations, page furniture, or
  trade dress may be reproduced. The monochrome language above is original and
  is not derived from any existing visual novel or writing application.
- Renderer dependencies are audited for licence compatibility before they are
  added.

## Decisions

All seven were reviewed and resolved on 2026-10-30. Decisions 1, 2, and 4
through 7 confirm the proposal; decision 3 changed it. Decision 1 was later
revised when the first slice landed.

1. **Presentation document.** Originally resolved as a sibling `weave.play`
   document so presentation changes never bump the story IR version. Revised:
   the implemented slice compiles scene and stage definitions and the four cue
   instructions into `StoryIr` version 5, because the runtime must deliver them
   with story events and because a host cannot stage a line without them. The
   fuller document — layout, fit and anchor, slots, transitions, conditions, the
   asset manifest, and the story-hash binding — remains a later contract, so
   visual adjustments still do not bump the story IR. Adding the slice required
   IR version 5 and a new schema.
2. **Stage geometry.** Resolved: named slots on a normalized grid, with the
   renderer owning coordinates. Authored coordinates were rejected because they
   make every layout change an authoring migration. Not yet implemented.
3. **Typeface.** Resolved: bundle a font rather than inherit the host stack.
   Weave Play ships IBM Plex Sans and IBM Plex Mono under SIL OFL 1.1, with the
   licence text and notices beside the binaries and an allowlist hash entry.
   Identical rendering on every platform is worth the redistribution obligation.
4. **Text reveal.** Resolved: 18ms per character, always skippable, with
   auto-advance disabled by default. When enabled, auto-advance is bounded and
   pauses on choices. Not yet implemented.
5. **Condition scope.** Resolved: conditions may read story variables and
   domain-module exports but may never write. Typed World and Character
   selection is deferred to `xpegt223` and reuses the same mechanism. Not yet
   implemented.
6. **Cue granularity.** Resolved: cues attach to a step, not to a character,
   word, or animation frame. Finer granularity would put presentation timing
   inside story ordering. The implemented slice delivers one ordered cue list per
   advance, which is the same granularity.
7. **Choice layout.** Resolved: a vertical list, at most five visible before
   scrolling, never reordered or relabelled by presentation. Not yet
   implemented.

Decisions 2, 3, and 4 constrain the visual direction and are binding on the
editor review.
