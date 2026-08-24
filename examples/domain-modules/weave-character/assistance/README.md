# Provider-neutral Character assistance fixtures

This directory is an original MIT-licensed synthetic corpus for Weave's optional assisted Character development contract. Ari Vale, Sable Reed, the Glasswind template, prompt instructions, provider coordinates, candidate prose, evidence explanations, advisory scores, review rationales, and expected results were authored for this fixture. Generation is deterministic and offline. No credential or external service is needed, and every serialized artifact records `credential_value_stored: false` where that boundary applies.

The single-profile flow contains:

- `input.character-collection.{json,ron}` and `glasswind.assistance-template.{json,ron}`: the exact source collection and versioned field/safety contract;
- `single.assistance-request.{json,ron}`: pinned profile, template, provider, fields, disclosed inputs, settings, seed, generation, and provenance;
- `single.assistance-preview.{json,ron}` and `single.assistance-approval.{json,ron}`: the complete credential-free payload/response contract and its separately authored fingerprinted approval;
- `single.assistance-provider-response.{json,ron}` and `single.assistance-candidate-set.{json,ron}`: strict provider-shaped output and its normalized typed, evidence-linked, safety-validated candidates;
- `single.assistance-advisory-review.{json,ron}`: deterministic advisory-only scores and issues with no selection or write-back;
- `single.assistance-decisions.{json,ron}` and `single.assistance-decision-review.{json,ron}`: all five explicit author actions and their complete immutable-set review;
- `single.assistance-receipt.{json,ron}` and `single.applied-character.{json,ron}`: independently reproducible suggestion-only application, with canon, extensions, and derived values unchanged; and
- `alternate.*` plus `providers.assistance-comparison.{json,ron}`: a second credential-free provider coordinate and a stable advisory comparison that selects no winner.

The `batch.*` artifacts pin an exact two-character scope, one-call scheduling windows, budgets, retry policy, job-local cache policy, deterministic cursor, per-character advisory/decision reviews, complete resumable job, atomic receipt, and output collection. Accepted values remain pending suggestions. Replaying the same reviews against the exact output is idempotent; changing an input fingerprint makes the job or review stale.

Rebuild or verify every artifact and all 14 generated schemas from the repository root:

```bash
cargo run -p weave-character --example assistance_fixture -- --write
cargo run -p weave-character --example assistance_fixture -- --check
```

Reproduce the checked single-profile flow:

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

`assistance-inspect` shows one candidate with its approved-input evidence. `assistance-compare` reproduces the checked provider-coordinate view. The `assistance-batch-preview`, `assistance-batch-approve`, `assistance-batch-start`, `assistance-batch-resume-offline`, `assistance-batch-cancel`, and `assistance-batch-apply` commands expose every resumable batch boundary. All commands strictly parse JSON or RON and call the same host-independent Rust functions used by the editor session.

Credentials belong only in a host-owned secret channel. If an adapter must render credential state in a diagnostic, it replaces the entire value with exactly `[REDACTED]`; prefixes, suffixes, lengths, decoded details, raw response bodies, and credential-bearing URLs never enter these fixtures.
