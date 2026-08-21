# tree-sitter-weave

`tree-sitter-weave` is the incremental parser and syntax-query package for the [Weave narrative scripting language](../README.md). It covers declarations, knots, nested choices and conditionals, template interpolation, grammar references, pattern systems, expressions, and error-tolerant editing states.

## Build and test

The generated C parser is checked in so editors can consume a release without Node.js:

```bash
npm ci
npm run generate
npm run test:grammar
npm test
cargo test -p tree-sitter-weave
```

`npm run test:grammar` runs corpus and highlight fixtures. `npm test` verifies the Node binding, incremental reparsing through incomplete edits, and the shared package/language version.

## Editor integration

Consumers can use any standard Tree-sitter integration:

- C/C++ hosts compile `src/parser.c` and `src/scanner.c`, adding `src` to the header search path.
- Rust hosts depend on the `tree-sitter-weave` crate and pass `tree_sitter_weave::LANGUAGE.into()` to `tree_sitter::Parser::set_language`.
- Node hosts install the npm package and import the default export from `tree-sitter-weave`.
- Editors that manage parser repositories can point at this repository with `tree-sitter-weave` as the grammar subdirectory, then load `queries/highlights.scm`, `queries/locals.scm`, and `queries/tags.scm`.

The grammar advertises `source.weave` and the `.weave` file suffix in `tree-sitter.json`. The highlight query uses conventional Tree-sitter capture names, so downstream themes do not need Weave-specific color keys.

For an editor without a repository manager, integration consists of four steps:

1. Compile `src/parser.c` and `src/scanner.c` into the editor's Tree-sitter parser directory.
2. Register the resulting language as `weave`, with `.weave` mapped to the `source.weave` scope.
3. Copy the three files in `queries/` into the editor's `queries/weave` runtime directory.
4. Restart or reload the editor's parser registry, then open a `.weave` file.

Keep the parser and query files from the same release. The checked-in highlight fixture uses the representative source from the workspace README and can be run downstream with `tree-sitter test` before packaging an editor extension.

## Version policy

Grammar releases use the same version as the normative language guide and workspace root. A Weave language `0.1.x` release therefore ships a `tree-sitter-weave` `0.1.x` release. Language syntax changes update all versions together; parser-only fixes may increment the shared patch version. `test/version.test.mjs` prevents the language guide, npm package, Rust crate, grammar metadata, and workspace version from drifting.

Generated files under `src/` and `bindings/` are release artifacts. Change `grammar.js`, `src/scanner.c`, queries, or tests, run `npm run generate`, and commit the regenerated output.
