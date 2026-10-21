# 04 — Drag and Drop

The drag-and-drop surface: what the app calls it, how a drop is validated,
what the affordance looks like, and what a completed drag actually changes.

**Evidence:** `evidence/runtime/11_mid_drag.png` (mid-drag, drop indicator
visible), `evidence/runtime/13_drop_complete.png` (after drop),
`evidence/runtime/14_restore_mid_drag.png` (reverse drag, drop indicator),
the `__objc_methname` section of the shipped arm64 slice
(`/tmp/scrivener_methods.txt`, 16 042 selectors), and the live `.scrivx`.

---

## 4.1 The API surface

**OBS.** The shipped `arm64` slice's `__objc_methname` section contains 16 042
distinct selectors. 128 of them concern dragging or dropping. Grouped by
subsystem:

### Binder (outline) drag and drop

```
outlineView:acceptDrop:item:childIndex:
outlineView:validateDrop:proposedItem:proposedChildIndex:
outlineView:updateDraggingItemsForDrag:
outlineView:draggingSession:willBeginAtPoint:forItems:
outlineView:draggingSession:endedAtPoint:operation:
outlineView:namesOfPromisedFilesDroppedAtDestination:forDraggedItems:
outlineView:didDragTableColumn:
```

**OBS.** `acceptLocalDropInView:parent:childIndex:` and
`acceptLocalDropInView:parent:childIndex:labelIndex:` — the *local* drop path
is separate from the generic outline path and takes a **parent** and a
**child index** explicitly.

**OBS.** `acceptExternalDrop:onView:inParent:atIndex:importedBinderDocuments:`
(and the `shouldSelect:` and `labelIndex:` variants) — an external drop
returns the documents it created, and can also assign a label.

### Corkboard drag and drop

```
corkboardView:acceptDrop:atCardIndex:sectionIndex:threadIndex:dropOperation:
corkboardView:acceptFreeformDrop:atCoordinate:draggedCardIndex:
corkboardView:validateDrop:proposedCardIndex:proposedDropOperation:actualCardIndex:sectionIndex:threadIndex:
corkboardView:validateFreeformDrop:
corkboardView:relativeDragIndexForCardAtIndex:
corkboardView:updateDraggingItemsForDrag:
corkboardView:draggingSession:willBeginAtPoint:forCardIndexes:
corkboardView:draggingSession:endedAtPoint:operation:
corkboardView:shouldUseNarrowDropZoneForFreeformDrop:
corkboardViewAllowsThreadChangeDropsOnly:
```

**OBS.** The Corkboard drop coordinates are **four-dimensional**:
`cardIndex`, `sectionIndex`, `threadIndex`, `dropOperation`. The Outliner
drop coordinates are **two-dimensional**: `proposedItem`, `proposedChildIndex`.

**INF.** The Corkboard's richer coordinates mean a card can be dropped into a
different *section* (a partitioned group of cards) and, in freeform mode, at
an arbitrary coordinate. `threadIndex` suggests cards can be threaded
(one card above another in a stack).

**INF.** `corkboardView:shouldUseNarrowDropZoneForFreeformDrop:` is a
deliberate UX decision: near a card edge, the drop zone narrows so the
drop lands *between* cards rather than *on* one. That is how a grid avoids
ambiguity between "insert before" and "insert after".

### Drag source and pasteboard

```
pasteboardItemForDraggedBinderItems:source:freeformInfo:
pasteboardItemForDraggedBinderItems:source:freeformInfo:additionalDeclaredTypes:
setUpPasteboardForDrag:withBinderItems:source:freeformInfo:
setUpPasteboardForDrag:withBinderItems:source:freeformInfo:additionalDeclaredTypes:
dragImage:at:offset:event:pasteboard:source:slideBack:
beginDraggingSessionWithItems:event:source:
draggedItems
draggedItemsAreLocal
binderDocumentsDragIsLocal:
collectionIDDragIsLocal:
isKeywordDragLocal:
draggingPasteboard
draggingTypes
mainDragType
setMainDragType:
setInternalDragType:
```

**OBS.** `setUpPasteboardForDrag:withBinderItems:source:freeformInfo:` and
the `freeformInfo:` parameter on the pasteboard-item factory show that the
**freeform card position is carried in the pasteboard**, not looked up
separately. A drag therefore carries its own arrangement payload.

