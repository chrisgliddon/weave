# 01 — Window Layout

How the Scrivener main window is divided, and what each region is for.

**Evidence:** `evidence/runtime/01_baseline.png`, `evidence/ib_nibs/MainDocument.json`,
`evidence/ib_nibs/DocumentView.json`, `evidence/ib_nibs/InspectorView.json`.

---

## 1.1 The three-pane frame

**OBS.** The main window is a single `AXSplitGroup` (`MainDocument.nib`,
class `SCRMainWindowController`) that splits into:

| Region | AX role | Interface Builder outlet | Source NIB |
| --- | --- | --- | --- |
| Binder (left) | `AXScrollArea` → `AXOutline` | `binder`, `binderView`, `binderTabView`, `binderController` | `MainDocument.nib` |
| Editor (centre) | `AXScrollArea` → `AXTextArea` | `mainDocumentViewPlaceholder`, `mainSplitController` | `MainDocument.nib` + `DocumentView.nib` |
| Inspector (right) | `AXGroup` / split group | `inspectorSplitView`, `inspectorPlaceholder` | `InspectorView.nib` |

**OBS.** `MainDocument.nib` declares 184 outlets and 140 target-action
connections on `SCRMainWindowController`. The outlet names are an unusually
clear map of the whole feature surface:

```
binder, binderView, binderController, binderTabView, binderSearchHolderView,
binderPlaceholder, binderMenu, binderMoveToMenu, binderCopyToMenu,
binderOpenInMenu, binderAddToCollectionMenu, binderChangeIconMenu,
binderAffectsMenu, binderCopyToProjectMenu, binderDocTemplatesMenu,
binderDocumentTargetPanel, binderDocumentTargetTextField, ...
mainSplitController, mainDocumentViewPlaceholder, supportingDocumentViewPlaceholder,
formatBarController, inspectorSplitView, inspectorPlaceholder,
collectionsController, collectionsHeaderView, collectionsHeaderTitle,
arbitraryCollectionController, arbitraryCollectionTable,
searchField, searchResultsController, searchResultsTable, searchView,
quickSearchPanel, toolbarAddMenu, toolbarInsertMenu, toolbarLayoutsMenu,
toolbarSearchMenu, toolbarFocusModeMenu, toolbarViewOptionsMenu,
moveUpDownSegmentedControl, moveLeftRightSegmentedControl, moveAllSegmentedControl,
viewModeSegmentedControl, toggleSplitTypeButton1, toggleSplitTypeButton2,
snapshotTitlePanel, duplicateDocsSheet, emptyTrashSheet, ...
```

**INF.** The Binder is not merely a tree widget — it owns its own menus, search
holder, document-target settings, and a placeholder for the *search results*
tab. The Binder is one tab of a tab view (`binderTabView`) that shares a frame
with the collection list.

---

## 1.2 The Inspector lives in the same split as the editor

**OBS.** The accessibility tree shows the centre and right regions as a single
`AXSplitGroup` (`[0,6]`), with the editor in `[0,6,1]` and the Inspector in
`[0,6,8]`. The Inspector itself is another `AXSplitGroup` containing a
**Synopsis** pane (`[0,6,8,1,1]`) and a **Notes** pane (`[0,6,8,1,3]`), split
horizontally and independently collapsible.

**OBS.** `InspectorView.nib` has 119 outlets and 38 target-actions. Its
structure is a tab view (`documentTabView`) with named tab buttons:
`bookmarksTabButton`, `commentsTabButton`, `customMetaDataTabButton`,
`notesTabButton`, `snapshotsTabButton`, plus a non-tabbed
`documentInspectorView` (the general metadata pane) and `centeredView`.

**INF.** Scrivener splits metadata into two places:

- a **general pane** always present in the Inspector (status, label, dates,
  compile flag, section type, index card preview) — see note 06;
- **optional panes** that occupy a full tab (Bookmarks, Comments, Custom
  Metadata, Notes, Snapshots).

Each optional pane is collapsible via a `collapse…Button` outlet
(`collapseSynopsisButton`, `collapseNotesButton`, `collapseKeywordsButton`,
`collapseCustomMetaDataButton`, `collapseGeneralPaneButton`) and the
corresponding `toggle…Collapsed` action. This gives the user a per-pane
height rather than an all-or-nothing Inspector.

---

## 1.3 Toolbar is a mode switch, not just a button row

