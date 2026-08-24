# Weave Character Profile contract

Weave Character v1 is a portable, data-only contract for fictional character canon, evidence, projections, suggestions, and deterministic template synthesis. It is not a personality test, clinical instrument, hiring tool, or claim about a real person. The contract does not copy or administer questionnaire items. It uses the public six-factor and 24-facet vocabulary documented by the [HEXACO-PI-R authors](https://hexaco.org/scaledescriptions); the authors' [inventory history](https://hexaco.org/history) distinguishes those 24 factor-specific facets from later interstitial scales. The synthetic fixture values, identities, prose, algorithms, extension examples, and expected outputs are original MIT-licensed Weave material.

The Rust authority is `weave-character`. Download the generated [profile schema](downloads/weave-character-profile-v1.schema.json), [template schema](downloads/weave-character-template-v1.schema.json), [overlay schema](downloads/weave-character-overlay-v1.schema.json), [synthesis schema](downloads/weave-character-synthesis-v1.schema.json), [diagnostic schema](downloads/weave-character-diagnostic-v1.schema.json), [collection schema](downloads/weave-character-collection-v1.schema.json), [operation-request schema](downloads/weave-character-operation-request-v1.schema.json), [proposal schema](downloads/weave-character-proposal-v1.schema.json), [review schema](downloads/weave-character-review-v1.schema.json), and [progress schema](downloads/weave-character-progress-v1.schema.json).

The reviewed date-context workflow has separate generated schemas for its [offline pack](downloads/weave-character-temporal-pack-v1.schema.json), [ranking configuration](downloads/weave-character-temporal-config-v1.schema.json), [immutable proposal](downloads/weave-character-temporal-proposal-v1.schema.json), [complete review](downloads/weave-character-temporal-review-v1.schema.json), and [reproducible receipt](downloads/weave-character-temporal-receipt-v1.schema.json).

Explainable alignment views have separate generated schemas for the [declarative pack](downloads/weave-character-alignment-pack-v1.schema.json), [project selection](downloads/weave-character-alignment-config-v1.schema.json), [immutable proposal](downloads/weave-character-alignment-proposal-v1.schema.json), [complete review](downloads/weave-character-alignment-review-v1.schema.json), and [independently reproducible receipt](downloads/weave-character-alignment-receipt-v1.schema.json).

Guided authoring adds generated schemas for the [workspace](downloads/weave-character-authoring-workspace-v1.schema.json), [revision](downloads/weave-character-authoring-revision-v1.schema.json), [pre-apply preview](downloads/weave-character-authoring-preview-v1.schema.json), [original questionnaire pack](downloads/weave-character-questionnaire-pack-v1.schema.json), [answers](downloads/weave-character-questionnaire-answers-v1.schema.json), [proposal](downloads/weave-character-questionnaire-proposal-v1.schema.json), [review](downloads/weave-character-questionnaire-review-v1.schema.json), [receipt](downloads/weave-character-questionnaire-receipt-v1.schema.json), and [final review](downloads/weave-character-final-review-v1.schema.json).

Typed presentation authoring has generated schemas for the immutable [catalog](downloads/weave-character-presentation-catalog-v1.schema.json), exact [allocation request](downloads/weave-character-presentation-request-v1.schema.json), transparent [proposal](downloads/weave-character-presentation-proposal-v1.schema.json), complete [review](downloads/weave-character-presentation-review-v1.schema.json), replayable [receipt](downloads/weave-character-presentation-receipt-v1.schema.json), and portable [lock revision](downloads/weave-character-presentation-lock-revision-v1.schema.json).

Relationship graphs have separate generated schemas for the immutable [kind pack](downloads/weave-character-relationship-kind-pack-v1.schema.json), project [graph policy](downloads/weave-character-relationship-policy-v1.schema.json), deterministic [proposal configuration](downloads/weave-character-relationship-config-v1.schema.json), immutable [proposal](downloads/weave-character-relationship-proposal-v1.schema.json), complete [review](downloads/weave-character-relationship-review-v1.schema.json), replayable [receipt](downloads/weave-character-relationship-receipt-v1.schema.json), direct authored/imported [revision](downloads/weave-character-relationship-revision-v1.schema.json), and read-only [reconciliation report](downloads/weave-character-relationship-reconciliation-v1.schema.json).

Reusable expression and dialogue have generated schemas for the immutable [expression pack](downloads/weave-character-expression-pack-v1.schema.json), normalized [direct revision](downloads/weave-character-expression-revision-v1.schema.json), exact [assignment request](downloads/weave-character-expression-assignment-request-v1.schema.json), replayable [assignment receipt](downloads/weave-character-expression-assignment-receipt-v1.schema.json), typed [resolution request](downloads/weave-character-expression-resolution-request-v1.schema.json), deterministic [resolution](downloads/weave-character-expression-resolution-v1.schema.json), non-mutating [lint report](downloads/weave-character-expression-lint-v1.schema.json), and [coverage report](downloads/weave-character-expression-coverage-v1.schema.json).

Provider-neutral assistance has generated schemas for its versioned [template](downloads/weave-character-assistance-template-v1.schema.json), pinned [request](downloads/weave-character-assistance-request-v1.schema.json), credential-free [preview](downloads/weave-character-assistance-preview-v1.schema.json), explicit [approval](downloads/weave-character-assistance-approval-v1.schema.json), strict [provider response](downloads/weave-character-assistance-provider-response-v1.schema.json), immutable [candidate set](downloads/weave-character-assistance-candidate-set-v1.schema.json), [advisory review](downloads/weave-character-assistance-advisory-review-v1.schema.json), complete [author decision review](downloads/weave-character-assistance-decision-review-v1.schema.json), reproducible [receipt](downloads/weave-character-assistance-receipt-v1.schema.json), [batch request](downloads/weave-character-assistance-batch-request-v1.schema.json), complete [batch preview](downloads/weave-character-assistance-batch-preview-v1.schema.json), resumable [job](downloads/weave-character-assistance-job-v1.schema.json), atomic [batch receipt](downloads/weave-character-assistance-batch-receipt-v1.schema.json), and advisory-only [provider comparison](downloads/weave-character-assistance-comparison-v1.schema.json).

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

## Explainable classification, vocation, and roles

Projection packs turn already-authored canonical HEXACO evidence into optional categorical displays and fictional role prompts. They do not add evidence, infer canon, measure aptitude, recommend employment, predict conduct, diagnose a person, or define a normative distribution. Categorical personality labels are always `lossy: true`, `derived`, and `independent_evidence: false`. Vocation, social-role, and narrative-role candidates remain unpublished suggestions until an author explicitly accepts, edits, or overrides them.

`ProjectionPack` is independently distributable data with a stable id/version, compatible profile versions, title/description, license and public provenance, methodology and limitations, a declared canonical input map, taxonomies, entries, eligibility/exclusion rules, signed weights, score thresholds, default capacities, and complete synthetic calibration vectors. V1 inputs are restricted to exact canonical HEXACO factor/facet paths. Pack validation rejects an undeclared input, mismatched trait/path pair, empty evidence vector, zero weight, invalid threshold/capacity, unsorted eligibility set, unsupported profile version, inconsistent output id/kind/lossiness, or failed calibration.

Scoring uses fixed-point integer arithmetic:

1. Convert each present score to millionths; a retained band uses its documented v1 projection anchor.
2. Center it as `2 × value_micros − 1000000`.
3. Multiply by the entry's signed thousandth weight and sum.
4. Divide covered absolute weight by total absolute weight for coverage in millionths.
5. Divide the signed sum by covered absolute weight with half-away-from-zero rounding.
6. Require character eligibility, no exclusion, the maximum of pack/project coverage floors, and the entry's inclusive minimum score.

Every missing input stays visible in `ordered_evidence` with its declared path/trait/weight and no value, contribution, or invented lineage. Candidates rank by score descending, then a lowercase SHA-256 over the exact pack coordinate, configuration hash, character/taxonomy/entry ids, seed, and evidence trace, then stable id. A changed seed changes deterministic tie order and trace identity, never the score formula.

`ProjectionConfig` selects sorted taxonomies and an exact eligible character pool, project coverage floor, fill-missing or rebalance mode, per-entry capacity overrides, and sorted one-slot reservations. Proposal operates across the whole collection. It retains authored, editorially overridden, extension-locked, or value-locked assignments; applies valid reservations first; then fills remaining capacity in deterministic global rank order. Rebalance can reconsider only unlocked `reviewed`, `edited`, or `derived` values. It cannot displace authored, overridden, or locked values. Distribution output records per-taxonomy/entry counts plus retained, reserved, and unavailable slots.

`ProjectionProposal` embeds the exact collection, collection SHA-256, complete pack and pack coordinate, complete config and SHA-256, seed, every candidate trace, selected disposition, distribution, and exact review manifest. The same inputs reproduce the same bytes. `ProjectionReview` must cover every proposed/reserved manifest slot exactly once:

| Decision | Published result | Required author record |
|---|---|---|
| `accept` | Proposed pack entry; categorical views become `derived`, roles become `reviewed` | Lock plus optional rationale |
| `edit` | Another qualified declared pack entry as `edited` | Entry id, lock, rationale |
| `override` | Explicit editorial entry/label as `overridden`, without a claimed pack score | Entry id, label, lock, rationale |
| `reject` | No public value | Rationale retained in review/receipt |
| `withhold` | No public value | Rationale retained in review/receipt |

Apply replays the entire proposal and review against the current collection before returning one atomic receipt/output collection. Approved values retain kind, taxonomy/output/entry ids, label, lossiness, decision, optional score, coverage, explanation, rationale, exact canonical input paths, pack id/version/SHA-256, proposal SHA-256, review SHA-256, and lock. A separate fingerprinted lock revision changes only selected locks. The implementation snapshots canon, derived OCEAN, suggestions, and every non-projection extension before apply; any attempted change to HEXACO, OCEAN, alignment, identity, birth, relationships, expression, temporal context, presentation, or ruleset-owned data fails.

The JSON/RON CLI and `ProjectionSession` editor service call the same host-independent functions. `projection-propose` selects a pack/config/seed, `projection-inspect` returns the complete evidence/threshold/capacity/tie trace, `projection-review` validates a complete decision source document, `projection-apply` supports dry-run or atomic output plus a receipt, and `projection-lock` validates or commits a lock revision. Editor propose, inspect, review, dry-run/commit, lock, config, seed, and rebalance produce the same artifacts. `.weave` source reads only already-approved domain-pack values; it cannot bypass the review by overriding `profile.projections`.

The original MIT-licensed [Glasswind Lenses fixtures](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/projections) exercise three characters, four taxonomies, signed weights, two calibration vectors, explicit eligibility, capacity one per entry, one reservation, deterministic batch allocation, all five review decisions, a locked accepted value, an editorial override, unlock, and rebalance. The domain projection exposes approved values through typed `profile.projections.values` and exact `source_packs`. Its `write_back` object contains all-false `hexaco`, `ocean`, `alignment`, `identity`, `birth`, `relationships`, and `ruleset` fields. The checked Bevy and PixiJS consumers validate those labels, explanations, rationales, input paths, fingerprints, locks, and authority markers directly from Story IR.

## Provider-neutral assisted Character development

Assistance is an optional authoring workflow for biographies, motivations, fears, guarded truths, dramatic tensions, narrative hooks, presentation cues, role ideas, relationship cues, contextual reactions, and expression examples. It is complete without any service: the built-in offline provider produces deterministic, inspectable scaffolds from the exact approved inputs. The core crate defines a small adapter trait but contains no network client or provider SDK, so a host can add an adapter without changing the artifacts or review rules.

An `AssistanceTemplate` pins its identity, semantic version, hashable instructions, required inputs, target paths, text and candidate bounds, offline scaffold version, safety contract, license, and provenance. An `AssistanceRequest` additionally pins the exact profile fingerprint, provider adapter/version/engine/mode, requested fields, disclosed input vocabulary, settings, optional seed, generation number, and provenance. Provider parameters are ordinary bounded public settings; credentials are never part of the request.

Preview is the mandatory privacy boundary. It renders the exact provider payload, every included structured value, known character ids, serialized character count, and expected response contract while recording `provider_called: false` and `credential_value_stored: false`. Execution requires a separately authored approval whose SHA-256 references that exact preview. An adapter obtains any credential through a host-owned secret channel and reports only `not_required`, `configured`, or `missing`. Secret formatting replaces the whole value with exactly `[REDACTED]`; no request, cache, artifact, error, or comparison contains credential material.

Responses pass through a strict, unknown-field-denying JSON contract and normalize into typed immutable candidates. Validation enforces field/template agreement, target paths, candidate counts, public-text bounds, placeholder and credential-shape rejection, control-character rejection, known relationship references, disclosed-input evidence hashes, and a complete safety receipt. Malformed output and provider failures expose only stable codes and static safe messages; raw response bodies are not retained.

Candidate generation and judgment remain separate. A deterministic offline advisory may attach relevance, consistency, and safety scores in integer millionths, structured issues, or a safe proposed edit, but it is marked advisory-only and cannot rank a winner or write any value. The author must explicitly accept, edit, reject, defer, or request regeneration for every candidate in the exact set. A complete decision review fingerprints both the set and input profile. Accepted or edited values become pending `CharacterSuggestion` records only; canon, extensions, derived values, and runtime domain-pack exports remain unchanged. Apply is independently reproducible, idempotent for its exact output, and fails closed when the profile, template, preview, approval, candidate set, or review is stale.

Batch requests reuse the same per-profile contract with explicit id/prefix/missing-suggestion filters, deterministic target ordering, hard character/call/candidate/input-character budgets, bounded retry codes, a scheduling-window rate limit, an optional job-local payload cache, and an exact collection fingerprint. Jobs serialize their cursor, counters, completed prefix, safe failures, candidate sets, and cancellation rationale, so they can stop and resume without duplicating accepted suggestions. Atomic batch apply requires complete reviews for every successful target. Provider comparison aligns candidate sets for one exact profile by provider coordinate and candidate id; it exposes advisory evidence without selecting or applying a winner.

The CLI follows the same boundary. This complete credential-free offline path reproduces the checked fixture:

```bash
cargo run -p weave-character -- assistance-preview \
  examples/domain-modules/weave-character/assistance/input.character-collection.json \
  examples/domain-modules/weave-character/assistance/glasswind.assistance-template.json \
  examples/domain-modules/weave-character/assistance/single.assistance-request.json \
  --output target/single.assistance-preview.json

cargo run -p weave-character -- assistance-approve \
  target/single.assistance-preview.json \
  --author org.weave.reviewer.fixture \
  --rationale "Approve the exact visible offline scope for this original synthetic fixture." \
  --output target/single.assistance-approval.json

cargo run -p weave-character -- assistance-generate-offline \
  examples/domain-modules/weave-character/assistance/input.character-collection.json \
  examples/domain-modules/weave-character/assistance/glasswind.assistance-template.json \
  target/single.assistance-preview.json \
  target/single.assistance-approval.json \
  --output target/single.assistance-candidate-set.json

cargo run -p weave-character -- assistance-advise \
  target/single.assistance-candidate-set.json \
  --reviewer org.weave.reviewer.advisory \
  --output target/single.assistance-advisory-review.json

cargo run -p weave-character -- assistance-review \
  target/single.assistance-candidate-set.json \
  examples/domain-modules/weave-character/assistance/single.assistance-decisions.json \
  --author org.weave.reviewer.fixture \
  --rationale "Exercise accept, edit, reject, defer, and regenerate as explicit fixture decisions." \
  --output target/single.assistance-decision-review.json

cargo run -p weave-character -- assistance-apply \
  examples/domain-modules/weave-character/profile.character.json \
  target/single.assistance-candidate-set.json \
  target/single.assistance-decision-review.json \
  --advisory target/single.assistance-advisory-review.json \
  --dry-run \
  --receipt-output target/single.assistance-receipt.json \
  --profile-output target/single.applied-character.json
```

`assistance-inspect` exposes one exact candidate and its evidence, `assistance-compare` builds the advisory-only provider view, and `assistance-batch-*` previews, approves, starts, resumes, cancels, and atomically applies bounded jobs. The editor's `CharacterAssistanceSession` calls these same library functions for preview, approval, adapter execution, evidence inspection, advisory review, complete author decisions, dry-run, and atomic commit. The original MIT-licensed [assistance fixture guide](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/assistance) documents every JSON/RON artifact and batch command.

## Optional extension records

Every extension header declares a namespace, positive version, authority, rationale, value/review/lock/freshness states, sorted lineage, and the forbidden write-back policy.

| Typed v1 extension | Portable payload | Boundary |
|---|---|---|
| `identity_presentation` | Stable identity refs and safe project-relative presentation/asset refs | Presentation cannot infer personality, alignment, date context, or protected identity |
| `expression` | Normalized lexicon, preference, and behavioral-signature refs | Authored expression remains distinct from generated prose |
| `behavioral_signatures` | Stable cues and bounded strengths | Signatures are projections, not canonical trait measurements |
| `role_projections` | Approved categorical/vocation/social/narrative values with exact pack, proposal, review, explanation, rationale, input-path, decision, and lock lineage | Categorical views remain lossy derived displays; roles cannot become personality evidence or write into another extension |
| `relationships` | Pack-pinned, layered edges with stable endpoints, kind, review, lock, validity, consent, evidence, rationale, and lineage | The pack controls self edges, direction, inverse semantics, multiplicity, and required metadata; an owning profile cannot impersonate another source id or write into canon |
| `alignment_view` | Exact pack/review/application fingerprints and approved pack-defined values | Rejected/withheld values stay in the receipt; labels are non-diagnostic and cannot mutate canon |
| `date_context` | Exact context pack/version/hash and accepted record ids | Context is an authoring cue, never causal personality evidence |
| `tabletop` | Preserved inactive portable payload | The separately versioned [tabletop adapter contract](tabletop_adapters.md) owns capabilities, definition, state, events, and switching; Character canon remains read-only |
| `opaque` | Preserved inactive portable payload | Unknown semantics are retained but never executed or written back |

Extension payloads use bounded, finite `DomainValue` trees. Asset paths are project-relative and reject absolute paths, empty segments, dot segments, traversal, and backslashes. Relationship targets, role taxonomies, expression references, and alignment views remain namespaced and independently inspectable.

## Reusable expression and dialogue

The expression extension is a character-linked authoring layer, never a generator and never new canonical personality evidence. Its normalized records cover lexicon terms and phrases, categorized prefer/avoid values, reusable vocabulary pools, behavioral signatures, speech/writing constraints, and reviewed template assignments. Every record keeps a stable id, exact character id, category where applicable, origin, review state, source ids, typed applicability, and an optional rationale. `canonical_personality_write_back` remains false in the runtime projection.

Applicability is closed and provider-free. A record or dialogue variant may name scenario ids and require all of a sorted set of typed predicates: a present HEXACO factor or facet band, an exact relationship kind and optional other character, an accepted date-context cue id, or a World-context tag. Missing context makes the predicate inapplicable; it never invents a value. Direct revisions add, edit, or remove records through one profile-fingerprinted atomic transition. Normalization collapses authored whitespace, creates lowercase comparison forms, sorts/deduplicates ids and predicates, and canonicalizes mutation order without mutating the input artifact.

An `ExpressionPack` is immutable, independently distributable data. It declares a semantic coordinate, license, provider provenance, eligibility, limitations, assignable records, vocabulary pools, dialogue templates, and minimum category/scenario coverage. An assignment request pins the exact profile and pack SHA-256, selected entry/pool/template ids, reviewer, rationale, seed, and provenance. Apply independently validates and reproduces the request, then returns a receipt containing the complete input, exact pack, request, and output. A stale profile or pack, ineligible profile version, missing link, or incomplete selection fails before mutation.

Dialogue templates have one assigned speaker rule, a stable scenario, a declared fallback, and a closed placeholder map. Runtime values can come only from speaker display name or subject pronoun, listener display name, an exact relationship kind, a supplied date label, a supplied World place name, or one explicitly linked lexicon term. Arbitrary variables, environment values, credentials, hidden services, model calls, and network lookup are unavailable. Every variant retains a source id, line, and column so diagnostics can identify authoring locations without echoing rejected text.

Resolution is deterministic and offline. Unreviewed suggestions are excluded. Among applicable variants whose placeholders are available, authored text outranks accepted reviewed text, then declared priority and a request-seeded SHA-256 tie-breaker establish stable order. If no contextual variant remains, the required authored or accepted context-free fallback is used. The resolution preserves exact profile/pack/request fingerprints, substitutions, rendered text, and a sorted trace for every candidate. Lint and coverage are read-only: lint reports conflicting constraints, unreviewed suggestions, invalid links/placeholders, exact and near duplicates, and unsafe text structure; coverage reports record/category counts, assigned templates, reviewed variants, and fallback availability against every exact assigned pack.

The checked [Glasswind expression corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/expression) is original MIT-licensed synthetic material. It includes direct normalization, explicit assignment, every expression record family, personality/relationship/date/World predicates, a contextual authored winner, a stable fallback, clean lint and coverage, and a deliberately restricted placeholder with a source-located diagnostic. The corpus is generated and verified without network access.

### Expression CLI workflow

The same functions back the `weave-character` CLI and the editor's `ExpressionSession`. List/show filters are deterministic; revise and assign support dry-run; committed writes use atomic same-directory replacement.

```bash
cargo run -p weave-character -- expression-normalize \
  examples/domain-modules/weave-character/expression/raw.expression-revision.json \
  --output target/normalized.expression-revision.json

cargo run -p weave-character -- expression-revise \
  examples/domain-modules/weave-character/omitted-extensions.character.json \
  examples/domain-modules/weave-character/expression/normalized.expression-revision.json \
  --output target/revised.character.json

cargo run -p weave-character -- expression-assign \
  examples/domain-modules/weave-character/expression/revised.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/assignment.expression-request.json \
  --receipt-output target/assignment.expression-receipt.json \
  --profile-output target/applied.character.json

cargo run -p weave-character -- expression-lint \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-coverage \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-resolve \
  examples/domain-modules/weave-character/expression/applied.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json \
  --output target/contextual.expression-resolution.json

cargo run -p weave-character -- projection-propose \
  examples/domain-modules/weave-character/projections/input.character-collection.json \
  examples/domain-modules/weave-character/projections/glasswind.projection-pack.json \
  examples/domain-modules/weave-character/projections/selection.projection-config.json \
  --seed 20260824 \
  --output target/proposal.projection-proposal.json

cargo run -p weave-character -- projection-inspect \
  target/proposal.projection-proposal.json \
  org.weave.character.ari_vale \
  org.weave.projection.glasswind_lenses.narrative_role

cargo run -p weave-character -- projection-review \
  target/proposal.projection-proposal.json \
  examples/domain-modules/weave-character/projections/decisions.projection-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every original Glasswind Lenses display and role independently; retain only explicit fictional editorial choices outside canon." \
  --output target/review.projection-review.json

cargo run -p weave-character -- projection-apply \
  examples/domain-modules/weave-character/projections/input.character-collection.json \
  target/proposal.projection-proposal.json \
  target/review.projection-review.json \
  --dry-run \
  --receipt-output target/receipt.projection-receipt.json
```

## Provenance-aware relationship graphs

A `RelationshipKindPack` is immutable data, independently authored, and licensed under MIT, Apache-2.0, or CC0-1.0. Each kind declares its family, whether it is directed, symmetric, or paired with an exact inverse kind, whether self edges and concurrent multiples are allowed, any pedigree semantics, required metadata, and explicit limitations. The graph pins the pack id, semantic version, and SHA-256. No label is interpreted by name: inverse, kinship, partnership, consent, and multiplicity behavior comes only from the selected pack.

Every edge retains a stable owner/source/target coordinate and one explicit authority layer: `authored`, `imported`, `computed_affinity`, `suggested_narrative`, or `reviewed_suggestion`. Review, lock, freshness, optional validity dates, inverse id, notes, consent, safeguard exceptions, score, ordered evidence contributions, lineage, and rationale remain separate fields. A computed score is an inspectable authoring aid, not canonical evidence or objective interpersonal truth. It cannot mutate personality, identity, presentation, alignment, date context, tabletop state, or any other extension.

A `RelationshipProposalConfig` pins the complete input collection hash, exact sorted roster, reference date, seed, kind-pack fingerprint, ordered targets, ordered evidence rules, pair-specific consent records, project safeguards, and provenance. The provider-free scorer accepts only four declared evidence families:

- similarity between explicitly present canonical trait measurements;
- exact overlap of reviewed normalized preferences in one namespaced category;
- an exact context reference retained by both profiles; and
- an existing authored, imported, or reviewed-canon relationship of one selected kind.

Every contribution retains its input paths, input SHA-256, availability, fixed-point contribution, and explanation. Unavailable evidence contributes zero. The proposal embeds its complete collection, pack, config, per-profile hashes, deterministic ranking trace, disposition, safeguards, candidate edges, and distribution, making repeated JSON or RON generation byte-stable and independently replayable.

Every candidate requires exactly one `accept`, `edit`, `override`, `exception`, `reject`, or `withhold` decision. An edit may change metadata but not candidate identity or topology. An override remains explicit. An exception must name every blocking age, kinship, partnership, or consent safeguard and is allowed only when project policy enables reviewed exceptions. Reject and withhold publish no edge but remain in the review and receipt with rationale. Apply revalidates and reproduces the proposal and complete review against the unchanged input before returning one atomic collection transition. Dry-run returns the same receipt without changing editor or source state.

Validation and reconciliation share stable, redaction-safe codes:

| Code | Meaning |
|---|---|
| `R100` | Missing source or target character |
| `R101` | Self edge forbidden by the selected kind |
| `R102` | Missing, mismatched, or broken inverse edge |
| `R103` | Invalid or reversed validity dates |
| `R104` | Contradictory pedigree or ancestry cycle |
| `R105` | Stale pack, graph, edge, proposal, review, or evidence fingerprint |
| `R106` | Duplicate edge or forbidden concurrent multiplicity |
| `R107` | Minimum partnership-age safeguard |
| `R108` | Close-kin partnership safeguard |
| `R109` | Concurrent-partnership safeguard |
| `R110` | Affirmative-consent safeguard |
| `R111` | Missing or inconsistent kind semantics |
| `R112` | Invalid edge or required metadata |
| `R113` | Locked edge changed without a deliberate override |

Reconciliation never edits the graph. It returns sorted diagnostics plus fingerprinted suggestions to add a missing inverse, remove a duplicate, or review a safeguard. Applying a repair requires a separate reviewed revision. Row-oriented and dense-matrix CSV exports begin with `review_only`, neutralize spreadsheet formulas, and are never accepted as graph input.

### Relationship CLI workflow

The checked [relationship corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/relationships) is original MIT-licensed synthetic material. It includes three adult fictional characters; directed, symmetric, and inverse-paired kinds; authored and imported edges; all four scoring inputs; computed and suggested layers; all six review outcomes; explicit safeguards; conflict diagnostics; review-only CSV; and byte-identical JSON/RON artifacts.

```bash
cargo run -p weave-character -- relationship-revise \
  examples/domain-modules/weave-character/relationships/blank.character-collection.json \
  examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json \
  examples/domain-modules/weave-character/relationships/authored.relationship-revision.json \
  --dry-run

cargo run -p weave-character -- relationship-propose \
  examples/domain-modules/weave-character/relationships/input.character-collection.json \
  examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json \
  examples/domain-modules/weave-character/relationships/scoring.relationship-config.json \
  --output target/proposal.relationship-proposal.json

cargo run -p weave-character -- relationship-review \
  target/proposal.relationship-proposal.json \
  examples/domain-modules/weave-character/relationships/decisions.relationship-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every deterministic candidate, preserve every rationale, and publish only explicit human decisions." \
  --output target/review.relationship-review.json

cargo run -p weave-character -- relationship-apply \
  examples/domain-modules/weave-character/relationships/input.character-collection.json \
  target/proposal.relationship-proposal.json \
  target/review.relationship-review.json \
  --dry-run \
  --receipt-output target/receipt.relationship-receipt.json \
  --collection-output target/applied.character-collection.json

cargo run -p weave-character -- relationship-list \
  examples/domain-modules/weave-character/relationships/applied.character-collection.json \
  examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json \
  examples/domain-modules/weave-character/relationships/project.relationship-policy.json \
  --format edge-csv \
  --output target/relationships.review.csv

cargo run -p weave-character -- relationship-reconcile \
  examples/domain-modules/weave-character/relationships/conflicted.character-collection.json \
  examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json \
  examples/domain-modules/weave-character/relationships/project.relationship-policy.json \
  --output target/reconciliation.relationship-reconciliation.json
```

## Typed identity and presentation authoring

Stable character id, display name, and aliases remain explicit canon. The optional `org.weave.character.identity_presentation` extension holds attributed pronouns; authored `origin` or `context` notes; namespaced appearance descriptors; a named palette; sorted style tags; portable avatar, portrait, sprite, model, or illustration references; and reviewed catalog assignments. Pronoun forms are free authored text rather than a closed identity taxonomy. Every asset uses a safe project-relative path, declared media type, accessible alternative text, and optional SHA-256. Palette values use canonical uppercase `#RRGGBB` or `#RRGGBBAA` sRGB notation.

The same sparse operation vocabulary is serialized in JSON and RON, used by source tooling, and called directly by the editor and CLI. It includes `set_pronouns`, `upsert_identity_context_note`, `upsert_appearance_descriptor`, `set_presentation_palette`, `set_presentation_style_tags`, `upsert_presentation_asset`, and their clear/remove counterparts. Catalog application adds `upsert_presentation_assignment` operations containing the exact catalog id, semantic version, SHA-256, entry, value, proposal hash, review hash, attribution state, and lock. A blank character has no template and only the explicitly authored operations; a template-backed or cloned character retains the immutable template separately and stores only its chosen overrides.

Every inspected effective field has one explicit `origin_kind`:

| Origin | Meaning |
|---|---|
| `template` | The value is inherited from the exact original template coordinate |
| `authored_override` | A sparse author/editor/source operation replaces or adds the value |
| `accepted_suggestion` | A validated questionnaire or presentation receipt supplied the reviewed value |
| `template_migration` | The value is inherited from a later explicitly reviewed template release |

Template migration records the prior and new id/version/SHA-256 coordinates, applied draft revision, reviewer, and rationale. Preview rebases a clone, reports the effective field diff and stale downstream views, and retains every sparse override. It never edits the prior template, mutates the workspace, or silently replaces an override.

### Deterministic presentation catalogs

A `PresentationCatalog` is immutable, data-only, independently authored, and licensed under MIT, Apache-2.0, or CC0-1.0 with machine-readable provenance. It declares closed slot value kinds (`appearance`, `palette_color`, `style_tag`, or `asset`), entries, exact eligible character ids and/or stable id prefixes, explicit exclusions, and optional capacities. Eligibility never examines the profile. In particular, the allocator cannot read or modify personality evidence, alignment, birth context, ruleset state, protected identity characteristics, display name, aliases, pronouns, or context prose.

An allocation request pins the input collection SHA-256, catalog id/version/SHA-256, finite sorted character and slot ids, available asset paths, seed, and mode. `fill_missing` retains every existing assignment. `rebalance` reconsiders only reviewed, unlocked assignments; locked assignments and authored or overridden values are retained. For each open slot the allocator filters declared eligibility/exclusion, available asset inventory, and capacity, chooses among the least-used entries, and uses a SHA-256 of the pinned seed and exact coordinates only as a deterministic tie-break. The proposal exposes every candidate's eligibility, exclusion, prior usage, capacity, capacity availability, seed hash, and selected flag, plus the resulting distribution.

Every proposed character/slot pair requires one `accept`, `edit`, `override`, `reject`, or `withhold` decision. An edit must name another eligible, compatible entry in the same catalog. An override must use the slot's closed value kind and records author rationale without pretending to be a catalog entry. Apply independently reproduces the proposal and review, generates sparse operations, and verifies that canon, derived personality, suggestions, and every other extension are byte-semantically unchanged before returning an atomic collection receipt. Propose and review are always dry-runs. `presentation-apply --dry-run` writes the exact reproducible receipt but never a collection; `presentation-lock --dry-run` writes nothing. Lock and unlock revisions pin the current collection hash and exact character/slot targets. A later rebalance cannot move a lock.

Validation fails before application with a stable diagnostic path and redaction-safe message for a missing catalog asset, invalid or duplicate palette slot, noncanonical color, duplicate alias, broken template coordinate, stale input, incomplete review, ineligible edit, incompatible override kind, unsafe asset path, duplicate operation target, or lower-precedence/locked replacement. No rejected value is echoed in the diagnostic.

### Presentation CLI workflow

The checked [Glasswind presentation corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/presentation) is original MIT-licensed synthetic material. Its two SVG avatars, catalog prose, appearance descriptors, palette, style tags, names, decisions, and expected outputs were authored for Weave. These commands reproduce its JSON proposal, review, receipt, and applied collection exactly; using the paired RON inputs with `--format ron` reproduces the RON artifacts.

```bash
cargo run -p weave-character -- presentation-propose \
  examples/domain-modules/weave-character/presentation/input.character-collection.json \
  examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.json \
  examples/domain-modules/weave-character/presentation/allocation.presentation-request.json \
  --output target/proposal.presentation-proposal.json

cargo run -p weave-character -- presentation-review \
  target/proposal.presentation-proposal.json \
  examples/domain-modules/weave-character/presentation/decisions.presentation-review.json \
  --reviewer org.weave.reviewer.presentation_fixture \
  --rationale "Review every synthetic presentation allocation; retain catalog coordinates, balance traces, locks, and explicit override rationale." \
  --output target/review.presentation-review.json

cargo run -p weave-character -- presentation-apply \
  examples/domain-modules/weave-character/presentation/input.character-collection.json \
  target/proposal.presentation-proposal.json \
  target/review.presentation-review.json \
  --receipt-output target/receipt.presentation-receipt.json \
  --collection-output target/applied.character-collection.json

cargo run -p weave-character -- presentation-lock \
  examples/domain-modules/weave-character/presentation/applied.character-collection.json \
  examples/domain-modules/weave-character/presentation/unlock-avatar.presentation-lock-revision.json \
  --dry-run
```

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

## Guided authoring and conflict review

`CharacterAuthoringWorkspace` is a reopenable project artifact, not another profile format. Each
draft retains an optional exact `CharacterTemplate`, one sparse `CharacterOverlay`, immutable
questionnaire receipts, blocking invalidations, and an optional `CharacterFinalReview`.
`synthesize_character` remains the only way to obtain the effective profile. Consequently direct
editing, concise picker choices, imported fields, questionnaire results, the editor, and the CLI
all converge on the same attributed values, lock/precedence checks, derived recomputation, and
complete profile validator.

Template availability and authoring access are ordinary project data. The workspace has no field
for an account, purchase, subscription, tier, provider, or service entitlement. A bundled template
must have an exact semantic version, SHA-256 coordinate, validated provenance, and an allowed
MIT/Apache-2.0/CC0-1.0 source license. The checked bundled templates are original MIT material.

Every editor control has one persisted source representation:

| Authoring control | Source representation | Authority boundary |
|---|---|---|
| Identity and aliases | `set_display_name` / `set_aliases` overlay operation | Canonical attributed text |
| Birth date | `set_birth_date` with explicit year, month-day, or full precision | Omission represents unknown components |
| Direct 24-facet editor | `set_hexaco_trait` | Canonical attributed measurement and confidence |
| Concise picker | The same `set_hexaco_trait`, using a closed `TraitBand` | No separate preset authority |
| Imported profile field | The same canonical action with `state: imported` | Import mode cannot masquerade as authored input |
| Behavior questionnaire | Pack + answers + immutable proposal | Suggestions only; no canonical mutation |
| Confidence/conflict review | Complete questionnaire review | Every facet and triggered conflict is decided |
| Identity presentation | Sparse typed presentation operations or one reviewed catalog receipt | Cannot read or write personality, alignment, birth, ruleset, or protected identity data |
| Derived OCEAN | `CharacterProfile.derived.ocean` | Read-only, lossy, visually marked `derived` |
| Alignment and date context | Existing independently reproducible receipts | Only the receipt-owned extension may change |
| Inner life and voice | `upsert_inner_life` / `upsert_voice` | Canonical attributed prose |
| Final review | `CharacterFinalReview` | Exact profile hash, summary, and unresolved diagnostics |

The ordinary revision vocabulary deliberately excludes suggestions, derived fields, and every
extension except the author-editable identity-presentation fields above. Unknown fields fail strict
deserialization; placeholder strings such as `<unknown>` fail validation; broken template
references fail synthesis; and direct attempts to change derived or pack-owned data return `C109`
before the candidate workspace exists. Presentation catalogs, alignment, and historical context
enter a draft only through complete existing receipts, which are independently reproduced and
checked to have changed only their owned extension and provenance.

### Original narrative questionnaires

A `CharacterQuestionnairePack` is inert data. It declares an exact id/version/hash, methodology,
limitations, license, public provenance, independently authored fictional behavior prompts, signed
fixed-point facet weights, and optional narrative-tension rules. A distributable pack must state
both `narrative_authoring_only: true` and `independently_authored_prompts: true`. It cannot claim to
be a psychometric, clinical, hiring, or real-person assessment.

Every pack must cover all 24 factor-specific facets. Responses are integers from -2 through 2. For
one facet, scoring sums the declared signed contributions and divides by twice the sum of answered
absolute weights, producing deterministic millionths from zero through one million. No answer is
imputed: a missing item remains in the trace with no response or contribution. Confidence is
`unknown` with no answer, `low` for partial coverage, `moderate` for one completely covered item,
and `high` for two or more completely covered items. A review may lower computed confidence but
cannot silently raise it.

Every facet receives exactly one `accept`, `edit`, `override`, `reject`, or `withhold` decision.
Accept creates reviewed canon, edit creates an authored value, and override creates a reviewed
high-precedence replacement. Reject and withhold preserve the trace but create no operation. Each
triggered cross-axis rule also requires exactly one action:

| Conflict action | Effect |
|---|---|
| `accept` | Retain the reviewed suggestions; include a rationale when pack policy requires it |
| `reject` | Omit the rule's declared facet suggestions from the application |
| `deliberate_exception` | Retain the tension with a mandatory author rationale |

The receipt embeds the exact pack, answers, input profile, proposal, review, generated canonical
operations, applied/rejected facet lists, and output profile. Parsing a receipt reproduces the
entire operation; a changed prompt, weight, answer, confidence, conflict disposition, input field,
or output byte is stale.

The included [Lantern Choices fixture](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/authoring) is original MIT-licensed material with 24 fictional prompts and one explicit narrative tension. It contains no copied inventory item or protected presentation.

### Preview, migration, invalidation, and final review

`authoring-preview` is required before persistence by the editor flow and available directly to
text workflows. It exposes every template base and effective value, exact template/overlay origin,
inherited/overridden state, lock state, protected ownership, before/after field difference, derived
OCEAN difference, migration effect, and downstream invalidation. The candidate draft is included,
so apply independently repeats the preview instead of trusting an opaque UI mutation.

A template migration names the exact prior SHA-256, embeds another reviewed release of the same
template id, and records reviewer plus rationale. It rebases sparse operations against the new
template only inside the preview. A missing base value, newly locked field, stale old hash, changed
template identity, or invalid effective result stops the migration atomically. No automatic
conversion or hidden default is available.

HEXACO changes recompute OCEAN in the candidate and show that invalidation before apply. If an
approved alignment view exists, a personality change creates a blocking `alignment_review`
invalidation. If accepted date context exists, a birth or personality change creates a blocking
`date_context_review` invalidation. Those records remain until a fresh exact reviewed receipt is
adopted. Any edit clears the old final review.

The final review includes stable identity and typed presentation, birth data, every present canonical factor/facet,
the explicitly derived OCEAN display, approved alignment, accepted date context, inner-life and
voice records, complete provenance, and stable unresolved diagnostics. `accepted` is invalid while
any blocking diagnostic remains; `needs_changes` preserves the complete summary. Reviewed export
requires an accepted final review by default.

### Text workflow

The `weave-character` binary exposes the editor-equivalent verbs `authoring-create`,
`authoring-list`, `authoring-show`, `authoring-clone`, `authoring-preview`, `authoring-revise`,
`authoring-validate`, `authoring-review`, `authoring-export`, and `authoring-reopen`. Questionnaire
and presentation propose/review/apply commands produce the same bytes as the Rust/editor functions;
`presentation-revision` converts a reviewed collection receipt into the exact sparse authoring
revision for one matching draft. Mutating commands
validate the complete candidate and use a same-directory atomic replacement.

```bash
cargo run -p weave-character -- authoring-preview \
  examples/domain-modules/weave-character/authoring/workspace.created.authoring-workspace.json \
  examples/domain-modules/weave-character/authoring/questionnaire.authoring-revision.json \
  --output target/lumen.authoring-preview.json

cargo run -p weave-character -- authoring-revise \
  examples/domain-modules/weave-character/authoring/workspace.created.authoring-workspace.json \
  examples/domain-modules/weave-character/authoring/questionnaire.authoring-revision.json \
  --output target/lumen.authoring-workspace.json
```

The checked JSON/RON corpus covers blank and template creation, clone safety, direct/picker,
questionnaire, and presentation convergence, revision, derived invalidation, blocking alignment
invalidation, template migration with retained overrides, final accept/needs-changes review,
export, and reopen.

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

Expression validation and lint use a separate stable, redaction-safe vocabulary:

| Code | Family |
|---|---|
| `X100` | Empty required expression or dialogue text |
| `X101` | Text exceeds its declared bound |
| `X102` | Unknown or unresolved template token |
| `X103` | Conflicting voice constraints |
| `X104` | Restricted personalization or secret-shaped content |
| `X105` | Unreviewed suggestion excluded from runtime use |
| `X106` | Missing or inconsistent character, record, pool, assignment, or source link |
| `X107` | Exact duplicate dialogue variant |
| `X108` | Near-duplicate dialogue variant |
| `X109` | Invalid placeholder declaration or use |
| `X110` | Required typed placeholder value is unavailable |
| `X111` | Stale profile, pack, request, or assignment fingerprint |
| `X112` | Invalid version, ordering, structure, state, or provenance |

Parsing, validation, and synthesis return no partial effective profile. File output uses an atomic same-directory temporary file.

## Corpus health, drift, safety, and coverage

`CharacterHealthManifest` routes one bounded, project-relative set of canonical and authoring artifacts through the existing typed Character validators. It supports collections and profiles, template/overlay/synthesis lineage, temporal/alignment/presentation receipts, relationships, expression, projections, assistance, and runtime domain packs. Exactly one collection is primary. Each document declares a stable id, artifact kind, JSON or RON encoding, relative path, semantic `logical_id`, and optional owning character; matching logical ids are how the audit compares portable RON/JSON pairs.

`audit_character_health` is deterministic and read-only. It hashes the strict manifest and exact source bytes, audits isolated parsed values, and returns a `CharacterHealthReport`; it never normalizes, migrates, applies, or writes project data. The serialized report explicitly records `read_only: true` and `source_payloads_retained: false`. It carries source files and optional line/column coordinates, but no rejected field value or source payload. Credential-shaped input produces `H110`; the remediation requires replacing the whole value with exactly `[REDACTED]`.

The stable `H###` vocabulary composes the narrower Character, relationship, and expression validators:

| Codes | Audit family |
|---|---|
| `H100`–`H110` | Invalid/missing/versioned fields, identifiers, facet confidence, derived consistency, stale fingerprints, references, template overlays, protected authority, and sensitive values |
| `H200`–`H209` | Relationship inverse/symmetry, pedigree, dates, duplicate edges, affinity freshness, and configured age/kinship/partnership/consent safeguards |
| `H300`–`H307` | Expression links, placeholders/text/constraints, unsafe personalization, review state, and exact/near duplicate variants |
| `H400`–`H413` | Projection and assistance provenance/version/explanation/review/freshness, provider metadata, and accepted-source lineage |
| `H500`–`H504` | RON/JSON equivalence, canonical ordering, runtime references, authoring/runtime authority, and optional CSV loss |
| `H600`–`H601` | Explicit coverage or distribution policy failures |
| `H700`–`H701` | Invalid or unused reviewed suppressions |

Every diagnostic has a stable id and code, severity, collection/character/document scope, field path, available source location, redaction-safe explanation, actionable remediation, and optional suppression id. A suppression pins its own format version, code and optional scope/path prefix, reviewer, rationale, and positive review revision. The report retains the exact matched diagnostic ids and emits `H701` when a valid suppression no longer matches, making cleanup reviewable. `H110` sensitive-value findings cannot be suppressed; an attempted exception leaves the finding active and emits blocking `H700`.

Coverage reports exact factor/facet/confidence numerators and denominators globally and per character, plus stale/current values and relationship/expression/role/suggestion counts. Trait-band, relationship-kind, role-taxonomy, and expression-category distributions are descriptive. They never become demographic, personality, relationship, role, or expression quotas. A constraint is enforced only when the project manifest names the metric, bounds, rationale, and public policy URL explicitly.

The CLI renders equivalent text, JSON, or RON. Text enumerates every report field, coverage row, distribution entry, diagnostic, and suppression rather than collapsing machine-readable details. `--character`, `--code`, `--minimum-severity`, and `--include-suppressed` filter presentation only. `--ci` evaluates the complete unfiltered report and returns status `2` only when an active diagnostic meets the manifest's `failure_threshold`; no threshold means diagnostics remain informational. Manifest/command failures use status `1`. An output path may not replace the manifest or any audited source.

```bash
cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/healthy/project.health-manifest.json

cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/unsafe/project.health-manifest.json \
  --format json \
  --code H304 \
  --minimum-severity error \
  --ci
```

The editor's `CharacterHealthSession` calls the same audit and filter functions. Its summary exposes the exact counts and CI decision; every diagnostic becomes a navigation link carrying its document id, optional character id, field path, source file, and coordinate. Refreshing re-audits the retained bytes without modifying them.

The checked [health manifest schema](downloads/weave-character-health-manifest-v1.schema.json), [health report schema](downloads/weave-character-health-report-v1.schema.json), and [six-project synthetic corpus](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-character/health) cover healthy, incomplete, stale, unsafe, malformed, and migration-required outcomes. The healthy project includes semantically identical collection/runtime JSON and RON pairs. Every fixture and expected report is original MIT-licensed Weave data.

## End-to-end module projection

The checked Ari Vale profile also passes through the ordinary domain-module boundary. `character_module_manifest()` returns the same declarative `ModuleManifest` used by World, and `character_domain_pack()` validates the complete `CharacterProfile` before projecting it into one finite `DomainValue` tree. Neither function parses `.weave`, depends on the editor, contacts a provider, or loads executable package code.

The runtime projection retains:

- the stable character id, attributed display name, and aliases;
- optional attributed pronouns, origin/context notes, appearance descriptors, palette slots, style tags, portable assets, and reviewed catalog coordinates with `canonical_personality_write_back: false`;
- all six factor summaries and all 24 facets with input form, normalized projection score, confidence, state, review, lock, freshness, rationale, and lineage;
- the exact OCEAN algorithm, input paths, scores, confidence, `lossy: true`, `independent_evidence: false`, and omitted Honesty-Humility marker;
- normalized expression lexicon, preferences, vocabulary pools, behavioral signatures, voice constraints, exact template assignments and pack hashes, typed applicability, source ids, and `canonical_personality_write_back: false`;
- the layered relationship graph with its pack coordinate, direction-preserving edges, review/lock/freshness state, validity, inverse ids, notes, consent, safeguard exceptions, advisory scores, ordered evidence, rationale, lineage, and `canonical_personality_write_back: false`;
- optional approved alignment shorthand with exact pack, review, application, coverage, decision, explanation, and canonical input-path records, `canonical_personality_write_back: false`, and no rejected/withheld values;
- optional approved categorical, vocation, social-role, and narrative-role values with exact pack/proposal/review coordinates, explanations, rationales, input paths, decisions, locks, categorical lossiness, and an explicit all-false write-back contract;
- optional reviewed date context with exact pack coordinates, accepted record ids, accepted/edited/overridden/auto-approved cues, relevance and uncertainty, the review hash, separate fact/cue source ids, and `canonical_personality_write_back: false`; and
- sorted profile source and transformation identifiers.

`projection_score` is the exact normalized score for a `score` input and only the documented compatibility anchor for a retained band input. It never replaces the original profile measurement. Missing factor or facet evidence remains an absent optional path.

The manifest declares `profile.hexaco`, `profile.ocean`, `profile.expression`, `profile.alignment`, `profile.projections`, `profile.date_context`, `profile.presentation`, `profile.relationships`, stable identity, contract version, and provenance paths read-only. The shared compiler rejects an override that targets, encloses, or descends from any such path. Authors revise canonical personality, expression, presentation, relationships, or projections through the owning artifact workflow, run the validator/projector, and receive a pack whose derived views were recomputed before compilation. A story may still author a permitted attributed display-name value; that source override remains separate in Story IR and cannot change the stable character id or any reviewed extension record.

[`ari-vale.weave`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.weave) selects the reviewed Character domain pack through ordinary module syntax, then reads typed projection, expression, relationship, pronoun, palette, asset, catalog, alignment, factor/facet, and OCEAN paths. Projection review, expression assignment/resolution, relationship review, presentation, and alignment selection occur at the authoring boundary in the shared Character CLI/editor workflow; source selects only the resulting validated Character pack, so an unreviewed suggestion cannot enter narrative logic. The story compiles to byte-stable [JSON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.story.json) and [RON](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/ari-vale.story.ron). The separate [reviewed temporal story](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave) exercises the temporal projection. Adjacent `weave.modules.json` and `weave.lock` files make both builds deterministic and offline. The editor exposes projection evidence/review/apply/lock/rebalance beside expression, relationship, presentation, alignment, and temporal workflows. The finite Bevy and PixiJS consumers read the same portable approved projections and validate their no-write-back boundary without an editor dependency.

```weave
module character {
    id: "org.weave.character"
    version: "=1.6.0"
    pack: "ari_vale@=1.0.0"
}

VAR creativity = character.profile.hexaco.openness.creativity.projection_score
VAR ocean_openness = character.profile.ocean.openness.score
VAR expression_term = character.profile.expression.lexicon.trailmark.surface
VAR expression_preference = character.profile.expression.preferences.clear_questions.target
VAR expression_template = character.profile.expression.template_assignments.arrival_greeting.template_id
VAR expression_pack = character.profile.expression.template_assignments.arrival_greeting.pack.id
VAR expression_pack_sha256 = character.profile.expression.template_assignments.arrival_greeting.pack.sha256
VAR expression_write_back = character.profile.expression.canonical_personality_write_back
VAR alignment_pack = character.profile.alignment.pack.id
VAR alignment_pack_version = character.profile.alignment.pack.version
VAR alignment_pack_sha256 = character.profile.alignment.pack.sha256
VAR alignment_horizon = character.profile.alignment.values.horizon.label
VAR relationship_target = character.profile.relationships.edges.mentor_sable.target_character_id
VAR relationship_origin = character.profile.relationships.edges.mentor_sable.origin
VAR relationship_pack = character.profile.relationships.kind_pack.id
VAR relationship_pack_sha256 = character.profile.relationships.kind_pack.sha256
VAR presentation_subject = character.profile.presentation.pronouns.subject
VAR presentation_accent = character.profile.presentation.palette.colors.accent
VAR presentation_avatar = character.profile.presentation.assets.authored_avatar.path
VAR personality_lens = character.profile.projections.values.personality_lens.label
VAR vocation = character.profile.projections.values.vocation.label
VAR social_role = character.profile.projections.values.social_role.label
VAR narrative_role = character.profile.projections.values.narrative_role.label
VAR projection_pack = character.profile.projections.values.narrative_role.pack.id
VAR projection_review = character.profile.projections.values.narrative_role.review_sha256
VAR projection_write_back = character.profile.projections.write_back.hexaco
```

## Canonical fixtures and commands

The public fixture is a wholly synthetic character named Ari Vale. It includes every factor and facet, a full date, attributed identity presentation, inner-life and voice records, every typed extension family, a pending suggestion, one locked field, a reviewed override, and an unknown opaque extension preserved at version 99. The projection fixtures add an original pack, four taxonomies, signed fixed-point weights, calibration vectors, a three-character capacity-limited batch, reservation, complete traces and distribution, every review decision, locks, unlock, and rebalance. The assistance fixtures add an original template, complete offline typed scaffolds, exact disclosure/approval artifacts, strict provider-response normalization, evidence, advisory and all five author decisions, suggestion-only application, a second credential-free coordinate, provider comparison, and a rate-limited resumable two-character batch with atomic receipt. The health fixtures add six original corpus manifests with exact text/JSON/RON reports for healthy, incomplete, stale, unsafe, malformed, and migration-required outcomes. The expression fixtures add an original immutable pack, direct normalization/revision, explicit exact assignment, every record family, all four typed context predicate families, clean lint/coverage, deterministic contextual and fallback resolutions, and source-located placeholder failure. The relationship fixtures add a three-character roster, immutable kind pack and policy, directed/symmetric/inverse authored graph, imported edge, four-source affinity scorer, computed and suggested candidates, all six review decisions, atomic receipt, conflict reconciliation, and review-only CSV. The presentation fixtures add Sable Reed, an exact catalog/seed/asset inventory, balanced capacity-limited allocation, transparent traces, a complete review, one explicit override, a locked avatar, an unlock revision, and replayable JSON/RON output. The collection fixtures add a bidirectional relationship, an exact rename request, a payload-free interrupted cursor, the byte-stable proposal and review, and the atomically renamed result. The alignment fixtures add an original five-axis pack, exact provider hash, calibration boundaries, every review action, an independently reproducible receipt, and an approved-only profile. The context fixtures add three exact offline packs, selected/downgraded/skipped coverage, four decisions, an independently reproducible receipt, the enriched profile, and a separately locked temporal runtime story. Invalid fixtures and deterministic fake-adapter tests cover unsupported versions, duplicate aliases, missing presentation assets, invalid palette slots, incompatible presentation overrides, conflicting overlays, stale input/review/progress/presentation/alignment/temporal/relationship/expression/assistance lineage, incomplete alignment/temporal/relationship/assistance reviews, malformed proposals or provider responses, bad relationships, restricted expression placeholders, credential-shaped assistance values, and derived canonical evidence.

```bash
cargo run -p weave-character --example character_fixture -- --check
cargo run -p weave-character --example expression_fixture -- --check
cargo run -p weave-character --example projection_fixture -- --check
cargo run -p weave-character --example assistance_fixture -- --check
cargo run -p weave-character --example health_fixture -- --check

cargo run -p weave-character -- schema profile \
  --output target/weave-character-profile-v1.schema.json

cargo run -p weave-character -- validate profile \
  examples/domain-modules/weave-character/profile.character.json

cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/healthy/project.health-manifest.json \
  --format json \
  --output target/character-health.json

cargo run -p weave-character -- expression-validate \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-resolve \
  examples/domain-modules/weave-character/expression/applied.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json \
  --output target/contextual.expression-resolution.json

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

Canonical JSON and RON pairs are semantically equal and byte-stable. The fixture generators rebuild all valid, invalid, schema, template, overlay, synthesis, assistance, health, projection, expression, presentation, alignment, temporal, module-manifest, and domain-pack artifacts without network access. The documentation gate additionally checks health manifests/reports and all six CI outcomes, assistance templates/requests/previews/approvals/provider responses/candidate sets/reviews/jobs/receipts/comparisons, projection packs/configs/proposals/reviews/receipts/locks/rebalance, expression normalization/assignment/lint/coverage/resolution, and presentation/alignment/temporal review artifacts; checks dry-run, stale, redaction, and lock boundaries; verifies approved-only Story IR; recompiles both locked stories; and tests both engine consumers.
