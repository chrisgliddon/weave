# Runnable examples

Every `.weave` source below is compiled by the documentation gate. The finite standalone and Bevy programs are also executed to completion, and the browser story's generated JSON must match its source byte for byte.

| Example | What it proves | Run from the repository root |
|---|---|---|
| Basic source | Grammar expansion and terminal flow | `cargo run -p weave-compiler -- examples/stories/basic.weave --output target/basic.ron` |
| Branching source | Conditions, choices, variables, and diverts | `cargo run -p weave-compiler -- examples/stories/branching.weave --output target/branching.ron` |
| Pattern source | Structured pattern draws and semantic branches | `cargo run -p weave-compiler -- examples/stories/patterns.weave --output target/patterns.ron` |
| Standalone Rust | Compiler-to-runtime flow without an engine | `cargo run -p weave-example-standalone` |
| Bevy dialogue game | Interactive dialogue, choices, observable state, hot reload, and pattern-event UI | `cargo run -p weave-example-bevy-dialogue` |
| Browser player | JSON, WASM, deterministic restart, and save/restore | `./scripts/build-web-player.sh` |
| Visual editor | Native GPUI project workflow | `cargo run -p weave_editor` |

## Verify all documentation

Install the pinned site generator once, then run the repository-owned build:

```bash
cargo install mdbook --version 0.5.4 --locked
python3 scripts/build-docs.py
```

The command checks source links, the quickstart source, all example stories, finite example binaries, generated web JSON, rustdoc, site search, local HTML targets, document language, main landmarks, image alternatives, contribution links, and the private vulnerability-reporting path.

The Bevy game also has a window-free integration gate for CI and servers:

```bash
cargo run -p weave-example-bevy-dialogue -- --smoke-test
```
