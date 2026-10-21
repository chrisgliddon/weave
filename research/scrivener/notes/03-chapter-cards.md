# 03 — Chapter Cards (Corkboard)

The Corkboard view: how a container's children become cards, how the cards are
arranged, and what a card is made of.

**Evidence:** `evidence/runtime/08_corkboard_cards.png`,
`evidence/runtime/11_mid_drag.png`, `evidence/runtime/13_drop_complete.png`,
`evidence/runtime/23_corkboard_final.png`, `evidence/ib_nibs/DocumentView.json`.

---

## 3.1 The three editor views

**OBS.** The `View` menu offers `Document`, `Corkboard`, `Outline`. The
`MainDocument.nib` actions are `changeViewOrGroupMode:` and
`changeViewOrGroupModeFromMenu:`. `DocumentView.nib` adds
`changeViewOrGroupMode` and `changeCorkboardArrangement:`.

**OBS.** All three views render the *children of the current container*:

- **Document** — a WYSIWYG rich-text editor (see note 05).
- **Corkboard** — the children as index cards.
- **Outline** — the children as a table row with inline metadata editors.

**OBS.** When the selected node is a leaf, the Corkboard shows the empty
state `"Scene" contains no subdocuments.` (evidence
`evidence/runtime/07_corkboard_attempt.png`).

**INF.** The view mode is a property of *the container you are looking into*.
Scrivener does not ask "corkboard or document?" up front; it asks "what are
you looking at?", then renders that container's children in whichever view.
This is the single most transferable idea in the whole app: **one navigation
model, three renderings.**

**INF.** The empty state is explicit and names the offending document. Weave
should do the same for an empty Chapter rather than showing a blank pane.

---

## 3.2 What a card is

**OBS.** With a `Part` selected in the Binder and the Corkboard active, the
editor column showed two `Chapter` cards stacked vertically.

**OBS.** The accessibility tree exposed each card as an `AXTextArea` with
`AXDescription` (not title) of the form:

```
index card @ 1159,199 size 300x200
index card @ 1159,429 size 300x200
```

and the REA capture reported their titles as:

```
[0, 6, 1, 0, 0, 0] AXTextArea title='Chapter (1 of 2 index cards)'
[0, 6, 1, 0, 0, 1] AXTextArea title='Chapter (2 of 2 index cards)'
```

**OBS.** A sibling group in the tree was `AXGroup title='corkboard content area'`
at `[0, 6, 1, 0, 0, 2]`.

**OBS.** The cards are 300 × 200 pt.

**INF.** Each card is a **text area** — a `NSTextView` — not a picture of a
document. The card is an editable text surface that displays the child's
body text (truncated to the first line in the observed case, since the
Chapters were empty). The accessibility description `"index card"` and the
title `"Chapter (N of 2 index cards)"` show the card is an *accessibility
element with a computed ordinal*, not a generic table cell.

**INF.** The title format encodes three facts: the child's *title*, its
*ordinal* in the card array, and the *total* card count. A screen reader
hearing "Chapter (1 of 2 index cards)" knows both the item and its position
without extra navigation. Weave should generate equivalent accessible names.

**INF.** Because the card is a text view, editing happens **on the card**.
An author can type a chapter's opening line directly in the Corkboard without
opening the card. That is why the observed `Chapter` card showed no body
while the `Scene` card in the Document view showed "Hello world, this is a
scene." — the cards were empty and only the Scene held text.

---

## 3.3 Card anatomy

**OBS.** From the screenshot (`evidence/runtime/08_corkboard_cards.png`), each
card shows:

1. a **row header** across the top: a coloured square icon, the document title
   in bold, and a **thin horizontal rule** under the title;
2. the body area below, empty in the observed case.

**OBS.** The `DocumentView.nib` action list includes `changeCardRatio:`,
`changeCardSpacing:`, `changeCardWidth:`, `changeNumberOfCardsAcross:`, and
`changeCorkboardLabelArrangement:`.

**OBS.** `View ▸ Corkboard Options` menu items:

```
Cards Across            -> Auto-Fit, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, Other...
Arrange by Label
Layout
Show Label Colors Along Edges
Show Stamps
Show Keyword Colors
Show Card Numbers
Number Per Section
Show Blank Cards as Ghosts
Freeform
Snap To Grid
Commit Freeform Order
```

**OBS.** `DocumentView.nib` actions backing these:
`changeCardRatio:`, `changeCardSpacing:`, `changeCardWidth:`,
`changeNumberOfCardsAcross:`, `changeNumberOfKeywordChips:`,
`changeCorkboardArrangement:`, `changeCorkboardLabelArrangementColumnsMode:`,
`changeCorkboardMultipleSectionsArrangement:`,
`toggleArrangeCorkboardByLabel:`, `toggleSizeCardsToFitWidth:`,
`toggleOutlinerHidesIcons:`.

**INF.** A card has a **header/body split**: the header is chrome (icon,
title, rule) and the body is the child's text. The rule under the title
visually separates the title from the excerpt — a strong, cheap way to make a
flat list of cards scannable.

**INF.** "Cards Across" with `Auto-Fit` and a numeric 1–10 range plus
`Other…` means the grid is *column-count* based, and `changeCardRatio:`
implies a fixed **w:h** aspect ratio, not independent width and height. The
observed 300 × 200 is exactly 3:2.

