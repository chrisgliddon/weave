# Build a third-party domain module

This tutorial builds `Lantern Weather`, a small data-only module, with the same public files available to any third-party author. The complete reviewed fixture is in [`examples/domain-modules/third-party-tutorial`](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/third-party-tutorial).

Domain modules cannot execute code. A manifest declares closed types and exports; a pack supplies validated values. Weave resolves those artifacts before type checking and embeds the selected values in Story IR.

## 1. Declare the module

Create `module.weave-module.json`. Set both format integers to `1`, choose a globally stable dot-separated ID, declare the compatible Weave range, and list only supported capabilities. This tutorial defines a closed weather symbol and a bounded temperature:

```json
{
  "contract_version": 1,
  "pack_format_version": 1,
  "id": "dev.weave.lantern_weather",
  "version": "1.0.0",
  "namespace": "weather",
  "title": "Lantern Weather",
  "summary": "A tiny original weather module built with Weave's public declarative contract.",
  "authors": [{ "name": "Your public author name", "url": "https://example.com" }],
  "license": "MIT",
  "license_url": "https://opensource.org/license/mit",
  "weave_version": ">=0.1.0, <0.2.0",
  "capabilities": [
    { "id": "data", "version": 1 },
    { "id": "editor_schema", "version": 1 }
  ],
  "dependencies": [],
  "types": {
    "Condition": { "kind": "symbol", "values": ["clear", "rain", "snow"] }
  },
  "exports": {
    "condition": {
      "type": { "kind": "named", "name": "Condition" },
      "required": true,
      "source": "pack",
      "description": "The authored weather condition."
    },
    "temperature_c": {
      "type": {
        "kind": "number",
        "integer": false,
        "minimum": -40.0,
        "maximum": 60.0
      },
      "required": true,
      "source": "pack",
      "description": "The authored temperature in degrees Celsius."
    }
  },
  "provenance": {
    "sources": [{
      "id": "module_original",
      "kind": "original",
      "url": "https://example.com/lantern-weather",
      "revision": "v1.0.0",
      "license": "MIT",
      "license_url": "https://opensource.org/license/mit",
      "attribution": "Original module authored for this release.",
      "modified": false
    }],
    "transformations": [],
    "claims": {
      "exports.condition": ["module_original"],
      "exports.temperature_c": ["module_original"],
      "types.Condition": ["module_original"]
    }
  }
}
```

The fixture uses the repository's real public URL and license. Replace the example author, URLs, revision, and attribution with truthful public metadata for your module. Public or derived source material also requires its immutable SHA-256 and a documented transformation chain.

## 2. Author a data pack

Create `pack.weave-domain.json`. The module requirement is a range, while the pack itself has an exact semantic version:

```json
{
  "pack_format_version": 1,
  "id": "harbor_evening",
  "version": "1.0.0",
  "title": "Harbor Evening",
  "module": { "id": "dev.weave.lantern_weather", "version": "^1.0" },
  "dependencies": [],
  "values": {
    "condition": { "kind": "symbol", "value": "rain" },
    "temperature_c": { "kind": "number", "value": 12.5 }
  },
  "provenance": {
    "sources": [{
      "id": "pack_original",
      "kind": "original",
      "url": "https://example.com/lantern-weather",
      "revision": "v1.0.0",
      "license": "MIT",
      "license_url": "https://opensource.org/license/mit",
      "attribution": "Original fictional weather values.",
      "modified": false
    }],
    "transformations": [],
    "claims": {
      "values.condition": ["pack_original"],
      "values.temperature_c": ["pack_original"]
    }
  }
}
```

Validate the release before publishing:

```bash
cargo run -p weave-domain --bin weave-module -- validate \
  module.weave-module.json \
  --pack pack.weave-domain.json
```

Validation rejects unknown fields, incompatible versions, undeclared or mistyped values, missing provenance, invalid SPDX expressions, dependency failures, and credential-shaped data without echoing the rejected value.

## 3. Publish or install

`publish` stages the immutable canonical registry layout; `install` applies the same checks to a local registry. Both canonicalize artifacts to JSON, create SHA-256 sidecars, and refuse to replace different bytes at an existing coordinate.

```bash
cargo run -p weave-domain --bin weave-module -- publish \
  module.weave-module.json \
  --pack pack.weave-domain.json \
  --output target/lantern-publication

cargo run -p weave-domain --bin weave-module -- install \
  module.weave-module.json \
  --pack pack.weave-domain.json \
  --registry modules-registry

cargo run -p weave-domain --bin weave-module -- list \
  --registry modules-registry
```

Generate a portable discovery index with `weave-module index --registry modules-registry --output registry-index.json`.

## 4. Configure one project

Place `weave.modules.json` beside the story. Paths are UTF-8, sorted, project-relative, and confined to the project directory. Reference authored files directly while developing:

```json
{
  "schema_version": 1,
  "registries": [],
  "manifests": ["module.weave-module.json"],
  "packs": ["pack.weave-domain.json"]
}
```

For an installed release, use its project-relative registry instead:

```json
{
  "schema_version": 1,
  "registries": ["modules-registry"],
  "manifests": [],
  "packs": []
}
```

Validate all discovered artifacts with `weave-module validate-project weave.modules.json`. No command searches user directories, contacts a network registry, or reads environment-based package state.

## 5. Activate and compile

Activate the module under a story-local alias:

```weave
module weather {
    id: "dev.weave.lantern_weather"
    version: "^1.0"
    pack: "harbor_evening@^1.0"
}

=== start ===
{weather.condition == rain:
    Rain beads on the lantern glass. It is {weather.temperature_c} degrees outside.
- else:
    The lantern shines through a quiet evening.
}
-> END
```

Compile normally once, review the generated `weave.lock`, then require it in reproducible builds:

```bash
cargo run -p weave-compiler -- story.weave --format json --output story.json
cargo run -p weave-compiler -- story.weave --locked --format json --output story.json
```

The lock records exact module and pack versions, SHA-256 digests, and dependency edges. An artifact change, newly selected compatible release, missing lock, or altered lock fails `--locked` without replacing the prior output.

The Weave Editor uses the same adjacent project file. Valid watched artifact changes refresh inline types and live preview. Invalid changes report an error while preserving the last valid catalog and compiled preview.

## 6. Release compatible upgrades

Publish a new immutable coordinate for every change. A source requirement of `^1.0` may select `1.1.0`; `weave.lock` makes that selection reviewable. A major release such as `2.0.0` is selected only after the source requirement changes. Unsupported contract integers, pack integers, host capabilities, or Weave ranges fail closed—Weave never runs a hidden migration.

Dependencies use stable identities and semantic-version ranges in manifests and packs. The resolver selects one exact release per identity, validates the complete closure, rejects missing/conflicting/cyclic dependencies, and records dependencies before dependents in the lock.
