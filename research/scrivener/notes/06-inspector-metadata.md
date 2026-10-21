# 06 — Inspector and Metadata

What the Inspector shows for a document, and how those fields map onto the
persisted document model.

**Evidence:** `evidence/ib_nibs/InspectorView.json`,
`evidence/ib_nibs/CustomMetaDataInspectorView.json`,
`evidence/runtime/16_inspector_chapter.png`, `evidence/runtime/19_inspector_shown.png`,
and the live `Weave UX.scrivx`.

---

## 6.1 Pane structure

**OBS.** `InspectorView.nib` has 119 outlets and 38 target-actions. The
outlets divide into:

**Tabs** (one of these occupies the Inspector at a time):

```
bookmarksTabButton      commentsTabButton      customMetaDataTabButton
notesTabButton          snapshotsTabButton
```

**General metadata pane** (the always-present one):

```
generalMetaDataBackgroundView   generalMetaDataHeaderBar
generalPaneTitleButton          metaDataHolder
metaDataPanePlaceholder         metaDataSplitView
metaDataDocTitleLabel
```

**Field outlets inside the general pane:**

```
statusButton      statusPopup        labelButton       labelPopup
sectionTypePullDown
createdDateTextField   modifiedDateTextField
includeInCompileButton
indexCardView          imageView
customMetaDataButton   customMetaDataHolder
```

**Tab-specific outlets:**

```
bookmarks:   bookmarksTableView, bookmarksController, bookmarksPopup,
             bookmarksQuickRefViewController, bookmarksDocTitleLabel, …
comments:    commentsView, commentsTitleField, addCommentButton,
             deleteCommentButton, …
snapshots:   snapshotsTitleField, takeSnapshotButton, deleteSnapshotButton, …
custom meta: customMetaDataHolder, customMetaDataPlaceholderView, …
notes:       notesPaneView, notesScrollView, notesSplitView, …
keywords:    keywordsTableView, keywordsController, keywordsPopUp, …
```

**Common / chrome:**

```
documentTabView, documentInspectorView, documentInspectorPlaceholder,
headerView, footerBar, centeredView, imageView, progressView, noMatchesLabel,
tableView, dataSource, delegate, menu, view
```

**OBS.** The corresponding actions:

```
changeStatus:          changeLabel:           changeSectionType:
changeCreatedDate:     changeTab:  (×5)
toggleIncludeInCompile:   toggleLocked: (×2)
toggleGeneralMetaDataCollapsed:  (×2)
toggleSynopsisCollapsed:  (×2)
toggleNotesCollapsed:      (×2)
toggleKeywordsCollapsed:   (×2)
toggleCustomMetaDataCollapsed: (×2)
toggleShowSynopsisOrPicture:
showCustomMetaDataSettings:   showKeywordsHUD:
addCommentAtSelection:  addFootnoteAtSelection:
takeSnapshot:  deleteSnapshot:  renameBookmark:  renameResult:
changeBookmarksViewed:  removeSelectedComments:  remove: (×3)
reloadSimilarTextsList:
```

**INF.** Each pane has a **collapsible** state with a dedicated collapse
button *and* a dedicated toggle action, plus a title button that is separate
from the collapse control. This is a two-part affordance: a title that opens
a menu (to add fields, reconfigure) and a disclosure triangle that collapses.

---

## 6.2 Fields observed at runtime

**OBS.** With a `Chapter` selected, the Inspector's synopsis pane showed the
document title in an `AXTextField` (`[0, 6, 8, 1, 1] AXTextField =Chapter`),
and the window footer showed two `AXPopUpButton`s:

```
[0, 6, 14, 0] AXPopUpButton value='No Label'
[0, 6, 14, 1] AXPopUpButton value='First Draft'   (or 'No Status')
```

**OBS.** The Outliner view showed the same two attributes as inline
per-row editors: `Label` = `No La…` and `Status` = `First Draft`, plus
`Section Type` = `Scene`.

**OBS.** The status bar in every capture carried the same `No Label` and
`First Draft` pop-ups, confirming these two attributes are edited from at
least three places (Inspector, Outliner row, status bar).

**INF.** **Label** and **Status** are the two attributes Scrivener considers
first-class enough to expose in every surface. Status is a *workflow* axis
(First Draft → Revised → Final); Label is a *taxonomy* axis (user-defined
colour-coded names). They are orthogonal.

**INF.** `Section Type` is a third, structural axis. The observed Chapter's
children had `Section Type` = `Scene`, and the top-level `Chapter` folders
themselves had `Section Type` = `Chapter` (the Outliner showed `Section Type`
= `Scene` for the Scene children). In a Scrivener project, "Section Type" is
what a document *is* in the manuscript's structure — the thing a compiler
assembles.

---

## 6.3 Mapping onto the document model

**OBS.** The `.scrivx` `MetaData` elements observed:

```xml
<MetaData>
  <IncludeInCompile>Yes</IncludeInCompile>
  <IconFileName>Information</IconFileName>
</MetaData>
```

and on one document:

```xml
<MetaData>
  <StatusID>2</StatusID>
  <IncludeInCompile>Yes</IncludeInCompile>
</MetaData>
```

