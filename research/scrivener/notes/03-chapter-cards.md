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
flat list of cards scannable. (The second pass established *which* text the
body carries: the synopsis if there is one, otherwise the body text. See
§3.4.1.)

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

## 3.4.1 What a card body shows

The card body rule was established by giving one document a body and a
synopsis with different marker text, then reading the rendered card.

**OBS.** With **both** a synopsis and a body set, the card shows the
**synopsis** (`evidence/runtime/50_card_body_vs_synopsis.png`). The body
text was not shown at all.

**OBS.** With the synopsis **cleared** and the body still set, the card shows
the **body** (`evidence/runtime/51_card_body_no_synopsis.png`).

**OBS.** The two captures are the same document, differing only in whether
the synopsis is set, so the pair isolates the rule.

**INF.** The rule is:

> **The card body shows the document's synopsis if it has one, and the
> document's body text if it does not.**

This corrects the first pass's guess (§3.3) that a card always carries the
body text. The two fields are not alternatives chosen by the author — the
synopsis *wins* whenever it exists. A card is therefore a **summary
surface**: it shows the author's own one-line summary, and falls back to raw
prose only when no summary was written.

**OBS.** The synopsis and the body are stored **separately**. The body is
RTF in `Files/Data/<UUID>/content.rtf`; the synopsis is plain text in
`Files/Data/<UUID>/synopsis.txt`. A document with both has both files.

**OBS.** The card body is **not scrollable**. The accessibility tree for the
Corkboard area contains exactly two `AXTextArea` elements (one per card)
and no `AXScrollBar` inside a card; the only scroll bar belongs to the
board's own `AXScrollArea`. A long body is clipped, not scrollable.

**OBS.** The card body **truncates with an ellipsis** rather than clipping
mid-glyph. A body of five paragraphs rendered four paragraphs and then
`…` on the truncated word
(`evidence/runtime/36_corkboard_with_body.png`).

**OBS.** The truncation point is **not a fixed character count**. Bodies of
250, 290, 300, 310, 320, 400, and 2 000 characters produced the *same*
visible card text, and the visible text is the same number of rendered
lines each time
(`evidence/runtime/39_corkboard_400_chars.png`,
`evidence/runtime/40_corkboard_2000_chars.png`). The body is cut where the
card's text area runs out, so the visible amount depends on the **line
count**, which depends on the font metrics and the card width.

**OBS.** The ellipsis character is a single `…` (U+2026) appended
immediately after the last complete word, with no trailing space.

**INF.** Weave should adopt the synopsis-first rule, because it makes the
card a **summary surface** rather than a miniature document view. An author
who writes a synopsis gets a clean one-liner on every card; an author who
does not gets the opening of their prose, which is a reasonable fallback.
The Weave model needs a distinct `synopsis` field on a chapter, separate
from the body, and the card renderer prefers it.

**INF.** Weave should also make the truncation **explicit**. A card that
silently cuts text mid-sentence gives the reader no signal that more
exists; the ellipsis is a small, cheap affordance and is worth copying.

**INF.** Because the visible length depends on layout rather than on a
character budget, a Weave card cannot promise a fixed "first N characters".
If Weave wants a stable contract, it should define the rule in **lines**
(a fixed number of rendered lines) or in **characters** — but it should pick
one deliberately rather than inherit an accidental layout dependency.

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

## 3.5.1 The complete column set

The column vocabulary was enumerated by reading `View ▸ Outliner Options`
through the accessibility API, which flattens the `Columns` submenu into the
`Outliner Options` menu. The full menu is in
`evidence/runtime/30_outliner_columns.txt`; the resulting table is in
`evidence/runtime/33_outliner_container_rows.png` and
`evidence/runtime/34_outliner_row_cells.txt`.

**OBS.** `View ▸ Outliner Options` contains **17 columns** and **4
modifiers** of the title column, plus two independent options that are not
columns at all (`Center Content`, `Use Fixed Row Height`).

| # | Menu entry | Column | Enabled by default |
| --- | --- | --- | --- |
| 1 | `Title` | Title and Synopsis | yes |
| 2 | `and Synopsis` | modifier of the title column | yes |
| 3 | `with Icons` | modifier of the title column | yes |
| 4 | `with Numbers` | modifier of the title column | no |
| 5 | `Label` | Label | yes |
| 6 | `Status` | Status | yes |
| 7 | `Section Type` | Section Type | yes |
| 8 | `Keywords` | Keywords | no |
| 9 | `as Color Chips` | modifier of the Keywords column | yes |
| 10 | `Created Date` | Created Date | no |
| 11 | `Modified Date` | Modified Date | no |
| 12 | `Word Count` | Word Count | no |
| 13 | `Character Count` | Character Count | no |
| 14 | `Total Word Count` | Total Word Count | no |
| 15 | `Total Character Count` | Total Character Count | no |
| 16 | `Include in Compile` | Include in Compile | no |
| 17 | `Target` | Target | no |
| 18 | `Target Type` | Target Type | no |
| 19 | `Progress` | Progress | no |
| 20 | `Total Target` | Total Target | yes |
| 21 | `Total Progress` | Total Progress | yes |

