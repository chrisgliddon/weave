# Five-minute quickstart

This path compiles one complete `.weave` story and runs the standalone Rust example. It needs Git and the Rust toolchain selected by `rust-toolchain.toml`.

## 1. Clone Weave

```bash
git clone https://github.com/chrisgliddon/weave.git
cd weave
```

## 2. Read the first story

The checked-in source at `examples/stories/basic.weave` contains one grammar and one terminal knot:

```weave
// Seeded grammar expansion and a simple terminal flow.
grammar greetings {
    opening: ["Hello", "Welcome", "Good evening"]
}

=== start ===
#greetings.opening#, traveler.
-> END
```

## 3. Compile it

```bash
cargo run -p weave-compiler -- \
  examples/stories/basic.weave \
  --output target/basic.ron
```

The command type-checks the source and writes versioned runtime IR to `target/basic.ron`. Add `--format json` for interoperable JSON.

## 4. Run a story

```bash
cargo run -p weave-example-standalone
```

The finite example compiles `examples/stories/patterns.weave`, starts the runtime with a fixed seed, prints each line and choice, selects the first available choice, and exits at `END`.

## Next steps

- Extend the syntax with [variables, choices, and conditions](language_guide.md).
- Produce structured draws with [pattern systems](pattern_systems.md).
- Load source directly in a game through the [Bevy plugin](bevy_integration.md).
- Browse every checked command in [runnable examples](examples.md).

The documentation build executes this compiler path and the standalone example, so these commands cannot drift silently from the workspace.