**OBS.** `<StatusID>2</StatusID>` appeared on the `Scene` whose body was
"Hello world, this is a scene." — and the runtime UI showed that document's
status as `First Draft`.

**OBS.** `TextSettings` held `<TextSelection>915,0</TextSelection>` on the
`Novel Format` document and `0,0` elsewhere.

**OBS.** The `StatusID` value is an **integer**, not a string.

**OBS.** The `__objc_classname` string section of the binary contains the
preference key `SCRAlwaysShowSynopsisInInspector`.

---

## 6.3.1 The label and status lists (observed)

The status and label lists were closed by setting a label and a status on a
document in a scratch copy of the project and reading the resulting file.

**OBS.** Setting `Blue` on a document through the Inspector's label popup
wrote a `<LabelID>` element into that document's `MetaData`:

```xml
<MetaData>
    <LabelID>11</LabelID>
    <IncludeInCompile>Yes</IncludeInCompile>
    <IconFileName>Information</IconFileName>
</MetaData>
```

The element is **absent** when no label is set; the UI then shows `No Label`.

**OBS.** The label and status *definitions* live in the project file, not in
the document and not in a user-level file. The `.scrivx` root carries both
lists:

```xml
<LabelSettings>
    <Title>Label</Title>
    <DefaultLabelID>-1</DefaultLabelID>
    <Labels>
        <Label ID="-1">No Label</Label>
        <Label ID="7" Color="0.993495 0.701207 0.732587">Red</Label>
        <Label ID="8" Color="0.995418 0.790946 0.652385">Orange</Label>
        <Label ID="9" Color="0.997722 0.89273 0.652569">Yellow</Label>
        <Label ID="10" Color="0.715855 0.948712 0.697692">Green</Label>
        <Label ID="11" Color="0.702319 0.888276 0.974252">Blue</Label>
        <Label ID="12" Color="0.957566 0.766747 0.999616">Purple</Label>
    </Labels>
</LabelSettings>
<StatusSettings>
    <Title>Status</Title>
    <DefaultStatusID>-1</DefaultStatusID>
    <StatusItems>
        <Status ID="-1">No Status</Status>
        <Status ID="1">To Do</Status>
        <Status ID="6">In Progress</Status>
        <Status ID="2">First Draft</Status>
        <Status ID="3">Revised Draft</Status>
        <Status ID="4">Final Draft</Status>
        <Status ID="5">Done</Status>
    </StatusItems>
</StatusSettings>
```

**OBS.** A label carries a **colour** as three float components on the
`Color` attribute. A status carries no colour.

**OBS.** `-1` is the sentinel for "unset" in both lists, and is the
`DefaultLabelID` / `DefaultStatusID` value. It is a *member* of the list
(the popup shows `No Label` / `No Status` as the first entry), not a
separate null.

**OBS.** The lists are **project-scoped**. Each of the seven `.scriv` files
inspected carried its own `<LabelSettings>` and `<StatusSettings>` in its
own `.scrivx`, and the ID ranges differ between them: the template-derived
projects use label IDs 7–12 and status IDs 1–6, while `Sun Tzu - The Art of
War` uses label IDs 0–5 and status IDs 0–5. A user-level definition would
have had one ID space, not two.

**OBS.** The `Settings/` directory holds UI state (`ui-common.xml`,
`ui.plist`, `projectpreferences.xml`, `compile.xml`, `templateinfo.xml`).
No status or label definition appears in any of them; the only other file
mentioning `StatusItems` or `<Labels>` is the `.scrivx` itself.

**OBS.** The list elements are ordered *by definition order*, not by ID: the
status list reads `To Do, In Progress, First Draft, Revised Draft, Final
Draft, Done` while the IDs run 1, 6, 2, 3, 4, 5. The popup order therefore
follows the file, not the numeric ID.

**INF.** The metadata model is therefore: a project owns two ordered,
ID-keyed lists; a document references a status and a label by integer ID, or
by absence for "unset". Renaming a status rewrites one list entry, never a
document. This is exactly the shape Weave wants for a project-scoped status
and label.

**INF.** The metadata model is:

| Concept | Persisted as | Notes |
| --- | --- | --- |
| Status | `<StatusID>` integer | Index into a per-project status list; the human name is resolved at display time |
| Label | absent in observed files | Probably a `<Label>` element when set; the label list is project-level |
| Include in compile | `<IncludeInCompile>` Yes/No | Binary flag |
| Icon | `<IconFileName>` | A named icon, not a file path |
| Caret / selection | `<TextSelection>` `offset,length` | Under `TextSettings`, not `MetaData` |
| Created / Modified | `Created` / `Modified` attributes on `BinderItem` | ISO-ish local timestamps with offset |

**INF.** Separating `<MetaData>` from `<TextSettings>` inside a
`BinderItem` is a clean split: metadata is *about* the document, text
settings are *view state* for the document. A Weave model should do the same
rather than flattening everything into one attribute bag.

**INF.** Storing status as an **ID** rather than a name means renaming a
status ("First Draft" → "Draft") does not touch every document. Weave should
use IDs for any user-editable enum.

