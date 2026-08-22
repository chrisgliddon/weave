import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { decodeDomainValue, readModuleExport } from "../src/domain-values.js";
import { composedWorldPresentation } from "../src/world-presentation.js";

const storyUrl = new URL("../../domain-modules/contract/tracer.story.json", import.meta.url);
const worldStoryUrls = [
  "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.json",
  "../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.json",
  "../../domain-modules/weave-world/corpus/stories/maldives.story.json",
  "../../domain-modules/weave-world/reference-place.story.json",
].map((path) => new URL(path, import.meta.url));
const composedWorldUrl = new URL(
  "../../domain-modules/weave-world/composed-setting.story.json",
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

test("reads four exact portable world seeds", async () => {
  const stories = await Promise.all(
    worldStoryUrls.map(async (url) => JSON.parse(await readFile(url, "utf8"))),
  );
  assert.deepEqual(
    stories.map((story) => ({
      preset: readModuleExport(story, "world", ["seed", "identity", "preset"]),
      climate: readModuleExport(story, "world", ["seed", "climate", "band"]),
      hazards: readModuleExport(story, "world", ["seed", "hazards", "tendencies"]),
    })),
    [
      {
        preset: "british_columbia_temperate_forest",
        climate: "temperate_oceanic",
        hazards: ["heavy_precipitation", "seasonal_freeze", "slope_instability"],
      },
      {
        preset: "hokkaido_japan",
        climate: "humid_continental",
        hazards: ["seasonal_freeze"],
      },
      {
        preset: "maldives",
        climate: "tropical_oceanic",
        hazards: ["coastal_inundation"],
      },
      {
        preset: "aotearoa_new_zealand",
        climate: "temperate_oceanic",
        hazards: ["heavy_precipitation", "slope_instability"],
      },
    ],
  );
});

test("composed world rules, places, and environment alter PixiJS presentation", async () => {
  const story = JSON.parse(await readFile(composedWorldUrl, "utf8"));
  assert.deepEqual(composedWorldPresentation(story), {
    harbor: "Emberwake Harbor",
    road: "Lantern Road",
    primaryBiome: "temperate_conifer_forest",
    climateBand: "humid_continental",
    coastal: true,
    beaconRule: true,
    behavior: "beacon escort via Lantern Road",
    background: "#102825",
    foreground: "#d9f4e3",
  });
  assert.equal(story.modules.world.authored_overrides.length, 46);
  assert.deepEqual(story.modules.world.authored_overrides[0].path, [
    "places",
    "emberwake_harbor",
    "attributes",
    "booleans",
    "has_beacon",
  ]);
});

test("rejects invalid tagged values without echoing them", () => {
  const rejected = "credential-shaped-value";
  assert.throws(
    () => decodeDomainValue({ kind: "number", value: rejected }),
    (error) => error instanceof TypeError && !error.message.includes(rejected),
  );
});
