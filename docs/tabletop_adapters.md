# Selectable tabletop adapter contract

`weave-tabletop` defines a stable, engine-independent boundary for optional tabletop rules. A
project can install many compatible adapter releases and select none or exactly one primary
adapter for a character or campaign. The selected coordinate embedded in portable output contains
the adapter id, semantic version, state-schema version, and SHA-256 of the complete validated
manifest. Discovery order is irrelevant and a changed byte produces a different coordinate.

This is the foundation contract, not a claim that every game procedure is interchangeable.
Adapters own their definitions and state. Weave Character remains system-neutral.

## Crate and extension-surface decision

The boundary is deliberately split:

- `weave-domain` owns the finite portable type/value language used by every module;
- `weave-character` owns canonical Character evidence and does not depend on tabletop rules;
- `weave-tabletop` owns adapter manifests, selection, definition/state isolation, creation steps,
  capabilities, deterministic entropy, resolver registration, typed events, visibility, switching
  previews, migrations, diagnostics, schemas, and the source-license gate;
- `weave-compiler` and source syntax continue to consume ordinary declarative domain-module
  projections, so they never branch on an adapter id; and
- the editor discovers panels from manifest capabilities, creation steps, and operation schemas.

Adapter packages are inert declarative data. They cannot contain native code, scripts, WebAssembly,
dynamic libraries, build hooks, network discovery, or editor components. Complex rule procedures
may use the public `TabletopResolver` trait only when trusted resolver code is explicitly compiled
into and registered by a host for one exact adapter id and version. Registration is never triggered
by opening a pack. The shared registry validates requests, state, entropy usage, output state,
events, visibility, and fingerprints around every call, so hosts need no adapter-specific parser,
editor, runtime, Bevy, or PixiJS branches.

This is the v1 decision: declarative distribution with an explicit trusted executable boundary.
There is no general-purpose package-executable extension surface.

## Manifest contract

An adapter manifest uses format 1 and declares:

- globally namespaced id, semantic version, positive state-schema version, source namespace,
  title, factual compatibility label, summary, Weave range, and Character-contract range;
- `declarative_data_with_registered_resolver` and resolver contract version 1;
- a sorted subset of the closed capability vocabulary;
- bounded reusable types plus closed immutable-definition, mutable-state, resolver-request, and
  event-payload types;
- ordered generic creation steps whose fields are either adapter-owned or optional canonical
  suggestions;
- named operations with one required capability, request type, allowed typed event kinds, and an
  exact declaration of whether entropy is consumed;
- the fixed v1 visibility matrix;
- optional reviewed migration declarations that preserve canonical Character data; and
- the complete public-source record described below.

Unknown fields, duplicate JSON keys, recursive or unbounded types, unordered or duplicate symbols,
unknown named types, missing event payload types, unsupported capability versions, undeclared
capabilities, and secret-shaped text fail validation.

## Capability discovery

The closed v1 vocabulary is:

| Capability | Generic surface it permits |
|---|---|
| `character_creation` | Manifest-driven creation steps and fields |
| `derived_values` | Deterministic adapter-owned derived values with visible inputs |
| `checks_and_conflicts` | Checks, contests, conflicts, and typed results |
| `resources_and_conditions` | Mutable tracks, pools, harm, status, and conditions |
| `equipment_and_abilities` | Adapter-owned inventory, moves, powers, and abilities |
| `advancement` | Explicit advancement operations and events |
| `encounters` | Encounter-scoped participants and state |
| `clocks` | Adapter-owned progress clocks |
| `scenes` | Rules-facing scene state distinct from presentation timing |
| `factions` | Adapter-owned faction records and operations |
| `world_state` | Adapter-owned world state, optionally referencing Weave World ids |
| `campaign_state` | Campaign-wide state and procedures |

