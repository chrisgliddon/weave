import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { decodeDomainValue, readModuleExport } from "../src/domain-values.js";

const storyUrl = new URL("../../domain-modules/contract/tracer.story.json", import.meta.url);

test("reads the exact portable tracer exports", async () => {
  const story = JSON.parse(await readFile(storyUrl, "utf8"));
  assert.equal(readModuleExport(story, "constellation", ["phase"]), "twilight");
  assert.equal(
    readModuleExport(story, "constellation", ["observation", "label"]),
    "Glasswing constellation",
  );
  assert.equal(readModuleExport(story, "constellation", ["observation", "intensity"]), 0.625);
});

test("rejects invalid tagged values without echoing them", () => {
  const rejected = "credential-shaped-value";
  assert.throws(
    () => decodeDomainValue({ kind: "number", value: rejected }),
    (error) => error instanceof TypeError && !error.message.includes(rejected),
  );
});
