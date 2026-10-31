# Weave Play presentation contract

Status: **staged presentation slice implemented; sibling document deferred.**
Scenes, stages, fit, anchor, slots, layers, variants, and bounded transitions
compile into story IR version 6 and drain through the runtime. The remaining
presentation document — dialogue framing, conditions, runtime asset loading, and
step keys — is still a later contract. The repository asset declaration and
licence gate is implemented alongside the examples. The [Decisions](#decisions)
log records the change and why.

Weave Play renders a compiled story as a staged scene instead of a diagnostic
transcript. This document defines the portable presentation contract that the
editor, Bevy, and browser renderers share. It defines what a scene looks like
and when, never what a story does.

## Scope

In scope for the implemented slice:

- background scenes with fit and anchor;
- character stages with named slots, layers, display names, and image variants;
- speaker selection and nameplate data;
- move, swap, and bounded transition cues alongside the original scene, show,
  hide, and speaker cues;
- ordered presentation cues delivered alongside story events; and
- project-relative, safe asset path validation for every image reference.

Deferred to the later presentation document:

- dialogue framing, text reveal, and choice presentation;
- presentation conditions over story variables;
- runtime asset loading and load-time hash checks;
- step keys and the story-hash binding; and
- asset-level fallbacks for a missing background or base stage image.

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
declarations and the `@scene`, `@show`, `@hide`, `@speaker`, `@move`, `@swap`,
and `@transition` directives compile into `StoryIr` version 6:

- `scene` declarations lower into the sorted `presentation` map with
  `background`, `fit` (`cover` or `contain`), and `anchor` (normalized `x`, `y`);
- `stage` declarations lower with `image`, `name`, `slot`, `layer` (`0..=9`),
  and a sorted `variants` map from variant id to image path;
- each directive lowers into a `Cue` instruction in its knot, in source order;
- the runtime executes cue instructions without ending the current advance and
  queues a `PresentationEvent` for each one; and
- a host drains those events with `Story::take_presentation()` before rendering
  the line or choice set that follows.

Defaults are `fit: cover`, `anchor: 0.5, 0.5`, `slot: center`, and `layer: 0`.
Naming an undeclared scene or stage is a compile error, as is moving an
undeclared stage, swapping a variant the stage does not declare, or naming an
unknown slot, fit, or transition kind. Anchors outside `0..=1`, layers outside
`0..=9`, and transition durations outside `0..=2000` are compile errors. Asset
paths are validated at compile time and must be relative and safe.

This slice changes the IR contract, so IR version moved from 5 to 6. The staging
vocabulary is now data: slots are names, not coordinates; layer is a paint
order; fit and anchor describe how a background fills a frame; and transitions
carry only a kind and a nominal duration. Renderers choose geometry, easing, and
pixels. Conditions, runtime asset loading, and the story-hash binding remain a
separate contract, so those adjustments do not force another story-format bump.
The repository declaration manifest described under [Assets](#assets) is a
publication gate, not a runtime loader.

## Example

This is a complete scene using only implemented syntax:

```weave
scene ferry_dusk {
    background: "art/ferry_dusk.png"
    fit: cover
    anchor: 0.5, 0.35
}

stage mara {
    image: "art/mara_neutral.png"
    name: "Mara"
    slot: left
    layer: 1
    variant surprised {
        image: "art/mara_surprised.png"
    }
}

stage keeper {
    image: "art/keeper_neutral.png"
    name: "The Keeper"
    slot: right
    layer: 2
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
@move keeper center
@transition slide 180
The water remembers every passenger.
-> deck

=== deck ===
@swap mara surprised
@transition fade 240
@hide keeper
@hide mara
@speaker
Then we had better give it something to remember.
-> END
```

The compiled `StoryIr` excerpt is:

```ron
StoryIr(
    version: 6,
    entry: "start",
    // ...
    presentation: PresentationIr(
        scenes: {
            "ferry_dusk": SceneIr(
                background: Some("art/ferry_dusk.png"),
                fit: cover,
                anchor: AnchorIr(
                    x: 0.5,
                    y: 0.35,
                ),
            ),
        },
        stages: {
            "keeper": StageIr(
                image: Some("art/keeper_neutral.png"),
                name: Some("The Keeper"),
                slot: right,
                layer: 2,
            ),
            "mara": StageIr(
                image: Some("art/mara_neutral.png"),
                name: Some("Mara"),
                slot: left,
                layer: 1,
                variants: {
                    "surprised": "art/mara_surprised.png",
                },
            ),
        },
    ),
    knots: {
        "ask": KnotIr(
            content: [
                Instruction(
                    span: Span(
                        start: 535,
                        end: 547,
                        line: 37,
                        column: 1,
                    ),
                    kind: InstructionKind(
                        kind: cue,
                        value: CueIr(
                            kind: show,
                            value: "keeper",
                        ),
                    ),
                ),
                Instruction(
                    span: Span(
                        start: 548,
                        end: 563,
                        line: 38,
                        column: 1,
                    ),
                    kind: InstructionKind(
                        kind: cue,
                        value: CueIr(
                            kind: speaker,
                            value: Some("keeper"),
                        ),
                    ),
                ),
                Instruction(
                    span: Span(
                        start: 564,
                        end: 583,
                        line: 39,
                        column: 1,
                    ),
                    kind: InstructionKind(
                        kind: cue,
                        value: CueIr(
                            kind: move,
                            value: move(
                                stage: "keeper",
                                to: center,
                            ),
                        ),
                    ),
                ),
                Instruction(
                    span: Span(
                        start: 584,
                        end: 605,
                        line: 40,
                        column: 1,
                    ),
                    kind: InstructionKind(
                        kind: cue,
                        value: CueIr(
                            kind: transition,
                            value: transition(
                                kind: slide,
                                ms: 180,
                            ),
                        ),
                    ),
                ),
                // ...
            ],
        ),
    },
)
```

That excerpt was produced by compiling the example above. Instruction and cue
values are adjacently tagged: `InstructionKind` names the instruction family and
`CueIr` carries the cue kind and payload. Scalar cues carry a string or `null`;
`move`, `swap`, and `transition` carry a struct payload. The span values are the
compiler's byte offsets into that source. Map keys are sorted, so `keeper`
precedes `mara` even though `mara` is declared first.

The same presentation definitions in JSON:

```json
{
  "version": 6,
  "presentation": {
    "scenes": {
      "ferry_dusk": {
        "background": "art/ferry_dusk.png",
        "fit": "cover",
        "anchor": { "x": 0.5, "y": 0.35 }
      }
    },
    "stages": {
      "keeper": {
        "image": "art/keeper_neutral.png",
        "name": "The Keeper",
        "slot": "right",
        "layer": 2
      },
      "mara": {
        "image": "art/mara_neutral.png",
        "name": "Mara",
        "slot": "left",
        "layer": 1,
        "variants": { "surprised": "art/mara_surprised.png" }
      }
    }
  }
}
```

## Cues

A cue is one runtime event with a kind and a payload.

| Kind | Payload | Effect |
|---|---|---|
| `scene` | scene id | Replace the background and apply its fit and anchor. |
| `show` | stage id | Make a declared stage visible at its slot and layer. |
| `hide` | stage id | Remove a stage. |
| `speaker` | stage id or none | Select the nameplate source; none is narration. |
| `move` | stage id and slot | Move a stage to `far_left`, `left`, `center`, `right`, or `far_right`. |
| `swap` | stage id and variant id | Replace the stage image with the declared variant image. |
| `transition` | kind and milliseconds | Request `cut`, `fade`, `slide`, or `wipe` for 0 to 2000ms. |

Cues apply in source order. A host applies the cues drained for one advance
before presenting that advance's line or choices. The runtime never reorders,
relabels, filters, or disables a choice, and presentation never feeds back into
story state. Cue order derives from source order alone: comments, blank lines,
save and restore, JSON versus RON, and repeated runs with the same seed cannot
change it.

Consequences a host may rely on:

- Replaying the same compiled story with the same seed and choices emits the
  same ordered cue sequence.
- A transition's duration is presentation data. It never changes story state,
  control flow, choice order, text, or random draws, and saving and restoring
  does not replay or delay a transition.
- Two renderers may interpolate transitions differently and still agree on cue
  sequence and final staging.
- A renderer that cannot honour a transition applies every surrounding cue in
  order and may fall back to `cut`.
- If `@swap` names a variant that is declared on the stage, the host uses its
  image. If the variant is absent at load time, the stage keeps its current
  image and the host surfaces a diagnostic; it never invents a replacement
  image. A missing base image or background follows the later asset-fallback
  contract.

## Deferred syntax

The following source syntax and document members are part of the later contract
and are **not accepted by the current compiler**. They are listed so hosts and
authors do not mistake them for shipped features.

| Deferred construct | Why it is deferred |
|---|---|
| `when:` conditions | Needs condition evaluation and typed path resolution. |
| `weave.play.ron` / `weave.play.json` | Needs the full sibling document and story-hash binding. |
| Runtime asset manifest (load-time hashes and story binding) | Needs load-time asset state and a reproducible story hash. The repository declaration manifest under [Assets](#assets) is implemented. |
| Asset-level fallbacks and declared fallback choice | Needs load-time asset state that is not part of static story IR. |
| Step keys and `story.hash` | Needs the sibling document and a reproducible story hash. |

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

The repository records every referenced presentation asset in
`examples/stories/assets.weave-assets.json`. The manifest is a publication
declaration, not the runtime loader: it proves that a path is known, that its
licence is recorded, and, when the file is actually redistributed, that its
bytes are pinned.

The file is a JSON object with `version: 1` and an `assets` array. Each entry
declares:

| Field | Required | Meaning |
|---|---|---|
| `path` | yes | Normalized project-relative path. The manifest directory is the project root. |
| `licence` | yes | SPDX expression such as `MIT`, `CC0-1.0`, or `Apache-2.0 OR MIT`. Empty, placeholder, and malformed expressions fail. |
| `provenance` | yes | `original` or `bundled`. |
| `author` | for `original` | Who authored the synthetic asset. |
| `source` | for `bundled` | Absolute `http` or `https` URL of the exact upstream artifact. |
| `sha256` | for `bundled` | Lowercase SHA-256 of the redistributed file. |
| `note` | no | Free-form context, such as why a path is a placeholder. |

- `Original` — authored for this project, with no third-party lineage. An
  original path may name a placeholder that is not committed yet; the
  declaration is still required.
- `Bundled` — a redistributed third-party artifact. Its manifest entry is the
  allowlist entry: it names the exact source, licence, and hash.

Run the offline checker with `python3 scripts/check-assets.py`. It reads every
`.weave` under `examples/stories`, extracts each scene `background`, stage
`image`, and variant `image`, and fails the build when:

- a referenced path has no manifest entry, or an entry is unused;
- a path escapes the project root;
- a licence is empty, a placeholder, or not a syntactically valid SPDX
  expression;
- a `bundled` entry has no source URL or SHA-256;
- a `bundled` file is missing or its SHA-256 does not match; or
- the manifest is malformed or declares duplicate paths.

A bundled asset without a hash and a licence therefore fails the build before
anything can be redistributed. A missing original placeholder does not fail;
the renderer's documented fallback handles it. `scripts/build-docs.py` runs the
same checker next to the corpus check, so the documentation gate enforces the
same rule.

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

Stage slots are named positions on a normalized horizontal grid: `far_left`,
`left`, `center`, `right`, `far_right`. The compiled story carries the slot name
and paint-order layer; the renderer owns the exact coordinates. A slot contains
its stages ordered by `layer`, then by stable declaration and cue order. The
camera does not follow a stage: moving a stage is a cue, not continuous
animation.

## Renderer responsibilities

| Guarantee | Owner |
|---|---|
| Cue sequence and final staging for a given story, seed, and choices | Story runtime plus this contract |
| Scene and stage identity, fit, anchor, slot, layer, variants, and declared asset paths | Story IR v6 |
| Dialogue and choice text, order, and identifiers | Story runtime |
| Layout state selection thresholds | Contract |
| Missing-variant behaviour (keep the current image and report) | This contract |
| Asset-level fallback for a missing background or base stage image | Later contract |
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
| `W1041` | Scene repeats a `background`, `fit`, or `anchor` setting. |
| `W1042` | Stage repeats an `image`, `name`, `slot`, or `layer` setting. |
| `W1043` | Presentation directive is missing a required argument. |
| `W1044` | Presentation directive has extra arguments. |
| `W1045` | Variant repeats its `image` setting. |
| `W1046` | Transition duration is not a whole number of milliseconds. |
| `W2200` | Duplicate scene declaration. |
| `W2201` | Duplicate stage declaration. |
| `W2202` | Unknown scene reference. |
| `W2203` | Unknown stage reference. |
| `W2204` | Invalid or unsafe asset path. |
| `W2205` | Unknown stage slot. |
| `W2206` | Unknown scene fit. |
| `W2207` | Anchor component outside `0..=1`. |
| `W2208` | Stage layer outside `0..=9`. |
| `W2209` | Unknown transition kind. |
| `W2210` | Transition duration outside `0..=2000`. |
| `W2211` | Stage does not declare the swapped variant. |
| `W2212` | Duplicate variant identifier on one stage. |
| `W2213` | Unknown stage in a `@move`. |
| `W2215` | Variant has no `image`. |

`W1040` through `W1046` are reported while parsing presentation syntax, and
`W2210` is reported there when the duration lexeme is a whole number outside
`0..=2000`. The remaining `W2200` through `W2215` codes are reported while
checking it. All are compile errors and prevent output. Every diagnostic
carries a span on the offending token or literal.

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
| Missing variant image | The stage keeps its current image and the host reports `P001`; no substitute image is invented. |
| Missing base asset | Fallback for that element once the fallback contract lands; until then the renderer decides. |
| Runtime failure during an advance | Every cue the runtime produced before the failure is applied in source order, then the failure is surfaced. A host never discards those cues and never defers them to a later frame. |
| Compile failure while playing | The last valid frame is retained and a non-blocking diagnostic banner appears. Playback resumes on the next successful compile. |
| Story hash mismatch | Deferred until the sibling document exists. |

## Compatibility

- Any added, removed, renamed, or redefined serialized field or enum variant
  requires a new IR version and a new schema. Adding fit, anchor, slots, layers,
  variants, and the move, swap, and transition cues therefore moved story IR
  from 5 to 6.
- RON and JSON encode one model and must decode to equal values.
- The schema validates structure and types, not authoring bounds: the `0..=1`
  anchor, `0..=9` layer, and `0..=2000` millisecond duration limits are compiler
  checks, so a decoded story may carry wider values from another producer.
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
- Original assets name their author and record that they are synthetic. A
  referenced path that is an undeclared placeholder still needs an `original`
  entry, so the path cannot appear from nowhere later.
- Bundled third-party assets require an allowlist entry naming the exact
  upstream artifact and hash, and their licence text and notices ship beside
  the project. In `examples/stories/assets.weave-assets.json` that entry is the
  manifest record itself: `source` names the upstream artifact and `sha256`
  pins the redistributed bytes.
- A bundled asset without a hash, a source URL, or a licence expression fails
  `scripts/check-assets.py`, and therefore fails `scripts/build-docs.py`. There
  is no unverified bundled path.
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
   the implemented slices compile scene and stage definitions plus every cue
   instruction into `StoryIr`, because the runtime must deliver them with story
   events and because a host cannot stage a line without them. Fit, anchor,
   slots, layers, variants, and bounded transitions landed in version 6. The
   remaining document — dialogue framing, conditions, the asset manifest, and
   the story-hash binding — remains a later contract, so visual adjustments
   still do not bump the story IR. The staging slice required IR version 6 and a
   new schema.
2. **Stage geometry.** Resolved and implemented: named slots on a normalized
   grid, with the renderer owning coordinates. Authored coordinates were
   rejected because they make every layout change an authoring migration.
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
