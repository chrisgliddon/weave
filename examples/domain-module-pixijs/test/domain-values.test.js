import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { decodeDomainValue, readModuleExport } from "../src/domain-values.js";

const storyUrl = new URL("../../domain-modules/contract/tracer.story.json", import.meta.url);
const worldStoryUrl = new URL(
  "../../domain-modules/weave-world/reference-place.story.json",
  import.meta.url,
);

test("reads the exact portable tracer exports", async () => {
  const story = JSON.parse(await readFile(storyUrl, "utf8"));
  assert.equal(readModuleExport(story, "constellation", ["phase"]), "twilight");
  assert.equal(
    readModuleExport(story, "constellation", ["observation", "label"]),
    "Glasswing constellation",
  );
  assert.equal(readModuleExport(story, "constellation", ["observation", "intensity"]), 0.625);
});

test("reads the exact portable world seed", async () => {
  const story = JSON.parse(await readFile(worldStoryUrl, "utf8"));
  assert.equal(
    readModuleExport(story, "world", ["seed", "identity", "preset"]),
    "aotearoa_new_zealand",
  );
  assert.deepEqual(readModuleExport(story, "world", ["seed", "terrain"]), [
    "coastal",
    "island",
    "mountain",
  ]);
  assert.equal(
    readModuleExport(story, "world", [
      "seed",
      "climate",
      "selected_station_annual_mean_temperature_c",
      "minimum",
    ]),
    8.8,
  );
});

test("rejects invalid tagged values without echoing them", () => {
  const rejected = "credential-shaped-value";
  assert.throws(
    () => decodeDomainValue({ kind: "number", value: rejected }),
    (error) => error instanceof TypeError && !error.message.includes(rejected),
  );
});
