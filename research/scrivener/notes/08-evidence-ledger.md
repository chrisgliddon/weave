# 08 — Evidence Ledger

Every conclusion in these notes, with its evidence, its confidence, the search
boundary that bounds it, and what remains unknown.

---

## 8.1 Target identity

| Fact | Value | Evidence |
| --- | --- | --- |
| Bundle id | `com.literatureandlatte.scrivener3` | `inspect_plist` on `Info.plist` |
| Product | Scrivener 3, short version **3.5.2**, build **17486** | `inspect_plist` |
| Architectures | `x86_64` (offset 16384, 62 558 704 B) and `arm64` (offset 62 586 880, 58 828 368 B) | `list_architectures` |
| Executable SHA-256 | `1783f01e265222134dc20ae893f5d69f6eeef9ac4483a7c87b622f33ca9640e6` | `open_binary` |
| arm64 slice SHA-256 | `f239d411e5a16efd1544e1761da0c2a014e77bfd59e7e0baf0cfe746d48a64e7` | `lipo -thin arm64` |
| Signature | signed, team `W4QTL7X778`, Apple root authority chain, hardened runtime **false** | `inspect_signature` |
| Frameworks | `ScrAppKit.framework` (8 198 336 B), `AppRegMAS.framework`, `libswift_Concurrency.dylib` | bundle listing |
| Plug-ins | `ScrivQuickLook.appex`, `ScrivThumbnail.appex` | bundle listing |
| Interface Builder resources | 67 `.nib` in `Contents/Resources/Base.lproj`, 0 storyboards | filesystem survey |
| Runtime instance | PID 29402, CGWindowID 23639, title `Weave UX`, bounds (905, 33, 823, 1044) | `CGWindowListCopyWindowInfo`, `observe_native_ui` |

---

## 8.2 Conclusion ledger

Confidence key: **OBS** = directly observed, **INF** = inferred from
observations, **UNK** = not established.