**INF.** "Show Blank Cards as Ghosts" implies the card array can contain
*empty slots* — placeholder rows that show as faint outlines. This is how
index-card grids invite filling in a sequence.

---

## 3.4 Card ordering

**OBS.** The card array order is the container's child order — the same order
the Binder shows. The observed `Part` had children `Chapter`, `Chapter`, and
the Corkboard showed two cards; after a successful drag the Binder order
changed to match the new card order (see note 04).

**OBS.** `DocumentView.nib` has `commitFreeformCorkboardOrder:`,
`endCommitCorkboardOrderSheet:`, `changeCorkboardArrangement:`, and the
Corkboard Options menu offers `Freeform`, `Snap To Grid`, and
`Commit Freeform Order`.

**INF.** The Corkboard has **two arrangement modes**:

- **grid** (default) — cards in a column-wrapped grid, order = child order;
- **freeform** — cards positioned freely on a canvas, with a separate
  "commit" step that writes the visual arrangement back into the child order.

**INF.** `commitFreeformCorkboardOrder:` + `endCommitCorkboardOrderSheet:`
(the "…" button in the Corkboard footer, visible in the screenshots as the
icon next to the grid toggle) means freeform position is **staged** and only
committed on request. The card array is not continuously rewritten while
dragging.

**INF.** `changeCorkboardMultipleSectionsArrangement:` and `Number Per
Section` indicate the grid can be **sectioned** — a container's children can
be partitioned into labelled groups, each with its own card count. This maps
to Weave's "Act" concept: a Chapter could be split into numbered sections
with their own card counts.

**INF.** `Show Label Colors Along Edges` and `Show Keyword Colors` use the
document's **label** and **keywords** as a visual channel on the card itself
— colour is metadata, not decoration.

---

## 3.5 The Outliner: the table-shaped sibling of the Corkboard

**OBS.** Switching to the Outliner view (screenshot
`evidence/runtime/23_corkboard_final.png`) rendered the same two `Scene`
children as table rows with these columns:

```
Title and Synopsis | Label | Status | Section Type
```

**OBS.** Each cell contains an **inline editor**: `Label` and `Status` are
pop-up buttons showing `No La…` and `First Draft`; `Section Type` is a
pop-up showing `Scene`. The first column shows a document icon plus the
title.

**OBS.** `DocumentView.nib` declares 9 separate `toggleOutlinerColumnVisible:`
connections, plus `toggleOutlinerShowsTitlesOnly:`,
`toggleOutlinerCentersContent:`, `toggleOutlinerHidesIcons:`,
`toggleOutlinerColumnVisible:`.

**OBS.** `View ▸ Outliner Options` has `Columns` and `Use Fixed Row Height`.

**OBS.** `DocumentView.nib` actions also include `filterOutlinerOrCorkboard:`,
`dismissOutlinerAndCorkboardFilterBar:`, `toggleFilterControlsVisible:`,
`toggleFilterIncludesTextAndNotes:`, `toggleFilterOptions:`,
`changeFilterOptions:`, `changeLabelOrStatusFilter:`,
`changeMetaDataFilterOption:`, `toggleRegExFiltering:`.

**INF.** The Outliner is the **table form of the same child list**. It is not
a separate feature; it is the Corkboard's data projected into columns. The
pair `filterOutlinerOrCorkboard:` is decisive — the same filter mechanism
serves both views.

**INF.** Because both views project the same child list, the *filter* is
shared: you can filter the Outliner and the Corkboard shows the same subset.
Weave should build one filtered child-list model and render it either way.

**INF.** The Outliner is where per-row metadata editing lives. The Corkboard
is where spatial arrangement and body-text reading live. Two different jobs,
same data.

---

## 3.6 Card affordances in the footer

**OBS.** The Corkboard footer (`[0, 6, ...]`) carries, left to right: a
`+` add button, a duplicate button, a `⋮`-style options button, a
right-arrow button, an item count (`2 items`), then two right-aligned
buttons: a "custom cards across" button and a "layout/arrangement" button.

**OBS.** The `DocumentView.nib` action list includes `showCustomCardsAcrossSheet:`
and `changeNumberOfCardsAcross:`.

**INF.** The footer is a **Corkboard-specific toolbar**: its contents change
with the view mode (the Document view footer instead shows zoom, word count,
focus mode, comments). Weave should have a view-mode-scoped footer, not one
global bar.

**INF.** The item count (`2 items`) is always visible in the Corkboard, giving
immediate feedback that a filter or a drag changed the set size.

---

## Residual unknowns

- **UNK** The card's body rendering rules — how much text is shown, whether
  the card is scrollable, and how a long body truncates — were not observed
  because the observed Chapters had no body text.
- **UNK** The `Layout` submenu under `Corkboard Options` was not enumerated.
- **UNK** The `Arrange by Label` submenu was not enumerated.
- **UNK** Whether cards can be resized by dragging an edge (the
  `changeCardRatio:` / `changeCardWidth:` actions suggest a fixed ratio, but
  no resize gesture was exercised).
- **UNK** Multi-card selection and multi-card drag were not exercised.
