# Weave Editor development

The editor is a native GPUI application. It shares parser, compiler, runtime, formatter, and pattern types with the workspace crates rather than maintaining editor-only copies.

The lockfile pins GPUI to the public Zed revision compatible with the pinned gpui-flow release. GPUI is Apache-2.0 and gpui-flow is MIT licensed; both are consumed as dependencies without copied source. The macOS build enables GPUI's runtime shader path so contributors do not need the optional Xcode Metal toolchain component.

Run the visible editor:

```sh
cargo run -p weave_editor
```

Run the finite native initialization smoke check:

```sh
cargo run -p weave_editor -- --smoke-test
```

Startup errors are caught at the process boundary and reported on standard error with a non-zero exit status.
