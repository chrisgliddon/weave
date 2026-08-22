# Tree-sitter integration

`tree-sitter-weave` is the incremental concrete-syntax parser for `.weave` files. It ships a generated C parser, external indentation scanner, conventional highlight captures, local-variable queries, symbol tags, and C, Rust, Node, Go, Python, and Swift bindings. Domain-module aliases and their `id`, `version`, and `pack` fields are part of the checked grammar and syntax queries.

## Verify the grammar

```bash
cd tree-sitter-weave
npm ci
npm run generate
npm run test:grammar
npm test
```

Corpus fixtures cover declarations, nested narrative flow, incomplete constructs, and erroneous editing states. The highlight fixture asserts the representative README syntax, and the Node test compares incremental and fresh trees through a sequence of unfinished choices.

## Editor installation model

1. Build `src/parser.c` and `src/scanner.c` for the editor's Tree-sitter host.
2. Register the language name `weave`, scope `source.weave`, and `.weave` file suffix.
3. Install `queries/highlights.scm`, `queries/locals.scm`, and `queries/tags.scm` from the same release as the parser.
4. Reload the editor's language registry.

The [grammar package guide](https://github.com/chrisgliddon/weave/blob/main/tree-sitter-weave/README.md) has host-specific build details and the generated-artifact policy. Grammar, npm, Rust crate, workspace, and normative language-guide versions are checked together.
