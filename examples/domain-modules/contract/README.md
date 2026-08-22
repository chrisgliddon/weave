# Synthetic domain-contract fixture

This directory contains the canonical version-1 module manifest and data pack used by contract tests. `Synthetic Constellation`, `Glasswing Sky`, their values, and their terminology are original fictional material authored for Weave. They use no external source material.

Both artifacts are licensed under the repository's MIT license. Their machine-readable provenance points back to this public project and uses the authored revision label `domain-contract-v1`. The matching `.json` and `.ron` files decode to equal Rust values and are regenerated canonically by `weave-module normalize`.

`tracer.weave` activates this module and pack explicitly, reads a symbol and a nested object through typed paths, branches on the symbol, and interpolates the object fields. Build both portable forms from the repository root:

```bash
cargo run -p weave-compiler -- \
  examples/domain-modules/contract/tracer.weave \
  --module-manifest examples/domain-modules/contract/module.weave-module.json \
  --module-pack examples/domain-modules/contract/pack.weave-domain.json \
  --output examples/domain-modules/contract/tracer.story.ron

cargo run -p weave-compiler -- \
  examples/domain-modules/contract/tracer.weave \
  --module-manifest examples/domain-modules/contract/module.weave-module.json \
  --module-pack examples/domain-modules/contract/pack.weave-domain.json \
  --format json \
  --output examples/domain-modules/contract/tracer.story.json
```

The checked artifacts are consumed directly by the sibling `domain-module-bevy` and `domain-module-pixijs` examples. Neither consumer discovers the source artifacts or depends on the editor at runtime.
