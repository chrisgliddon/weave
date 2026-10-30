# Weave Play presentation contract

Status: **approved for implementation.** The seven contract decisions were
reviewed and resolved on 2026-10-30. Six confirmed the proposal; one changed it,
and the typeface is now bundled rather than inherited from the host. The
resolved answers are recorded in [Decisions](#decisions).

Weave Play renders a compiled story as a staged scene instead of a diagnostic
transcript. This document defines the portable presentation contract that the
editor, Bevy, and browser renderers share. It defines what a scene looks like
and when, never what a story does.

Contract version `1`. The document is authored as RON or JSON and the two forms
decode to the same value.

## Scope

In scope:

- backgrounds, named stage slots, character images and variants, layering;
- speaker selection and nameplate data;
- dialogue framing, text reveal, and choice presentation;
- a bounded set of transitions;
- presentation conditions over story variables; and
- deterministic ordering of every presentation cue.

Out of scope, and deliberately not modelled:

- player-controlled movement, camera control, collision, and physics;
- inventory, character-sheet, or map interfaces;
- save-game management, settings screens, and menus;
- audio, voice, and localized rendering; and
- any decision that changes story state, control flow, choice order, or text.

A host that needs free-roaming movement renders it outside this contract. Weave
Play is a staged scene: the camera does not move, and the author advances the
scene by advancing the story.

## Why the contract is a sibling document

Presentation compiles to a `weave.play.ron` or `weave.play.json` document
emitted beside the story, not into `StoryIr`.

`StoryIr` is a language and runtime contract at version 4, and
[json_format](json_format.md#compatibility-contract) states that any change to a
serialized field or enum variant requires a new IR version and schema. Folding
stage geometry, fit, anchor, and transition timing into the IR would mean a new
story format version for every visual adjustment, and would force every runtime
host to accept a rendering vocabulary it may never use.

The cost of a sibling document is that two artifacts must agree. The contract
closes that gap with an explicit story hash and a generated join key, both
described under [Step identity](#step-identity-and-cue-ordering). A host may
load a story without presentation, and the editor may load presentation without
running a story, but a host must never load a presentation document that was
compiled against a different story.

## Model

The document has six members: the contract version, the story binding, the
asset manifest, the base presentation, the scene and stage definitions, and the
per-step cues.

### Example

This is a complete branching scene. The source is on the left and the compiled
contract is below it.

```weave
scene ferry_dusk {
    background: "art/ferry_dusk.png"
    fit: cover
}

stage mara {
    image: "art/mara_neutral.png"
    slot: left
    name: "Mara"
}

stage keeper {
    image: "art/keeper_neutral.png"
    slot: right
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
@move mara to center
@speaker mara
Then we had better give it something to remember.
-> END
```

Hashes below are abbreviated for readability; a real document carries full
SHA-256 digests.

```ron
(
    version: 1,
    story: (
        name: "ferry",
        hash: "sha256:0f3c9a5e",
    ),
    assets: {
        "art/ferry_dusk.png": (
            kind: Image,
            sha256: "9a1e4c07",
            licence: "CC0-1.0",
            provenance: Original,
        ),
        "art/keeper_neutral.png": (
            kind: Image,
            sha256: "31b8f2aa",
            licence: "CC0-1.0",
            provenance: Original,
        ),
        "art/mara_neutral.png": (
            kind: Image,
            sha256: "5d0c7e61",
            licence: "CC0-1.0",
            provenance: Original,
        ),
    },
    base: (
        scene: None,
        stages: [],
    ),
    scenes: {
        "ferry_dusk": (
            background: Some("art/ferry_dusk.png"),
            fit: Cover,
            anchor: (x: 0.5, y: 0.5),
            fallback: Ink,
        ),
    },
    stages: {
        "keeper": (
            image: Some("art/keeper_neutral.png"),
            slot: Right,
            name: "The Keeper",
            fallback: Silhouette,
        ),
        "mara": (
            image: Some("art/mara_neutral.png"),
            slot: Left,
            name: "Mara",
            fallback: Silhouette,
        ),
    },
    steps: {
        "ask#0": [
            (order: 0, when: None, cue: (kind: "show", value: (stage: "keeper", transition: (kind: "fade", value: (ms: 180))))),
            (order: 1, when: None, cue: (kind: "speaker", value: (stage: Some("keeper")))),
        ],
        "deck#0": [
            (order: 0, when: None, cue: (kind: "hide", value: (stage: "keeper", transition: (kind: "fade", value: (ms: 180))))),
            (order: 1, when: None, cue: (kind: "move", value: (stage: "mara", to: Center, transition: (kind: "slide", value: (ms: 220))))),
            (order: 2, when: None, cue: (kind: "speaker", value: (stage: Some("mara")))),
        ],
        "start#0": [
            (order: 0, when: None, cue: (kind: "scene", value: (id: "ferry_dusk", transition: (kind: "cut")))),
            (order: 1, when: None, cue: (kind: "show", value: (stage: "mara", transition: (kind: "cut")))),
            (order: 2, when: None, cue: (kind: "speaker", value: (stage: Some("mara")))),
        ],
    },
    layout: (
        states: {
            compact: (margin: 16, measure: 44, dialogue_scale: 1.0, choice_scale: 1.0),
            regular: (margin: 32, measure: 62, dialogue_scale: 1.0, choice_scale: 1.0),
            wide: (margin: 48, measure: 72, dialogue_scale: 1.125, choice_scale: 1.0),
        },
        dialogue: (
            reveal: (kind: "per_character", value: (delay_ms: 18)),
            skip: Allowed,
            auto_advance: Disabled,
        ),
        choices: (
            arrangement: Vertical,
            max_visible: 5,
        ),
    ),
)
```

The same document in JSON:

```json
{
  "version": 1,
  "story": { "name": "ferry", "hash": "sha256:0f3c9a5e" },
  "assets": {
    "art/ferry_dusk.png": {
      "kind": "image", "sha256": "9a1e4c07",
      "licence": "CC0-1.0", "provenance": "original"
    },
    "art/keeper_neutral.png": {
      "kind": "image", "sha256": "31b8f2aa",
      "licence": "CC0-1.0", "provenance": "original"
    },
    "art/mara_neutral.png": {
      "kind": "image", "sha256": "5d0c7e61",
      "licence": "CC0-1.0", "provenance": "original"
    }
  },
  "base": { "scene": null, "stages": [] },
  "scenes": {
    "ferry_dusk": {
      "background": "art/ferry_dusk.png",
      "fit": "cover",
      "anchor": { "x": 0.5, "y": 0.5 },
      "fallback": "ink"
    }
  },
  "stages": {
    "keeper": {
      "image": "art/keeper_neutral.png",
      "slot": "right",
      "name": "The Keeper",
      "fallback": "silhouette"
    },
    "mara": {
      "image": "art/mara_neutral.png",
      "slot": "left",
      "name": "Mara",
      "fallback": "silhouette"
    }
  },
  "steps": {
    "ask#0": [
      { "order": 0, "when": null, "cue": { "kind": "show",
        "value": { "stage": "keeper", "transition": { "kind": "fade", "value": { "ms": 180 } } } } },
      { "order": 1, "when": null, "cue": { "kind": "speaker",
        "value": { "stage": "keeper" } } }
    ],
    "deck#0": [
      { "order": 0, "when": null, "cue": { "kind": "hide",
        "value": { "stage": "keeper", "transition": { "kind": "fade", "value": { "ms": 180 } } } } },
      { "order": 1, "when": null, "cue": { "kind": "move",
        "value": { "stage": "mara", "to": "center",
          "transition": { "kind": "slide", "value": { "ms": 220 } } } } },
      { "order": 2, "when": null, "cue": { "kind": "speaker",
        "value": { "stage": "mara" } } }
    ],
    "start#0": [
      { "order": 0, "when": null, "cue": { "kind": "scene",
        "value": { "id": "ferry_dusk", "transition": { "kind": "cut" } } } },
      { "order": 1, "when": null, "cue": { "kind": "show",
        "value": { "stage": "mara", "transition": { "kind": "cut" } } } },
      { "order": 2, "when": null, "cue": { "kind": "speaker",
        "value": { "stage": "mara" } } }
    ]
  },
  "layout": {
    "states": {
      "compact": { "margin": 16, "measure": 44, "dialogue_scale": 1.0, "choice_scale": 1.0 },
      "regular": { "margin": 32, "measure": 62, "dialogue_scale": 1.0, "choice_scale": 1.0 },
      "wide": { "margin": 48, "measure": 72, "dialogue_scale": 1.125, "choice_scale": 1.0 }
    },
    "dialogue": {
      "reveal": { "kind": "per_character", "value": { "delay_ms": 18 } },
      "skip": "allowed",
      "auto_advance": "disabled"
    },
    "choices": { "arrangement": "vertical", "max_visible": 5 }
  }
}
```

### Members

| Member | Meaning |
|---|---|
| `version` | Contract version. A host rejects any other value. |
| `story` | The compiled story this document belongs to: display name and content hash. |
| `assets` | Every referenced asset, keyed by its normalized project-relative path. |
| `base` | Presentation in force before the first cue: no scene, no stages. |
| `scenes` | Background definitions keyed by scene id. |
| `stages` | Character definitions keyed by stage id. |
| `steps` | Cues keyed by step key. |
| `layout` | Typography, margins, reveal, and choice arrangement per layout state. |

### Cues

A cue is an adjacently tagged object with a `kind` and a `value`.

| Kind | Value | Effect |
|---|---|---|
| `scene` | `id`, `transition` | Replace the background. |
| `show` | `stage`, `transition` | Place a stage slot on screen. |
| `hide` | `stage`, `transition` | Remove a stage slot. |
| `move` | `stage`, `to`, `transition` | Move a visible slot to another slot. |
| `swap` | `stage`, `variant`, `transition` | Replace a slot's image without leaving the scene. |
| `speaker` | `stage` | Select the nameplate source; `None` is narration. |

A transition is `cut`, or `fade`, `slide`, or `wipe` with a duration in
milliseconds. The duration is a presentation hint only.

Dialogue text and choice text do not appear in this document. Both come from
`StoryEvent` at runtime: `Line` and `Choices`. Duplicating them would let
presentation and story diverge, and would make the contract a partial copy of
the story rather than a description of how to stage it.

Consequently the presentation layer never reorders, relabels, filters, or
disables a choice. Choice identifiers and order are story semantics, and
`StoryEvent::Choices` already carries both.

## Step identity and cue ordering

A step is one point where the runtime presents something: a narrative line, or
a pending choice set. Each step has a key of the form `<knot>#<index>`, where
`index` counts presentable instructions within that knot.

Step keys are generated by the compiler. They are not an authoring surface, and
the compiled document is the only place they appear.

Within a step, cues carry an explicit `order` that is unique inside that step.
Across steps, cue sequence follows execution order, never document order. A host
applies cues for a step before presenting that step's line or choices.

Consequences a host may rely on:

- Replaying the same compiled story with the same seed and choices emits the
  same ordered cue sequence.
- Presentation timing never feeds back into story state.
- Two renderers may interpolate transitions differently and still agree on cue
  sequence and final staging.

Because keys are generated, they are stable only against the same compiled
story. That is why the document records `story.hash`: a presentation document
compiled against a different story is rejected rather than silently mis-staged.

## Presentation conditions

A cue may carry a `when` condition. Conditions are read-only: they select among
cues for a step and can never assign, mutate, or advance the story.

```ron
(
    order: 0,
    when: Some((kind: "compare", value: (
        path: "visits",
        op: Gte,
        value: (kind: "number", value: 2),
    ))),
    cue: (kind: "show", value: (stage: "keeper", transition: (kind: "cut"))),
)
```

A condition is one of `all`, `any`, `not`, or `compare`. A comparison names a
story or module path, an operator (`eq`, `ne`, `lt`, `lte`, `gt`, `gte`), and a
typed literal. Resolution rules:

- Paths resolve against the same variables the runtime exposes, including
  namespaced domain-module exports.
- A path that does not exist at compile time is a compile error, not a silent
  `false`.
- A value whose type does not match the operator is a compile error.
- A condition that evaluates false skips its cue; the previous presentation
  persists.
- Conditions are evaluated at the step boundary, before any unconditional cue
  for that step.

Typed domain values select presentation; they do not enter the story's control
flow. Driving cues from Weave World and Weave Character data is tracked
separately in `xpegt223` and reuses this mechanism unchanged.

## Assets

Every asset reference is a normalized project-relative path using `/`
separators. Absolute paths, drive letters, URL schemes, `..` segments, and
symlinks that resolve outside the project root are rejected.

Resolution happens against an explicit project asset root. A host receives the
root from the project, never from the document. The document cannot name a
location outside the project, and a reference that escapes the root is a
diagnostic, not a fallback.

Assets are declared in `assets` with a kind, a SHA-256 hash, a licence
expression, and a provenance value:

- `Original` — authored for this project, with no third-party lineage.
- `Bundled` — a redistributed third-party artifact; requires an allowlist entry
  in the repository that also names the exact upstream artifact and revision.

A hash mismatch, a missing entry, or an undeclared reference is a diagnostic. A
referenced file that is absent at load time is an error state, never a silent
skip.

### Fallbacks

Each scene and stage declares a fallback, used when the asset is missing,
unreadable, unsupported, or fails its hash check:

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
- stage slots occupy the upper two thirds and never overlap the dialogue
  measure; and
- the scene scales to the viewport without letterboxing the dialogue.

Stage slots are named positions laid out on a normalized grid: `far_left`,
`left`, `center`, `right`, `far_right`. A slot's geometry is the renderer's
choice; the name is the contract. The `move` cue changes the slot, not the
coordinates.

## Renderer responsibilities

| Guarantee | Owner |
|---|---|
| Cue sequence and final staging for a given story, seed, and choices | Contract |
| Slot names, layering order, fit and anchor semantics | Contract |
| Dialogue and choice text, order, and identifiers | Story runtime |
| Layout state selection thresholds | Contract |
| Fallback selection when an asset is unavailable | Contract |
| Font rasterization, hinting, and exact pixel metrics | Renderer |
| Easing curves and interpolation between transition frames | Renderer |
| Texture filtering, scaling filters, and colour management | Renderer |
| Window chrome, menus, and author controls | Renderer |
| Audio, voice, and haptics | Out of scope |

Two renderers are conformant when, for the same compiled story, seed, and
choices, they emit the same cue sequence and reach the same final staging. They
are not required to produce identical pixels.

## Diagnostics

Presentation diagnostics use stable `P` codes with a severity, a source span
when the document was compiled from source, and a remediation sentence.

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
| Story with no presentation document | The story runs with `base`: ink field, no stages, default layout. Lines and choices render normally. |
| Scene with no stages | Background only; dialogue anchored to the bottom safe area. |
| Step with no cues | The previous presentation persists. |
| Stage declared but never shown | Nothing renders; the definition is inert. |
| Missing asset | Fallback for that element, rest of the frame retained. |
| Compile failure while playing | The last valid frame is retained and a non-blocking diagnostic banner appears. Playback resumes on the next successful compile. |
| Story hash mismatch | The presentation document is refused; the story runs with `base` and `P010` is reported. |

## Compatibility

- Any added, removed, renamed, or redefined serialized field or enum variant
  requires a new contract version and a new schema.
- RON and JSON encode one model and must decode to equal values.
- Map member order carries no meaning; the compiler emits sorted maps so diffs
  and reviews stay stable.
- Unknown members and unknown enum variants are rejected, not ignored. A host
  must not treat a tolerant decoder as version negotiation.
- `story.hash` binds the document to one compiled story. Recompiling a story
  produces a new presentation document; a stale one is refused.
- Playtest records are a separate format with a separate compatibility
  guarantee (`bs6g7xs8`). A playtest record refers to a contract version but
  never embeds one.

## Licence and provenance gate

Before an asset or dependency ships with the repository:

- Every entry in `assets` carries a licence expression and a provenance value.
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
through 7 confirm the proposal; decision 3 changed it.

1. **Presentation document.** Resolved: a sibling `weave.play` document, so
   presentation changes never bump the story IR version. Extending `StoryIr` to
   version 5 was rejected because it couples a renderer vocabulary to the
   language contract.
2. **Stage geometry.** Resolved: named slots on a normalized grid, with the
   renderer owning coordinates. Authored coordinates were rejected because they
   make every layout change an authoring migration.
3. **Typeface.** Resolved: bundle a font rather than inherit the host stack.
   Weave Play ships IBM Plex Sans and IBM Plex Mono under SIL OFL 1.1, with the
   licence text and notices beside the binaries and an allowlist hash entry.
   Identical rendering on every platform is worth the redistribution obligation.
4. **Text reveal.** Resolved: 18ms per character, always skippable, with
   auto-advance disabled by default. When enabled, auto-advance is bounded and
   pauses on choices.
5. **Condition scope.** Resolved: conditions may read story variables and
   domain-module exports but may never write. Typed World and Character
   selection is deferred to `xpegt223` and reuses the same mechanism.
6. **Cue granularity.** Resolved: cues attach to a step, not to a character,
   word, or animation frame. Finer granularity would put presentation timing
   inside story ordering.
7. **Choice layout.** Resolved: a vertical list, at most five visible before
   scrolling, never reordered or relabelled by presentation.

Decisions 2, 3, and 4 constrain the visual direction and are binding on the
editor review.

