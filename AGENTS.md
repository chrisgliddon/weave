# Agent Instructions

These instructions apply to the entire repository.

## Task tracking

- Use repository-local `dex` data as the source of truth for planned and active work. Do not hand-edit `.dex/tasks.jsonl` or dex metadata embedded in GitHub issues.
- Run `dex status` and `dex list` before choosing work. Run `dex start <id>` when work begins and `dex complete <id> --result "..." --commit <sha>` when committed work is complete; use `--no-commit` only for work that intentionally has no code commit.
- Create newly discovered work with a concrete outcome, requirements, and acceptance criteria. Record real dependencies with `--blocked-by` rather than relying on prose.
- GitHub Issues synchronization is an optional public mirror, never a prerequisite for creating, inspecting, starting, completing, or querying tasks. The local task workflow must remain usable without network access or GitHub credentials.
- Never place a secret in a dex task name, description, result, or commit message because task data may become public when mirrored.

## Subagent orchestration

The main agent acts as orchestrator: it owns task tracking, scope, integration, commits, and branch promotion. Subagents perform bounded slices and adversarial reviews. A subagent starts with fresh context, runs in the same checkout, and can edit files, so treat it as a concurrent writer rather than a sandbox.

### When to delegate

Delegate when work splits into independent, file-scoped slices: a vertical feature slice across several files, researching or authoring fixtures, mechanical fan-out, or an adversarial review. Do not delegate a one-file edit, a design decision, or anything a few tool calls would finish; briefing and reviewing costs more than the work.

### Models

- Implementation, authoring, and mechanical fan-out: `ollama-cloud/deepseek-v4.1-flash:1017`. Keep the pinned tag; the floating `ollama-cloud/deepseek-v4.1-flash` may move under the repository.
- Adversarial review: `ollama-cloud/glm-5.3`. Use the full model rather than `glm-5.3-flash` when the change touches a contract, a filesystem boundary, serialization, or release artifacts. Reviews cost more per token than implementation, so reserve them for risky changes.
- Resolve a loosely named model with the models tool before delegating; never guess an identifier. If the user names a model, use exactly that one.

### Briefing

Every prompt states:

- the single outcome, the exact files the subagent may change, and an explicit list of files it must not touch;
- the design or contract it must follow, pasted in full, because the subagent has none of the conversation;
- acceptance criteria and the exact verification commands it must run;
- that it must not commit, push, promote branches, rewrite history, or run `git checkout`, `git stash`, or `git reset`;
- the report shape: files changed, commands run, results, deviations, and anything incomplete.

### Parallelism

Run independent slices in parallel only when their file sets are disjoint. One writer per file set. Use background mode for work that does not gate the next step, and foreground when the result is needed to continue. The orchestrator integrates and resolves overlaps; subagents never reconcile each other.

### Verification

Never accept a subagent's claim that tests pass. Run the gate yourself, inspect the diff, and check for scope creep and for local absolute paths or secrets introduced in new text. The public-source and secret rules above apply to subagent prompts and output exactly as to the orchestrator's own.

### Adversarial review

For changes touching contracts, validation, serialization, filesystems, or release artifacts, run a second subagent with `ollama-cloud/glm-5.3`. Its prompt must:

- frame it as an attacker, not a collaborator;
- forbid source edits and require it to delete probe files and leave `git status` as it found it;
- name the specific risks to attack and invite findings beyond them;
- require severity-ranked findings, each with `file:line`, an exact reproduction, and the observed result;
- require a verdict of block, fix-then-ship, or ship, naming the decisive finding.

Fix blocking and high findings before shipping, then re-review the fix delta. Record any review that could not run.

### Ownership and failure

Only the orchestrator commits, pushes, promotes, and updates dex. If a subagent fails, hits a provider limit, or is cancelled, inspect the working tree before rerunning: it may have left partial edits, a scratch test, or a half-finished refactor. Decide explicitly whether to keep or revert. A review that did not run is not a review; say so rather than implying coverage.

## Local-first development — mandatory

- `dev` is the default integration branch. Promote verified commits from `dev` to `staging`, then from `staging` to `production`; perform and validate each promotion locally before pushing it.
- Keep required authoring, formatting, building, testing, documentation generation, packaging, and release preparation runnable from a local checkout with documented commands.
- Do not add GitHub Actions workflows or files below `.github/workflows/`. Do not make correctness, validation, documentation, packaging, or releases depend on GitHub-hosted automation, the GitHub API, or GitHub credentials.
- GitHub may host repository refs and optional public mirrors, but normal development and validation must continue to work when GitHub is unavailable.

## Public-source isolation — mandatory

Treat everything committed, synchronized, or shared from this repository as public by default.

- Never mention, identify, link to, or allude to the existence of non-public source work, even generically. This includes repository or product names, codenames, corpora, datasets, records, characters, organizations, local paths, and provenance claims.
- When carrying a general technique into Weave, specify the behavior independently from first principles. Never copy or closely paraphrase non-public code, prose, prompts, schemas, configuration, calibrated parameters, records, or data.
- Use only original, synthetic, or compatibly licensed public fixtures and datasets. Record the license, citation, attribution, and transformation provenance required by each public source.
- Apply this boundary to source, documentation, tests, examples, task metadata, issue and pull-request text, commit messages, results, logs, diagnostics, screenshots, and shareable chat output.
- Before committing, synchronizing, or publishing newly authored material, inspect the changed public text for non-public identifiers, local absolute paths, and provenance language. Prefer an allowlist of intended public terms; if origin or licensing is uncertain, stop and ask.
- If a prohibited reference is published, stop propagating it, remove it from the repository and synchronized trackers, report the exposure privately, and verify the current public artifacts are clean. Ask before any destructive history-rewrite operation.

## Secrets and credentials — mandatory

Never print, log, paste, commit, or sync a secret or credential. This includes API keys, access tokens, passwords, cookies, authorization headers, private keys, signing material, database URLs, webhook secrets, and credential-bearing URLs.

- Store secrets only in environment variables or an approved secret store. Commit the environment variable name or an obviously fake placeholder, never its value.
- Before sharing command output, logs, diagnostics, fixtures, screenshots, task text, issue text, or chat text, replace the **entire** sensitive value with the exact marker `[REDACTED]`. Do not preserve a prefix, suffix, length, or decoded metadata.
- Prefer constructing safe output from an allowlist. Do not print raw process environments, HTTP headers, configuration objects, or credential files and attempt to clean them afterward.
- If a real secret is exposed, stop using it, report the exposure privately, and rotate or revoke it. Redaction after disclosure does not make the old credential safe.

Use this exact Rust implementation whenever a secret must appear in formatted or structured diagnostic context. Do not derive `Debug` for this wrapper and do not expose the inner field:

```rust
use std::fmt;

pub const REDACTED: &str = "[REDACTED]";

pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.0;
        formatter.write_str(REDACTED)
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.0;
        formatter.write_str(REDACTED)
    }
}
```

Use it at the logging boundary:

```rust
let github_token = std::env::var("GITHUB_TOKEN")?;

tracing::debug!(
    github_token = %Secret::new(&github_token),
    "GitHub authentication is configured"
);
```

Safe rendered output is exactly:

```text
github_token=[REDACTED]
Authorization: Bearer [REDACTED]
database_url=[REDACTED]
```

In examples that require a value, use an unmistakable non-secret placeholder:

```bash
export GITHUB_TOKEN="<set-locally-never-commit>"
```
