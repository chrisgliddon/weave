# Pluggable domain modules

Domain modules add typed world, character, ruleset, and other authoring data without adding domain-specific branches throughout Weave. Contract version `1` is host-independent and declarative: the same manifest and pack models are consumed by source tooling, the compiler, runtime hosts, the editor, Bevy, and browser applications.

The public Rust authority is `weave-domain`. Download the generated [module-manifest schema](downloads/domain-module-manifest-v1.schema.json), [domain-pack schema](downloads/domain-pack-v1.schema.json), [project schema](downloads/domain-project-v1.schema.json), [lock schema](downloads/domain-lock-v1.schema.json), and [registry-index schema](downloads/domain-registry-index-v1.schema.json). The [third-party tutorial](domain_module_tutorial.md) builds and packages a module from scratch.

## Trust and ownership boundary

Contract v1 deliberately has no executable third-party extension surface. A module cannot declare scripts, native libraries, WebAssembly, build steps, callbacks, network requests, environment access, or dynamic host hooks. It can declare only:

- closed types and named exports;
- bounded homogeneous maps and schema-declared stable entity hierarchies;
- immutable pack data and initial values for isolated state;
- versioned host capabilities from Weave's allowlist;
- exact dependency requirements;
- editor labels and schema metadata carried by those declarations; and
- machine-readable license, source, transformation, and field provenance.

The `operations` capability is a versioned declarative data contract interpreted by trusted Weave code; it is not permission to execute package code. Adding any executable extension boundary would require a new contract version, a separate trust policy, explicit host opt-in, and a security review.

`weave-domain` owns portable models, schema generation, validation, compatibility negotiation, provenance, and deterministic dependency ordering. `weave-core` owns source syntax, static types, diagnostics, and compiled IR references. `weave-compiler` resolves selected installed artifacts and lowers validated values. `weave-runtime` reads only compiled values and stores mutable module state separately from immutable pack data. The editor consumes the same public schemas and diagnostics; Bevy and PixiJS consumers read exported RON or JSON without editor dependencies.

## Artifact layout and discovery

A repository or installed registry uses this canonical layout:

```text
modules/
└── org.weave.synthetic_constellation/
    └── 1.0.0/
        ├── module.weave-module.json
        ├── module.weave-module.json.sha256
        └── packs/
            └── glasswing_sky/
                └── 1.0.0/
                    ├── pack.weave-domain.json
                    └── pack.weave-domain.json.sha256
```

Authored project references may use canonical JSON or RON. Publication and installation normalize them to canonical JSON with SHA-256 sidecars. Installed artifacts are immutable, and a project lock records exact module and pack versions, artifact hashes, and dependency edges.

Discovery is never ambient. A compiler or editor searches only the adjacent `weave.modules.json`, an explicit `--module-project`, or explicit `--module-registry` roots—never the current user's entire machine, network, environment, or unrelated package-manager state. Project paths are sorted, project-relative, and confined beneath the project root. Project requirements select semantic-version ranges; the resolver chooses the highest compatible installed release, sorts candidates by module identity and semantic version, verifies its checksum, and writes the exact selection to `weave.lock`. `--locked` compilation rejects any selection or hash change.

## Source installation and activation

Installation makes an artifact available to the project; activation makes one selected module visible to a story. Contract v1 uses this text-first activation form:

```weave
module constellation {
    id: "org.weave.synthetic_constellation"
    version: "=1.0.0"
    pack: "glasswing_sky@=1.0.0"
}
```

`constellation` is the story-local alias. The declared module identity remains globally stable, while an author may choose a clearer alias. Every editor activation action must produce this source declaration, and every valid declaration must be editable through the visual module inspector. Removing the declaration removes its values on the next successful compile; it never silently rewrites ordinary story variables.

Expressions use the alias as the first path segment:

```weave
{ constellation.phase == twilight }
The observed sign is {constellation.observation.label}.
{ end }
```

An alias occupies a distinct top-level namespace but still cannot collide with a story variable, grammar, pattern, knot, or another active alias. Transitive dependencies are addressed internally by full module identity and do not become source-visible unless the story activates them explicitly.

For a project, put a closed discovery file beside the story:

```json
{
  "schema_version": 1,
  "registries": [],
  "manifests": ["module.weave-module.json"],
  "packs": ["pack.weave-domain.json"]
}
```

