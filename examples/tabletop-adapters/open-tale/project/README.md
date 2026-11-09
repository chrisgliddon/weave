# Open Tale canonical project

This is the canonical synthetic project for `org.weave.tabletop.open_tale@1.0.0`, schema 1.
All names, prose, and scenario inputs are original and synthetic. The project source is MIT
licensed, matching the repository; no upstream rules text, artwork, layout, or asset is copied.

`project.json` selects the exact adapter coordinate, records seed `20261031`, and lists the
scripted path. `expected-output.json` records the expected creation fingerprints and the receipt,
state, entropy, and event data for every scripted operation.

Run the complete project from a local checkout. It compiles the test harness, performs the
deterministic creation, plays the scripted path, replays every receipt, and compares the result
with the checked-in expected output:

```bash
cargo test -p weave-tabletop --test compatibility -- --exact canonical_project_open_tale
```

The run performs no network access. Regenerate deliberately after a reviewed adapter change:

```bash
WEAVE_WRITE_COMPATIBILITY_PROJECTS=1 cargo test -p weave-tabletop --test compatibility
```