Syntax tooling, editor panels, resolver requests, and host controls ask the selected manifest for a
capability. If it is absent they stop with `TT106` and identify the missing capability; they do not
guess a substitute, partially run an operation, or expose a dormant control. Optional modules may
exist only under their owning adapter namespace and never enable unrelated resolution procedures.

`tabletop_domain_manifest` and `tabletop_domain_pack` project the exact reviewed selection into the
ordinary domain-module boundary. The generated `capabilities` object contains only declared fields;
source access to an absent capability is therefore rejected statically, while immutable
`adapter`/`capabilities`/`definition` paths are read-only and only the separately typed `state`
export may change. The checked Lantern Trail `.weave` source proves this path without compiler code
that recognizes Lantern Trail or any other adapter id.

## Character authority and lifecycle

Canonical Character data and adapter data have separate artifacts, hashes, schemas, namespaces,
authority, and lifecycle:

```text
Character profile (canonical, system-neutral, immutable to adapter)
    └── profile SHA-256 reference
        └── tabletop character projection
            ├── active adapter definition (immutable)
            ├── inactive definitions (archived, never converted implicitly)
            └── explainable canonical suggestions (review lineage only)

tabletop state (mutable, adapter-owned)
    ├── exact adapter coordinate
    ├── owner + revision
    ├── deterministic entropy seed/cursor
    └── schema-validated value
```

`canonical_character_write_back` must be `false`. Adapter definitions and state cannot contain or
replace the canonical profile. A creation field marked `canonical_suggestion` must be optional. A
suggestion saves its target path, proposed value, ordered source paths, explanation, explicit
decision, and rationale where required; accepting it is still a separate Character review/apply
operation. Physical, intellectual, social, supernatural, alignment, date, or historical values are
never inferred silently.

RON authoring exports and JSON authoring exports retain the same full projection and review
lineage. A compact runtime projection may omit unapproved suggestions and hidden event payloads,
but it must retain exact adapter and payload fingerprints.

## Deterministic resolver boundary

Every request pins the resolver format, request id, exact adapter coordinate, operation,
capability, immutable definition plus its SHA-256, expected before-state SHA-256, and typed input.
The mutable state pins the same definition hash, so a changed definition makes an old request and
state visibly stale. The registry validates that complete
envelope before calling trusted code. The resolver receives only the operation, typed portable
input, adapter-owned portable state, and a deterministic entropy stream; it does not receive an
editor, filesystem, network client, clock, Bevy world, PixiJS application, or Character mutation
handle.

Entropy algorithm `sha256_counter_v1` hashes a fixed domain separator, the serialized `u64` seed,
and the serialized `u64` cursor, then consumes the first eight bytes. Each draw advances the cursor
once. Bounded draws use rejection sampling. This entropy is for reproducible game resolution, not
credential generation. State save/restore serializes the seed and cursor, and replay re-executes
the exact request from the exact before-state and requires the complete receipt to match.

The receipt atomically records the public operation/capability, exact request and before-state
hashes, before revision and entropy cursor, entropy consumed, complete next state, and a contiguous
typed event stream. Receipt validation takes the exact request artifact and independently checks
that lineage. Invalid output leaves the caller's prior state unchanged.

## Event visibility

Each event contains sequence, kind, capability, visibility, payload SHA-256, and an optional typed
payload. The v1 matrix is fixed:

| Audience | Payloads visible |
|---|---|
| Portable runtime | `public` |
| Authoring/editor | `public`, `authoring` |
| Authority host | `public`, `authoring`, `host_only` |

Projection removes unauthorized payloads but retains envelopes and payload hashes. That supports
hidden rolls and authority-host-only results without lying about event order or making a redacted
client authoritative. Hosts must not serialize a broader projection to a narrower audience.
Unrevealed result payloads must not also be copied into shared mutable state; they remain events
until the owning procedure explicitly reveals or applies a public state transition.

## Deactivation, switching, and migration

Selecting none deactivates tabletop rules without touching Character canon. Switching previews:

