# Weave JSON Story Format

`weavec --format json` serializes the same `StoryIr` model as the default RON output. JSON is intended for browser, service, and non-Rust hosts; it does not define a second compiler IR.

## Compile and validate

```bash
weavec story.weave --format json
weavec story.weave --format json --output story.json
weavec --schema --output weave-story-ir.schema.json
```

The canonical schema for IR version 2 is available as a site [download](downloads/weave-story-ir-v2.schema.json). `weavec --schema` generates that document from the Rust IR types, and the compiler test suite verifies that the checked-in file is byte-for-byte current.

Every document contains a numeric top-level `version`. A consumer must check it before execution. Weave's runtime accepts exactly `weave_core::ir::IR_VERSION`; it rejects older or newer versions instead of guessing at compatibility.

## Compatibility contract

- RON and JSON encode one model and must decode to equal `StoryIr` values.
- A change that adds, removes, renames, or changes the meaning of a serialized field or enum variant requires a new IR version and a new schema file.
- Map key order and JSON object member order carry no meaning. The compiler uses sorted maps so emitted files and reviews remain deterministic.
- `source_name` and instruction `span` values are diagnostic metadata. Runtime identity never depends on an input path.
- Choice identifiers derive from the semantic knot/instruction path. Comments, blank lines, source filenames, and output format do not change them.
- Pattern definitions, draw methods, spreads, and semantic fields are part of the shared IR. Loading equal RON and JSON with the same random seed produces equal draws, events, and saved state.

Unknown fields are not a substitute for version negotiation. Hosts should reject an unsupported top-level version even if their JSON decoder would otherwise ignore an unfamiliar member.

## Safe output behavior

Files are serialized completely before writing. A file destination is replaced through a temporary file in the same directory. Missing or invalid parent directories fail without creating a partial output file; standard output is available by passing `--output -`.
