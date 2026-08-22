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

## Domain-module inspection boundary

The editor loads the adjacent `weave.modules.json` through the public `weave-domain` project API, then gives the same `DomainCatalog` to `ProjectSession`, `DomainSession`, `TextBuffer`, and live preview. A valid source `module` declaration follows the same compiler path as `weavec`; module inspections expose export descriptions, schemas, types, sources, and selected values. Character alignment sessions select an exact data-only pack and configuration, expose every proposed value and fixed-point “why” trace, create complete accept/reject/edit/withhold/override reviews, and support dry-run, atomic apply, stale detection, and refresh; only approved values reach the read-only runtime projection. Tabletop panels are similarly discovered from the exact selected adapter's declared capabilities, creation steps, and resolver-operation schemas; an absent capability returns `TT106` instead of an adapter-specific fallback. A registered Weave World composition receipt additionally exposes ordered candidate/selection records and leaf differences beside the resolved place/environment/climate preview. “Promote to authored” copies one effective generated leaf into canonical source; reset removes that override and reveals the composed value again. Both actions recompile atomically, so an invalid object-level promotion or schema violation leaves source and preview unchanged. Formatting or switching between editor-backed text views does not change the compiled module IR.

Watching is bounded to the open project and its approved project-relative artifacts. A fully validated module update swaps into all editor surfaces and rebuilds preview. A malformed, incompatible, or tampered update is reported while the last valid catalog, graph, and preview remain active. The editor never searches the machine or network for a module named in source.

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

Every command is also available through the File, Edit, View, or Run menu. Errors are reported in the workspace instead of terminating the editing session.
