# Weave Editor

The Weave Editor is the standalone GPUI application in the `weave_editor` workspace package. It keeps one canonical project model behind the visual graph, source editor, compiler feedback, play preview, and pattern browser.

## Launch

```bash
cargo run -p weave_editor
```

For startup verification without a visible window, use `cargo run -p weave_editor -- --smoke-test` on a supported desktop platform.

## Workspace

- **Project** lists the current file and recent projects. Opening a UTF-8 `.weave` file starts native file watching.
- **Graph** projects declarations, knots, choices, diverts, threads, variables, and patterns into typed nodes and connections.
- **Text** edits the same source with syntax colors, diagnostics, Unicode-safe cursor movement, selection, clipboard operations, undo/redo, and search.
- **Inspector** shows the selected graph node's source location, model data, and validation state.
- **Patterns** searches built-in and project systems, filters by origin, and previews deterministic draws without mutating active story state.
- **Preview** compiles after a short debounce and lets you step, choose, restart with a fixed seed, inspect variables, and reveal a failing source span.

## Graph and text synchronization

Text edits update the graph only after they parse successfully. While source is incomplete or invalid, the editor keeps the last valid graph and preview, shows compiler diagnostics, and does not discard the new text.

Semantic graph edits write canonical formatted source. Layout-only graph movement is stored separately and does not rewrite `.weave` content. Undo and redo treat a synchronized semantic change as one operation instead of replaying a feedback loop between views.

### Mindmapping the story

The graph works without writing syntax. A **beat** is a knot card; a **branch** is a choice card.

| Action | How |
|---|---|
| Add a beat | `Shift+Enter`, or **+ Beat** |
| Add a branch to the selected beat | `Tab`, or **+ Branch**, then type the label |
| Edit the selected card's prose or label | `Enter`, or **Edit** |
| Rename the selected beat | `F2` |
| Connect two cards | Drag from a card handle to another beat card |
| Move around | Arrow keys, drag the canvas, or use the minimap |
| Remove the selected card | `Delete`, or **Delete** |

Prose typed into a beat card becomes the knot's first prose line. A branch label becomes the choice label, and dragging that branch to a beat sets its target. Renaming a beat rewrites every divert, thread, and branch that points at it. A beat that other cards still reference cannot be deleted until those connections are removed, and the editor says how many remain.

Text that Weave would read as a command, such as a line starting with `*`, `->`, or `===`, is refused with a message instead of being written into source. Use the card controls for those constructs.

## Domain-module inspection boundary

The editor loads the adjacent `weave.modules.json` through the public `weave-domain` project API, then gives the same `DomainCatalog` to `ProjectSession`, `DomainSession`, `TextBuffer`, and live preview. A valid source `module` declaration follows the same compiler path as `weavec`; module inspections expose export descriptions, schemas, types, sources, and selected values. Character alignment sessions select an exact data-only pack and configuration, expose every proposed value and fixed-point “why” trace, create complete accept/reject/edit/withhold/override reviews, and support dry-run, atomic apply, stale detection, and refresh; only approved values reach the read-only runtime projection. Tabletop panels are similarly discovered from the exact selected adapter's declared capabilities, creation steps, and resolver-operation schemas; an absent capability returns `TT106` instead of an adapter-specific fallback. A registered Weave World composition receipt additionally exposes ordered candidate/selection records and leaf differences beside the resolved place/environment/climate preview. “Promote to authored” copies one effective generated leaf into canonical source; reset removes that override and reveals the composed value again. Both actions recompile atomically, so an invalid object-level promotion or schema violation leaves source and preview unchanged. Formatting or switching between editor-backed text views does not change the compiled module IR.

Watching is bounded to the open project and its approved project-relative artifacts. A fully validated module update swaps into all editor surfaces and rebuilds preview. A malformed, incompatible, or tampered update is reported while the last valid catalog, graph, and preview remain active. The editor never searches the machine or network for a module named in source.

`CharacterPresentationSession` opens an exact collection, immutable presentation catalog, and pinned allocation request through the shared Character contract. Its proposal view exposes eligibility, exclusions, capacities, balance counts, seed hashes, retained assignments, and missing assets. Complete accept/edit/override/reject/withhold review supports dry-run and atomic apply. Lock/unlock revisions and a `rebalance` request use the same contract; a dry-run preserves the selected collection and a commit clears stale previews. The editor never derives eligibility from personality, alignment, birth, ruleset, pronoun, name, or protected identity fields.

