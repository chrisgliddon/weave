# Weave JSON Story Format

`weavec --format json` serializes the same `StoryIr` model as the default RON output. JSON is intended for browser, service, and non-Rust hosts; it does not define a second compiler IR.

## Compile and validate

```bash
weavec story.weave --format json
weavec story.weave --format json --output story.json
weavec --schema --output weave-story-ir.schema.json
```

The canonical schema for IR version 7 is available as a site [download](downloads/weave-story-ir-v7.schema.json). `weavec --schema` generates that document from the Rust IR types, and the compiler test suite verifies that the checked-in file is byte-for-byte current. The version-2 through version-6 schemas remain published only for consumers of historical artifacts.

Every document contains a numeric top-level `version`. A consumer must check it before execution. Weave's runtime accepts exactly `weave_core::ir::IR_VERSION`; it rejects older or newer versions instead of guessing at compatibility.

## Compatibility contract

- RON and JSON encode one model and must decode to equal `StoryIr` values.
- A change that adds, removes, renames, or changes the meaning of a serialized field or enum variant requires a new IR version and a new schema file.
- Map key order and JSON object member order carry no meaning. The compiler uses sorted maps so emitted files and reviews remain deterministic.
- `source_name` and instruction `span` values are diagnostic metadata. Runtime identity never depends on an input path.
- Choice identifiers derive from the semantic knot/instruction path. Comments, blank lines, source filenames, and output format do not change them.
- Pattern definitions, draw methods, spreads, and semantic fields are part of the shared IR. Loading equal RON and JSON with the same random seed produces equal draws, events, and saved state.
- Explicitly selected [community packages](community_patterns.md) are validated and embedded as ordinary pattern definitions; a runtime never needs registry access to load the compiled story.
- Explicitly activated [domain modules](domain_modules.md) are embedded under `modules`, keyed by story-local alias. Each entry records exact module and pack identities, tagged effective export values, and canonically ordered authored override metadata, so RON, Rust, JavaScript, Bevy, and browser consumers see the same finite data and provenance without a registry or editor dependency.

Unknown fields are not a substitute for version negotiation. Hosts should reject an unsupported top-level version even if their JSON decoder would otherwise ignore an unfamiliar member.

## Writer metadata in IR 7

IR 7 adds:

- `channels`: every declared channel keyed by name, always including `narration`. Each has `animated`, an optional `cps`, and optional `meta` literals.
- `strings`: `STRINGS` tables keyed by table then entry key, each entry pre-rendered as `text`, `rich`, `spans`, optional `beats`, `tags`, and `meta`.
- A narrative `text` instruction carries a line object: `id`, `template`, `tags`, and `meta`. Templates may contain `pause`, `style_open`, and `style_close` parts.
- A `channel` instruction for each `@to`.
- Optional `tags` and `meta` on choices.

## Line frames

Hosts built on the web player or the C ABI receive each line as one flat JSON frame. A real frame from the writer-metadata example:

```json
{"beats":[{"at":19,"pause":0.8}],"channel":"dialog","draws":[],"id":"shore.wick","kind":"line","meta":{},"presentation":[{"kind":"speaker","value":"keeper"}],"rich":"The <em>wick</> is spent. I was waiting for someone to fetch oil.","spans":[{"from":4,"style":"em","to":8}],"speaker":"keeper","tags":[],"text":"The wick is spent. I was waiting for someone to fetch oil."}
```

- `id`, `channel`, and `speaker` (absent while narrating) identify the line.
- `text`, `rich`, `spans`, and `beats` follow the rendering rules in the language guide; offsets count Unicode scalar values in `text`.
- `tags` and `meta` come from the line's tags.
- `draws` and `presentation` are unchanged from earlier versions.

Choice frames list choices with `id`, `text`, `once`, and, when present, `tags` and `meta`.

## Safe output behavior

Files are serialized completely before writing. A file destination is replaced through a temporary file in the same directory. Missing or invalid parent directories fail without creating a partial output file; standard output is available by passing `--output -`.
