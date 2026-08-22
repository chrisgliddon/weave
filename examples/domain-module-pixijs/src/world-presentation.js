import { readModuleExport } from "./domain-values.js";

export function composedWorldPresentation(story) {
  const primaryBiome = readModuleExport(story, "world", ["seed", "primary_biome"]);
  const climateBand = readModuleExport(story, "world", ["seed", "climate", "band"]);
  const harbor = readModuleExport(story, "world", ["places", "emberwake_harbor", "name"]);
  const road = readModuleExport(story, "world", ["places", "lantern_road", "name"]);
  const coastal = readModuleExport(story, "world", [
    "places",
    "emberwake_harbor",
    "environment_override",
    "coastal",
  ]);
  const beaconRule = readModuleExport(story, "world", [
    "rules",
    "booleans",
    "beacons_answer_storms",
  ]);
  const travelMode = readModuleExport(story, "world", ["rules", "symbols", "travel_mode"]);

  const wintryConifers =
    primaryBiome === "temperate_conifer_forest" && climateBand === "humid_continental";
  return {
    harbor,
    road,
    primaryBiome,
    climateBand,
    coastal,
    beaconRule,
    behavior:
      beaconRule && coastal && travelMode === "tidebound"
        ? `beacon escort via ${road}`
        : "ordinary overland travel",
    background: wintryConifers ? "#102825" : "#111326",
    foreground: wintryConifers ? "#d9f4e3" : "#f2ecff",
  };
}