`RelationshipSession` opens an exact collection, immutable kind pack, reference date, and project safeguard policy. It lists and filters pack-validated edge layers, inspects direction/inverse semantics and ordered evidence, generates deterministic provider-free proposals, requires one accept/edit/override/exception/reject/withhold decision per candidate, and replays the complete review before dry-run or atomic commit. Direct authored/imported revisions use the same fingerprinted transition. Conflict inspection returns `R100`–`R113` diagnostics and read-only repair suggestions without changing the graph; edge and matrix CSV are explicitly review-only. A failed or stale operation preserves the prior collection, pack, policy, and proposal.

## Guided Character authoring

`CharacterAuthoringSession` opens the same versioned workspace used by the
`weave-character authoring-*` commands. Create, list, show, clone, revise, validate, final review,
export, and reopen therefore call the same contract functions and produce the same JSON/RON bytes.
An editor failure leaves the complete prior workspace selected and unchanged.

The focus order is stable: identity, presentation, birth date, direct facets, questionnaire,
confidence review, derived OCEAN, alignment, date context, inner life, voice, conflicts, final
review, and export.
Tab and Shift+Tab move through every control, Home/End jump to its bounds, and Enter or Space
activates the focused control. Each accessibility record exposes a tab role, human label, position,
total, keyboard hint, and the exact source representation. The OCEAN panel is explicitly read-only
and derived.

