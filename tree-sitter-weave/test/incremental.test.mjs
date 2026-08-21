import assert from "node:assert/strict";
import { test } from "node:test";
import Parser from "tree-sitter";

function point(source) {
  const lines = source.split("\n");
  return {
    row: lines.length - 1,
    column: Buffer.byteLength(lines.at(-1) ?? ""),
  };
}

test("incremental trees match fresh parses through incomplete edits", async () => {
  const { default: language } = await import("../bindings/node/index.js");
  const parser = new Parser();
  parser.setLanguage(language);

  const sources = [
    "=== start ===\n* [Choose a path\n",
    "=== start ===\n* [Choose a path]\n",
    "=== start ===\n* [Choose a path] {if ready\n",
    "=== start ===\n* [Choose a path] {if ready}\n    -> ending\n",
    "=== start ===\n* [Choose a path] {if ready}\n    -> ending\n\n=== ending ===\n-> END\n",
  ];

  let previous = sources[0];
  let tree = parser.parse(previous);
  assert.ok(tree.rootNode.hasError, "the first editing state should be incomplete");

  for (const source of sources.slice(1)) {
    tree.edit({
      startIndex: 0,
      oldEndIndex: Buffer.byteLength(previous),
      newEndIndex: Buffer.byteLength(source),
      startPosition: { row: 0, column: 0 },
      oldEndPosition: point(previous),
      newEndPosition: point(source),
    });
    const incremental = parser.parse(source, tree);
    const fresh = parser.parse(source);
    assert.equal(incremental.rootNode.toString(), fresh.rootNode.toString());
    tree = incremental;
    previous = source;
  }

  assert.equal(tree.rootNode.hasError, false);
});