`weavec` discovers that adjacent file automatically. `--module-project FILE` selects another project file, `--module-registry DIRECTORY` selects installed registries directly, and the original explicit artifact flags remain available for one-off builds:

```bash
cargo run -p weave-compiler -- \
  examples/domain-modules/contract/tracer.weave \
  --module-manifest examples/domain-modules/contract/module.weave-module.json \
  --module-pack examples/domain-modules/contract/pack.weave-domain.json \
  --format json \
  --output target/tracer.story.json
```

The compiler resolves the complete dependency closure against that bounded catalog, validates every selected pack, type-checks module paths, and embeds only source-activated immutable values and state seeds in Story IR. An activation may add `override export.path: constant` lines. Those replacements are validated against the same manifest, canonicalized by path, and retained in `authored_overrides` rather than mutating or misattributing the selected pack. A normal project build writes `weave.lock`; a `--locked` build requires an exact match. It does not contact a network registry or execute package code.

For structured authoring, a manifest may declare an `authoring.entity_collections` entry over a bounded map export. It names the stable id, editable label, integer order, parent, symmetric relation, and explicit inheritance-source fields. Generic editor actions can then create, rename, reorder, nest, relate, or reset entities without package code. Validation requires each id to equal its map key, every reference to resolve, relationships to be symmetric, and parent/inheritance chains to be acyclic.

## End-to-end synthetic tracer

[`tracer.weave`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/contract/tracer.weave) is the minimal contract proof. It activates the fictional `Synthetic Constellation` module, initializes an ordinary story variable from its `phase` export, branches on the semantic symbol, and interpolates nested observation data. Repeated compilation produces byte-identical [RON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/contract/tracer.story.ron) and [JSON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/contract/tracer.story.json).

The compiled artifacts are consumed without editor dependencies by:

- a finite [Bevy 0.18 example](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-bevy), which loads the RON values into an ECS resource; and
- a [PixiJS v8 example](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-pixijs), which validates IR version `4`, decodes the tagged JSON values, and renders them.

The editor's `DomainSession` and `TextBuffer` accept the same explicit `DomainCatalog`. Their module inspection model exposes the validated descriptions, schemas, types, sources, and values used by compilation, so text formatting and editor-backed compilation produce the same module IR.

## Identity, namespaces, and types

Module identities contain at least two dot-separated lowercase ASCII segments, for example `org.weave.synthetic_constellation`. Pack identifiers, default namespaces, capabilities, exports, and object fields use lowercase ASCII identifiers. Named reusable types use UpperCamelCase names.

The contract supports null, boolean, bounded finite number, bounded string, closed symbol, bounded homogeneous list, a bounded homogeneous stable-identifier map, closed object, and manifest-local named types. Map keys use the same lowercase stable-identifier rules as exports and fields. Recursive named types are rejected in version 1 so validation, editor inspection, serialization, and host consumption always operate on finite bounded trees. JSON and RON use an explicit tagged `DomainValue`; strings and semantic symbols cannot be confused.

Exports declare whether their compiled value is immutable `pack` data or versioned mutable `state`. A selected pack supplies initial values. Runtime state is keyed by the stable module identity and module version, not by an editor label, and is serialized separately so removing or upgrading a module cannot reinterpret unrelated story state.

## Deterministic resolution

Resolution follows these rules:

1. Parse closed schemas, rejecting unknown fields, duplicate JSON keys, invalid identifiers, non-finite numbers, unbounded collections, credential-like values, and malformed provenance.
2. Negotiate the exact contract, pack, capability, Weave, module, and dependency versions before reading values.
3. Reject multiple active releases of one module and reject namespace or alias collisions; never pick a winner based on discovery order.
4. Resolve the complete dependency graph. Missing, incompatible, or cyclic dependencies fail the build.
5. Topologically order dependencies before dependents. When several nodes are ready, sort by full module identity.
6. Validate every pack value against its closed export type and every exported pack value against a provenance claim.
7. Apply source-authored replacements in sorted path order, validate the complete effective values and declared entity constraints, and preserve the replacements separately from pack provenance.
8. Record exact versions and hashes in the project lock. Repeated locked builds must produce byte-equivalent module IR.

The Rust `resolve_module_order` contract test passes the same manifests in opposite discovery orders and requires the same result.

## Compatibility and migration

