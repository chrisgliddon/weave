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
