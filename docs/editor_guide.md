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
