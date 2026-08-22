# Domain module in Bevy

This finite Bevy 0.18 example deserializes the canonical compiled RON tracer, extracts the
`constellation` module's typed exports through `weave-core`, inserts the result as a Bevy resource,
and reports it from a `Startup` system. It has no editor dependency and performs no artifact
discovery at runtime.

Run it from the repository root:

```bash
cargo run -p weave-example-domain-module-bevy
```

The process performs one Bevy update and exits, so the same command is suitable for documentation
and CI smoke checks.