---

## 4.4 Index cards and the picture toggle

**OBS.** `InspectorView.nib` has an `indexCardView` outlet and a
`toggleShowSynopsisOrPicture:` action. `MainDocument.nib` has a
`synopsisPictureToggleButton` outlet.

**OBS.** The `Document` menu has `Change Icon…` and `MainDocument.nib` has
`binderChangeIconMenu` and `binderIconBuildingMenuDelegate`-adjacent
machinery (`SCRBinderIconBuildingMenuDelegate` class).

**INF.** A document can show **either its synopsis text or a picture** in the
Inspector — the same "show picture instead of text" pattern as a Finder
list item. The `indexCardView` is where a Scrivener *index card* (a linked
RTF/image card) would render.

---

## 6.5 Custom metadata

**OBS.** `CustomMetaDataInspectorView.nib` (143 objects, 29 connections) is a
separate document in the bundle.

**OBS.** `InspectorView.nib` has `customMetaDataButton`,
`customMetaDataHeaderBar`, `customMetaDataHolder`,
`customMetaDataPlaceholderView`, `customMetaDataTabButton`,
`collapseCustomMetaDataButton`, `showCustomMetaDataSettings:`,
`toggleCustomMetaDataCollapsed:`.

**OBS.** The `Project` menu has `Project Settings…`, and the binary contains
`SCRCustomMetaDataInspectorView`-adjacent classes.

**INF.** Custom metadata is a **project-level schema** with a settings UI
(`showCustomMetaDataSettings:`) and a per-document values inspector. The
Inspector tab list therefore mixes *fixed* panes (Bookmarks, Comments,
Snapshots, Notes) with *schema-driven* panes (Custom Metadata). A Weave
"Chapter properties" panel should support user-defined fields the same way.

---

## 6.6 Keyword and bookmark surfaces

**OBS.** `InspectorView.nib` has `keywordsTableView`, `keywordsController`,
`keywordsPopUp`, `keywordsAddButton`, `keywordsRemoveButton`,
`keywordsContextualMenu`, `keywordsMenu`, `keywordsHolderView`,
`collapseKeywordsButton`, `addNewKeyword`, `remove:`.

**OBS.** The bundle contains `KeywordsPanel.nib` (13 objects),
`KeywordsOutlineView.nib` (146 objects, 42 connections),
`Bookmarks.nib`, `BookmarksView.nib`, `NewBookmarkFolderChooser.nib`,
`EditBookmarkPopover.nib`, and `SnapshotsManager.nib`.

**OBS.** The `Project` menu has `Show Project Keywords` and
`Show Project Bookmarks` and `Show Project Bookmarks as Quick Reference`.

**OBS.** `MainDocument.nib` has actions `addBinderOrSearchSelectionToBookmarks:`,
`removeBinderOrSearchSelectionFromBookmarks:`, `openAsQuickReferenceInBinder:`,
`openInCopyholderInBinder:`.

**INF.** Bookmarks can be **project-wide** and are viewable *inside the
Binder* as a Quick Reference — i.e. a saved collection appears as a virtual
node in the tree. Weave's "Chapter collections" could follow this: a saved
selection rendered as a tree node.

**INF.** The keyword pane is a *list with add/remove and a popup*, and
`toggleOutlinerHidesIcons:` / `changeNumberOfKeywordChips:` suggest keywords
also appear as coloured chips on cards.

---

## 6.7 What Weave should carry over

**INF.** For a Weave chapter, the minimum viable metadata set that matches
what Scrivener treats as first-class:

| Field | Type | Why |
| --- | --- | --- |
| Title | string | Editable in place in the tree |
| Status | enum (ID-backed) | Workflow axis; surfaced in tree, outliner, status bar |
| Label | enum (ID-backed) | Taxonomy axis; drives colour |
| Section type | enum | Structural role (Chapter / Scene) |
| Include in compile | bool | Explicit inclusion control |
| Synopsis | text | One-line summary shown in the Inspector and Outliner |
| Notes | text | Longer per-document notes, in the Inspector |
| Keywords | list | Multi-tag with colour |
| Created / Modified | timestamps | Read-only, shown in the Inspector |
| Icon | named icon | Visual marker in the tree |

**INF.** Two of these — status and label — should be editable from the tree
row itself, from the status bar, and from the Inspector, because Scrivener
surfaces them everywhere and the cost of each is small.

---

## Residual unknowns

- **UNK** The custom-metadata schema definition format.
- **UNK** The Snapshots and Comments tab contents.
- **UNK** Whether the general metadata pane's field set is configurable per
  project.
- **UNK** Whether a project can carry a *second* status or label list for a
  different purpose; only one list of each was observed.

### Closed since the first pass

- The `StatusID` → name mapping: `<StatusItems>` in the project file, as
  recorded in §6.3.1.
- The `<Label>` element's serialised form: it is `<LabelID>`, an integer,
  and it is absent when unset (§6.3.1).
- Whether the status and label lists are per project or per user: **per
  project**, proven by the differing ID ranges across seven project files
  (§6.3.1).
