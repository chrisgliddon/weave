# 05 — The WYSIWYG Editor

The Document view: the rich-text surface, its formatting toolbar, and the
document chrome around it.

**Evidence:** `evidence/runtime/15_wysiwyg_editor.png`,
`evidence/runtime/03_chapter_selected.png`, `evidence/ib_nibs/DocumentView.json`,
`evidence/ib_compact.json` (which inventories
`ScrAppKit.framework/…/TextFormatBar.nib`).

---

## 5.1 The format bar

**OBS.** A second toolbar row sits directly beneath the main toolbar and above
the editor. Left to right it carries:

| Control | Accessibility role |
| --- | --- |
| Style picker (`No Style`) | `AXPopUpButton` |
| Font family (`AppleSystemU…`) | `AXPopUpButton` |
| Weight (`Regular`) | `AXPopUpButton` |
| Size (`18`) | `AXPopUpButton` |
| Bold `B` | `AXButton` |
| Italic `I` | `AXButton` |
| Underline `U` | `AXButton` |
| 4 × alignment (left / centre / right / justify) | `AXButton` |
| Highlight colour swatch | `AXButton` |
| Font colour swatch | `AXButton` |
| Indent / outdent | `AXButton` |
| Line spacing (`1.1`) | `AXPopUpButton` |
| Lists menu (`≡▾`) | `AXMenuButton` |

**OBS.** In the Document view screenshot the format bar replaced the
document-style controls: a paragraph-style chip (`¶` with dropdown), an
`Aa` (character style) chip, then `B I U`, four alignment buttons, a
highlight swatch, a colour swatch, `1.1` spacing, and a list menu.

**OBS.** The `ScrAppKit.framework` bundle contains `TextFormatBar.nib`
(274 objects, 71 connections) — the format bar is a **shared framework
component**, reused across editors.

**OBS.** The format bar's controls become *context-sensitive*: in the
Corkboard/Outliner views the row showed `B I U` plus alignment plus colours
plus spacing but no font family/size/weight pickers, because the selection was
not a text selection.

**INF.** The format bar is a single reusable bar that **enables and disables
groups** based on the current selection and view. Weave should do the same
rather than swapping the whole bar per view.

**INF.** The presence of a *paragraph style* picker (`No Style`) and a
*character style* picker (`Aa`) alongside direct formatting means Scrivener
supports both a style hierarchy and direct formatting, with the style pickers
as the abstraction layer. The `styles.xml` file in `Files/` corroborates a
project-level style sheet.

---

## 5.2 The document view itself

**OBS.** The Document view shows a centred text column with **horizontal
dashed guide lines** at regular intervals down the page, representing the
manuscript's ruled lines. A text cursor sits at the top-left of the column
when the document is empty.

**OBS.** The document header above the text column reads `Manuscript: Part`
with a back/forward chevron pair, then the breadcrumb, then right-aligned
controls.

