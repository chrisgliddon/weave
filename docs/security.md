# Security

Do not report a suspected vulnerability through a public issue, pull request, discussion, dex task, or documentation change.

Use the private [GitHub Security Advisory form](https://github.com/chrisgliddon/weave/security/advisories/new). The canonical [SECURITY.md](https://github.com/chrisgliddon/weave/blob/main/SECURITY.md) lists the useful report details, supported-version policy, response expectations, and coordinated-disclosure process.

Use synthetic or throwaway data in a reproduction. Never include production credentials, personal data, or unrelated confidential material. Replace every credential value in text, logs, screenshots, and fixtures with `[REDACTED]`.

Character assistance artifacts are credential-free by contract. A host adapter must obtain any required credential through its own secret channel, report only whether configuration is present, and never place the value in a request, preview, job, candidate set, review, receipt, diagnostic, cache key, or provider comparison. Both `Display` and `Debug` rendering at that boundary must replace the whole value with exactly `[REDACTED]`; retaining a prefix, suffix, length, or decoded detail is forbidden.

Character corpus-health reports follow the same boundary. The auditor accepts source bytes only as input, records `source_payloads_retained: false`, and emits hashes, safe paths/coordinates, static explanations, and remediation instead of rejected values. Credential-shaped source produces `H110`; audit output must replace the complete value with `[REDACTED]`. `H110` is not suppressible: an attempted suppression leaves it active and adds `H700`. Health manifests and reports are safe to review only after the same whole-value redaction has been applied to surrounding logs and command output.
