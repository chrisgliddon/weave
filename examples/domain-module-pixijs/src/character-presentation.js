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

export function temporalCharacterPresentation(story) {
  const context = readModuleExport(story, "character", ["profile", "date_context"]);
  if (
    typeof context !== "object" ||
    context === null ||
    context.canonical_personality_write_back !== false ||
    !Array.isArray(context.accepted_record_ids) ||
    !Array.isArray(context.cues)
  ) {
    throw new TypeError("invalid reviewed temporal Character context");
  }
  const cues = context.cues.map((cue) => {
    if (
      typeof cue?.id !== "string" ||
      typeof cue?.record_id !== "string" ||
      typeof cue?.kind !== "string" ||
      typeof cue?.content !== "string" ||
      typeof cue?.decision !== "string" ||
      !Array.isArray(cue?.fact_source_ids) ||
      cue.fact_source_ids.length === 0 ||
      !Array.isArray(cue?.cue_source_ids) ||
      cue.cue_source_ids.length === 0
    ) {
      throw new TypeError("invalid reviewed temporal Character cue");
    }
    return {
      id: cue.id,
      recordId: cue.record_id,
      kind: cue.kind,
      content: cue.content,
      decision: cue.decision,
      factSourceIds: cue.fact_source_ids,
      cueSourceIds: cue.cue_source_ids,
      relevance: cue.relevance,
      uncertainty: cue.uncertainty,
      sensitivity: cue.sensitivity,
    };
  });
  return {
    canonicalPersonalityWriteBack: context.canonical_personality_write_back,
    acceptedRecordIds: context.accepted_record_ids,
    cues,
  };
}
