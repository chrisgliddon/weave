# Third-party domain-module tutorial fixture

This directory is a complete, original MIT-licensed module built only with Weave's public declarative files. `Lantern Weather`, `Harbor Evening`, and their values are fictional tutorial material; no external dataset or executable extension is used.

Validate and compile it from the repository root:

```bash
cargo run -p weave-domain --bin weave-module -- validate \
  examples/domain-modules/third-party-tutorial/module.weave-module.json \
  --pack examples/domain-modules/third-party-tutorial/pack.weave-domain.json

cargo run -p weave-compiler -- \
  examples/domain-modules/third-party-tutorial/story.weave \
  --format json \
  --output target/lantern-weather.story.json
```

`weavec` discovers the adjacent `weave.modules.json`, resolves the compatible module and pack, and writes an exact `weave.lock` beside it. Use `--locked` on reproducible builds after reviewing that lock. See the [full tutorial](../../../docs/domain_module_tutorial.md) for publication, installation, upgrades, and editor reload behavior.