**OBS.** `draggedItemsAreLocal`, `binderDocumentsDragIsLocal:`,
`collectionIDDragIsLocal:` and `isKeywordDragLocal:` are four separate
*locality* predicates — one per draggable kind. Each drag source declares
whether it is dragging within the same project.

**INF.** Locality is decided **per drag source kind**, not globally. A
document drag, a collection-id drag, and a keyword drag each answer the
question differently, and the drop handler consults the matching predicate.
Weave should model drag locality the same way — as a property of the payload,
not the session.

**OBS.** `containsSourceOfDrag:` — the drop target can ask whether a
candidate contains the drag's source, which is the standard guard against
dropping a container into its own descendant.

**OBS.** `acceptLitLatOutlineDataDragWithItems:onView:inParent:atIndex:importedItems:`
— a vendor-named cross-application drag format for the outline data type.

---

## 4.2 The drop affordance

**OBS.** During a live drag (Corkboard, dragging card 1 downward over card 2),
the screenshot `evidence/runtime/11_mid_drag.png` shows a **solid pink/magenta
horizontal bar** drawn in the gap between card 1 and card 2, spanning the
full width of the card column.

**OBS.** The bar is the app's accent colour (the same pink used for the
selected Binder row and the active toolbar segment). It is ~4 px tall and
sits *in the gap*, not on top of a card.

**OBS.** The reverse drag (card 2 upward, `evidence/runtime/14_restore_mid_drag.png`)
shows the same bar, this time between the top edge of the content area and
card 1.

**INF.** The indicator is a **insertion bar**, not a card highlight. It
communicates "the dropped item will be placed *here*, between these two
neighbours" — the standard outline/binder idiom, and the correct one for a
reorder operation.

**INF.** Because the bar is drawn in the gap, the app must compute a
**nearest-gap** target from the pointer position, rather than a nearest-card
target. This is what `corkboardView:relativeDragIndexForCardAtIndex:` and
`corkboardView:shouldUseNarrowDropZoneForFreeformDrop:` support.

---

## 4.3 An observed reorder, end to end

**OBS.** Sequence, all captured:

1. `08_corkboard_cards.png` — Corkboard of `Part` showing two `Chapter` cards
   stacked vertically. Binder order under `Part`: `Chapter`, `Chapter`.
2. `11_mid_drag.png` — mid-drag; pink insertion bar visible between the two
   cards.
3. `13_drop_complete.png` — after the drop. The **first** card is now
   outlined in pink (selected), and the Binder order under `Part` has become
   `Chapter`, `Chapter` where the moved document is now **second**.
4. `14_restore_mid_drag.png` — the reverse drag in flight, insertion bar
   visible above card 1.
5. After the reverse drop, the Binder order was restored to the original
   `Chapter` (with two `Scene` children), `Chapter` (with one `Scene` child).

**OBS.** The accessibility tree before the drop:

```
[0, 6, 1, 0, 0, 0] AXTextArea title='Chapter (1 of 2 index cards)'
[0, 6, 1, 0, 0, 1] AXTextArea title='Chapter (2 of 2 index cards)'
```

**OBS.** The accessibility tree of the Binder after the drop showed the
first `Part`'s children as `Chapter`, `Chapter` with the *moved* document's
own child list now nested under the second slot, confirming the move was a
**reparenting/reordering within the same parent**, not a copy.

**OBS.** The persisted `.scrivx` after the drag and the reverse drag matches
the restored Binder order exactly:

```
Part [Folder]
├── Chapter [Folder]
│   ├── Scene [Text]
│   └── Scene [Text]
└── Chapter [Folder]
    └── Scene [Text]
```

**INF.** A card drag **rewrites the child order of the container**, which
means it rewrites the `<Children>` element order in the `.scrivx` and nothing
else. The document bodies, UUIDs, and metadata are untouched. The reordering
is therefore a *structural* operation with a very small write footprint.

**INF.** The moved document kept its own children. So dragging a Chapter card
moves the **whole subtree**.

---

## 4.4 The move verbs

**OBS.** `MainDocument.nib` declares the actions:

```
moveBinderDocumentsUp:
moveBinderDocumentsDown:
moveBinderDocumentsLeft:
moveBinderDocumentsRight:
moveBinderDocumentsFromSegmentedControl:
moveSelectedItemsRight:
```

