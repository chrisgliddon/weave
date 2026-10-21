# 02 — The Binder Tree

The left-hand outline that holds the project hierarchy, and the document model
behind it.

**Evidence:** `evidence/runtime/01_baseline.txt`, `evidence/runtime/13_drop_complete.txt`,
`evidence/ib_nibs/MainDocument.json`, and the live project file
`Weave UX.scrivx` (read via `tools/scrivx_analyze.py`).

---

## 2.1 Observed tree

**OBS.** The Binder renders as an `AXOutline` (`[0,0,0,0]`) with 27 visible
`AXRow` children. Each row is an `AXCell` containing an `AXTextField` whose
value is the document title, plus an `AXDisclosureTriangle` when the node has
children.

**OBS.** Every row's text field exposes exactly two actions:
`AXShowMenu` and `AXConfirm`. The disclosure triangle exposes `AXPress`.
Rows expose `AXShowDefaultUI` and `AXShowAlternateUI`.

**INF.** `AXShowDefaultUI` / `AXShowAlternateUI` is the AppKit "primary vs
secondary click" contract. On an `NSOutlineView` row this is what makes
**double-click open** the document while **single-click selects** it. The
`AXConfirm` action on the text field is the "commit an edit" action, i.e. the
title is a **rename-in-place** field.

**INF.** The disclosure triangle as a separate element, rather than a
click-target on the row, is the standard AppKit `NSOutlineView` disclosure
button. Its presence next to a title with no children indicator means the
tree uses the *classic* macOS triangle, not the modern chevron.

**OBS.** The observed project tree (from the accessibility tree and from the
persisted `.scrivx`):

```
Novel Format          [Text]
Manuscript            [DraftFolder]
├── Part              [Folder]
│   ├── Chapter       [Folder]
│   │   ├── Scene     [Text]
│   │   └── Scene     [Text]
│   └── Chapter       [Folder]
│       └── Scene     [Text]
└── Part              [Folder]
    └── Chapter       [Folder]
        └── Scene     [Text]
Characters            [Folder]
Places                [Folder]
Front Matter          [Folder]
├── Manuscript Format [Folder]
├── Paperback         [Folder]
└── Ebook             [Folder]
Notes                 [Folder]
Research              [ResearchFolder]
└── Sample Output     [Folder]
Template Sheets       [Folder]
Trash                 [TrashFolder]
```

**OBS.** The `__objc_classname` section of the shipped binary lists 328
Objective-C class names, 169 with the `SCR` prefix. The Binder-relevant ones
are `SCRBinderController`, `SCRBinderDocument`, `SCRBinderDocumentSortDelegate`,
`SCRBinderHeaderView`, `SCRBinderFooterView`, `SCRBinderTouchBarView`,
`SCRBinderIconBuildingMenuDelegate`.

**INF.** `SCRBinderDocument` is a *document wrapper*, distinct from the
on-disk `BinderItem`. The wrapper is the runtime object; the `.scrivx`
`BinderItem` is its serialised form. A `SCRBinderDocumentSortDelegate`
existing separately implies sorting is a first-class, pluggable concern.

---

## 2.2 The document model on disk

**OBS.** The project directory contains:

```
Weave UX.scriv/
├── Weave UX.scrivx          <- the whole Binder tree, as XML
├── Files/
│   ├── Data/<UUID>/         <- one directory per document
│   ├── binder.backup
│   ├── binder.autosave
│   ├── styles.xml
│   ├── search.indexes
│   ├── writing.history
│   └── version.txt
├── Settings/
│   ├── ui-common.xml, ui.plist, compile.xml, projectpreferences.xml, ...
└── QuickLook/Preview.html
```

**OBS.** `Weave UX.scrivx` is a single XML document:

```xml
<ScrivenerProject Identifier="..." Version="2.0" Creator="SCRMAC-3.5.2-17486"
                 Device="..." Author="" Modified="..." ModID="...">
  <Binder>
    <BinderItem UUID="..." Type="Text" Created="..." Modified="...">
      <Title>Novel Format</Title>
      <MetaData>
        <IncludeInCompile>Yes</IncludeInCompile>
        <IconFileName>Information</IconFileName>
      </MetaData>
      <TextSettings>
        <TextSelection>915,0</TextSelection>
      </TextSettings>
    </BinderItem>
    <BinderItem UUID="..." Type="DraftFolder" ...>
      <Title>Manuscript</Title>
      <Children>
        <BinderItem .../>
      </Children>
    </BinderItem>
  </Binder>
</ScrivenerProject>
```

**OBS.** `Type` takes these values in the observed project: `Text`,
`Folder`, `DraftFolder`, `ResearchFolder`, `TrashFolder`, `Image`, `PDF`.

**OBS.** Nesting is expressed by *document order* inside a `<Children>`
element. There is no explicit index attribute — sibling order **is** the
document order of `<BinderItem>` children.

**OBS.** A document's *body text* is not in the `.scrivx`. It lives in
`Files/Data/<UUID>/`. The `.scrivx` carries only structure, title, metadata,
and a `TextSelection` caret position.

**INF.** The model separates **structure** (`.scrivx`, one XML file, ordered
tree) from **content** (one directory per document, addressed by UUID). The
UUID in the `BinderItem` is the join key. Reordering therefore mutates only
the XML, and is cheap; editing a body only touches that document's directory.

**INF.** `TextSelection` persisted in `TextSettings` means the caret position
and selection survive a reopen. That is a small touch with a large perceived
quality effect.

**INF.** `ModID` on the project root plus `Modified` on each item suggests the
format supports change detection across devices — relevant to Weave's own
sync story if the `.scrivx` pattern is adopted.

