# 07 — The UX Flow, and What Weave Should Adopt

A synthesis: the end-to-end flow an author experiences, then the design
choices that follow for Weave. The two are kept separate on purpose —
everything in §7.1–§7.6 is observation; §7.7 onward is a Weave-side proposal.

**Evidence:** all runtime captures under `evidence/runtime/`, the NIB decodes
under `evidence/ib_nibs/`, and the live `Weave UX.scrivx`.

---

## 7.1 The flow

**OBS.** An author writing a chapter in Scrivener moves through these states:

1. **Pick a container.** The Binder shows the tree. The author selects a
   container (here `Part`).
2. **Choose a rendering.** The toolbar radio group switches between
   `Document`, `Corkboard`, `Outline`. The `View` menu offers the same three.
3. **The rendering shows the container's children.**
   - Corkboard → index cards.
   - Outline → table rows with inline metadata.
   - Document → the rich-text body of the selected *leaf*.
4. **Open a child.** Double-clicking a card or a tree row opens that
   document in the editor; the breadcrumb updates to `Chapter: Chapter`.
5. **Write.** The format bar, the ruled manuscript guides, the live word
   count.
6. **Reorganise.** Drag a card in the Corkboard, or drag a node in the
   Binder, or use `Edit ▸ Move`. The tree and the card array stay in sync.
7. **Refine metadata.** Inspector, or the Outliner row, or the status bar.
8. **Compile.** `File ▸ Compile…` opens `CompileDraft.nib`.

**OBS.** The tree and the card array are **two views of one ordered child
list**, and this was confirmed at runtime: a card drag changed the Binder
order, and the persisted `.scrivx` matched.

**OBS.** `View ▸ Editor Layout` and `Window ▸ Layouts` are separate axes:
*which mode* vs *which panes*.

**INF.** The whole model reduces to four concepts:

```
Project ── ordered tree of Documents
                │
                └── each Document is either a leaf (text) or a container
                          │
                          └── a container's children have ONE order,
                              rendered as cards / rows / or not at all
```

---

## 7.2 The observations that matter most

**OBS.** *One ordered child list, three renderings.* (§7.1 step 3)

**OBS.** *A card is a text surface, not a picture.* Cards expose
`AXTextArea` and carry the child's text.

**OBS.** *Reorder is expressed as an insertion bar between items*, in both
the Binder and the Corkboard. (note 04)

**OBS.** *The same four move verbs — Up, Down, Left, Right — are available
as drag, as a toolbar segmented control, and as a menu.*

**OBS.** *Metadata surfaces are triplicated.* Status and label are editable
in the Inspector, in the Outliner row, and in the status bar.

**OBS.** *Search renders inside the tree.* The Binder's outline widget shows
either the document tree or a result list.

**OBS.** *Deletion is a move to a visible Trash node.*

**OBS.** *Structure and content are stored separately.* The `.scrivx` holds
the ordered tree; `Files/Data/<UUID>/` holds bodies. A reorder rewrites
only the tree.

**OBS.** *Status is stored as an ID, not a name.*

**OBS.** *The editor renders the target manuscript format* — ruled lines,
not a blank page.

---

## 7.3 The Weave-side design

Everything below is a **proposal derived from the observations**, not a
description of Scrivener.

### 7.3.1 One ordered child list per container

Weave's chapter tree should have a single source of truth: an ordered list of
children per container, with a stable `id` per child. The Binder, the chapter
card board, and the chapter table should all project that list.

Concretely, this argues for a Weave domain type roughly like:

```
Chapter {
  id
  parent_id            (null for a root chapter)
  order                (position among siblings)
  kind                 (chapter | scene | note | ...)
  title
  status_id
  label_id
  synopsis
  notes
  body                 (separate from structure, as in Scrivener)
}
```

Ordering by an explicit `order` column (or an explicit position list) is
simpler to mutate than a document-order list, and it makes "reorder" a
single-field write — matching Scrivener's small write footprint.

### 7.3.2 Three renderings of the same list

| Weave surface | Scrivener analogue | Purpose |
| --- | --- | --- |
| Chapter tree (Binder) | Binder | Structure + navigation |
| Chapter card board (Corkboard) | Corkboard | Spatial overview, reorder by drag |
| Chapter table (Outliner) | Outliner | Dense metadata editing, sorting, filtering |

The card board and the table should be **one filtered list** rendered two
ways, so a filter applies to both.

### 7.3.3 Cards carry the body text

If a Weave chapter card is a text surface showing the chapter's opening
paragraph, the author can write a chapter's first line without leaving the
board. This is the Corkboard's most valuable property and it is cheap to
copy: a card is a read-only-ish text view of the child's body, plus a
header row (icon, title, rule).

### 7.3.4 One insertion affordance