Before apply, the revision panel shows template base versus effective values, explicit
template/authored-override/accepted-suggestion/template-migration origins, locks, protected or
pack-owned paths, field differences, template migration
effects, the recomputed derived view, and blocking alignment/date-context invalidations. A final
accepted review is unavailable while blocking diagnostics remain. See the [Character guide](character_module.md#guided-authoring-and-conflict-review) and the checked [authoring corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/authoring).

## Assisted Character development

`CharacterAssistanceSession` opens an exact Character collection, versioned assistance template, and pinned request through the same contract used by the `weave-character assistance-*` commands. Preview exposes every disclosed field, serialized value, known character id, provider coordinate, template hash, setting, seed, and expected response contract without invoking an adapter. Approval fingerprints that exact preview. The built-in offline adapter then creates inspectable typed candidates; a host may instead supply an adapter implementation whose credentials stay outside every serializable artifact.

Candidate evidence and deterministic advisory scores, issues, and proposed edits remain separate from author decisions. The author must accept, edit, reject, defer, or request regeneration for every immutable candidate. Dry-run reproduces the full receipt without changing the session. Commit adds accepted values only to the pending suggestion queue, increments the collection revision once, and leaves canon, extensions, and derived values unchanged. A stale profile or review fails atomically and preserves the selected collection. See the [assistance contract](character_module.md#provider-neutral-assisted-character-development) and checked [JSON/RON corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/assistance).

## Character corpus health

`CharacterHealthSession` opens the same strict project manifest and runs the same deterministic, read-only audit as `weave-character health-audit`. The summary panel exposes document validity, character count, coverage, active/suppressed diagnostics, severity counts, the configured threshold, and CI decision. Descriptive distributions remain informational unless the manifest names a documented constraint.

Each diagnostic link retains its stable `H###` code and id, severity, document and optional character scope, exact field path, source file, and available line/column. Selecting a link therefore navigates to the relevant Character field without retaining rejected source text in the report. Code, severity, character, and suppression filters use the shared report filter; they change only the visible diagnostics. Refresh re-audits the same retained bytes and never edits, normalizes, migrates, or applies project data. See [corpus health](character_module.md#corpus-health-drift-safety-and-coverage) and its [synthetic golden projects](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/health).

## Manuscript chapters

The editor carries a manuscript model: one ordered child list per container,
with a stable id per child.

A chapter record holds an id, a parent id, an explicit order, a kind, a title,
a status id, a label id, and a synopsis. The id is the only reference to the
body: a body file is named after its chapter id, so the tree and the prose are
joined in exactly one place. The order is a stored value rather than an implied
array position, so a reorder is a single-field write, and sibling order
round-trips through save and reload without reordering.

A kind is a chapter, a scene, or a note. A scene is a leaf; a chapter holds
scenes, and a note holds notes of its own. A node can be converted between a
leaf and a container explicitly, and a container holding children cannot
become a leaf until they are moved, so the conversion is never silently
lossy. A container cannot be moved into its own descendant.

The tree is stored separately from the prose. The tree is a versioned
`weave.manuscript.ron` beside the project file, and each chapter's body is
its own file under `weave.manuscript/`. Opening a chapter reads only that
chapter's body, so the body of a sibling is not loaded when another chapter
is opened, and a reorder writes only the tree. A body holds paragraphs of
styled runs, and the project-level style sheet is stored beside the bodies.

Status and label are ID-backed references to project-scoped lists, so
renaming a status or a label does not rewrite the records that use it. An
unset status or label is `None` rather than a sentinel value. The lists are
written into the tree file and travel with the project.

Deleting a chapter moves it to a visible trash container rather than
destroying it; emptying the trash is a separate, explicit action. The trash
always sorts last among the root's children.

### Play mode

The preview panel has two presentations of one session. Debug shows the
transcript, pattern draws, variables, and playback controls. Play stages the
current line as a scene: a background laid out with the declared `fit` and
`anchor`, character images in five slot columns from `far_left` to `far_right`,
a speaker nameplate, the dialogue, and the pending choices as buttons. Stages
within a slot paint in `layer` order, then in insertion order, and a `@swap`
resolves the stage's declared variant image.

Staging is presentation only. A `@transition` fades, slides, or wipes the next
staged visual without changing story state or timing. When a variant or asset is
unavailable, the last staged frame stays and a muted note under the stage names
the declaration and the reason; the surface never invents a substitute image.

Both presentations read the session the panel already owns, so they cannot
disagree: the same choices, the same variable values, the same pattern draws,
and the same ending, with no second runtime and no source mutation.

Space or Enter advances, the number keys choose, and R restarts. Choices are
also clickable. A speaker is read from a short `Name: ` prefix on the line;
anything else is narrated, so a sentence that merely contains a colon is not
mistaken for a character name.

When the source stops compiling, the last valid frame stays on screen with a
banner naming the diagnostic. Play never blanks the scene because a draft is
temporarily broken.

## Files and conflicts

Save writes the `.weave` source atomically. When the source is valid, it also writes canonical `.ron` beside the source and updates `weave.lock` for a configured domain project. If the watched file changes externally while the editor has unsaved work, the project panel presents three explicit choices:

- keep the editor version;
- load the disk version; or
- save both versions.

Recovery data is separate from the project file and never silently overwrites it.

## Keyboard commands

`Primary` means Command on macOS and Control on other supported platforms.

| Command | Shortcut |
|---|---|
| New project | `Primary+N` |
| Open project | `Primary+O` |
| Save | `Primary+S` |
| Undo / redo | `Primary+Z` / `Primary+Shift+Z` |
| Graph / text view | `Primary+1` / `Primary+2` |
| Run or continue preview | `Primary+Enter` |
| Search in text | `Primary+F` |
| Edit the selected graph card | `Enter` |
| Rename the selected beat | `F2` |
| Add a beat / branch card | `Shift+Enter` / `Tab` |
| Remove the selected graph card | `Delete` |

Every command is also available through the File, Edit, View, or Run menu. Errors are reported in the workspace instead of terminating the editing session.

### Manuscript view

Switch to the Manuscript view with the header tab or `secondary-3`. It shows
two surfaces over the same ordered child list:

- the tree, which is the navigation surface and lists the whole hierarchy in
  stored order; and
- the card board, which renders the selected container's children as index
  cards in the same order; and
- the table, which renders the same children as rows for dense metadata
  editing.

Selecting a container shows its children. Selecting a leaf keeps the board on
its parent and moves the selection, so a card stays under the pointer. Each card
has a fixed aspect ratio and reflows to a configurable number of columns. A
container with no children shows an empty state that names it.

All three surfaces read one projection of the tree, so the order and the filtered
set they show cannot diverge. The board reads only the bodies of the container it
displays, which is why a manuscript with a hundred chapters still opens quickly.

The table shows the document title followed by whichever of Status, Label, and
Type are enabled. Clicking a Status, Label, or Type cell advances that value in
place and writes the tree; clicking the title selects the row. A chapter holding
scenes cannot become a leaf, so that transition is refused instead of silently
dropping its children.

The filter is shared: typing narrows the table and the board to the same set, and
the header reports how many rows survive out of the container's children. Escape
clears it.

Only the rows inside the viewport are built, with spacer rows above and below, so
the element tree stays proportional to the window rather than to the manuscript.
Column visibility and the active mode are remembered for the session.

### Moving and deleting chapters

The Chapter menu, the toolbar above the tree, and the keyboard all dispatch to
one handler, so the shortcuts and the menus cannot drift apart:

| Command | Shortcut | Effect |
|---|---|---|
| Move Up | `secondary-shift-up` | One place earlier among siblings |
| Move Down | `secondary-shift-down` | One place later among siblings |
| Move Left | `secondary-shift-left` | Promote one level, directly after the former parent |
| Move Right | `secondary-shift-right` | Demote one level, as the first child of the earlier sibling |
| Move to Front | `secondary-shift-home` | To the start of the sibling list |
| Move to Back | `secondary-shift-end` | To the end of the sibling list |
| Move to Trash | `secondary-backspace` | Move the selection into Trash |

A move is refused, with a reason, when there is no earlier or later sibling,
when a root-level chapter cannot be promoted, or when the earlier sibling is a
leaf and cannot hold children. A refused move changes nothing.

Deleting is not destructive: a deleted chapter moves to the visible Trash
container, where the board and table show it like any other container. Restoring
moves it back to the manuscript root. Emptying the trash is a separate command
and the only permanent delete.

Every move, delete, restore, and emptied trash can be undone with the usual Undo
command, or with the toolbar button. The history keeps the last hundred
structural changes; because only the tree is snapshotted, undo never copies the
prose.

### Dragging chapters

Cards and tree rows are drag sources. Dropping follows the research that shaped
this model, and the two drop targets are deliberately different:

- **Inside one board**, a drag draws a two-pixel accent insertion bar in the gap
  between the two cards the chapter would land between. The bar sits in the gap,
  never on a card, and is the only reorder affordance. Dragging past the last card
  removes it, so there is no ambiguous "end of list" target.
- **Onto a tree row**, the row is outlined in the accent and the drop reparents
  the chapter into that container. A container drop has no insertion bar, because
  it is not a sibling position.

The insertion point is a nearest-gap computation: the pointer is rounded by one
card stride, so the top and bottom halves of a card resolve to the same gap and a
pointer near an edge still lands on exactly one position.

A drag carries its chapter and its source container. The drop handler asks
whether the payload is local to the board; a local payload reorders siblings, a
foreign one is reparented at the drop position. A chapter cannot be dropped into
itself or into one of its own descendants, and a leaf cannot receive one; both
are refused with a message rather than silently ignored.

Every drop is recorded before it happens, so Undo restores the previous order,
including a reparent.

### Writing a chapter

The Write view shows the selected chapter as a manuscript page: a one-inch
margin, a 64-character measure, double-spaced lines, and a line guide on every
baseline. The page is monospaced on purpose, so the shape of the page is the
shape of the manuscript rather than an approximation of a browser.

Type to edit. Arrow keys move the caret, Shift extends the selection, Home and
End reach the paragraph bounds, Enter splits a paragraph, and Backspace deletes
backwards or joins the previous paragraph. Edits are written to the chapter's
body file as you type, and the word count in the toolbar follows the text.

A paragraph keeps its own style, and each run keeps a character style plus
direct bold, italic, and underline. The stylesheet at
`weave.manuscript/styles.ron` supplies the named styles, so applying a style
changes a reference rather than copying attributes onto every run.

The formatting toolbar mirrors what the selection allows:

- Bold, Italic, and Underline are enabled only while text is selected, because
  direct formatting applies to text rather than to a caret.
- The paragraph style picker is always available and applies to every paragraph
  the selection touches.
- The character style picker needs a selection, for the same reason as direct
  formatting.

The caret and selection are remembered per chapter in
`weave.manuscript/session.ron`, so closing a chapter and reopening it returns to
the same place. That file is view state, not manuscript data: losing it costs a
caret position, never a word.

### Status and label

Status and label are the attributes an author changes most while working, so they
are one click away wherever the selection appears:

- the tree row carries a status and a label chip;
- the table shows them as editable columns; and
- the window status bar repeats them for the current chapter.

Clicking any of them advances that value through the project list and back to
unset. An unset value reads as "Unset status" or "Unset label", never as a blank
cell.

Both are stored as identifiers into project-scoped lists, so renaming a status or
label edits one definition and every chapter that references it follows. The
Settings view lists both lists, adds definitions, and renames one in place: click
a name, type, then press Enter to save or Escape to cancel. Renaming never
rewrites chapter records, because the records hold ids rather than names.

The example projects in `examples/manuscripts/` are public-domain books in this
format and can be opened directly; see their
[README](https://github.com/chrisgliddon/weave/tree/main/examples/manuscripts)
for provenance.
