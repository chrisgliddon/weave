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

**OBS.** The `StatusID` value is an **integer**, not a string. The `Label`
did not appear in the observed `.scrivx` because no document had a label set
(the UI showed `No Label` / `No La…`).

**OBS.** The `__objc_classname` string section of the binary contains the
preference key `SCRAlwaysShowSynopsisInInspector`.

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

- **UNK** The full set of project labels and statuses, and the `StatusID` →
  name mapping (the project settings directory would hold it).
- **UNK** The `<Label>` element's exact serialised form, since no observed
  document had a label.
- **UNK** The custom-metadata schema definition format.
- **UNK** The Snapshots and Comments tab contents.
- **UNK** Whether the general metadata pane's field set is configurable per
  project.