Weave should adopt the insertion bar verbatim: during a drag, draw a bar in
the gap between the two neighbours the item would land between. It is
unambiguous, it is one component, and it is already proven at scale.

Two details worth copying:
- the bar uses the **accent colour**, so it reads as "the app is saying this",
  not as content;
- it is drawn **in the gap**, which removes the "on the card vs. between the
  cards" ambiguity entirely.

### 7.3.5 Four move verbs, everywhere

`Move Up`, `Move Down`, `Move Left`, `Move Right` — as a keyboard shortcut,
as a menu item, and as a toolbar control. Left/Right are the tree operations
that drag-and-drop cannot express: **promote** and **demote**. A drag can
reorder and can reparent, but promoting a node to sit *between* its parent
and its grandparent has no drag equivalent, so the verbs are not redundant
with drag.

Weave should also make the move handler a **single** operation with three
front ends, so the toolbar and the menu cannot drift.

### 7.3.6 Status and label everywhere

Pick two attributes and expose them in every surface where the document is
visible: the tree row, the card, the table row, and the status bar. Scrivener
does this with status and label, and it is why those two are usable at
speed.

In Weave's vocabulary, the natural axis pair is:
- **status** — draft / revision / final (workflow);
- **label** — a user-defined, colour-carrying tag (taxonomy).

Both should be **ID-backed** in storage so renaming does not rewrite rows.

### 7.3.7 Keep structure and body apart

Store the tree separately from the body content, joined by `id`. The
consequences Weave should want:
- a reorder writes one field;
- opening a chapter does not load the whole project;
- the tree can be cached, diffed, or synced separately from the prose.

### 7.3.8 Show the target format

If Weave writes manuscript prose, the editor should render the *manuscript*
page — line guides, margins, a typographic scale — not a generic text box.
This is what makes the editor feel like an authoring tool rather than a
form field.

### 7.3.9 Delete to a visible trash

A chapter deletion should move the node to a visible "Trash" container, not
destroy it. Cheap to implement, and it makes the destructive path safe.

---

## 7.4 What to watch out for

**INF.** Scrivener's model has costs. Before copying it wholesale:

- **A document's kind is a *folder* or a *text* item**, convertible with an
  explicit command. If Weave makes "chapter" a distinct type that can
  contain children, it gains a richer model but loses the ability to convert
  a leaf into a container and back. Consider keeping one node type with a
  `kind` field plus children, and allowing both.
- **Status/label are project-scoped lists.** That means the project settings
  screen must own them, and the Inspector needs a settings affordance.
- **The Corkboard's freeform mode adds a second, staged ordering.** If Weave
  starts with grid-only, it avoids the "commit order" concept entirely. That
  is a reasonable simplification: the freeform mode exists for a genuinely
  spatial workflow (storyboarding), and the grid already covers reordering.
- **Trash, Collections, Bookmarks, Search, Snapshots, Comments, and Custom
  Metadata** are all separate systems in Scrivener. Weave does not need all
  of them; the tree + card board + table + status/label set is the core.

---

## 7.5 A suggested Weave feature slice

**INF.** A coherent first slice for Weave, in dependency order:

1. **Ordered chapter tree** with `id`, `parent_id`, `order`, `kind`, `title`,
   `body`. The tree renders as a Weave outline.
2. **Chapter card board** rendering the selected container's children as
   cards, in tree order.
3. **Drag to reorder**, with the insertion bar, writing `order`.
4. **Chapter table** (Outliner analogue) with inline `status` and `label`
   editors.
5. **Move verbs** (Up/Down/Left/Right) as menu + shortcut.
6. **Status bar** showing and editing `status` and `label` for the selection.
7. **Trash** as a visible container.

Steps 1–4 are the minimum for "write chapters and organise the chapter
cards" to feel right. Steps 5–7 are what make it usable at speed.

---

## 7.6 Explicit non-conclusions

- **UNK** How Scrivener handles a *large* project — the Binder's performance
  with thousands of nodes was not exercised.
- **UNK** The compile/export pipeline in detail; `CompileDraft.nib` and
  `CompileFormatEditorView.nib` are large and were only inventoried.
- **UNK** Whether the Corkboard's card body shows the child's *first
  paragraph*, its *synopsis*, or its *full text* — the observed cards were
  empty.
- **UNK** Multi-select behaviour in the Binder and on the card board.
- **UNK** The interaction between the Binder's search results and
  drag-and-drop.

---

## 7.7 Boundaries observed

**INF.** A note on method rather than design: everything above came from
black-box observation of a locally installed application — accessibility
trees, screenshots, decoded NIBs, the shipped binary's selector names, and
the project's own persisted files. No Scrivener source or documentation was
read or copied. The Weave-side proposals in §7.3–§7.5 are stated in Weave's
own terms and do not depend on Scrivener internals.
