# Community pattern packages

Community packages let authors distribute reusable semantic pattern data without shipping trusted code. Package format `1` is strict JSON: it contains metadata, scalar semantic fields, uniform or weighted draws, reversal meanings, and spreads. It cannot declare scripts, native libraries, network access, build steps, or new draw algorithms.

The reviewed [Ember Omens sample](https://github.com/chrisgliddon/weave/tree/main/patterns/community/ember-omens) exercises the complete workflow. Its format is defined by the downloadable [JSON Schema](downloads/community-pattern-package-v1.schema.json).

## Validate, publish, and install

Run these commands from the repository root:

```bash
cargo run -p weave-patterns --bin weave-pattern -- \
  validate patterns/community/ember-omens/package.weave-pattern.json

cargo run -p weave-patterns --bin weave-pattern -- \
  publish patterns/community/ember-omens/package.weave-pattern.json \
  --output target/pattern-publication

cargo run -p weave-patterns --bin weave-pattern -- \
  install target/pattern-publication/ember_omens-1.0.0.weave-pattern.json \
  --registry target/pattern-registry
```

`validate` never writes. `publish` canonicalizes the reviewed package into a versioned artifact and writes an adjacent SHA-256 sidecar; it stages files locally and does not upload them. `install` verifies any adjacent sidecar, validates the package again, and writes the canonical data to `REGISTRY/id/version/package.weave-pattern.json`. Publication and installation are atomic and idempotent, and neither command overwrites different bytes at an existing version.

List the local registry or generate a portable discovery index:

```bash
cargo run -p weave-patterns --bin weave-pattern -- \
  list --registry target/pattern-registry

cargo run -p weave-patterns --bin weave-pattern -- \
  index --registry target/pattern-registry \
  --output target/pattern-index.json
```

An index contains only public discovery fields, relative artifact paths, and checksums. A hosted community catalog can publish that index beside the immutable artifacts and sidecars; Weave deliberately does not contact a default registry or auto-install remote content.

## Reference a package from a story

Select every external package explicitly when compiling:

```bash
cargo run -p weave-compiler -- \
  patterns/community/ember-omens/example.weave \
  --pattern-registry target/pattern-registry \
  --pattern 'ember_omens@^1.0' \
  --format json \
  --output target/community-pattern-story.json
```

The selector resolves the greatest installed semantic version satisfying the requirement. The package ID becomes the story-visible pattern name:

```weave
VAR reading = ember_omens.spread.three_signs.draw()

=== start ===
The first sign is {reading.kindling.name}: {reading.kindling.meaning}.
-> END
```

`weavec` makes the installed package's spread names available to static analysis, rejects a collision with an inline pattern, validates the resulting runtime definition, and embeds the data in ordinary story IR. The runtime needs neither the registry nor package code. Pin an exact selector such as `ember_omens@=1.0.0` for release builds; a range is useful while authoring.

## Format and compatibility

Each package has four independent compatibility controls:

- `schema_version` must be `1`.
- `pattern_model_version` must equal `weave_patterns::PATTERN_MODEL_VERSION`, currently `1`.
- `metadata.version` is the package's semantic version.
- `metadata.weave_version` is a semantic-version requirement that must include the running Weave release.

Unknown or duplicate JSON fields, malformed versions, unsupported versions, duplicate or unsafe identifiers, non-finite numbers, incompatible draw settings, missing meanings, invalid weights, over-capacity spreads, and checksum mismatches fail before installation or compilation. Package files are limited to 2 MiB, 4,096 elements, 64 fields per element, 64 spreads, and 128 positions per spread.

Every element must have a unique canonical `id`, plus `name` and `meaning` fields. Values may be strings, finite numbers, booleans, null, or symbols encoded as `{ "symbol": "value" }`. Format `1` permits `uniform` and `weighted` data-driven methods. Reversal overlays are explicit and currently limited to `reversed_fields.meaning`; a package cannot smuggle a specialized algorithm through metadata.

## Licensing and provenance

The metadata contract requires:

- at least one named author;
- an SPDX-expression-shaped `license` and public HTTPS `license_url`;
- durable attribution text;
- a public HTTPS source URL and immutable revision;
- an optional upstream SHA-256 and a `modified` declaration;
- discovery tags and explicit content warnings.

Syntax validation cannot establish that a contributor owns the data or selected the correct SPDX identifier. Catalog maintainers must verify those facts against the cited source before approval. Attribution and license metadata remain part of the installed package; compiled story data does not replace the obligation to distribute the applicable notices.

Package metadata and semantic strings must not contain credentials, private keys, access tokens, private repository URLs, or personal data that is not intentionally public. Replace any sensitive value in a report or review transcript with exactly `[REDACTED]`, rotate the credential, and follow the private process in [`SECURITY.md`](https://github.com/chrisgliddon/weave/blob/main/SECURITY.md) if exposure reached repository history or a published artifact.

## Moderation workflow

A catalog submission should include the package JSON, an original example story, its generated schema-valid artifact and checksum, and a review note covering provenance, license, modifications, content warnings, and test results. Maintainers should then:

1. run `validate`, publish into a clean staging directory, install into a clean registry, and compile the example with an exact version;
2. compare the cited source and optional hash, confirm authorship and license scope, and retain required notices;
3. review names, descriptions, semantic fields, and warnings for misleading, hateful, exploitative, or privacy-invasive content;
4. confirm the package contains only data and that URLs contain no embedded credentials;
5. review version changes as immutable releases—never replace approved bytes under the same version;
6. publish catalog status through review history and the curated index, not through an author-controlled "approved" field.

Reports about harmful content, provenance disputes, or licensing mistakes may use a public issue when no sensitive details are involved. Security flaws, leaked credentials, and bypasses of validation or integrity checks belong in the private vulnerability channel described by [`SECURITY.md`](https://github.com/chrisgliddon/weave/blob/main/SECURITY.md). A withdrawn package should disappear from new discovery indexes while its version and checksum remain documented for reproducible builds and migration guidance.
