# Weave Editor development

The editor is a native GPUI application. It shares parser, compiler, runtime, formatter, and pattern types with the workspace crates rather than maintaining editor-only copies.

The lockfile pins GPUI to the public Zed revision compatible with the pinned gpui-flow release. GPUI is Apache-2.0 and gpui-flow is MIT licensed; both are consumed as dependencies without copied source. The macOS build enables GPUI's runtime shader path so contributors do not need the optional Xcode Metal toolchain component.

Run the visible editor:

```sh
cargo run -p weave_editor
```

Run the finite native initialization smoke check:

```sh
cargo run -p weave_editor -- --smoke-test
```

Startup errors are caught at the process boundary and reported on standard error with a non-zero exit status.

## Graph interaction and performance

The graph uses `gpui-flow` for native pan, pointer-centered zoom, node dragging, box and additive selection, edge interaction, minimap navigation, and undo/redo. Arrow keys select the nearest node in that direction, and the graph toolbar creates a new knot at the viewport center.

The renderer-independent graph model uses a uniform spatial index for hit testing and viewport queries. Its explicit interactive budget is 16.67 ms per query (60 Hz); the regression suite exercises repeated visibility queries over 10,000 knots and also caps the size of the rendered working set.

Knot cards, choice diamonds, grammar boxes, pattern hexagons, and variable pills have distinct color and shape cues. Selection, keyboard focus, and validation add independent high-contrast borders, so state never relies on color alone. Choice, divert, and thread connections likewise use distinct paths, weights, labels, and semantic validation. Selecting a node populates the inspector with its preview, graph coordinate, source location, and any validation error.

## Source editing

The Text view edits raw UTF-8 `.weave` source with grapheme-aware cursor movement and deletion, selection, clipboard operations, undo/redo, Unicode-safe search, canonical formatting, and inline compiler diagnostics. Syntax colors cover declarations, knot headers, choices, diverts, strings, numbers, grammar references, pattern calls, comments, and punctuation. Source lines are virtualized, so the GPUI element tree stays proportional to the viewport rather than the file size.

## Project lifecycle

Use the File menu or Project sidebar to create, open, save, and reopen `.weave` projects. Saves replace the source atomically and write an adjacent `.ron` file whenever compilation succeeds. Unsaved changes are mirrored to an adjacent `.weave.recovery.json` snapshot without overwriting the project source.

The native file watcher observes the containing directory so it survives atomic file replacement. It debounces event bursts and fingerprints source to distinguish Weave's own saves from external edits. A clean project reloads external changes into both Text and Graph views. If memory and disk both changed, saving is blocked until the Project sidebar explicitly keeps the editor version, loads the disk version, or preserves both by writing a `.memory-conflict.weave` copy.

## Pattern browser

Enable View → Patterns to browse Tarot, I-Ching, Elder Futhark, and every valid project-authored pattern through the shared executable pattern API. The panel filters built-in or project definitions, searches element names and semantic meanings, exposes fields, draw methods, reversals, spreads, positions, and compiler errors, and links project definitions and uses back to source or graph nodes.

Single and spread previews are deterministic for the displayed seed. Every click constructs fresh pattern state and a fresh seeded entropy stream, so exploratory draws never advance or mutate Play Preview.

## Graph and text synchronization

One canonical project model owns raw source, its current parsed document, the last valid graph projection, stable node positions, revision tokens, and transaction history. Text edits retain their exact comments and ordering while updating the graph after validation. Incomplete syntax remains authoritative in Text while Graph shows the last valid projection and blocks semantic mutations with an explicit conflict message.

Graph actions for adding knots and choices or linking knots mutate the shared AST, format it through `weave-fmt`, and update Text as one undoable edit. Canvas-only position changes stay in graph metadata and never rewrite `.weave` source. Stale view revisions are rejected instead of merged silently, and synchronized undo/redo restores source, AST, and layout together.