| # | Conclusion | Conf | Evidence | Boundary | Open |
| --- | --- | --- | --- | --- | --- |
| 1 | The main window is a three-pane split: Binder / editor / Inspector | OBS | `01_baseline.png`, `01_baseline.txt`, `MainDocument.nib` outlets `binder`/`mainSplitController`/`inspectorSplitView` | One window, one layout | — |
| 2 | `SCRMainWindowController` owns 184 outlets, 140 target-actions | OBS | `MainDocument.nib` graph | One NIB | — |
| 3 | The view mode is a 3-way radio group with matching menu items | OBS | `AXRadioGroup [1,6,0]` with 3 buttons; `View` menu `Document`/`Corkboard`/`Outline` | Toolbar + View menu | — |
| 4 | Eight window layout presets exist | OBS | `Window ▸ Layouts` menu | 2 of 8 captured | 6 layouts not captured |
| 5 | The Binder renders an `AXOutline` of `AXRow`s with editable titles | OBS | `01_baseline.txt`; rows expose `AXShowMenu` + `AXConfirm` | Text/Image | — |
| 6 | Rows expose `AXShowDefaultUI`/`AXShowAlternateUI` | OBS | AX action list | macOS AX contract | — |
| 7 | The document model is a typed, ordered tree in a single XML file | OBS | `Weave UX.scrivx`; `Type` ∈ {Text, Folder, DraftFolder, ResearchFolder, TrashFolder, Image, PDF} | One project | — |
| 8 | Sibling order = XML document order; no index attribute | OBS | `Weave UX.scrivx` `<Children>` | — | — |
| 9 | Body text lives outside the tree, in `Files/Data/<UUID>/` | OBS | project directory layout; `TextSelection` carries only a caret | — | — |
| 10 | Status is stored as an integer ID | OBS | `<StatusID>2</StatusID>` on the document showing `First Draft` | One document | ID→name map not read |
| 11 | A `Chapter` is a container with its own metadata, not a special type | OBS | `Weave UX.scrivx` `Type="Folder"`; `convertBetweenGroupAndTextInBinder` action | — | — |
| 12 | Move is exposed as Up/Down/Left/Right in menu, toolbar, and drag | OBS | `Edit ▸ Move`; `MainDocument.nib` `moveUpDownSegmentedControl` etc. | — | — |
| 13 | Deletion moves to a visible Trash node | OBS | `Trash` top-level node; `Move to Trash`; `Empty Trash…` | — | — |
| 14 | Search results render in the Binder's own outline | OBS | outlets `searchResultsTable`/`binderSearchHolderView`; search field `All (Any Word)` | — | — |
| 15 | Corkboard renders the selected container's children as cards | OBS | `08_corkboard_cards.png`; `AXTextArea` ×2 titled `Chapter (N of 2 index cards)` | — | — |
| 16 | A card is a text surface, not a picture | OBS | card nodes are `AXTextArea`, description `index card`, 300×200 | Cards were empty | Body-render rules **UNK** |
| 17 | The card grid is column-based with a 3:2 card ratio | OBS | `Cards Across` = Auto-Fit, 1–10, Other; `changeCardRatio:`; observed 300×200 | — | Resize gesture **UNK** |
| 18 | Card headers show icon, bold title, and a rule | OBS | `08_corkboard_cards.png` | — | — |
| 19 | A drag shows a coloured insertion bar in the gap between cards | OBS | `11_mid_drag.png`, `14_restore_mid_drag.png` | Grid mode | Animation **UNK** |
| 20 | A card drag reorders the container's children | OBS | Binder order before/after; `.scrivx` matches after restore | One drag, one container | — |
| 21 | The dragged item carried its whole subtree | OBS | the moved Chapter kept its own `Scene` child | — | — |
| 22 | A card drag rewrites only the tree, not the bodies | INF | `.scrivx` child order changed; body files untouched | — | — |
| 23 | Corkboard coordinates are 4-D (card/section/thread/operation); Outliner coordinates are 2-D | OBS | `__objc_methname` selectors | Selector names | Semantics of `threadIndex` **UNK** |
| 24 | Drag locality is decided per drag-source kind | OBS | `draggedItemsAreLocal`, `binderDocumentsDragIsLocal:`, `collectionIDDragIsLocal:`, `isKeywordDragLocal:` | — | — |
| 25 | Freeform card position rides in the pasteboard | OBS | `setUpPasteboardForDrag:withBinderItems:source:freeformInfo:` | — | — |
| 26 | Cycle prevention exists | OBS | `containsSourceOfDrag:` | — | — |
| 27 | Corkboard has a grid mode and a staged freeform mode | OBS | `Freeform`, `Snap To Grid`, `Commit Freeform Order`; `commitFreeformCorkboardOrder:` | — | Freeform not exercised |
| 28 | Corkboard can be partitioned into sections | OBS | `changeCorkboardMultipleSectionsArrangement:`, `Number Per Section` | — | — |
| 29 | Label and status colour the card edges | OBS | `Show Label Colors Along Edges`, `Show Keyword Colors` | — | — |
| 30 | The Outliner is the table projection of the same child list | OBS | `filterOutlinerOrCorkboard:`; `23_corkboard_final.png` shows the same two Scenes as rows | — | — |
| 31 | Outliner columns: Title and Synopsis, Label, Status, Section Type | OBS | `23_corkboard_final.png` | 4 of ≥15 togglable columns | 9 `toggleOutlinerColumnVisible:` bindings; full column set **UNK** |
| 32 | Outliner cells are inline metadata editors | OBS | `Label`/`Status`/`Section Type` are `AXPopUpButton`s in cells | — | — |
| 33 | Status and label are editable from Inspector, Outliner row, and status bar | OBS | `AXPopUpButton` at `[0,6,14,0..1]` and in Outliner cells; Inspector outlets `statusPopup`/`labelPopup` | — | — |
| 34 | The format bar is context-sensitive | OBS | Document view shows style/font/size/weight; Outliner view shows only B/I/U + alignment | — | — |
| 35 | The format bar is a shared framework component | OBS | `ScrAppKit.framework/…/TextFormatBar.nib`, 274 objects | — | — |
| 36 | The editor renders the target manuscript format | OBS | `15_wysiwyg_editor.png` — ruled dashed guides | — | — |
| 37 | The editor supports a split view and independent selection coupling | OBS | `toggleDocumentViewSplitType:`, `matchSplitDocuments:`, `toggleSelectionAffectsOtherEditor:`, `Binder Selection Affects` | — | Split not exercised |
| 38 | The Inspector is a tab view of 5 optional panes plus a general pane | OBS | `InspectorView.nib` tab buttons + `generalMetaData*` outlets | — | Pane contents not opened |
| 39 | Each Inspector pane is independently collapsible | OBS | `collapse*Button` outlets + `toggle*Collapsed` actions | — | — |
| 40 | General metadata fields: status, label, section type, created, modified, include-in-compile, icon | OBS | `InspectorView.nib` outlets | — | — |
| 41 | The breadcrumb shows `Parent: Title` | OBS | `15_wysiwyg_editor.png` (`Manuscript: Part`), `16_inspector_chapter.png` (`Chapter: Chapter`) | — | — |
| 42 | Custom metadata is a project-schema-driven pane | OBS | `CustomMetaDataInspectorView.nib`; `showCustomMetaDataSettings:` | — | Schema format **UNK** |
| 43 | The app exposes 328 ObjC class names, 169 `SCR`-prefixed | OBS | `__objc_classname` section of the arm64 slice | — | — |
| 44 | The app exposes 16 042 distinct selectors, 128 drag/drop-related | OBS | `__objc_methname` section | — | — |
| 45 | `SCRAllowDropOnsInCorkboard` is a user preference | OBS | preference-key string section | — | Default value **UNK** |