**OBS.** The word count in the status bar updates live (`0 words` when the
Chapter was empty, `6 words` when the Scene with "Hello world, this is a
scene." was open).

**INF.** The dashed lines are the **page/line guides** that mirror how the
text will print. This is a WYSIWYG decision: the author sees the ruled
manuscript, not a blank page. Weave's editor should render the target
manuscript format, not a generic block of text.

**OBS.** `MainDocument.nib` actions `toggleRuler:` and `toggleWrapToPage:`
exist, plus `toggleFormatBar:` and `toggleTextFocusMode`-family actions in
`DocumentView.nib` (`changeTextFocusMode:` appears 4× in `MainDocument.nib`).

**INF.** Focus mode (`toggleTextFocusMode`) dims everything except the
current paragraph. For a long-form editor this is a first-class feature, not
a novelty.

---

## 5.3 Editor configuration

**OBS.** `DocumentView.nib` exposes these editor-shaped actions:

```
changeCurrentElement:
changeSectionType:
changeStatus:
changeLabel:
changeFilterOptions:
changeVisibleOutlineColumns:
changeLabelOrStatusFilter:
changeMetaDataFilterOption:
toggleOutlinerColumnVisible:
toggleOutlinerShowsTitlesOnly:
toggleOutlinerCentersContent:
toggleOutlinerHidesIcons:
toggleIncludeInCompile:
toggleLockInspector:
toggleLockedInPlace:
toggleBacktrack:
toggleSelectionAffectsOtherEditor:
toggleLockBinderDocGroupViewMode:
toggleRegExFiltering:
toggleFilterIncludesTextAndNotes:
```

**OBS.** `DocumentView.nib` also has `matchSplitDocuments:`, `openSelectionInEditor:`,
`openSelectionInCopyholder:`, `openSelectionInOtherEditor:`, `revealSelectionInBinder:`,
`revealInBinderFromContextualMenu:`, `revealInOtherEditor:`, `scaleTextView:`,
`goToPage:`, `goToNextPage:`, `goToPreviousPage:`, `scrollToNextPage:`,
`scrollToPreviousPage:`.

**OBS.** `MainDocument.nib` has `toggleDocumentViewSplitType:` (twice) and
outlets `toggleSplitTypeButton1`, `toggleSplitTypeButton2`.

**INF.** The editor can be **split** into two panes (`toggleDocumentViewSplitType:`)
so two documents can be viewed side by side. `matchSplitDocuments:` suggests
a "scroll-match" between the two. `toggleSelectionAffectsOtherEditor:` and
`toggleBinderSelectionAffects` make the coupling optional — matching the
explicit `Binder Selection Affects` menu in note 02.

**INF.** `toggleBacktrack:` is a "where was I" history feature; `Edit` had
`Undo`/`Redo`.

**INF.** `toggleLockedInPlace:` and `toggleLockInspector:` allow the author
to pin the current document or the Inspector while they browse elsewhere —
a real workflow need in a long manuscript.

**OBS.** The `Navigate` menu has `Reveal in Binder`, `Reveal Draft Folder`,
`Go To`, `Open Quick Reference`, `Collections`, `Open`, `Editor`, `Media`,
`Move Focus To`, `Inspect`, `Outline Groups`, `Binder Selection Affects`,
`Editor Selection Affects`, `Clear All Navigation Options`.

**INF.** `Reveal in Binder` and `revealSelectionInBinder:` are the
bidirectional link between the editor and the tree: whatever is in the
editor, the tree can highlight, and vice versa.

---

## 5.4 The text engine

**OBS.** `ScrAppKit.framework` exposes these editor classes:

```
KBBaseTextView
KBAttributesChangingTextView
KBFormattingChangingTextView
KBLinedTextView
KBNotesTextView
KBCommentsEditor
KBFieldEditor
```

**OBS.** The main binary adds `SCRTextView`, `SCRTabbingTextView`,
`SCRNotesTextView`, `SCRMetadataTextView`, `SCROutlinerTextStorage`,
`SCRTextEditorController`, `SCRTextEditorSettingsManager`, `SCREditorUIStates`.

**OBS.** `KBBaseTextView` has methods `colorIconForMenuItem:inView:`,
`colorPanelAffectsBackgroundColor`, `restorePreviewPanelControl`;
`KBAttributesChangingTextView` has `touchBarSupportsFormattingOnly`;
`KBFormattingChangingTextView` is the counterpart.

**INF.** `KBAttributesChangingTextView` and `KBFormattingChangingTextView` are
two sides of the same contract: one reacts when the *text selection's
attributes* change, the other when the *formatting state* changes. Together
they keep the format bar's enabled/disabled state in sync with the caret —
the mechanism behind the context-sensitive toolbar described in 5.1.

**INF.** `KBLinedTextView` is the ruled-manuscript view. `KBNotesTextView` and
`SCRNotesTextView` are the Inspector's notes pane. `KBCommentsEditor` is the
comment composer.

**INF.** `SCREditorUIStates` suggests the editor's *visual configuration* is
a first-class persisted object — which is what makes "Focus Mode" a
recoverable state rather than a transient flag.

**OBS.** The `ScrAppKit` bundle also contains `LinguisticFocus.nib`,
`KBFindPanel.nib`, `FormatFinder.nib` (219 objects, "Find by Formatting"),
`Replace.nib`, `IndentsAndTabsPanel.nib`, `Comments`-related nibs, and
`MultiMarkdown` / `Ebook-HTML5` resource directories.

**INF.** `FormatFinder` = "find by formatting" — the editor indexes formatting
as well as text, so an author can find every bolded passage. `LinguisticFocus`
relates to the system language services.

---

## 5.5 Insert affordances

**OBS.** The `Insert` menu carries: `Table…`, `Image from File…`,
`Image Linked to File…`, `Image Linked to Document`, `Comment`,
`Inline Annotation`, `Footnote`, `Inline Footnote`, `Break`,
`Horizontal Line`, `Auto-Number`, `Draft Word Count`, `Draft Character Count`,
`Endnote Marker`, `Current Date & Time`, `Media Time Stamp`.

**OBS.** `MainDocument.nib` actions: `insertImageFromFile:`,
`insertLinkedImageToFile:`, `insertMathTypeEquation:`, `addCommentAtSelection:`,
`addFootnoteAtSelection:`, `importFiles:`, `importWebFilesInBinder:`.

**INF.** A "linked image" and an "inline footnote" are separate concepts —
the editor distinguishes *content* from *anchored content*. A footnote is
anchored at a caret position but lives elsewhere. Weave's editor needs the
same distinction for anything Weave would call an aside or a reference.

---

## 5.6 Compile-adjacent editors

**OBS.** `ScrAppKit` / the main bundle contain `CompileDraft.nib` (2 436
objects, 531 connections) and `CompileFormatEditorView.nib` (5 208 objects,
1 022 connections) — by far the two largest NIBs in the app after the main
menu.

**OBS.** `ScrAppKit.framework/Resources/Base.lproj/` contains
`StylesManager.nib`, `StylesPanel.nib`, `StylesUpdateSheet.nib` — the
project-wide style management UI.

**INF.** The compile pipeline and the style manager are the most complex
surfaces in the app. A project stylesheet (`styles.xml` in `Files/`) governs
the editor's formatting vocabulary. If Weave wants WYSIWYG fidelity, it needs
an equivalent style model, not just inline formatting.

---

## Residual unknowns

- **UNK** The exact set of paragraph styles offered by the `No Style` popup
  was not enumerated.
- **UNK** The list of auto-number / AutoComplete behaviours.
- **UNK** Whether the format bar is user-customisable (it was a fixed row in
  every observed state).
- **UNK** The `StylesManager` / `StylesPanel` behaviour, and the full
  `styles.xml` schema.
- **UNK** The split-document (`toggleDocumentViewSplitType:`) behaviour was
  not exercised.