**OBS.** The `Columns` entry itself is a **disabled header**, not a column.
The AX API reports the `Columns` submenu's items as siblings of the
`Outliner Options` items, so the submenu has no separate checkable
identity.

**OBS.** The four entries that are *modifiers* rather than columns are
`and Synopsis`, `with Icons`, `with Numbers` (all of the title column) and
`as Color Chips` (of the Keywords column). The title column's persisted
flags are separate booleans: `<TitlesOnly>No</TitlesOnly>`,
`<HideIcons>No</HideIcons>`, `<ShowNumbers>No</ShowNumbers>`, and
`<KeywordsAsChips>Yes</KeywordsAsChips>`.

**OBS.** The **default visible set** is the six columns marked above:
Title and Synopsis, Label, Status, Section Type, Total Target, Total
Progress. This is confirmed independently by the persisted
`Settings/ui-common.xml`:

```xml
<Outliner>
    <Columns>
        <ColID>title</ColID>
        <ColID>label</ColID>
        <ColID>status</ColID>
        <ColID>sectionType</ColID>
        <ColID>totalTargetCount</ColID>
        <ColID>totalProgress</ColID>
    </Columns>
    <TitlesOnly>No</TitlesOnly>
    <HideIcons>No</HideIcons>
    <ShowNumbers>No</ShowNumbers>
    <KeywordsAsChips>Yes</KeywordsAsChips>
    <FixedRowHeights>No</FixedRowHeights>
</Outliner>
```

**OBS.** The persisted column identifiers are `title`, `label`, `status`,
`sectionType`, `totalTargetCount`, `totalProgress`. Enabling every column
in the running app and re-reading the file adds the remaining identifiers
in the same order the menu lists them: `keywords`, `createdDate`,
`modifiedDate`, `wordCount`, `characterCount`, `totalWordCount`,
`totalCharacterCount`, `includeInCompile`, `target`, `targetType`,
`progress`.

**OBS.** The **column header controls** are `AXButton`s with these titles:
`Title and Synopsis`, `Label`, `Status`, `Section Type`, `Total Target`,
`Total Progress`, `Keywords`, `Created Date`, `Modified Date`, `Words`,
`Characters`, `Total Words`, `Total Characters`, `Include In Compile`,
`Target`, `Target Type`, `Progress`. Note the menu names differ from the
header names for four columns: `Word Count` → `Words`, `Character Count` →
`Characters`, `Total Word Count` → `Total Words`, `Total Character Count` →
`Total Characters`.

**OBS.** The **cell editors**, read from a live row, are:

| Column | Editor | Example value |
| --- | --- | --- |
| Title and Synopsis | `AXTextField` | `1.1.1. Scene` |
| Label | `AXPopUpButton` | `No Label` |
| Status | `AXPopUpButton` | `First Draft` |
| Section Type | `AXPopUpButton` | `Scene` |
| Keywords | `AXStaticText` | `0` |
| Created Date | `AXTextField` | `October 21, 2026 at 9:53 AM` |
| Modified Date | `AXTextField` | `October 21, 2026 at 10:23 AM` |
| Word Count | `AXTextField` | `0 words` |
| Character Count | `AXStaticText` | `0` |
| Total Word Count | `AXTextField` | `0` |
| Total Character Count | `AXTextField` | `0` |
| Include in Compile | `AXCheckBox` | on |
| Target | `AXTextField` | `0` |
| Target Type | `AXPopUpButton` | `Words` |
| Progress | `AXStaticText` | `0` |
| Total Target | `AXTextField` | `0 words` |
| Total Progress | `AXStaticText` | `0` |

**OBS.** The title column shows a **computed hierarchical number**
(`1.1.1. Scene`) alongside the title. The number is derived from the
document's position in the tree, not stored on the document.

**INF.** The column set splits cleanly into three groups: **identity**
(title), **taxonomy** (label, status, section type, keywords), and
**measure** (dates, counts, compile flag, target and progress). A Weave
chapter table needs the same split, and the identity and taxonomy groups are
the ones worth surfacing by default; the measure group is for power users
and can start hidden.

**INF.** Every taxonomy column is a **pop-up button**, not a free-text
field. The value is therefore always one of a known set, and an unset value
renders as an explicit `No Label` / `No Status` rather than as blank space.
Weave's status and label editors should be the same shape.

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

- **UNK** The `Layout` submenu under `Corkboard Options` was not enumerated.
- **UNK** The `Arrange by Label` submenu was not enumerated.
- **UNK** Whether cards can be resized by dragging an edge (the
  `changeCardRatio:` / `changeCardWidth:` actions suggest a fixed ratio, but
  no resize gesture was exercised).
- **UNK** Multi-card selection and multi-card drag were not exercised.

### Closed since the first pass

- The card's body rendering rules: §3.4.1.
- The full column set and the default visible set: §3.5.1.