---

## 8.3 Provider and tooling boundaries

| Attempt | Outcome | Consequence |
| --- | --- | --- |
| `open_binary` on the universal binary with Ghidra | rejected — `architecture_unsupported`; Ghidra cannot import a universal Mach-O | Thinned the binary with `lipo -thin arm64` |
| `open_binary` on the arm64 slice with Ghidra | bound, but `search_procedures` hit the 330 s startup deadline on the 58 MB slice | **No decompilation was performed.** All native facts come from the Mach-O symbol sections, which need no disassembler |
| `decode_interface_builder` on the whole bundle | 64-document / 20 000-object caps truncated the graph | Decoded each NIB in its own minimal bundle → all 67 with `hierarchy_complete: true` |
| `capture_native_ui_scenario` AX click on a Binder row | `AXError -25206` / `-25205` | Drove the app with real Quartz mouse events instead (`tools/ax_click.py`) |
| `observe_native_ui` while the thin slice was the active target | `target-mismatch` | Re-bound the active target to the app executable before each runtime capture |
| `open_binary` on a staged single-NIB bundle | needed `CFBundleExecutable` and a Mach-O executable | Used `/usr/bin/true` as the stub |

**INF.** Because Ghidra analysis did not complete, there is **no decompiled
pseudocode** in this research. Every native claim is either an accessibility
observation, a decoded NIB fact, a Mach-O section fact, or a persisted-file
fact. The selector names are the strongest native evidence available, and they
are sufficient to map the feature surface — but they do not show the
*implementation*.

---

## 8.4 Residual unknowns, consolidated