- the exact current and target coordinates;
- whether current definition/state must be archived;
- whether an exact inactive archive can be restored;
- incompatibility warnings; and
- an optional explicit reviewed migration matching source id/version/schema.

`automatic_conversion` is always false. Without a matching reviewed migration, definitions and
state are archived unchanged and the target starts or restores its own namespace. Even a declared
migration must say `reviewed: true` and `preserves_canonical_character: true`; its apply receipt is
separate and reviewable. A system name, similar field name, or shared die shape is never conversion
authority.

## Public-source and license gate

Every adapter records all of the following, even when independently authored:

- public HTTPS source URL;
- exact artifact plus revision, release, tag, commit, or authored revision;
- retrieval date and SHA-256 of the exact covered source bytes;
- source class, exact SPDX expression, and public license URL;
- covered files or sections;
- exclusions;
- retained attribution;
- required license-text artifact and SHA-256;
- required notices; and
- a factual compatibility/non-endorsement statement.

The allowlist is deliberately narrower than general SPDX validity:

- original Weave fixture material may use `MIT`;
- verified public inputs may use exactly `CC0-1.0`; and
- a separately bounded pack may use exactly `Apache-2.0` with explicit license text and notices.

The gate rejects unknown or ambiguous terms, license alternatives, reciprocal/copyleft or
share-alike terms, noncommercial terms, no-derivatives terms, and every other non-allowlisted
expression. It also rejects a changed source/license hash or incomplete exclusion list. Each bundle
must exclude unverified community supplements, logos, trade dress, artwork, layout, and unverified
assets. System names may appear only as courteous factual compatibility labels and never imply
affiliation, sponsorship, or endorsement.

Validation is offline. Acquiring or updating an upstream artifact is a separate reviewed process;
the validator checks only explicitly supplied bytes and never follows a URL.

## Diagnostics and schemas

Stable redaction-safe diagnostics range from `TT100` through `TT199`. They name contract fields,
capabilities, or lifecycle failures without echoing source payloads. Invalid selections,
fingerprints, licenses, schemas, write-back, stale state, resolver registrations, events, entropy,
and replay all fail before state publication.

Six checked schemas cover the [manifest](downloads/weave-tabletop-adapter-manifest-v1.schema.json),
[selection](downloads/weave-tabletop-adapter-selection-v1.schema.json),
[Character projection](downloads/weave-tabletop-character-projection-v1.schema.json),
[mutable state](downloads/weave-tabletop-state-v1.schema.json),
[request](downloads/weave-tabletop-resolution-request-v1.schema.json), and
[receipt/events](downloads/weave-tabletop-resolution-receipt-v1.schema.json).

## Reference commands

```bash
cargo run -p weave-tabletop --example tabletop_fixture -- --check

cargo run -p weave-tabletop -- validate selection \
  examples/tabletop-adapters/contract/selection.tabletop-selection.json \
  --manifest examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json

cargo run -p weave-tabletop -- license-gate \
  examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json \
  --source-artifact examples/tabletop-adapters/contract/source/synthetic-rules.txt \
  --license-text examples/tabletop-adapters/contract/LICENSE

cargo run -p weave-tabletop -- project-events \
  examples/tabletop-adapters/contract/receipt.tabletop-receipt.json \
  --manifest examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json \
  --audience runtime \
  --output target/runtime.tabletop-receipt.json

cargo run -p weave-tabletop -- validate receipt \
  examples/tabletop-adapters/contract/receipt.tabletop-receipt.json \
  --manifest examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json \
  --request examples/tabletop-adapters/contract/request.tabletop-request.json
```

The checked [Lantern Trail fixture](https://github.com/chrisgliddon/weave/tree/main/examples/tabletop-adapters/contract)
provides equivalent RON/JSON, exact source and license hashes, deterministic replay, all three
visibility levels, and invalid artifacts for conflicts, versions, and undeclared capabilities.
