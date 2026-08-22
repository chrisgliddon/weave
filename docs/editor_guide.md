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

The editor's public `DomainSession` and `TextBuffer` APIs accept an explicit `DomainCatalog`. A valid source `module` declaration is compiled against that catalog with the same compiler path as `weavec`; module inspections expose export descriptions, schemas, types, sources, and selected values. Formatting or switching between editor-backed text views does not change the compiled module IR.

Artifact installation and project-root discovery remain separate from this source-and-inspection boundary. A host must configure the catalog explicitly; the editor does not search the machine or network for a module named in source.

## Files and conflicts

Save writes the `.weave` source atomically. When the source is valid, it also writes canonical `.ron` beside the source. If the watched file changes externally while the editor has unsaved work, the project panel presents three explicit choices:

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
