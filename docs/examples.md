# Runnable examples

Every `.weave` source below is compiled by the documentation gate. The finite Rust programs are executed to completion, the PixiJS consumer is tested and bundled, and checked JSON/RON artifacts must match their source byte for byte.

| Example | What it proves | Run from the repository root |
|---|---|---|
| Basic source | Grammar expansion and terminal flow | `cargo run -p weave-compiler -- examples/stories/basic.weave --output target/basic.ron` |
| Branching source | Conditions, choices, variables, and diverts | `cargo run -p weave-compiler -- examples/stories/branching.weave --output target/branching.ron` |
| Pattern source | Structured pattern draws and semantic branches | `cargo run -p weave-compiler -- examples/stories/patterns.weave --output target/patterns.ron` |
| Community package | Validation, publication, installation, version resolution, and runtime embedding | `cargo run -p weave-patterns --bin weave-pattern -- validate patterns/community/ember-omens/package.weave-pattern.json` |
| Domain-module tracer | Explicit activation, typed paths, deterministic RON/JSON lowering, and runtime branching | `cargo run -p weave-compiler -- examples/domain-modules/contract/tracer.weave --module-manifest examples/domain-modules/contract/module.weave-module.json --module-pack examples/domain-modules/contract/pack.weave-domain.json --output target/tracer.story.ron` |
| Third-party module tutorial | Public manifest/pack authoring, adjacent discovery, exact locking, and compatible ranges | `cargo run -p weave-compiler -- examples/domain-modules/third-party-tutorial/story.weave --locked --format json --output target/lantern-weather.story.json` |
| Weave World corpus | Four exact country/region/ecosystem reference shorthands, typed environmental values, reviewed public provenance, and equivalent RON/JSON | `cargo run -p weave-world-corpus -- check examples/domain-modules/weave-world/corpus.weave-world.json --manifest examples/domain-modules/weave-world/module.weave-module.json` |
| Authored Weave World | Typed rules, stable nested and related places, explicit local inheritance/overrides, source lineage, and exact RON/JSON | `cargo run -p weave-compiler -- examples/domain-modules/weave-world/authored-setting.weave --module-manifest examples/domain-modules/weave-world/module.weave-module.json --module-pack examples/domain-modules/weave-world/pack.weave-domain.json --format json --output target/authored-setting.story.json` |
| Composed Weave World | Deterministic broad/regional/ecosystem precedence, exact transitive locking, authored-last overrides, and versioned full/compact JSON/RON | `cargo run -p weave-world --example world_fixture -- --check` |
| Weave Character end to end | Complete six-factor/24-facet profile, explainable classification/vocation/social/narrative projection packs with capacity/review/locks, reusable normalized expression and deterministic dialogue, typed identity presentation, recomputed read-only OCEAN, pluggable alignment, temporal context, exact RON/JSON, Bevy, and PixiJS consumption | `cargo run -p weave-character --example projection_fixture -- --check` |
| Guided Character authoring | Blank/template/clone source, sparse overrides, all 24 facets, original behavior questionnaire, effective field origins, migration preview with retained overrides, downstream invalidation, keyboard editor parity, final review, export, and reopen | `cargo run -p weave-character --example guided_authoring_fixture -- --check` |
| Assisted Character development | Credential-free scope preview, deterministic offline scaffolds, typed candidates, advisory and author reviews, suggestion-only apply, provider comparison, and resumable batch receipts in exact JSON/RON | `cargo run -p weave-character --example assistance_fixture -- --check` |
| Character corpus health | Read-only aggregate validation, stable source-located diagnostics, coverage/distributions, RON/JSON portability, filters, CI thresholds, and healthy/incomplete/stale/unsafe/malformed/migration goldens | `cargo run -p weave-character --example health_fixture -- --check` |
| Tabletop adapter contract | Exact selection, isolated definition/state, deterministic entropy/replay, typed visibility-scoped events, switching preview, and strict source licensing | `cargo run -p weave-tabletop --example tabletop_fixture -- --check` |
| Plug-And-Play end to end | Verified CC0 source bundle, deterministic rolled/authored creation, editor acceptance/reroll lineage, checks, Fortune, combat, group action, card initiative, bounded chases, save/reload, replay, and Rust/Bevy/PixiJS portability | `cargo run -p weave-tabletop --example plug_and_play_fixture -- --check` |
| Dungeonpunk end to end | Verified revision-pinned eight-page CC0 source, six-stat creation, HP/Stress/XP/load, Struggle consequences, damage/rest/death, growth, clocks, threats, structured GM prompts, save/reload, replay, and Rust/Bevy/PixiJS portability | `cargo run -p weave-tabletop --example dungeonpunk_fixture -- --check` |
| Standalone Rust | Compiler-to-runtime flow without an engine | `cargo run -p weave-example-standalone` |
| Bevy dialogue game | Interactive dialogue, choices, observable state, hot reload, and pattern-event UI | `cargo run -p weave-example-bevy-dialogue` |
| Bevy domain consumer | Portable module RON loaded into a Bevy resource without editor dependencies | `cargo run -p weave-example-domain-module-bevy` |
| PixiJS domain consumer | Portable module JSON decoded, tested, and rendered with PixiJS v8 | `npm --prefix examples/domain-module-pixijs test` |
| Browser player | JSON, WASM, deterministic restart, and save/restore | `./scripts/build-web-player.sh` |
| Visual editor | Native GPUI project workflow | `cargo run -p weave_editor` |

## Verify all documentation

Install the pinned site generator once, then run the repository-owned build:

```bash
cargo install mdbook --version 0.5.4 --locked
python3 scripts/build-docs.py
```

The command checks source links, the quickstart source, all example stories, finite example binaries, generated web JSON, rustdoc, site search, local HTML targets, document language, main landmarks, image alternatives, contribution links, and the private vulnerability-reporting path.

The Bevy game also has a window-free integration gate for CI and servers:

```bash
cargo run -p weave-example-bevy-dialogue -- --smoke-test
```

The [community package guide](community_patterns.md) provides the complete commands for publishing and installing Ember Omens, then compiling its package-backed example story.

The [domain-module guide](domain_modules.md) explains the shared manifest, packaging, activation, source-owned replacements, read-only paths, editor reload, compiled IR, and consumer boundaries. The [third-party tutorial](domain_module_tutorial.md) builds a complete original module with public tools only, the [Weave World guide](world_module.md) follows four openly redistributable reference-place seeds and an original authored hierarchy, the [Weave Character guide](character_module.md) follows one complete synthetic profile through normalized expression/dialogue, validation, and reviewed enrichments, and the [tabletop adapter guide](tabletop_adapters.md) defines the host-neutral capability, state, replay, visibility, switching, and public-source contract.
