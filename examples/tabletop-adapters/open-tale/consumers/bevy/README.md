# Open Tale Bevy consumer

The reference consumer moved into the repository workspace. It is now a compiled, tested Bevy
example at `examples/bevy-tabletop`:

```bash
cargo run -p weave-example-bevy-tabletop --bin open-tale
cargo test -p weave-example-bevy-tabletop
```

The command loads `examples/tabletop-adapters/open-tale` as data, replays the recorded eleven-step
playthrough through the trusted Open Tale resolver, and requires every receipt and the final state
to match the checked-in artifacts.

The former `bevy_consumer.rs` was not a workspace member and could not compile. It has been
removed; the fixture tree is unchanged.
