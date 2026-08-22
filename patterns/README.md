# Community pattern packages

This directory contains reviewed, data-only packages for Weave's community pattern format. The complete authoring, validation, installation, discovery, moderation, and compatibility contract is in [`docs/community_patterns.md`](../docs/community_patterns.md).

Each package directory contains an immutable `package.weave-pattern.json` and an original `.weave` example. Validate the sample with:

```bash
cargo run -p weave-patterns --bin weave-pattern -- \
  validate patterns/community/ember-omens/package.weave-pattern.json
```

Packages must contain public provenance and licensing metadata and must never contain scripts, executable extensions, private sources, or credentials. Replace sensitive values in reports with exactly `[REDACTED]`.
