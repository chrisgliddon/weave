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

## Verified Plug-And-Play implementation

The checked [Plug-And-Play fixture](https://github.com/chrisgliddon/weave/tree/dev/examples/tabletop-adapters/plug-and-play)
applies the contract to the public CC0-1.0 Plug-And-Play release. It is an independently authored
compatibility implementation, not an endorsement. The official Rules and Character Sheet PDFs
are exact hash-pinned review inputs and are not redistributed; the official plain-text CC0 legal
code is retained verbatim. `SOURCE.lock.json` records both PDF upload identities, filenames,
retrieval date, media types, hashes, redistribution decisions, reviewed behavior boundary, and
excluded visual/community material.

One versioned creation request supports either authored ratings or two deterministic candidate
sets of four d6 plus one d3. Rolled creation keeps both sets, the selected set, an explicit
one-to-one attribute assignment, seed, and per-set entropy cursors. Both paths require exactly two
independently zero-sum modifiers, bounded unique inventory, an explicit physical and mental basis
for Survivability, and visible base/effective values. The editor session previews every value,
records old/new seeds and request fingerprints on reroll, requires explicit acceptance, and exports
the same strict JSON/RON artifact used by CLI and runtime.

The trusted resolver implements and tests:

- ordinary checks that exceed the explicit difficulty, natural 6 criticals, natural 1 fumbles,
  advantage/disadvantage, and a mandatory replacement roll after spending one Fortune;
- Fortune tests that succeed below remaining Fortune and deliberately have no critical/fumble
  semantics;
- attacks using d6 + rating + half-rating, maximum critical damage, rolled ordinary damage,
  unarmed damage, melee-fumble self-damage, ranged jams, inventory enforcement, wounds at zero,
  the -5 wounded physical-check penalty, and death on later positive damage;
- group success only when individual successes outnumber failures;
- 54-card initiative without replacement, Ace high, suit order
  Spades > Hearts > Clubs > Diamonds, and valueless Jokers; and
- opposed chases bounded to one through three checks with immediate natural-result termination.

Spending three Fortune for story-scale narrative direction remains a human/authority-host decision,
not an automatic state-writing resolver operation. Hosts can record that decision in their own
reviewed narrative workflow without granting the adapter Character write-back authority.

The generated playthrough exercises every operation, independently replays every receipt, saves
the final state, lowers the accepted character to the ordinary domain-module contract, and compiles
the same story for standalone Rust, Bevy 0.18, and PixiJS v8. Bevy loads it as a `Resource` in the
`Startup` schedule. PixiJS validates the portable JSON before creating static `Text`; both hosts
show the public result and exact audit fingerprints while keeping the host-only entropy payload
redacted.

## Verified Dungeonpunk implementation

The checked [Dungeonpunk fixture](https://github.com/chrisgliddon/weave/tree/dev/examples/tabletop-adapters/dungeonpunk)
applies the same contract to the official eight-page CC0-1.0 Dungeonpunk core document. The
adapter pins Google Doc revision `15499`, the 2026-08-25 retrieval date, and exact PDF and UTF-8
text-export SHA-256 values. Those audit inputs are not redistributed. The community wiki,
supplements, official wording, artwork, branding, and layout are outside the reviewed adapter
boundary; fixture prose and scenarios are original.

Creation allocates exactly five points across Strength, Dexterity, Constitution, Intelligence,
Charisma, and Wisdom, with no starting rating above three. FATE is one, NONE is zero, and HP is a
deterministic 2d6 plus Constitution, with a third d6 when the selected endurance move applies.
Exactly three declarative special moves, explicit sorted gear, two distinct bonds, optional clocks,
and typed threats produce an immutable definition and separate mutable state. Exact half-Weight
units preserve equipped-item reductions without floating-point rounding; every Stress adds one
Weight and load above twelve Weight sets encumbrance.

The registered resolver exposes typed operations for:

- Struggle pools built from stat, advantage/disadvantage, help, push, impairment, and encumbrance;
  positive pools select the highest die, non-positive pools roll 2d6 and select the lowest, with
  six/twist/failure bands and failure XP;
- HP, Stress, impairment, minor and rolled damage tiers, Brace equipment marks, rest, collapse,
  an explicit entropy-consuming FATE death check, and a reviewed survival-at-cost choice;
- XP growth for moves and stat increases, carried load, bounded clocks and trigger events, armored
  threat HP, and closed general/threat-specific GM moves; and
- structured public consequences and GM prompts, authoring resource summaries, and host-only
  entropy traces whose payloads can be redacted without dropping their hashes.

The generated Vesper Ash playthrough replays every transition, round-trips JSON/RON saves, lowers
the final definition and state through the ordinary domain-module boundary, and compiles one
`.weave` story. Bevy 0.18 consumes its RON as a `Resource` in an explicitly ordered `Startup`
schedule; PixiJS v8 validates the JSON and redacted Struggle receipt before rendering `Text`.

## Verified Freehack implementation

The checked [Freehack fixture](https://github.com/chrisgliddon/weave/tree/dev/examples/tabletop-adapters/freehack)
pins the official Freehack 2.1 itch release and exact GitLab revision
`c98ac40f6b0bee2504c5c436dea2841a5d511ad9`. `SOURCE.lock.json` records the release PDF,
Markdown source, publication metadata, executable roll reference, CC0-1.0 legal code, retrieval
date, URLs, media types, hashes, and redistribution decisions. Upstream prose, art, icon,
stylesheet, branding, layout, trade dress, community material, and unverified assets are outside
the independently authored adapter boundary.

Creation is campaign-configured rather than setting-coded. A versioned schema chooses disabled,
open, or rarity-offer archetypes; arbitrary bounded authored, fixed, or seeded-random modifiers;
catalog or permitted authored features and inventory; optional inventory budgets; initial generic
tracks; and public or authority-only memories. The editor shows the manifest-defined steps,
records explicit reroll seed/hash lineage, invalidates acceptance after changes, and exports only
an accepted deterministic JSON/RON preview.

Support and opposition are positive checked-integer sums capped at the reviewed arithmetic domain.
Their squares form the exact favorable and unfavorable weights. Probability display uses rational
weights and floored integer basis points; resolution uses rejection-sampled deterministic entropy
over the nonzero signed domain and checked ceiling division for magnitude. No floating-point
probability, modulo bias, panic, or unchecked overflow participates in resolution.

Generic tracks declare an interval, target, advancing outcome, consequence, and visibility.
Simultaneous sections persist timing, mapping, abstraction, participant order, private submissions,
cancellation, timeout, resolution, and reveal policy. The generated playthrough covers public and
host-only checks, secret/public tracks and memories, mapped and unmapped timed sections,
cancellation and resubmission, deterministic ordering, timeout, save/restore, replay, and exact
RON/JSON pairs.

Freehack does not use the generic redacted-event envelope for players. Dedicated public state and
receipt schemas remove host-only event envelopes entirely and omit entropy, request/state lineage
derived from private inputs, hidden opposition, secret tracks, private memories, and pending
actions. Host-only operations produce no public receipt. The compiled Tavi Quill story is built
from a separate immutable public-only module; PixiJS imports only that story and public receipt,
while Bevy validates both the public artifact and a complete authority wrapper without logging
private values.

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
- exact primary and companion artifacts plus revision, release, tag, commit, or authored revision;
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
the validator checks only explicitly supplied primary bytes, every manifest-declared companion,
and the retained license text, and never follows a URL.

## Diagnostics and schemas

Stable redaction-safe diagnostics range from `TT100` through `TT199`. They name contract fields,
capabilities, or lifecycle failures without echoing source payloads. Invalid selections,
fingerprints, licenses, schemas, write-back, stale state, resolver registrations, events, entropy,
and replay all fail before state publication.

Seventeen checked schemas cover the [manifest](downloads/weave-tabletop-adapter-manifest-v1.schema.json),
[selection](downloads/weave-tabletop-adapter-selection-v1.schema.json),
[Character projection](downloads/weave-tabletop-character-projection-v1.schema.json),
[mutable state](downloads/weave-tabletop-state-v1.schema.json),
[request](downloads/weave-tabletop-resolution-request-v1.schema.json), and
[receipt/events](downloads/weave-tabletop-resolution-receipt-v1.schema.json), plus the
[Plug-And-Play creation request](downloads/weave-tabletop-plug-and-play-creation-request-v1.schema.json)
and [explainable creation preview](downloads/weave-tabletop-plug-and-play-creation-preview-v1.schema.json),
and the Dungeonpunk [creation request](downloads/weave-tabletop-dungeonpunk-creation-request-v1.schema.json)
and [creation preview](downloads/weave-tabletop-dungeonpunk-creation-preview-v1.schema.json), plus
Freehack [creation request](downloads/weave-tabletop-freehack-creation-request-v1.schema.json),
[creation preview](downloads/weave-tabletop-freehack-creation-preview-v1.schema.json),
[probability request](downloads/weave-tabletop-freehack-probability-request-v1.schema.json),
[probability preview](downloads/weave-tabletop-freehack-probability-preview-v1.schema.json),
[public state](downloads/weave-tabletop-freehack-public-state-v1.schema.json),
[public receipt](downloads/weave-tabletop-freehack-public-receipt-v1.schema.json), and
[authority receipt](downloads/weave-tabletop-freehack-authority-receipt-v1.schema.json).

## Reference commands

```bash
cargo run -p weave-tabletop --example tabletop_fixture -- --check
cargo run -p weave-tabletop --example plug_and_play_fixture -- --check
cargo run -p weave-tabletop --example dungeonpunk_fixture -- --check
cargo run -p weave-tabletop --example freehack_fixture -- --check

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

cargo run -p weave-tabletop -- plug-and-play-create \
  examples/tabletop-adapters/plug-and-play/creation.tabletop-creation.json \
  --output target/plug-and-play-preview.ron

cargo run -p weave-tabletop -- plug-and-play-resolve \
  examples/tabletop-adapters/plug-and-play/request.tabletop-request.json \
  --state examples/tabletop-adapters/plug-and-play/state.tabletop-state.json \
  --output target/plug-and-play-receipt.json

cargo run -p weave-tabletop -- dungeonpunk-create \
  examples/tabletop-adapters/dungeonpunk/creation.tabletop-creation.json \
  --output target/dungeonpunk-preview.ron

cargo run -p weave-tabletop -- dungeonpunk-resolve \
  examples/tabletop-adapters/dungeonpunk/request.tabletop-request.json \
  --state examples/tabletop-adapters/dungeonpunk/state.tabletop-state.json \
  --output target/dungeonpunk-receipt.json

cargo run -p weave-tabletop -- freehack-create \
  examples/tabletop-adapters/freehack/creation.tabletop-creation.json \
  --output target/freehack-preview.ron

cargo run -p weave-tabletop -- freehack-probability \
  examples/tabletop-adapters/freehack/probability.freehack-probability.json \
  --output target/freehack-probability-preview.json

cargo run -p weave-tabletop -- freehack-resolve \
  examples/tabletop-adapters/freehack/playthrough/preview_crossing_probability.tabletop-request.json \
  --state examples/tabletop-adapters/freehack/authority-state.tabletop-state.json \
  --authority-output target/freehack-authority-receipt.json \
  --public-output target/freehack-public-receipt.json
```

The checked [Lantern Trail fixture](https://github.com/chrisgliddon/weave/tree/dev/examples/tabletop-adapters/contract)
provides equivalent RON/JSON, exact source and license hashes, deterministic replay, all three
visibility levels, and invalid artifacts for conflicts, versions, and undeclared capabilities.
The checked Plug-And-Play fixture adds a complete CC0 public-source audit, concrete creation/editor
workflow, all supported mechanics, sequential save/reload playthrough, and standalone/Bevy/PixiJS
consumption without adapter-specific compiler syntax.
The checked Dungeonpunk fixture adds a second verified CC0 source boundary, manifest-driven
creation, Struggle and structured GM consequences, portable campaign state, explicit FATE
resolution, and the same standalone/Bevy/PixiJS boundary without compiler or editor special cases.
The checked Freehack fixture adds arbitrary campaign-shaped characters, exact integer
support/opposition resolution, generic tracks, simultaneous sections, and structurally separate
authority/public transports for standalone Rust, Bevy, and PixiJS.

## Adapter conformance harness

`weave-tabletop` ships a registry-driven conformance suite that checks every installed adapter
against one public contract. Run it from a local checkout; it is deterministic and performs no
network access:

```bash
cargo test -p weave-tabletop --test conformance
cargo test -p weave-tabletop --test conformance -- --ignored --nocapture
cargo run -p weave-tabletop --example tabletop_conformance
```

The suite discovers adapters through `weave_tabletop::conformance::standard_suite()`, not a
hard-coded test list. For each registered adapter it verifies:

- manifest validation, namespace attribution and isolation, and registry-wide namespace and
  identity uniqueness;
- creation from one seed is byte-for-byte reproducible, and resolution of the same before-state
  produces an identical receipt;
- entropy replays from the same seed, survives a saved cursor, and is actually consumed by the
  exercised operation;
- JSON and RON state round-trip, restore, and resume identically;
- authoring projections retain strictly more payloads than compact runtime projections while every
  projection keeps the event envelope and payload fingerprint;
- events are contiguous, declared by the operation, typed by the manifest, capability-consistent,
  and hash the exact payload;
- undeclared capabilities, undeclared operations, unsupported manifest/state/resolver versions,
  and malformed state artifacts fail closed;
- the machine-readable source policy matches the exact primary source digest, the allowlisted
  licence and source class, the retained licence text and notice text, and never redistributes a
  forbidden layout, artwork, or font file.

Policy expectations are data, not code: `conformance::policy::standard_source_policy()` returns one
`AdapterSourcePolicy` row per adapter, including its exact source digest, licence, required files,
required notices, and forbidden files. A changed hash, a missing review row, an unknown licence, or
a redistributed forbidden file is a data change plus a failing check — not a new branch in the
suite.

Every failure names the adapter, the check identifier, and the offending value, and the report ends
with one summary line per adapter. Adding an adapter requires no change to the checks:

1. implement `ConformanceAdapter` with the manifest, a trusted resolver factory, a deterministic
   creation fixture, and a short operation scenario;
2. add its `AdapterSourcePolicy` row to `standard_source_policy()`;
3. register the descriptor in `conformance::standard_suite()`.

The integration test `tests/conformance.rs` additionally runs deliberately broken fixtures that
must fail exactly one check each — changed source hash, missing source hash, namespace collision,
non-deterministic creation, missing notice, permissive resolver, policy licence mismatch, and
redistributed forbidden file — so the harness itself cannot silently stop detecting regressions.

## Portable adapter catalog

`examples/tabletop-adapters/catalog/catalog.json` is the machine-readable index of every adapter
shipped in this build. For each adapter it records:

- the exact id, version, state-schema version, namespace, title, and factual compatibility label;
- the licence expression, licence URL, source class, source URL, exact artifact, revision, retrieval
  date, and SHA-256;
- covered sections, exclusions, attribution, retained licence text, notices, and forbidden files;
- declared capabilities, ordered creation steps, and resolver operations; and
- conformance status and the standard checks that apply.

The file is derived data. `crates/weave-tabletop/tests/catalog.rs` rebuilds it from the live
`*_manifest()` functions, `standard_source_policy()`, and `standard_suite()`, then fails when the
checked-in file differs. A changed capability, operation, licence, source digest, or conformance
registration therefore cannot enter a release silently.

`conformance.status` is `standard_suite_registered` when the adapter appears in
`conformance::standard_suite()`, and `not_registered` otherwise. Unregistered adapters still appear
in the catalog with an empty `checks` list, so the shipped surface stays visible.

Regenerate the catalog only after the manifest and policy change is reviewed:

```bash
WEAVE_WRITE_ADAPTER_CATALOG=1 cargo test -p weave-tabletop --test catalog -- --ignored
cargo test -p weave-tabletop --test catalog
```

## Adding an adapter

An adapter is inert declarative data plus an optional trusted resolver. You do not change the
conformance checks, compiler, editor, or runtime to add one. Work through these steps in order.

### 1. Manifest

Add a `<name>_manifest()` function that returns an `AdapterManifest` in the adapter module. It must
declare format 1, a globally namespaced id such as `org.weave.tabletop.<name>`, a semantic version,
a positive `schema_version`, and a `namespace` that is a segment of the id. It also declares:

- `title`, a factual `compatibility_label`, `summary`, `weave_version`, and
  `character_contract_version`;
- `declarative_data_with_registered_resolver` with resolver contract version 1;
- a sorted subset of the closed capability vocabulary;
- bounded reusable types plus the immutable definition and mutable state types;
- ordered creation steps whose fields are adapter-owned or optional canonical suggestions;
- named operations with one required capability, a request type, typed event kinds, and an exact
  `consumes_entropy` declaration;
- the fixed v1 visibility matrix, optional reviewed migrations, and the complete provenance record.

Call `validate_adapter_manifest` in a test. Unknown fields, unbounded types, duplicate symbols,
undeclared capabilities, and secret-shaped text must fail.

### 2. Resolver

Implement `TabletopResolver` only when a rule procedure cannot be declarative. Register it with
`ResolverRegistry::register` for one exact adapter id and version. The resolver must:

- accept only the operation ids, request types, payload types, and event kinds declared by the
  manifest;
- consume entropy exactly when the operation declares it, using the supplied `EntropyStream`;
- return typed events and complete next state, or an error that leaves prior state unchanged; and
- never receive an editor, filesystem, network client, clock, Bevy world, PixiJS application, or
  Character write-back handle.

Registration is explicit host code. Opening a pack never loads or runs code.

### 3. Source-policy row

Add a `pub const <NAME>_SOURCE_POLICY: AdapterSourcePolicy` row in
`crates/weave-tabletop/src/conformance/policy.rs` and include it in `standard_source_policy()`.
The row repeats the exact namespace, licence, source class, source digest, required licence text
file and digest, required notices, required notice files, and forbidden file names.

The only accepted source-class and licence pairs are:

| Source class | Exact licence |
|---|---|
| `original` | `MIT` |
| `verified_cc0` | `CC0-1.0` |
| `separately_licensed_apache2` | `Apache-2.0` |

An Apache-2.0 pack must retain the full licence text and required notices, mark its
modifications, and never be described as MIT-only.

### 4. Conformance descriptor

Implement `ConformanceAdapter` for the adapter and register one descriptor in
`conformance::standard_suite()`. The descriptor supplies:

- `adapter_id`, which must equal the manifest id;
- `manifest()` and a fresh trusted `resolver()`;
- `create(seed)`, which must produce byte-identical canonical output for the same seed;
- `scenario(creation)`, which must exercise declared operations and consume entropy where
  declared; and
- `source_bundle()`, which declares exact bytes or digests for every primary artifact, companion,
  retained licence, and notice.

The suite owns every check. Adding an adapter changes no check logic.

### 5. Fixtures

Add `examples/tabletop-adapters/<name>/` with a README, `SOURCE.lock.json`, the manifest as JSON
and RON, a deterministic creation request and preview, state, request and receipt, selection,
Character projection, runtime artifacts, and the retained licence text. Add
`crates/weave-tabletop/tests/<name>.rs`.

All names, prose, settings, adventures, factions, UI copy, and expected outputs must be original
or synthetic. Never copy upstream prose, art, layout, or community material into a fixture.

### 6. Provenance rules

Every adapter, including an independently authored one, records:

- a public HTTPS source URL;
- exact primary and companion artifact names, with revision, release, tag, commit, or authored
  revision;
- the retrieval date and SHA-256 of the exact covered bytes;
- the source class, exact SPDX licence expression, and public licence URL;
- covered files or sections, exclusions, attribution, retained licence text and digest, notices,
  and a factual compatibility and non-endorsement statement.

Exclude community supplements, logos, trade dress, artwork, layout, fonts, and unverified assets.
System names may appear only as courteous factual compatibility labels. They never imply
affiliation, sponsorship, or endorsement.

The validator is offline. Acquiring or updating an upstream artifact is a separate reviewed
process, and a changed hash or missing exclusion must fail the gate.

### 7. Catalog

Add the manifest function to `live_manifests()` in `crates/weave-tabletop/tests/catalog.rs`, then
regenerate and verify the catalog as shown above. A source-policy row without a listed manifest
also fails the catalog test.

### 8. Verification commands

Run these from a local checkout:

```bash
cargo test -p weave-tabletop --test <name>
cargo test -p weave-tabletop --test conformance
cargo test -p weave-tabletop --test catalog
cargo test -p weave-tabletop
cargo clippy -p weave-tabletop --all-targets -- -D warnings
rustfmt --edition 2024 --check crates/weave-tabletop/tests/catalog.rs
```

## Applied switches, archives, and audit

`preview_adapter_switch` remains the manifest-only, non-mutating check. Hosts that hold installed
manifests and optional play state call `TabletopCharacterProjection::preview_switch` and
`TabletopCharacterProjection::apply_switch` for an explicit, reviewed transition. Both accept an
`AdapterSwitchRequest`, so the same serialized request can be previewed, archived, reloaded, and
applied without reinterpreting host state.

A character or play context has zero or one active primary projection. Inactive projections are
stored only under the exact `id_version` namespace key and never resolve for expressions, events,
panels, or capability discovery. `active_definition`, `projection_for_namespace`, and
`active_capabilities` expose the active projection only. Reactivation is the only path back.

The preview reports:

- the exact source and target coordinates and whether the target is installed;
- capabilities gained and lost, derived from the active and target manifests;
- inactive namespaces that will be archived, restored, explicitly discarded, or retained;
- definition fields and mutable resource paths that cannot transfer;
- active state validity conflicts and the SHA-256 of the supplied state, never its contents;
- each author decision required before applying; and
- dry-run diagnostics, including same-adapter migration checks.

Applying requires every required decision id in `accepted_decisions`; an unknown or missing
decision fails with `TT101`. The projection is cloned, validated in full, and committed only after
every check passes, so a failed migration or conflict leaves the original projection
byte-identical.

Cross-system definitions, attributes, abilities, items, resources, conditions, progression, and
campaign state are never converted. A cross-system switch archives the outgoing projection under
its own namespace and either restores an exact target archive or activates an independently
created target definition. The archive stores the immutable definition, its hash, and the exact
completed creation-step list, so restoration returns all three without reconstruction. An archive
is consumed only for its exact adapter coordinate, never for a matching namespace alone. An
explicit discard policy removes the outgoing projection without planning an archive, and only
after the matching decision is accepted.

A same-adapter release change is an in-place, migrated upgrade. It requires a declared migration
that is `reviewed`, `preserves_canonical_character`, and matches the source id, version
requirement, and schema version. The supplied migrated definition is dry-run validated against
the target schema before any write; a missing declaration, a schema downgrade, or a failed dry run
blocks the switch and rolls back.

Every applied switch returns an `AdapterSwitchAudit`. It records the initiating actor class and
stable actor id, the old and new coordinates, accepted decisions, diagnostics, archived, restored,
discarded, and retained namespaces, preserved or discarded state fingerprints, the migration id,
the replaced definition fingerprint, and the committed projection fingerprint. Canonical
`canonical_profile_sha256` and suggestion lineage are unchanged, and
`canonical_character_write_back` remains false. The audit contains fingerprints, identifiers, and
fixed diagnostic text only, so it never stores a credential or a raw state payload.

## Open Tale adapter

`crates/weave-tabletop/src/open_tale.rs` is an independently authored adapter for the public
Open Tale rules reference at <https://open-tale.github.io/>. Open Tale is released under the
Apache License 2.0. The adapter is not a copy and is not endorsed by the Open Tale project.

The manifest records the Apache-2.0 boundary. `OPEN_TALE_SOURCE_RECORD` states the canonical URL,
the implemented edition, and the licence, and the manifest pins its SHA-256
`17a3f9dd52fc291ec10757fd8c72f9a8423e09f582ad87e24820377d15fb3270`. That digest covers the
authored record, not a downloaded page. The complete Apache-2.0 text is retained in
`OPEN_TALE_LICENSE_TEXT`, pinned by
`8ada45cd9f843acf64e4722ae262c622a2b3b3007c7310ef36ac1061a30f6adb`. Artwork, fonts, and layout
are unlicensed and never bundled.

Creation allocates exactly 18 points across nine attributes, selects at most four skills with at
most six total ranks, and spends a declared wealth budget on catalog equipment. Overspending,
invalid levels, duplicate skills, and unknown items fail closed with the exact field path.

Checks roll two d6 through `sha256_counter_v1`. Advantage keeps the highest two of three dice;
disadvantage keeps the lowest two of three. A Heroic Badge adds one die, keeps the highest two,
and cancels disadvantage. Outcome bands are critical at difficulty plus five, success at
difficulty, partial within two below difficulty, and failure below that. Each mark and each ten
units of distance subtract one. The five derived qualities use inspectable integer formulas.

The registered resolver declares attribute and quality inspection, challenges, initiative,
combat turns, movement, recovery, purchasing, and advancement. Damage, marks, health, resolve,
wealth, level, and experience live in serializable adapter state. Every operation declares its
event kinds and whether it consumes entropy. Fixtures use original synthetic names and prose.

Run the adapter suite from a local checkout:

```bash
cargo test -p weave-tabletop --test open_tale
```

## Spark adapter

Spark uses step-die Body, Heart, Mind, and Spark attributes, tiered Broad, Common, and Deep Talents,
relationships, and three authored Beliefs. Creation validates the exact point budget, the three
Beliefs, and every relationship reference. The resolver gathers the owning step die and talent
bonus, applies declared supporters, resolves competing declarations, handles ties and escalation,
accepts explicit post-roll Influence or Harm boosts, and charges the winner's cost. Closing a scene
records confirmed or refuted Beliefs, changes Influence through an explicit review, recovers
permitted Harm, and keeps the rationale as authoring lineage.

Settings add factions, ties, faces, agendas, and optional world references under the adapter
namespace. Collaborative setting creation merges contributions in a deterministic order and keeps
every revision, incomplete work is resumable, and scene framing opens the same conflict loop.
Belief replacement records the old and new statement, requires explicit confirmation, and grants
Growth only when the replaced Belief was refuted. Setting Evolution requires satisfied conditions
and returns reviewable before and after changes without writing back to canonical character data.

The manifest pins the public source record, edition, CC0-1.0, and the SHA-256 of that checked-in
record. Example settings, logos, artwork, and presentation are excluded even where the upstream
guide places them under CC0.

```bash
cargo test -p weave-tabletop --test spark
```

## Wicked Ones adapter

Wicked Ones creation covers calling, action ratings grouped by Brains, Muscles, and Guts, dark
impulse, revelry, gear, abilities, a vile friend, Stress, and dark hearts. The action loop validates
intention, approach, action, position, and effect, applies declared bonuses and teamwork, returns
failure, mixed, success, and critical bands with structured consequences, and resolves resistance
and Stress deterministically. Flashbacks, shock, bloodied and feral states, death, and the final
death knell mutate serializable state through explicit operations.

The campaign model adds the dungeon, rooms, structures, creatures, minion packs, factions with
clocks, and raids. The campaign cycle has explicit phases with downtime actions, dungeon events,
calamity, raid planning and engagement into the action loop, blowback, recovery, and advancement.
A deterministic golden cycle and editor-facing views cover the full loop.

The manifest pins the public CC0-1.0 v1.4 source record and denies the two upstream properties that
are not covered by that release; no upstream prose, layouts, or artwork appear in fixtures.

```bash
cargo test -p weave-tabletop --test wicked_ones
```

## Open Adventure adapter

Open Adventure creation derives nine named attributes, lineage, culture, background,
specializations, favored actions, equipment references, Benefits and Drawbacks, and the defense,
reaction, Body, Mind, Spirit, action-score, and action-die values. Challenges and encounters use
exploding action dice with the seven-step progression, Effect, challenge-point pools, and
deterministic success, withdrawal, defeat, or host-decision endings. Wounds, magic, Story Points,
rewards, and advancement are explicit operations with structured public and authority events.

Optional modules declare a version, namespace, order, and CC0 provenance, apply add or replace
capability rules, and never change the core when inactive. A golden adventure replays across JSON
and RON after every step, and the adapter exposes its scope, exclusions, factual compatibility
naming, and contributor steps for hosts.

The manifest pins the public CC0 source record and digest. The Masterwork Edition layout, artwork,
and fonts are excluded.

```bash
cargo test -p weave-tabletop --test open_adventure
```