**OBS.** The top toolbar contains, left to right: a document-menu button, a
search button, an add button (with a dropdown), a delete button, an
attachments button, a focus-mode button, the **view-mode segmented control**
(document / corkboard / outliner icons), a share button, an inspector toggle,
and a ">>" overflow button.

**OBS.** In the accessibility tree the view-mode control is an `AXRadioGroup`
at `[1,6,0]` with three `AXRadioButton` children at `[1,6,0,0..2]`. The
corresponding `MainDocument.nib` outlet is `viewModeSegmentedControl`, and the
actions are `changeViewOrGroupMode:`, `changeViewOrGroupModeFromMenu:`.

**OBS.** The `View` menu offers the same three modes as separate items —
`Document`, `Corkboard`, `Outline` — plus `Show All Tabs`.

**INF.** A radio group is the correct semantic for a view mode, and pairing
the toolbar radio group with equivalent menu items means the mode is always
discoverable and always reflects one true state. Weave should use a single
enumerated `EditorMode` rather than three independent visibility booleans.

---

## 1.4 Window layouts are first-class

**OBS.** `Window ▸ Layouts` offers: `Manage Layouts…`, `Default`,
`Three-Pane (Outline)`, `Three-Pane (Corkboard)`, `Editor Only`,
`Corkboard Only`, `Centered Outline`, `Dual Navigation`.

**OBS.** `MainDocument.nib` contains an outlet `toolbarLayoutsMenu` and the
`Layouts.nib` document (135 objects, 45 connections) implements the chooser.

**INF.** Scrivener treats the *arrangement of panes* as a separate, saveable
axis from the *content mode*. A user can be in Corkboard mode inside a
Three-Pane (Corkboard) layout, or in Corkboard mode with no Inspector at all.
Weave should separate "which panel is focused" from "which panels are visible".

---

## 1.5 Breadcrumb, not a path bar

**OBS.** When a child document is open, the editor header shows a breadcrumb:
`Manuscript: Part`, `Chapter: Chapter` (evidence:
`evidence/runtime/15_wysiwyg_editor.png`, `16_inspector_chapter.png`).

**OBS.** The breadcrumb is not a `AXPathControl`. The editor header group
contains a back/forward chevron pair, a folder icon, the breadcrumb text, then
a set of right-aligned controls (a hamburger-style options button, up/down
chevrons, and a split-view toggle).

**INF.** The breadcrumb encodes *ancestry*, and the up/down chevrons imply
navigating sibling documents — a "previous/next sibling" idiom rather than a
filesystem path. For Weave, `Chapter: Chapter` is the shape to copy: the
immediate parent's type and the document's own title, so the reader always
knows which Chapter they are in without a deep path.

---

## 1.6 Status bar

**OBS.** The bottom bar of the editor column carries, left to right: a
zoom control (`100%`), a live word count (`6 words`), a focus-mode toggle, a
comments toggle, and — at the far right of the window — the **label** and
**status** pop-up buttons (`No Label`, `No Status`).

**OBS.** The `AXPopUpButton` values `No Label` and `First Draft` / `No Status`
appear in the tree at `[0,6,14,0]` and `[0,6,14,1]`.

**INF.** Metadata that is frequently changed (label, status) is duplicated
into the status bar so it is editable without opening the Inspector. This is
a strong pattern for Weave: the two most-used chapter attributes should be
always reachable.

---

## 1.7 Observed geometry

**OBS.** The observed window was 823 × 1044 pt at screen position (905, 33).
The Binder occupied roughly the left 220 pt, the editor roughly 340 pt, the
Inspector roughly 250 pt. `MainDocument.nib` records an `NSTabView` frame of
`{{0, 30}, {239, 406}}` for the Inspector's tab view, so the Inspector's
designed width is 239 pt.

**INF.** At 823 pt wide the three-pane frame is cramped — the editor gets
roughly 40% of the width. This is why Scrivener ships `Editor Only` and
`Corkboard Only` layouts. Weave's default at narrow widths should collapse
the Inspector rather than let the editor become unusable.

---

## Residual unknowns

- **UNK** The exact split-view divider drag behaviour, drop positions, and
  per-pane minimum sizes were not exercised.
- **UNK** Whether layout presets persist per project or per user was not
  established from the NIB alone; `Settings/ui-common.xml` in the project
  directory would be the place to check.
- **UNK** The `Dual Navigation` and `Centered Outline` layouts were listed
  but not captured.
