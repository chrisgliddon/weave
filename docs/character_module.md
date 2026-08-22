# Weave Character Profile contract

Weave Character v1 is a portable, data-only contract for fictional character canon, evidence, projections, suggestions, and deterministic template synthesis. It is not a personality test, clinical instrument, hiring tool, or claim about a real person. The contract does not copy or administer questionnaire items. It uses the public six-factor and 24-facet vocabulary documented by the [HEXACO-PI-R authors](https://hexaco.org/scaledescriptions); the authors' [inventory history](https://hexaco.org/history) distinguishes those 24 factor-specific facets from later interstitial scales. The synthetic fixture values, identities, prose, algorithms, extension examples, and expected outputs are original MIT-licensed Weave material.

The Rust authority is `weave-character`. Download the generated [profile schema](downloads/weave-character-profile-v1.schema.json), [template schema](downloads/weave-character-template-v1.schema.json), [overlay schema](downloads/weave-character-overlay-v1.schema.json), [synthesis schema](downloads/weave-character-synthesis-v1.schema.json), [diagnostic schema](downloads/weave-character-diagnostic-v1.schema.json), [collection schema](downloads/weave-character-collection-v1.schema.json), [operation-request schema](downloads/weave-character-operation-request-v1.schema.json), [proposal schema](downloads/weave-character-proposal-v1.schema.json), [review schema](downloads/weave-character-review-v1.schema.json), and [progress schema](downloads/weave-character-progress-v1.schema.json).

The reviewed date-context workflow has separate generated schemas for its [offline pack](downloads/weave-character-temporal-pack-v1.schema.json), [ranking configuration](downloads/weave-character-temporal-config-v1.schema.json), [immutable proposal](downloads/weave-character-temporal-proposal-v1.schema.json), [complete review](downloads/weave-character-temporal-review-v1.schema.json), and [reproducible receipt](downloads/weave-character-temporal-receipt-v1.schema.json).

Explainable alignment views have separate generated schemas for the [declarative pack](downloads/weave-character-alignment-pack-v1.schema.json), [project selection](downloads/weave-character-alignment-config-v1.schema.json), [immutable proposal](downloads/weave-character-alignment-proposal-v1.schema.json), [complete review](downloads/weave-character-alignment-review-v1.schema.json), and [independently reproducible receipt](downloads/weave-character-alignment-receipt-v1.schema.json).

## Trust and data boundaries

A `CharacterProfile` has four deliberately separate layers:

| Layer | Authority | Can alter canonical personality evidence? |
|---|---|---|
| `canon` | Stable identity, explicit date precision, canonical HEXACO evidence, authored inner life, and authored voice guidance | Yes, but only through a validated authored/imported/reviewed/overridden operation |
| `extensions` | Versioned presentation, expression, behavior, role, relationship, alignment, date-context, tabletop, or future records | No; every header serializes `canonical_personality_write_back: forbidden` |
| `suggestions` | Pending or rejected proposals with a target path and rationale | No; acceptance must create a separately attributed canonical operation |
| `derived` | Reproducible compatibility views with exact input paths | No; derived values are not independent evidence and have no write operation |

No extension is required. An omitted extension means “no assertion,” not an empty score, false value, or default category. Unknown core profile, template, overlay, and synthesis versions fail closed. A future extension version can be retained only as an `opaque` or `tabletop` record with `preserved_inactive`; typed extension variants reject unknown versions. Preservation never implies interpretation, activation, validation by the future schema, or permission to write back.

Profiles and overlays may contain sensitive fictional authoring material. Hosts should apply the repository security policy, keep private drafts out of public fixtures, never place credentials in character values, and render any detected secret as exactly `[REDACTED]`.

## Stable identity and dates

Profile, template, overlay, character, authority, relationship-kind, taxonomy, and external reference identifiers are dot-separated lowercase stable identifiers. Human-readable display names and aliases can change without changing the profile id. Inner-life, voice, suggestion, signature, role, relationship, and operation records use lowercase local ids that must equal their containing map key.

Birth dates are date-only values with one explicit `precision`:

| Precision | Known components | V1 behavior |
|---|---|---|
| `year` | Calendar and year | Month and day remain unknown |
| `month_day` | Calendar, month, and day | Year remains unknown; February 29 is valid because an unknown year may be a leap year |
| `full` | Calendar, year, month, and day | The exact proleptic-Gregorian leap-year rule is validated |

V1 supports `proleptic_gregorian` and years 1 through 9999. A date has no implied time, time zone, place, age at an unspecified reference instant, seasonal fact, or historical meaning. Other calendars and temporal facts require versioned extensions.

## Reviewed temporal context enrichment

Temporal enrichment turns an explicit birth-date precision into reviewable fictional authoring cues; it never treats a date, place, historical event, calendar fact, or environmental observation as a cause or measurement of personality. Propose and dry-run do not mutate the profile. Apply writes only a reviewed `org.weave.character.date_context` extension and pending suggestions, retains `canonical_personality_write_back: forbidden`, and fails atomically on stale inputs, incomplete reviews, locked context without a configured override, or any attempted canonical write-back.

Each `TemporalContextPack` is an immutable offline artifact with an id, semantic version, license, provider, SHA-256-addressable provenance, and bounded records. A record keeps the structured fact and its source ids separate from original fictional cues and their source ids. A cue declares its own signed HEXACO vector in thousandths; ranking reads that vector and the already-authored profile evidence, but never reads the fact payload. A `domain_module` provider can expose a pinned Weave World projection without making World mandatory or changing that separation.

Known precision is honored exactly:

| Character date | Eligible record extents | Match trace |
|---|---|---|
| `year` | Equal year/date, or a year/date range containing that year | `same_year` or `containing_period` |
| `month_day` | Equal recurring month/day only | `recurring_month_day` |
| `full` | Equal date, equal recurring month/day, equal year, or a range containing the date/year | `exact_date`, `recurring_month_day`, `same_year`, or `containing_period` |

Date-only matching never shifts a date through a time zone. Place applicability is exact in v1, while time-zone differences and uncertainty reduce relevance through documented fixed factors. Ranking then uses integer millionths, a declared trait-coverage threshold, deterministic SHA-256 seed tie-breaking, and a hard candidate limit. Every scanned cue receives a sorted coverage entry marked `selected`, `downgraded`, or `skipped`; every selected candidate retains the complete per-trait contribution trace and a plain-language explanation.

The review boundary is complete and explicit. Every candidate must be accepted, rejected, edited, withheld, overridden, or narrowly auto-approved under the proposal's pinned low-sensitivity policy. Sensitive acceptance requires rationale. A review fingerprints the exact profile and proposal. Apply independently reproduces the proposal from the embedded profile, exact pack versions and hashes, configuration, and seed; reproduces the review; and returns a receipt containing both inputs and the atomically derived output. Reusing that proposal after a committed editor apply is stale by construction.

The public corpus exercises exact selection, threshold downgrade, and filter skip. Its real-world example pins the [Wikidata Apollo 11 entity](https://www.wikidata.org/wiki/Q43653) through [revision 2526069885 of the structured entity JSON](https://www.wikidata.org/wiki/Special:EntityData/Q43653.json?revision=2526069885&flavor=simple), modified `2026-08-02T06:29:38Z`, with SHA-256 `52c78c8a7c1320e970a6aa1c2737c9e0a89e786be957f380f389d02ab84c006b`. Wikidata's [CC0 terms](https://www.wikidata.org/wiki/Wikidata:Licensing) cover the dated fact. The accompanying cue and vector are original MIT-licensed Weave material under a different source id, so consumers can audit fact lineage and fictional-cue lineage independently.

## Canonical HEXACO fields

The profile defines all six factors and their four factor-specific facets. The public terminology follows the official scale descriptions; Weave does not redistribute scale questions, scoring keys, normative samples, or assessment results.

| Factor | Canonical facets |
|---|---|
| Honesty-Humility | `sincerity`, `fairness`, `greed_avoidance`, `modesty` |
| Emotionality | `fearfulness`, `anxiety`, `dependence`, `sentimentality` |
| Extraversion | `social_self_esteem`, `social_boldness`, `sociability`, `liveliness` |
| Agreeableness | `forgivingness`, `gentleness`, `flexibility`, `patience` |
| Conscientiousness | `organization`, `diligence`, `perfectionism`, `prudence` |
| Openness to Experience | `aesthetic_appreciation`, `inquisitiveness`, `creativity`, `unconventionality` |

Each factor and facet is independently optional and uses exactly one measurement form:

- `score` is a finite normalized number from `0.0` through `1.0`.
- `band` is one of `very_low`, `low`, `middle`, `high`, or `very_high`.
- omission means unknown; validators and projections never fill a missing factor or facet from identity, date, presentation, alignment, relationship, role, tabletop, or other extension data.

Bands remain bands in canon. Only the v1 compatibility projection uses the fixed anchors `0.1`, `0.3`, `0.5`, `0.7`, and `0.9`. These anchors are a serialization algorithm, not assessment norms.

Every canonical value includes confidence, value state, review state, lock state, freshness, sorted lineage references, and a rationale when required. Confidence describes the representation and source, not a judgment of the character or person.

## Value states and precedence

States remain explicit in both RON and JSON. Deterministic replacement precedence, from highest to lowest, is:

1. `overridden`
2. `authored`
3. `reviewed`
4. `imported`
5. `derived`
6. `suggested`

`reviewed` and `overridden` require an accepted review and rationale. `suggested` remains pending or rejected. `derived` is mechanically produced and not independently reviewed. Canonical HEXACO fields reject `derived` and `suggested`; accepting a proposal creates a new reviewed or overridden field with its own lineage instead of relabeling the proposal in place.

A lower-precedence operation cannot replace a higher-precedence field. Replacing or removing any present value requires the exact SHA-256 of the prior canonical serialized field. A locked value additionally requires an incoming `overridden` value. Stale values and stale template fingerprints cannot enter canon.

## Lossy OCEAN compatibility view

`weave_hexaco_to_ocean_compatibility@1` is an original Weave interchange algorithm. It is deliberately labeled `lossy: true` and `independent_evidence: false`. The HEXACO literature describes a distinct six-factor framework rather than a drop-in five-factor encoding; the underlying research overview is available from [Ashton and Lee (2007)](https://doi.org/10.1177/1088868306294907). Weave therefore calls this an OCEAN compatibility view, not a validated psychometric conversion.

For each of Openness, Conscientiousness, Extraversion, Agreeableness, and Emotionality:

1. Use the explicit canonical factor measurement when present.
2. Otherwise, use the arithmetic mean only when all four canonical facets are present.
3. Convert a band to its fixed v1 anchor only inside this projection.
4. Round a computed facet mean to six decimal places.
5. Set derived confidence to the minimum confidence of the four inputs.
6. Record the exact canonical input path or paths.

The compatibility field called `neuroticism` receives the Emotionality projection, but the names do not assert construct equivalence. Honesty-Humility is explicitly recorded as the omitted factor. A missing factor and any incomplete four-facet set produce an omitted derived dimension, never imputation. When both a factor and facets exist, the factor is the sole projection input so evidence is not double-counted. The derived view cannot be edited or counted as a second source; parsing rejects serialized OCEAN values that do not exactly recompute from canon.

## Explainable, pluggable alignment views

An alignment view is optional fictional narrative shorthand derived from already-authored canonical evidence. It is not a diagnosis, psychometric assessment, moral rank, protected-class inference, causal model, behavior prediction, replacement for HEXACO, or authority over canon. A proposal has no public or runtime effect until every selected axis receives an explicit editorial decision.

`AlignmentPack` is an independently distributable, data-only contract. A pack declares its stable id and semantic version, license and license URL, methodology, limitations, exact canonical HEXACO inputs, signed weighted axes, ordered neutral label thresholds, synthetic calibration fixtures, and machine-readable provenance. Its provider is either `standalone` or an exact `domain_module` coordinate with module id/version, pack id/version, and the SHA-256 of the provider-supplied methodology, limitations, inputs, axes, and calibrations. Provider coordinates never load code or permit network discovery.

V1 input paths are a closed set of canonical HEXACO factor and facet paths. They cannot read identity, dates, extensions, suggestions, OCEAN, alignment output, or arbitrary domain data. Each input id must match its map key, `trait_id`, and exact profile path. Each axis uses non-zero signed thousandth weights and a complete increasing threshold list whose final bound is `1000000`. Pack validation runs every calibration fixture before the pack can be proposed.

The fixed-point algorithm is exact:

1. Convert a present score to integer millionths; a retained band uses only the documented v1 anchor.
2. Center each present input as `2 × profile_micros − 1000000`.
3. Multiply by the declared signed weight and sum with integer arithmetic.
4. Compute coverage as covered absolute weight divided by total absolute weight, in millionths.
5. Divide the signed sum by covered absolute weight with half-away-from-zero rounding.
6. Select the first threshold whose inclusive upper bound contains the score.

Missing evidence remains visible in the ordered trace with no score, contribution, or invented lineage. It contributes neither a value nor fabricated precision. An axis below `minimum_coverage_micros` begins `withheld`; score and coverage remain separate. The deterministic seed is pinned and included in the per-axis trace fingerprint, but v1 has no random scoring branch, so changing the seed changes trace identity rather than the score or label.

`AlignmentConfig` selects a sorted exact axis set, coverage threshold, and locked-view override policy. `AlignmentProposal` pins the profile id and SHA-256, pack id/version/SHA-256, full config and its SHA-256, seed, values in selected-axis order, and every canonical input path/value/weight/contribution/lineage record. Any change to those inputs makes the proposal stale.

Review is complete and explicit:

| Action | Public effect | Required record |
|---|---|---|
| `accept` | Publish the proposed pack label as `reviewed` | Optional rationale |
| `edit` | Publish another declared pack label as `edited` | Label id and rationale |
| `override` | Publish another declared pack label as `overridden` | Label id and rationale |
| `reject` | Publish nothing | Rationale retained in the review/receipt |
| `withhold` | Publish nothing | Rationale retained in the review/receipt |

Every selected axis must appear exactly once. An undeclared label, incomplete or extra decision, accept of an ineligible proposal, mismatched axis id, stale fingerprint, invalid provider content hash, secret-shaped text, or attempted canonical write-back fails before output. A locked existing alignment view additionally requires a configured override and rationale.

Apply independently reproduces the proposal and review, computes an application fingerprint, merges declared provenance, and atomically writes only the reviewed `org.weave.character.alignment` extension. The profile's `canon`, `derived`, and pre-existing authoring state remain unchanged. The public view retains exact pack/review/application fingerprints, used canonical input paths, coverage, optional score, explanation, and only `reviewed`, `edited`, or `overridden` values. Rejected and withheld axes stay in the authoring receipt and never enter the domain pack or Story IR. A committed editor apply makes the consumed proposal stale by construction; dry-run returns the exact receipt without changing editor or filesystem state.

The checked [Wayfinder Compass corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/alignment) is original MIT-licensed material with five neutral axes and three synthetic calibration boundaries. Its review demonstrates every action: `horizon` is accepted, `reciprocity` edited, `signal` rejected, `structure` overridden, and `tempo` withheld. A contributor can publish a different pack by defining the same declarative fields and passing the public schema/calibration validator; no Character-core code change is required.

## Optional extension records

Every extension header declares a namespace, positive version, authority, rationale, value/review/lock/freshness states, sorted lineage, and the forbidden write-back policy.

| Typed v1 extension | Portable payload | Boundary |
|---|---|---|
| `identity_presentation` | Stable identity refs and safe project-relative presentation/asset refs | Presentation cannot infer personality, alignment, date context, or protected identity |
| `expression` | Normalized lexicon, preference, and behavioral-signature refs | Authored expression remains distinct from generated prose |
| `behavioral_signatures` | Stable cues and bounded strengths | Signatures are projections, not canonical trait measurements |
| `role_projections` | Accepted taxonomy, role, rationale, and exact input paths | Roles cannot become personality evidence |
| `relationships` | Stable source/target character ids, namespaced kind, confidence | An owning profile cannot create a self-edge or impersonate another source id |
| `alignment_view` | Exact pack/review/application fingerprints and approved pack-defined values | Rejected/withheld values stay in the receipt; labels are non-diagnostic and cannot mutate canon |
| `date_context` | Exact context pack/version/hash and accepted record ids | Context is an authoring cue, never causal personality evidence |
| `tabletop` | Preserved inactive portable payload | Ruleset adapters interpret a separately versioned contract |
| `opaque` | Preserved inactive portable payload | Unknown semantics are retained but never executed or written back |

Extension payloads use bounded, finite `DomainValue` trees. Asset paths are project-relative and reject absolute paths, empty segments, dot segments, traversal, and backslashes. Relationship targets, role taxonomies, expression references, and alignment views remain namespaced and independently inspectable.

## Templates, overlays, and deterministic synthesis

A `CharacterTemplate` is an immutable, semantically versioned input containing one valid profile. A `CharacterOverlay` contains an optional exact template reference, the final character id, independent provenance, and sparse typed operations. The full `CharacterSynthesisResult` retains the template and overlay separately, the effective profile, and a field-origin map.

Synthesis is deterministic:

1. Strictly validate the template and overlay without mutation.
2. Require the template id, semantic version, and SHA-256 to match exactly; otherwise report stale input.
3. Merge provenance only when source/transformation ids are identical or disjoint; conflicting reuse fails.
4. Require operations to be unique and ordered by target path then operation id.
5. For every present target, require its exact prior-value SHA-256 before replacement or removal.
6. Enforce lock and state precedence, then apply each operation atomically to an isolated clone.
7. Recompute the derived view and validate the complete effective profile.
8. Serialize the original template, overlay, effective profile, and exact origins as the reproducible proof.

Updating a template changes its fingerprint. The old overlay then reports `C107` and cannot silently acquire template changes or replace an authored override. Callers may preview the changed template and author a reviewed overlay migration with new expected fingerprints. Blank synthesis omits the template reference and requires one attributed display-name operation; all other canon remains explicitly absent until authored.

The closed v1 operation set covers display name, aliases, birth date, every factor/facet, inner-life records, voice records, suggestions, and extensions. No operation can set an OCEAN field. RON and JSON use the same operation enum and diagnostic record.

## Collections and reviewed corpus operations

`CharacterCollection` is a versioned, deterministic map from stable character id to independently valid profiles. Its revision increases only when an accepted operation changes at least one profile. `CharacterOperationRequest` pins the canonical input collection SHA-256, a deterministic seed, one explicit scope, provenance, and one action. The action vocabulary covers create, clone, revise, rename, import, and recompute; list, show, validate, propose, review, resume, and apply are shared operations over those artifacts.

Scopes select all profiles, a sorted exact id set, or a deterministic filter by id prefix and/or extension namespace. Create, clone, revise, rename, and import name their direct targets in the action. Recompute applies its scope and changes only derived compatibility views. It never writes inference into canon or replaces a locked authored value.

The reviewed workflow has four boundaries:

1. Propose validates the complete input and request, applies the action to an isolated clone, migrates every affected relationship reference, and emits a byte-stable `CharacterProposal`. The proposal includes exact input/request/output hashes, sorted target ids, fingerprinted changes, the complete candidate collection, and the original request needed for independent reproduction. The accepted collection is unchanged.
2. Resume advances over that same sorted target list. `CharacterJobProgress` stores only the collection, input and request hashes, total, cursor, completed id prefix, and state. It contains no character payload or private authoring prose. A proposal appears only when every target has been accounted for.
3. Review accepts or rejects the entire proposal hash. Partial reviews and partial application are unsupported.
4. Apply validates the review, verifies that the current collection still matches the pinned input, and independently reproduces the proposal. A stale, rejected, malformed, or differently fingerprinted artifact returns an error before state changes. A successful apply returns one complete replacement collection; the CLI persists it through an atomic same-directory rename, and the editor swaps its in-memory collection in one transition.

Changing a display name leaves stable ids and references intact. Changing a stable id previews both owned source references and incoming target references. A locked relationship extension stops the whole rename unless the request explicitly enables the locked override and records a rationale. Import validates every JSON or RON profile before mutation, requires unique sorted ids, is a no-op when repeated with identical content, and refuses conflicts unless `force` plus a rationale are present. The proposal and review retain that override decision.

The source-tooling API, `CharacterCorpusSession` editor service, and `weave-character` CLI call these same functions and return the same typed diagnostics. A CLI proposal is therefore byte-identical to the source/editor proposal for the same collection, request, seed, and contract versions.

## Diagnostics and failure policy

Failures identify a stable path and static redaction-safe message without echoing rejected values:

| Code | Family |
|---|---|
| `C100` | Unsupported core or typed-extension version |
| `C101` | Invalid stable identifier |
| `C102` | Invalid value, range, text, ordering, or review-state combination |
| `C103` | Missing, conflicting, unsorted, or undeclared lineage |
| `C104` | Invalid extension envelope or payload |
| `C105` | Duplicate target, missing prior fingerprint, or lower-precedence replacement |
| `C106` | Locked field replacement without an explicit override |
| `C107` | Stale template, value, or synthesis proof |
| `C108` | Broken id, relationship, or template reference |
| `C109` | Attempted derived/suggested/extension write-back into canon |
| `C110` | Malformed RON/JSON, unknown fields, or duplicate JSON keys |

Parsing, validation, and synthesis return no partial effective profile. File output uses an atomic same-directory temporary file.

## End-to-end module projection

The checked Ari Vale profile also passes through the ordinary domain-module boundary. `character_module_manifest()` returns the same declarative `ModuleManifest` used by World, and `character_domain_pack()` validates the complete `CharacterProfile` before projecting it into one finite `DomainValue` tree. Neither function parses `.weave`, depends on the editor, contacts a provider, or loads executable package code.

The runtime projection retains:

- the stable character id, attributed display name, and aliases;
- all six factor summaries and all 24 facets with input form, normalized projection score, confidence, state, review, lock, freshness, rationale, and lineage;
- the exact OCEAN algorithm, input paths, scores, confidence, `lossy: true`, `independent_evidence: false`, and omitted Honesty-Humility marker; and
- optional approved alignment shorthand with exact pack, review, application, coverage, decision, explanation, and canonical input-path records, `canonical_personality_write_back: false`, and no rejected/withheld values;
- optional reviewed date context with exact pack coordinates, accepted record ids, accepted/edited/overridden/auto-approved cues, relevance and uncertainty, the review hash, separate fact/cue source ids, and `canonical_personality_write_back: false`; and
- sorted profile source and transformation identifiers.

`projection_score` is the exact normalized score for a `score` input and only the documented compatibility anchor for a retained band input. It never replaces the original profile measurement. Missing factor or facet evidence remains an absent optional path.

The manifest declares `profile.hexaco`, `profile.ocean`, `profile.alignment`, `profile.date_context`, stable identity, contract version, and provenance paths read-only. The shared compiler rejects an override that targets, encloses, or descends from any such path. Authors revise canonical personality evidence in the profile artifact, run the validator/projector, and receive a pack whose OCEAN view was recomputed before compilation. A story or editor may still author permitted presentation-facing fields such as the attributed display-name value; that override remains separate in Story IR and cannot change the stable character id.

[`ari-vale.weave`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.weave) selects the reviewed Character domain pack through ordinary module syntax, then reads the exact alignment pack id, version, hash, typed factor/facet, OCEAN, and approved alignment paths. Raw alignment-pack selection and review occur at the authoring boundary in the shared Character CLI/editor workflow; source selects only the resulting reviewed Character pack, so an unreviewed proposal cannot enter narrative logic. The story compiles to byte-stable [JSON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.story.json) and [RON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.story.ron). The separate [reviewed temporal story](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave) exercises the temporal projection. Adjacent `weave.modules.json` and `weave.lock` files make both builds deterministic and offline. The editor exposes alignment pack/config selection, values, exact “why” traces, review/override, dry-run, atomic apply, stale detection, and refresh alongside the temporal workflow. The finite Bevy consumer loads approved alignment and temporal RON projections into ECS resources, while the PixiJS consumer decodes the JSON projections, excludes rejected/withheld axes, verifies both forbidden write-back markers, and preserves the Apollo 11 fact/cue lineage split without an editor dependency.

```weave
module character {
    id: "org.weave.character"
    version: "=1.2.0"
    pack: "ari_vale@=1.0.0"
}

VAR creativity = character.profile.hexaco.openness.creativity.projection_score
VAR ocean_openness = character.profile.ocean.openness.score
VAR alignment_pack = character.profile.alignment.pack.id
VAR alignment_pack_version = character.profile.alignment.pack.version
VAR alignment_pack_sha256 = character.profile.alignment.pack.sha256
VAR alignment_horizon = character.profile.alignment.values.horizon.label
```

## Canonical fixtures and commands

The public fixture is a wholly synthetic character named Ari Vale. It includes every factor and facet, a full date, attributed inner-life and voice records, every typed extension family, a pending suggestion, one locked field, a reviewed override, and an unknown opaque extension preserved at version 99. The collection fixtures add Sable Reed, a bidirectional relationship, an exact rename request, a payload-free interrupted cursor, the byte-stable proposal and review, and the atomically renamed result. The alignment fixtures add an original five-axis pack, exact provider hash, calibration boundaries, every review action, an independently reproducible receipt, and an approved-only profile. The context fixtures add three exact offline packs, selected/downgraded/skipped coverage, four decisions, an independently reproducible receipt, the enriched profile, and a separately locked temporal runtime story. Invalid fixtures cover unsupported versions, conflicting overlays, stale input/review/progress/alignment/temporal lineage, incomplete alignment/temporal reviews, a malformed proposal, a bad relationship, and derived canonical evidence.

```bash
cargo run -p weave-character --example character_fixture -- --check

cargo run -p weave-character -- schema profile \
  --output target/weave-character-profile-v1.schema.json

cargo run -p weave-character -- validate profile \
  examples/domain-modules/weave-character/profile.character.json

cargo run -p weave-character -- alignment-propose \
  examples/domain-modules/weave-character/alignment/input.character.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  --config examples/domain-modules/weave-character/alignment/selection.alignment-config.json \
  --seed 20260822 \
  --output target/proposal.alignment-proposal.json

cargo run -p weave-character -- alignment-review \
  target/proposal.alignment-proposal.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  examples/domain-modules/weave-character/alignment/decisions.alignment-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every original Wayfinder Compass axis independently; publish only approved fictional shorthand and never treat a label as diagnosis, moral rank, canonical evidence, or runtime authority." \
  --output target/review.alignment-review.json

cargo run -p weave-character -- alignment-apply \
  examples/domain-modules/weave-character/alignment/input.character.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  target/proposal.alignment-proposal.json \
  target/review.alignment-review.json \
  --dry-run

cargo run -p weave-character -- context-propose \
  examples/domain-modules/weave-character/context/input.character.json \
  --pack examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/calendar.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/world.temporal-pack.json \
  --config examples/domain-modules/weave-character/context/ranking.temporal-config.json \
  --seed 19690720 \
  --output target/proposal.temporal-proposal.json

cargo run -p weave-character -- context-review \
  target/proposal.temporal-proposal.json \
  examples/domain-modules/weave-character/context/decisions.temporal-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every ranked temporal cue, preserve fact and fictional-cue lineage separately, and keep all accepted material outside canon." \
  --output target/review.temporal-review.json

cargo run -p weave-character -- context-apply \
  examples/domain-modules/weave-character/context/input.character.json \
  --pack examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/calendar.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/world.temporal-pack.json \
  target/proposal.temporal-proposal.json \
  target/review.temporal-review.json \
  --dry-run

cargo run -p weave-character -- collection-list \
  examples/domain-modules/weave-character/operations/collection.character-collection.json

cargo run -p weave-character -- collection-propose \
  examples/domain-modules/weave-character/operations/collection.character-collection.json \
  examples/domain-modules/weave-character/operations/rename.character-request.json \
  --output target/rename.character-proposal.json

cargo run -p weave-character -- collection-review \
  target/rename.character-proposal.json \
  --decision accepted \
  --reviewer org.weave.reviewer.example \
  --rationale "Approve the complete reference-safe rename." \
  --output target/rename.character-review.json

cargo run -p weave-character -- collection-apply \
  examples/domain-modules/weave-character/operations/collection.character-collection.json \
  target/rename.character-proposal.json \
  target/rename.character-review.json \
  --dry-run

cargo run -p weave-character -- synthesize \
  examples/domain-modules/weave-character/overlay.character.json \
  --template examples/domain-modules/weave-character/template.character.json \
  --format json \
  --output target/synthesis.character.json

cargo run -p weave-character -- module-manifest \
  --format json \
  --output target/module.weave-module.json

cargo run -p weave-character -- domain-pack \
  examples/domain-modules/weave-character/profile.character.json \
  --id ari_vale \
  --version 1.0.0 \
  --title "Ari Vale Synthetic Character" \
  --format json \
  --output target/ari_vale.weave-domain.json

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-character/ari-vale.weave \
  --locked \
  --format json \
  --output target/ari-vale.story.json

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave \
  --locked \
  --format json \
  --output target/ari-vale-temporal.story.json
```

Canonical JSON and RON pairs are semantically equal and byte-stable. The fixture generator rebuilds all valid, invalid, schema, template, overlay, synthesis, alignment, temporal, module-manifest, and domain-pack artifacts without network access. The documentation gate additionally reproduces alignment and temporal proposal/review/receipt bytes, checks both dry-run boundaries, verifies approved-only Story IR, recompiles both locked stories, and tests both engine consumers.