**OBS.** `MainDocument.nib` has outlets `moveUpDownSegmentedControl`,
`moveLeftRightSegmentedControl`, `moveAllSegmentedControl`, and the menu
representations `moveUpDownMenuRepresentation`,
`moveLeftRightMenuRepresentation`, `moveAllMenuRepresentation`.

**OBS.** The `Edit ▸ Move` menu offers `Move Up`, `Move Down`, `Move Left`,
`Move Right`.

**INF.** The same four operations are exposed three ways: a toolbar
segmented control, a menu, and drag-and-drop. All three dispatch to the same
underlying "move documents" operation.

**INF.** `moveAllSegmentedControl` and `moveAllMenuRepresentation` add a
**"move to front / back"** variant — a bulk repositioning control distinct
from single-step nudging.

**INF.** `moveBinderDocumentsFromSegmentedControl:` is the shared handler
that the segmented controls call, meaning the toolbar control and the menu
items cannot drift apart.

---

## 4.5 Other drag surfaces

**OBS.** Additional drag/drop selectors in the same binary:

```
tabListView:acceptDrop:onTabAtIndex:
tabListView:validateDrop:onTabAtIndex:
tabListView:shouldDragTabAtIndex:toNewIndex:adjustedIndex:
headerView:acceptDrop:  /  headerView:validateDrop:
headerView:draggingSession:willBeginAtPoint:
headerView:draggingSession:endedAtPoint:operation:
imageView:acceptDrop:  /  imageView:validateDrop:
assistantHeaderView:acceptDrop:  /  assistantHeaderView:validateDrop:
browser:acceptDrop:atRow:column:dropOperation:
browser:validateDrop:proposedRow:column:dropOperation:
browser:canDragRowsWithIndexes:inColumn:withEvent:
tableView:acceptDrop:row:dropOperation:
tableView:validateDrop:proposedRow:proposedDropOperation:
textView:shouldAcceptImageFileDrag:
textView:draggedCell:inRect:event:atIndex:
window:shouldDragDocumentWithEvent:from:withPasteboard:
acceptStringFromPasteboard:item:childIndex:dropInfo:
```

**OBS.** `SCRAllowDropOnsInCorkboard` appears among the binary's preference
key strings.

**INF.** `tabListView:shouldDragTabAtIndex:toNewIndex:adjustedIndex:` is
notable: the tab strip is itself a drag-reorder surface, and the "adjusted"
index shows the implementation accounts for the removal of the dragged item
shifting later indices. The same class of bug exists in any list reorder.

**INF.** `SCRAllowDropOnsInCorkboard` (a user preference) means dropping
*onto* a card (as opposed to between cards) is a **toggleable feature**. The
drop modes are configurable.

---

## 4.6 What a Weave implementation needs

**INF.** From the observed behaviour, a Weave chapter-card drag needs:

| Concern | Scrivener's answer | Note |
| --- | --- | --- |
| Reorder within a container | Drag a card into the gap; a bar shows the insertion point | 4.2, 4.3 |
| Reorder with a keyboard | `Move Up/Down/Left/Right` | 4.4 |
| Reorder with a toolbar | Segmented move control, same handler as the menu | 4.4 |
| Move to another container | Drag onto a card in another container (toggleable) | 4.5 |
| Prevent cycles | `containsSourceOfDrag:` | 4.1 |
| Carry arrangement data | `freeformInfo:` rides in the pasteboard | 4.1 |
| Know if the drag is local | Per-source-kind locality predicates | 4.1 |
| Disambiguate "on" vs "between" | Narrow drop zone near card edges | 4.1 |
| Persist the result | Rewrite child order in the container's document | 4.3 |

---

## Residual unknowns

- **UNK** The exact insertion-bar geometry, colour source, and whether it
  animates were observed only as a single still frame.
- **UNK** Cross-container card drops were not exercised.
- **UNK** Dragging a card *onto* another card (the "nest" drop) was not
  exercised; `SCRAllowDropOnsInCorkboard` implies it exists.
- **UNK** Autoscroll behaviour during a drag near the edge of the card area.
- **UNK** Undo after a card drag. The `Edit` menu showed `Undo` but the
  behaviour was not exercised.
- **UNK** Whether the drop indicator differs for "before" vs "after" — only
  the downward variant was captured in isolation.
