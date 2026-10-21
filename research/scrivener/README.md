# Scrivener UX Research

Reverse-engineering notes for building Weave's chapter authoring and card
organization features from the UX of Scrivener 3.

**Target:** Scrivener 3.5.2 (build 17486), macOS universal binary
`x86_64` + `arm64`, bundle id `com.literatureandlatte.scrivener3`, team
`W4QTL7X778`, installed at `/Applications/Scrivener.app`.

**Method:** REA (`rea` 6.7.0) for static analysis — Interface Builder NIB
decoding, Mach-O plist/signature/architecture inspection — plus passive
runtime observation of the running app through the macOS accessibility tree
(`rea observe-native-ui`) and targeted UI interaction.

Every claim in these notes is tagged:

| Tag | Meaning |
| --- | --- |
| **OBS** | Observed directly — a screenshot, an accessibility-tree node, a decoded NIB, a persisted file |
| **INF** | Inference from observed evidence |
| **UNK** | Not established by this investigation |

---

## Layout

```
research/scrivener/
├── README.md            this file — index and method
├── notes/               the findings, one file per concern (9 files)
├── evidence/
│   ├── ib_compact.json  structural summary of all 67 main-bundle NIBs
│   ├── ib_nibs/         full REA decode of the 5 NIBs the notes cite
│   ├── runtime/         11 screenshots + matching accessibility trees
│   └── obs_*.json       11 slimmed REA observations
└── tools/               8 reusable scripts used to gather and shape evidence
```

---

## Findings index

| Note | Subject |
| --- | --- |
| [`notes/01-window-layout.md`](notes/01-window-layout.md) | Overall window anatomy: Binder, editor, Inspector, toolbar, layouts |
| [`notes/02-binder-tree.md`](notes/02-binder-tree.md) | The Binder outline: structure, node types, selection, data model |
| [`notes/03-chapter-cards.md`](notes/03-chapter-cards.md) | Corkboard: Chapter cards, arrangement, and the card model |
| [`notes/04-drag-and-drop.md`](notes/04-drag-and-drop.md) | Drag-and-drop: API surface, drop indicator, observed reorder |
| [`notes/05-wysiwyg-editor.md`](notes/05-wysiwyg-editor.md) | The rich-text editor: format bar, document view, outliner |
| [`notes/06-inspector-metadata.md`](notes/06-inspector-metadata.md) | Inspector metadata fields and their document-model mapping |
| [`notes/07-ux-flow.md`](notes/07-ux-flow.md) | End-to-end writing flow and the model Weave should adopt |
| [`notes/08-evidence-ledger.md`](notes/08-evidence-ledger.md) | Finding ledger: evidence IDs, coverage, residual unknowns |
| [`notes/09-evidence-retention.md`](notes/09-evidence-retention.md) | What is kept, what was discarded, and how to regenerate it |

---

## Reproduction

Static decoding of every compiled NIB in the main bundle:

```bash
for f in /Applications/Scrivener.app/Contents/Resources/Base.lproj/*.nib; do
  n=$(basename "$f" .nib)
  d="$STAGE/$n.app"
  mkdir -p "$d/Contents/Resources/Base.lproj" "$d/Contents/MacOS"
  cp "$f" "$d/Contents/Resources/Base.lproj/"
  cp /usr/bin/true "$d/Contents/MacOS/$n"      # stub executable; the decoder
                                                # only needs a valid bundle
  # Info.plist with CFBundleIdentifier / CFBundleName / CFBundleExecutable
  rea decode-interface-builder "$d" --format json > "ib_$n.json"
done
```

The REA decoder caps at 64 documents / 20 000 objects per call, which truncates
a whole-bundle run. Decoding each NIB in its own minimal bundle gives complete
coverage: all 67 main-bundle NIBs decode with `hierarchy_complete: true`.

Runtime observation of a running window:

```bash
rea observe-native-ui /Applications/Scrivener.app/Contents/MacOS/Scrivener \
  --pid <PID> --window-id <CGWindowID> --screenshot --accessibility \
  --max-nodes 4000 --format json > obs.json
```

The CGWindowID is discoverable through the window server
(`CGWindowListCopyWindowInfo`). The running Scrivener instance opened for this
research holds a project named `Weave UX.scriv` whose Binder mirrors Weave's
own domain vocabulary (`Part` → `Chapter` → `Scene`), which made it a
well-matched subject.

---

## Public-source isolation

This research documents the *observable UX behaviour* of a locally installed
third-party application. Everything recorded here is a black-box observation:
window geometry, accessibility-tree structure, menu labels, persisted file
formats, and Objective-C selector names read from the shipped binary's symbol
table. No Scrivener source code, documentation prose, or proprietary assets
were copied. Where a finding matters to Weave, the note states the behaviour
first and the Weave-side design choice separately, so the two can be reviewed
independently.
