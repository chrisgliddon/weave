# Agent Instructions

These instructions apply to the entire repository.

## Task tracking

- Use `dex` as the source of truth for planned and active work. Do not hand-edit `.dex/tasks.jsonl` or dex metadata embedded in GitHub issues.
- Run `dex status` and `dex list` before choosing work. Run `dex start <id>` when work begins and `dex complete <id> --result "..." --commit <sha>` when committed work is complete; use `--no-commit` only for work that intentionally has no code commit.
- Create newly discovered work with a concrete outcome, requirements, and acceptance criteria. Record real dependencies with `--blocked-by` rather than relying on prose.
- Repository-local dex changes synchronize to GitHub Issues automatically. Never place a secret in a dex task name, description, result, or commit message because synced data becomes public.

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
