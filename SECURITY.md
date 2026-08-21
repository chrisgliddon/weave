# Security Policy

## Supported versions

Weave is in early development. Security fixes target the latest commit on the default branch and the latest published release, when releases exist. Older snapshots may not receive fixes.

## Reporting a vulnerability

Do not report a suspected vulnerability through a public GitHub issue, pull request, discussion, dex task, or other public channel.

Report it privately with [GitHub Security Advisories](https://github.com/chrisgliddon/weave/security/advisories/new). If you cannot access the advisory form, contact a maintainer through a private channel already available to you and ask for a secure reporting path without including vulnerability details in the initial message.

Include as much of the following as possible:

- The affected version, release, or full commit SHA.
- The affected component, such as the parser, compiler, runtime, pattern data, Bevy plugin, editor, file watcher, LSP, or WASM player.
- Reproduction steps and the smallest proof of concept that demonstrates the issue.
- Expected impact, required attacker access, and affected platforms.
- Whether exploitation causes code execution, unintended file access, data exposure or corruption, denial of service, or another security boundary failure.
- Any known mitigations or workarounds.
- Whether the issue or proof of concept is already public.
- Whether you want public credit and, if so, the name or handle to use.

Use throwaway data in reproductions. Do not include production credentials, personal data, or unrelated confidential material. Follow [AGENTS.md](AGENTS.md) and replace every credential value in text, logs, screenshots, and fixtures with `[REDACTED]`.

## What to expect

Maintainers will keep the investigation private, attempt to reproduce and assess the report, and coordinate remediation and disclosure with the reporter. Confirmed reports will be credited unless the reporter prefers to remain anonymous. Response and remediation times depend on severity and maintainer availability; this project does not currently promise a fixed service-level agreement.

## Coordinated disclosure

Please allow a reasonable opportunity to investigate and release a fix before publishing details. Coordinate the disclosure date, advisory text, proof of concept, and attribution with the maintainers. If the issue is already public, still use the private advisory so mitigations can be coordinated without expanding exposure.
