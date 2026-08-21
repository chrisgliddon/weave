# Weave Language Server

`weave-lsp` gives any Language Server Protocol client the same parser diagnostics, source spans, inferred variable types, and canonical formatting used by the Weave toolchain. It communicates over standard input and output and contains no editor-specific integration code.

## Install

Build and install the binary from a Weave checkout:

```bash
cargo install --path crates/weave-lsp
```

An editor should start `weave-lsp` with no arguments and use `weave` as the language identifier for files ending in `.weave`. Protocol traffic uses UTF-16 positions, as required by clients that do not negotiate another position encoding.

## Capabilities

The server advertises:

- push diagnostics from `weave-core`, including stable diagnostic codes and remediation text;
- hierarchical document symbols for knots, grammars, grammar rules, pattern systems, collections, and spreads, plus variable symbols;
- workspace-aware definitions and references, preferring a definition in the current file;
- inferred variable types and declaration details on hover;
- keyword and workspace-symbol completion, narrowed to flow targets after `->`;
- validated, workspace-wide rename edits;
- whole-document formatting through `weave-fmt`; and
- incremental text synchronization with exact UTF-16-to-UTF-8 conversion.

At initialization, every `.weave` file under each workspace folder is indexed. The server ignores `.git`, `.dex`, `target`, and `node_modules` directories. Open documents are always authoritative over files on disk, and changing or closing one document refreshes diagnostics for the workspace. A reference may resolve to a uniquely named declaration in another indexed file; compiling and packaging those project files remains the build system's responsibility.

## Neovim 0.11+

After placing `weave-lsp` on `PATH`, add the following to your Neovim configuration:

```lua
vim.filetype.add({
  extension = { weave = "weave" },
})

vim.lsp.config.weave = {
  cmd = { "weave-lsp" },
  filetypes = { "weave" },
  root_markers = { ".git", "Cargo.toml" },
}

vim.lsp.enable("weave")
```

Open a `.weave` file inside the project, then run `:checkhealth vim.lsp` if the client does not attach. Running `weave-lsp` in a terminal is only a transport smoke test: the process waits for framed LSP messages and normally prints nothing.

## Protocol contract

The implementation follows the editor-independent [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) and uses `tower-lsp-server` for JSON-RPC routing, request cancellation, lifecycle enforcement, and stdio framing. Malformed request parameters are rejected by the protocol layer without terminating the server; invalid incremental ranges are logged and ignored without corrupting the last valid document snapshot.

The end-to-end protocol test launches the real binary and verifies initialization, workspace indexing, diagnostics after incremental edits, symbols, cross-file navigation, references, hover, completion, rename, formatting, cancellation, malformed parameters, and graceful shutdown.