| Area | Unknown |
| --- | --- |
| Binder | The six `changeBinderSelectionMode:` bindings; multi-select behaviour; icon assignment rules; the `SCRBinderDocumentSortDelegate` rules beyond the three `Edit ▸ Sort` items |
| Cards | Card body rendering rules; `Layout` and `Arrange by Label` submenus; card resize gesture; multi-card selection and multi-card drag; blank-card "ghost" rendering |
| Drag and drop | Cross-container drops; "drop onto a card" nesting; autoscroll; undo after a drag; the "before" vs "after" indicator difference |
| Editor | The paragraph-style list; the list/auto-number behaviours; whether the format bar is user-customisable; the split-document behaviour |
| Metadata | The `StatusID` → name mapping; the serialised form of `Label`; the custom-metadata schema; the Snapshots and Comments tabs; whether the general pane's field set is project-configurable |
| Layout | 6 of 8 layout presets not captured; split-divider behaviour; whether layouts persist per project |
| Compile | The `CompileDraft` / `CompileFormatEditorView` behaviour; the `styles.xml` schema |
| Search | Interaction between search results and drag-and-drop; the full query-builder surface |
| Performance | Behaviour with very large projects |

---

## 8.5 Method notes and side effects

**OBS.** The research operated on a live, user-owned Scrivener instance
holding a project named `Weave UX.scriv`. The following mutations were made
to that project and **reverted**:

| Mutation | Reverted by | Verified by |
| --- | --- | --- |
| Reordered the two `Chapter` children of the first `Part` (card drag) | Reverse card drag | `.scrivx` child order matches the original |
| Changed the window size to 823×1044 at (905, 33) | Explicit resize | `AXSize`/`AXPosition` readback |
| Switched view modes and window layouts | `View ▸ Document`, `Window ▸ Layouts ▸ Default` | Screenshot readback |
| Toggled the Inspector | `View ▸ Show/Hide Inspector` | Screenshot readback |

**OBS.** The final `.scrivx` on disk matches the original structure exactly:

```
Manuscript [DraftFolder]
├── Part [Folder]
│   ├── Chapter [Folder]
│   │   ├── Scene [Text]
│   │   └── Scene [Text]
│   └── Chapter [Folder]
│       └── Scene [Text]
└── Part [Folder]
    └── Chapter [Folder]
        └── Scene [Text]
```

**OBS.** The "Hello world, this is a scene." body in the first `Scene` was
**not** authored by this research; it was already present in the project
before the first observation and is still present.

**INF.** The `binder.autosave` and `binder.backup` files in `Files/` would
allow the pre-research state to be confirmed byte-for-byte if a stricter
check is wanted.

---

## 8.6 Tooling written for this research

| Tool | Purpose |
| --- | --- |
| `tools/ib_extract.py` | Summarise a decoded NIB into a structure table |
| `tools/ib_tree.py` | Render a NIB's containment tree with outlets and actions |
| `tools/ib_compact.py` | Build `ib_compact.json` — the full 67-document structural summary |
| `tools/save_observation.py` | Split a REA observation into a PNG, an AX text tree, and the raw Evidence |
| `tools/slim_observations.py` | Drop base64 screenshots and uninformative AX nodes from observations |
| `tools/ax_click.py` | Post real Quartz mouse events (click, drag, drag-hold, release) |
| `tools/scrivener_ax.applescript` | Query/select Binder rows and drive menus via System Events |
| `tools/scrivx_analyze.py` | Parse a `.scrivx` into a tree or a per-node detail record |

---

## 8.7 What was *not* done

- **No decompilation.** Ghidra did not finish importing the arm64 slice
  within its 330 s startup deadline. Nothing in these notes depends on
  decompiled code.
- **No traffic or filesystem surveillance** of the running app.
- **No modification of the Scrivener application bundle.** The bundle was
  read only; staged copies were built in a temporary directory.
- **No reading of Scrivener source, documentation, or templates.**
- **No capture of the Inspector's tab contents** (Bookmarks, Comments,
  Snapshots, Custom Metadata) — the tab strip could not be brought to the
  front in the observed session.
- **No exercise of the compile pipeline.**
