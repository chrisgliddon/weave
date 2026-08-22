import { readModuleExport } from "./domain-values.js";

export function characterPresentation(story) {
  const score = (factor) =>
    readModuleExport(story, "character", [
      "profile",
      "hexaco",
      factor,
      "summary",
      "projection_score",
    ]);
  return {
    id: readModuleExport(story, "character", ["profile", "identity", "id"]),
    displayName: readModuleExport(story, "character", [
      "profile",
      "identity",
      "display_name",
      "value",
    ]),
    factorScores: {
      honestyHumility: score("honesty_humility"),
      emotionality: score("emotionality"),
      extraversion: score("extraversion"),
      agreeableness: score("agreeableness"),
      conscientiousness: score("conscientiousness"),
      openness: score("openness"),
    },
    creativity: readModuleExport(story, "character", [
      "profile",
      "hexaco",
      "openness",
      "creativity",
      "projection_score",
    ]),
    oceanOpenness: readModuleExport(story, "character", [
      "profile",
      "ocean",
      "openness",
      "score",
    ]),
    oceanIsLossy: readModuleExport(story, "character", ["profile", "ocean", "lossy"]),
    oceanIsIndependentEvidence: readModuleExport(story, "character", [
      "profile",
      "ocean",
      "independent_evidence",
    ]),
  };
}