---

## 2.3 Selection semantics

**OBS.** `MainDocument.nib` declares the action `changeBinderSelectionMode:`
**six times** — six distinct bindings (likely a segmented control with a
value per mode, or a popup with per-mode targets).

**OBS.** The `Navigate` menu contains `Binder Selection Affects` and
`Editor Selection Affects` submenus, and `Clear All Navigation Options`.

**OBS.** The `View` menu has `Show/Hide Binder` and `Show Collections`.
`MainDocument.nib` has outlets `binderAffectsMenu` and a
`collectionsController` plus `arbitraryCollectionController` /
`arbitraryCollectionTable`.

**OBS.** The `Document` menu carries `Snapshots`, `Duplicate`, `Split`,
`Merge`, `New Folder from Selection`, `Ungroup`, `Move To`, `Copy To`,
`Copy to Project`, `Add to Collection`, `Add to Project Bookmarks`,
`Change Icon`, `Move to Trash`.

**OBS.** The `Edit` menu has a `Move` submenu with exactly four items:
`Move Up`, `Move Down`, `Move Left`, `Move Right`.

**INF.** The four-way Move is the keyboard equivalent of drag-and-drop, and it
maps precisely onto the tree geometry: Up/Down = reorder among siblings,
Left = promote to parent, Right = demote to first child. This is the same
vocabulary as outliner processors in other tools. Weave should adopt exactly
these four verbs.

**INF.** `Binder Selection Affects` / `Editor Selection Affects` make
selection coupling an *explicit, user-controlled* relationship rather than an
implicit one. When both are on, selecting in the Binder scrolls the editor to
that document and vice versa.

---

## 2.4 Creating and grouping

**OBS.** `MainDocument.nib` actions include `addNewDefaultItemInBinder`,
`addNewGroupItemInBinder`, `addNewTextItemInBinder`, `addNewGroupItem`,
`addNewTextItem`, `groupInBinder`, `ungroupInBinder`,
`convertBetweenGroupAndTextInBinder`.

**OBS.** The `Project` menu offers `New Text`, `New Folder`,
`New from Template`, and the `Documents` menu offers `New Folder from
Selection` and `Ungroup`.

**OBS.** `DocumentView.nib` has the same operations in the editor context:
`addNewDefaultItem`, `addNewGroupItem`, `addNewTextItemFromMenu`,
`group`, `ungroup`, `convertBetweenGroupAndText`, `duplicateSelectedItems`,
`deleteSelectedItems`.

**OBS.** The shipped binary exposes a preferences key
`SCRAlwaysCreateNewItemsAsSiblings` (in the `__objc_classname` string
section, which for this binary also carries preference-key strings).

**INF.** There is a deliberate distinction between a *group* (a container) and
a *text document*, and a command to convert between them. A "Chapter" in
Weave is therefore a **container** first — it holds scenes and has its own
metadata — not a special document type.

**INF.** `New Folder from Selection` groups an existing selection, and
`Ungroup` dissolves a group back into its parent. Both are needed; a model
that only supports "create a container" cannot express the second.

**INF.** `SCRAlwaysCreateNewItemsAsSiblings` implies new items can instead be
created as *children* of the current selection. The default is a user
preference, so the behaviour must be configurable, not hard-coded.

---

## 2.5 Trash

**OBS.** `Trash` is a top-level `TrashFolder` in the tree, and the `Project`
menu has `Empty Trash…`. `MainDocument.nib` declares `emptyTrash`,
`emptyTrashSheet`, `emptyTrashProgressIndicator`, and the `Documents` menu
has `Move to Trash`.

**INF.** Deleting is a *move*, not a destructive operation, and the trash is
an ordinary visible node in the tree. Weave's chapter deletion should be
reversible without an undo stack.

---

## 2.6 Search shares the Binder frame

**OBS.** The `MainDocument.nib` outlets include `binderSearchHolderView`,
`searchView`, `searchViewField`, `searchField`, `searchResultsController`,
`searchResultsTable`, `quickSearchPanel`, `toolbarSearchMenu`.

**OBS.** At runtime the top-left of the Binder column shows a search field
reading `All (Any Word)`, with a magnifier button beside it in the toolbar.

**OBS.** The `View` menu has `Show/Hide Binder`, `Show Collections`,
`Show Project Search` is not present but `toggleProjectSearch` is a
`MainDocument.nib` action; `searchOrOpen`, `search`, `changeSearchFor:`,
`changeSearchOperator:`, `changeSearchOption:`, `changeSearchOptions:`,
`resetSearchSettingsToDefault:`, `saveSearch` are all actions.

**INF.** Search results are presented **in the Binder's own column**, using
the same `AXOutline` surface, rather than in a separate panel. The Binder
becomes a view over either the document tree *or* a result list. The search
field itself carries a mode chip (`All (Any Word)`) that opens a builder with
field, operator and option — a structured query, not a plain substring.

**INF.** For Weave this suggests: one outline widget, two data sources.

---

## Residual unknowns

- **UNK** The exact `changeBinderSelectionMode:` values — six bindings were
  counted but their meanings were not recovered from the NIB alone.
- **UNK** `SCRBinderDocumentSortDelegate` sorting rules beyond the
  `Edit ▸ Sort` menu (`Ascending (A-Z)`, `Descending (Z-A)`,
  `Collection into Binder Order`).
- **UNK** Whether the Binder supports multi-select and range-select was not
  exercised; the accessibility tree exposes no `AXSelected` on rows other
  than the single selected one, so multi-select behaviour is **UNK**.
- **UNK** Icon assignment rules (`binderChangeIconMenu`, `IconFileName`
  metadata) were not enumerated.
