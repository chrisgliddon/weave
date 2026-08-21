import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const files = new URL("../", import.meta.url);

test("grammar packages match the Weave language version", async () => {
  const packageJson = JSON.parse(await readFile(new URL("package.json", files), "utf8"));
  const treeSitterJson = JSON.parse(
    await readFile(new URL("tree-sitter.json", files), "utf8"),
  );
  const grammarCargo = await readFile(new URL("Cargo.toml", files), "utf8");
  const workspaceCargo = await readFile(new URL("../Cargo.toml", files), "utf8");
  const languageGuide = await readFile(
    new URL("../docs/language_guide.md", files),
    "utf8",
  );

  const grammarVersion = grammarCargo.match(/^version = "([^"]+)"$/m)?.[1];
  const workspaceVersion = workspaceCargo.match(
    /\[workspace\.package\][\s\S]*?^version = "([^"]+)"$/m,
  )?.[1];
  const languageVersion = languageGuide.match(
    /^Language specification version: `([^`]+)`\.$/m,
  )?.[1];

  assert.equal(packageJson.version, treeSitterJson.metadata.version);
  assert.equal(packageJson.version, grammarVersion);
  assert.equal(packageJson.version, workspaceVersion);
  assert.equal(packageJson.version, languageVersion);
});
