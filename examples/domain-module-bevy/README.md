# Domain module in Bevy

This finite Bevy 0.18 example deserializes the canonical compiled RON tracer and Weave World
reference seed, extracts their typed exports through `weave-core`, inserts the results as Bevy
resources, and reports them from `Startup` systems. It has no editor dependency and performs no
artifact discovery at runtime.

Run it from the repository root:

```bash
cargo run -p weave-example-domain-module-bevy
```

The process performs one Bevy update and exits, so the same command is suitable for documentation
and CI smoke checks.
