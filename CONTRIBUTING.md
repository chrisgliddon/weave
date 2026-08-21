# Contributing to Weave

Thank you for helping build Weave. Contributions should be reviewable without requiring maintainers to guess the author's intent.

## Before you begin

- Search existing GitHub issues before opening a new one.
- Open an issue before starting a large change to the language, serialized formats, runtime behavior, public Rust APIs, or editor architecture.
- Keep one issue and one pull request focused on one coherent outcome.
- Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md), never in a public issue or pull request.
- Follow the mandatory secret-redaction rules in [AGENTS.md](AGENTS.md). Logs and reproductions must use `[REDACTED]` for every credential value.

## Development setup

```bash
git clone https://github.com/chrisgliddon/weave.git
cd weave
cargo build --workspace
```

Run the editor with:

```bash
cargo run -p weave_editor
```

## Writing issues

Write issue titles in English and describe the observable outcome. Avoid titles such as `fix`, `update`, `changes`, `some improvements`, or `WIP`. Use exactly one of these title prefixes:

- `[Feature]` for a new user-visible capability.
- `[Improvement]` for making existing behavior measurably better without introducing a distinct capability.
- `[Bug]` for behavior that is incorrect relative to documented or reasonably expected behavior.

Copy the matching template exactly, replace every angle-bracket field, and remove only sections explicitly marked optional. Do not leave `TODO`, `TBD`, empty headings, dangling issue references, or template instructions in a submitted issue.

### Feature template

Title: `[Feature] <specific user-visible capability>`

````markdown
## Summary

<Describe the capability in one or two sentences.>

## Problem or opportunity

<Who needs this, what are they trying to do, and why is the current behavior insufficient?>

## Proposed behavior

<Describe the externally observable behavior, including important inputs, outputs, and failure cases.>

## User experience

<Describe the author, runtime, Bevy, CLI, or editor workflow from start to finish.>

## Weave or Rust example

```weave
<Show the smallest concrete syntax or API example.>
```

## Acceptance criteria

- [ ] <A specific, observable success condition.>
- [ ] <A relevant error, edge case, or compatibility condition.>
- [ ] <Required tests and documentation are identified.>

## Scope and non-goals

**In scope:** <List what this issue includes.>

**Out of scope:** <List nearby work this issue deliberately excludes.>

## Alternatives considered

<Describe meaningful alternatives and why this proposal is preferable.>

## Additional context (optional)

<Add mockups, prior art, related issues, or implementation constraints.>
````

### Improvement template

Title: `[Improvement] <specific behavior to improve>`

````markdown
## Summary

<State the improvement and the intended measurable result.>

## Current behavior

<Describe what happens now, where it happens, and the evidence used to assess it.>

## Proposed improvement

<Describe the desired behavior and how it differs from the current behavior.>

## Motivation

<Explain the affected workflow, users, maintenance cost, performance cost, or reliability concern.>

## Measurement

<Define how improvement will be verified: benchmark, latency, memory, frame time, diagnostic quality, reduced steps, or another observable measure.>

## Acceptance criteria

- [ ] <A specific observable or measurable success condition.>
- [ ] <Existing behavior that must remain compatible.>
- [ ] <Required regression tests and documentation are identified.>

## Constraints and tradeoffs

<List compatibility, serialized-format, Rust API, platform, accessibility, or complexity constraints.>

## Additional context (optional)

<Add profiles, traces, screenshots, examples, related issues, or prior art. Redact all credentials with [REDACTED].>
````

### Bug template

Title: `[Bug] <specific incorrect behavior>`

````markdown
## Summary

<Describe the incorrect behavior and its impact in one or two sentences.>

## Environment

- Weave version or commit: <version or full commit SHA>
- Rust version: <output of rustc --version>
- Operating system and version: <OS and version>
- Affected component: <compiler, runtime, patterns, Bevy plugin, formatter, or editor>
- Bevy/GPUI version, if relevant: <version or not applicable>

## Steps to reproduce

1. <First exact step.>
2. <Second exact step.>
3. <The step that demonstrates the bug.>

## Minimal reproduction

```weave
<Paste the smallest non-sensitive story that reproduces the problem.>
```

## Expected behavior

<Describe what should happen and cite the relevant documentation when possible.>

## Actual behavior

<Describe what happens instead, including exact diagnostics or UI state.>

## Logs or screenshots (optional)

<Attach only what is necessary. Replace every secret or credential value with [REDACTED].>

## Regression

<State whether this worked before. Include the last known good version or commit when known.>

## Impact and frequency

<Describe severity, how often it occurs, available workarounds, and whether data can be lost or corrupted.>
````

## Pull requests

Pull requests must be complete and reviewable:

- Use a specific English title. Prefer Conventional Commits where they fit, such as `fix: preserve comments during graph sync` or `feat: add three-rune spreads`.
- Explain what changed and why, link the issue, and call out compatibility or serialized-format effects.
- Include meaningful validation. If a check was not run, state which check and why.
- Include screenshots or recordings for visible editor changes.
- State which AI models materially assisted with the change, or write `None`. Prompts and transcripts are not required.
- Remove template comments and placeholders before opening the pull request. Do not submit placeholder or speculative pull requests.

## Quality checks

Run these before submitting a Rust change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Build and verify the complete documentation site with the pinned mdBook release:

```bash
cargo install mdbook --version 0.5.4 --locked
python3 scripts/build-docs.py
```

Add focused tests for changed behavior. Update documentation and examples whenever public syntax, behavior, APIs, or workflows change.

## Commits

Keep commits concise and internally consistent. Do not mix unrelated formatting or cleanup into a functional change. Never commit secrets, generated credentials, or unredacted diagnostic output.
