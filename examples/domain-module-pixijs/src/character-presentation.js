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

export function alignmentCharacterPresentation(story) {
  const alignment = readModuleExport(story, "character", ["profile", "alignment"]);
  const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
  const isHash = (value) => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
  if (
    !isObject(alignment) ||
    typeof alignment.view_id !== "string" ||
    !isObject(alignment.pack) ||
    typeof alignment.pack.id !== "string" ||
    typeof alignment.pack.version !== "string" ||
    !isHash(alignment.pack.sha256) ||
    !isHash(alignment.review_sha256) ||
    !isHash(alignment.applied_sha256) ||
    alignment.canonical_personality_write_back !== false ||
    !Array.isArray(alignment.input_paths) ||
    !alignment.input_paths.every((path) => typeof path === "string") ||
    !isObject(alignment.values)
  ) {
    throw new TypeError("invalid reviewed Character alignment");
  }
  const values = Object.entries(alignment.values)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([axisId, value]) => {
      if (
        !isObject(value) ||
        value.id !== axisId ||
        typeof value.label_id !== "string" ||
        typeof value.label !== "string" ||
        !["reviewed", "edited", "overridden"].includes(value.decision) ||
        !Number.isInteger(value.coverage_micros) ||
        value.coverage_micros < 0 ||
        value.coverage_micros > 1_000_000 ||
        (value.score_micros !== undefined && !Number.isInteger(value.score_micros)) ||
        !Array.isArray(value.input_paths) ||
        !value.input_paths.every((path) => typeof path === "string")
      ) {
        throw new TypeError("invalid approved Character alignment value");
      }
      return {
        id: value.id,
        labelId: value.label_id,
        label: value.label,
        decision: value.decision,
        scoreMicros: value.score_micros,
        coverageMicros: value.coverage_micros,
        inputPaths: value.input_paths,
      };
    });
  return {
    viewId: alignment.view_id,
    pack: alignment.pack,
    reviewSha256: alignment.review_sha256,
    appliedSha256: alignment.applied_sha256,
    canonicalPersonalityWriteBack: alignment.canonical_personality_write_back,
    inputPaths: alignment.input_paths,
    values,
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
