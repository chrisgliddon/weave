# Security

Do not report a suspected vulnerability through a public issue, pull request, discussion, dex task, or documentation change.

Use the private [GitHub Security Advisory form](https://github.com/chrisgliddon/weave/security/advisories/new). The canonical [SECURITY.md](https://github.com/chrisgliddon/weave/blob/main/SECURITY.md) lists the useful report details, supported-version policy, response expectations, and coordinated-disclosure process.

Use synthetic or throwaway data in a reproduction. Never include production credentials, personal data, or unrelated confidential material. Replace every credential value in text, logs, screenshots, and fixtures with `[REDACTED]`.

Character assistance artifacts are credential-free by contract. A host adapter must obtain any required credential through its own secret channel, report only whether configuration is present, and never place the value in a request, preview, job, candidate set, review, receipt, diagnostic, cache key, or provider comparison. Both `Display` and `Debug` rendering at that boundary must replace the whole value with exactly `[REDACTED]`; retaining a prefix, suffix, length, or decoded detail is forbidden.
