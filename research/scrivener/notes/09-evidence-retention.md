# Evidence Retention

What is kept in the repository, what was discarded, and how to regenerate it.

## Kept (committed)

| Path | Size | What it is |
| --- | --- | --- |
| `evidence/ib_compact.json` | 144 KB | Full structural summary of all **67** main-bundle NIBs: per document, node kinds, edge relations, every outlet, every target-action, every class, every control. This is the machine-readable backbone of the notes. |
| `evidence/ib_nibs/MainDocument.json` | 1.6 MB | Full REA Interface Builder decode of the main window — the document the window-layout and Binder findings are read from. |
| `evidence/ib_nibs/DocumentView.json` | 1.8 MB | Full decode of the editor/corkboard/outliner document. |
| `evidence/ib_nibs/InspectorView.json` | 850 KB | Full decode of the Inspector. |
| `evidence/ib_nibs/Layouts.nib` → `Layouts.json` | 211 KB | Full decode of the window-layout chooser. |
| `evidence/ib_nibs/CustomMetaDataInspectorView.json` | 143 KB | Full decode of the custom-metadata pane. |
| `evidence/runtime/*.png` | ~16 MB | 33 window screenshots, each the direct visual record of a note. |
| `evidence/runtime/*.txt` | small | The matching accessibility tree for each screenshot, as indented text. |
| `evidence/obs_*.json` | ~1 MB | 11 slimmed REA observations: window identity, node list, limits and gaps. Base64 screenshots stripped. |

The second pass added captures `30`–`54`, covering the Outliner column set,
the card body rule, and the drag affordances. Each capture is named for what
it shows, and every one is cited by a note.

## Discarded (regenerable)

| What | Why | How to regenerate |
| --- | --- | --- |
| The other 62 raw NIB decodes (~30 MB) | `ib_compact.json` already carries their outlets, actions, classes and counts; the full graph is only needed for a document under active study | `tools/ib_compact.py` plus the per-NIB decode loop in `README.md` |
| The whole-bundle decode (24 MB) | Truncated by REA's 64-document / 20 000-object caps; superseded by the per-NIB approach | `rea decode-interface-builder /Applications/Scrivener.app` |
| Base64 screenshots inside the observations | Duplicated the PNGs on disk at ~1 KB per KB of noise | `tools/save_observation.py` writes the PNG separately |
| The staged single-NIB bundles | Temporary artefacts under a system temp dir, not part of the research record | `README.md` §Reproduction |

## Why this split

The notes make claims that a reviewer must be able to check. A claim about a
Binder outlet should be checkable against `ib_compact.json` without
downloading 30 MB. A claim about a drop indicator should be checkable against
a screenshot in the same directory. A claim about the persisted document model
should be checkable by running `tools/scrivx_analyze.py` against the user's
own project.

The five retained NIB decodes are the ones cited by name in the notes; the
other 62 are represented in `ib_compact.json` at the level of detail the notes
actually use.

The second pass kept its evidence small on purpose. The `.evidence.json`
files written by `save_observation.py` are the *raw* REA payloads and are
not committed; what is committed is the PNG, the AX text tree, and — for
the captures that carry a claim — a short `.txt` extract written by the
targeted probe tools.

## Regenerating everything

```bash
cd research/scrivener

# 1. Interface Builder structure for all 67 NIBs
BASE=$(mktemp -d)
for f in /Applications/Scrivener.app/Contents/Resources/Base.lproj/*.nib; do
  n=$(basename "$f" .nib)
  d="$BASE/$n.app"
  mkdir -p "$d/Contents/Resources/Base.lproj" "$d/Contents/MacOS"
  cp "$f" "$d/Contents/Resources/Base.lproj/"
  cp /usr/bin/true "$d/Contents/MacOS/$n"
  cat > "$d/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict><key>CFBundleIdentifier</key><string>research.scrivener.$n</string><key>CFBundleName</key><string>$n</string><key>CFBundleExecutable</key><string>$n</string><key>CFBundlePackageType</key><string>APPL</string></dict>
</plist>
EOF
  rea decode-interface-builder "$d" --format json > "evidence/ib_nibs/$n.json"
done
python3 tools/ib_compact.py evidence/ib_nibs --out evidence/ib_compact.json

# 2. Runtime observation
rea observe-native-ui /Applications/Scrivener.app/Contents/MacOS/Scrivener \
  --pid <PID> --window-id <CGWindowID> --screenshot --accessibility \
  --max-nodes 4000 --format json > evidence/obs_<name>.json
python3 tools/save_observation.py evidence/obs_<name>.json \
  --out-dir evidence/runtime --name <name>
python3 tools/slim_observations.py evidence
```

## Regenerating the second-pass captures

The second pass ran against a **copy** of the project, opened as its own
window, so the user's own project was never mutated. Reproduce it like this:

```bash
# 1. Copy the project and open the copy as a second window
SCRATCH=$(mktemp -d)
cp -R "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Scrivener/Weave UX.scriv" \
  "$SCRATCH/Scratch Research.scriv"
rm -f "$SCRATCH/Scratch Research.scriv/Files/user.lock"
osascript -e "tell application \"Scrivener\" to open POSIX file \"$SCRATCH/Scratch Research.scriv\""

# 2. Find the new window and bind REA to it
python3 tools/ax_window.py <PID>

# 3. Read a nested menu (System Events cannot do this)
python3 tools/ax_probe.py <PID> menus View "Outliner Options"

# 4. Read the accessibility tree of a window
python3 tools/ax_probe.py <PID> tree "Scratch Research" AXPopUpButton

# 5. Verify a move from the persisted file
python3 tools/scrivx_tree.py "$SCRATCH/Scratch Research.scriv/Scratch Research.scrivx" --meta
```

Mouse coordinates are **window-local** in a screenshot but **global** in a
posted event, so add the window's screen origin from `ax_window.py` before
clicking.

## Provenance

Every artefact here was produced from the locally installed
`/Applications/Scrivener.app` (Scrivener 3.5.2, build 17486) and from a
locally held project file. No Scrivener source, documentation, or third-party
assets were copied. The screenshots show a project named `Weave UX.scriv`
whose contents are the researcher's own; they contain no confidential text.