Compatibility is explicit at six independent boundaries:

| Boundary | Version form | Version-1 behavior |
|---|---|---|
| Manifest schema | `contract_version` integer | Must equal `1`; unknown versions fail closed |
| Pack schema | `pack_format_version` integer | Must equal `1`; unknown versions fail closed |
| Module release | semantic version | Project range resolves to one exact locked version |
| Weave release | semantic-version requirement | Must include the compiling Weave release |
| Host capability | identifier plus integer version | Identifier and version must be in the host allowlist |
| Dependency | module or pack identity plus semantic-version requirement | Every exact locked dependency must satisfy the range |

Patch and minor module releases may be selected only when the author's requirement allows them and schema validation still succeeds. Major releases require an explicit requirement change. Contract and pack integers do not imply semantic compatibility across values: they select the parser schema only.

The compiler never runs an embedded migration or silently edits source, packs, or locks. A future `weave-module migrate` workflow may write a new reviewable artifact, cite the source version and transformation, and update the lock atomically. Unsupported older or newer versions remain readable as ordinary files but cannot enter compilation or runtime state.

## Provenance and licensing

Every manifest and pack declares at least one public or original source. Public and derived sources require an HTTPS URL, immutable revision, SHA-256, SPDX expression, license URL, attribution, and modification flag. Original repository fixtures may use an authored revision without an upstream hash. Transformations name sorted inputs and independently describe what changed. Provenance claims map manifest or pack paths to those source or transformation identifiers.

The synthetic fixture is original MIT-licensed material created for this contract. It does not model a real place, person, game, or setting and uses no external dataset. Domain-specific modules must carry equivalent machine-readable provenance for every bundled corpus or derived value. The [Weave World reference-seed guide](world_module.md) shows that boundary with hashed public sources, field-level transformation claims, editor inspection, and portable consumers.

## Failure behavior and diagnostics

Loading fails closed before source type checking when an artifact is missing, malformed, incompatible, untrusted, or ambiguous. Diagnostics identify the activation and stable field path but never echo rejected values. Integration diagnostics use these stable families:

- `D100`–`D109`: contract, pack, and capability versions;
- `D110`–`D119`: identity, alias, and namespace collisions;
- `D120`–`D129`: dependency, lock, and integrity failures;
- `D130`–`D139`: license and provenance failures;
- `D140`–`D149`: type, value, and export failures; and
- `D150`–`D159`: installation, discovery, and file-access failures.

Within `D140`–`D149`, `D144` rejects non-constant replacements and `D145` rejects duplicate replacement paths. The editor keeps the last valid compiled module view when a watched artifact becomes invalid and reports the same source-linked diagnostic as the CLI. Structured entity edits are atomic: a dangling reference, asymmetric relation, or cycle leaves the prior source and preview intact. Runtime hosts never attempt to repair or reinterpret invalid compiled module data.

## Canonical contract commands

Generate the checked schemas:

```bash
cargo run -p weave-domain --bin weave-module -- schema manifest --output manifest.schema.json
cargo run -p weave-domain --bin weave-module -- schema pack --output pack.schema.json
cargo run -p weave-domain --bin weave-module -- schema project --output project.schema.json
cargo run -p weave-domain --bin weave-module -- schema lock --output lock.schema.json
cargo run -p weave-domain --bin weave-module -- schema registry --output registry.schema.json
```

Validate the canonical fixture and compare encodings:

```bash
cargo run -p weave-domain --bin weave-module -- validate \
  examples/domain-modules/contract/module.weave-module.json \
  --pack examples/domain-modules/contract/pack.weave-domain.json

cargo run -p weave-domain --bin weave-module -- normalize manifest \
  examples/domain-modules/contract/module.weave-module.json \
  --format ron --output module.weave-module.ron

cargo run -p weave-domain --bin weave-module -- publish \
  examples/domain-modules/contract/module.weave-module.json \
  --pack examples/domain-modules/contract/pack.weave-domain.json \
  --output target/domain-publication
```

The documentation build regenerates all five schemas, normalizes JSON to RON and RON to JSON, exercises publication, installation, project discovery, locking, and the third-party tutorial, recompiles the tracer and authored World hierarchy in both formats, validates the separately licensed optional naming pack, runs the finite Bevy consumer, and tests and bundles the PixiJS consumer.
