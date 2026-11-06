const PACK_FORMAT_VERSION = 1;
const ACCENT_SLOT = "accent";
const ALIGNMENT_AXIS = "horizon";
const INNER_LIFE_ID = "steady_hope";
const VOICE_ID = "gentle_directness";
const DATE_CUE_ID = "cue_000000000000000000000001";
const CANONICAL_COLOR = /^#[0-9A-F]{6}(?:[0-9A-F]{2})?$/u;
const BAND_ANCHORS = {
  very_low: 0.1,
  low: 0.3,
  middle: 0.5,
  high: 0.7,
  very_high: 0.9,
};

function requiredString(object, key, message) {
  const value = object?.[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(message);
  }
  return value;
}

function validateReferences(profile, characterIds) {
  const relationships = profile.relationships ?? {};
  for (const edge of Object.values(relationships)) {
    if (!edge || typeof edge !== "object") {
      throw new TypeError("invalid character pack relationship");
    }
    if (edge.source_character_id !== profile.id) {
      throw new TypeError("relationship source must equal its owning character");
    }
    if (!characterIds.has(edge.target_character_id)) {
      throw new TypeError("relationship target is absent from the pack");
    }
    if (
      edge.inverse_edge_id !== undefined &&
      !Object.hasOwn(relationships, edge.inverse_edge_id)
    ) {
      throw new TypeError("inverse relationship edge is absent from the pack");
    }
  }
}

function traitScore(measurement) {
  if (measurement?.form === "score" && Number.isFinite(measurement.score)) {
    return measurement.score;
  }
  if (measurement?.form === "band" && Object.hasOwn(BAND_ANCHORS, measurement.band)) {
    return BAND_ANCHORS[measurement.band];
  }
  throw new TypeError("a required personality factor is missing or is not a score");
}

function opennessBand(score) {
  if (score >= 0.7) return "high";
  if (score >= 0.4) return "middle";
  return "low";
}

function greetingFor(alignmentLabel, band) {
  if (alignmentLabel === "Seeking" && band === "high") {
    return "Morning. I brought a spare map in case the path surprises us.";
  }
  if (alignmentLabel === "Seeking") {
    return "Morning. Let us check the trail together.";
  }
  if (band === "high") {
    return "Morning. There is more to notice here than yesterday.";
  }
  return "Morning. One step at a time.";
}

function birthDateLabel(date) {
  if (date?.precision === "year" && Number.isInteger(date.year)) {
    return String(date.year).padStart(4, "0");
  }
  if (date?.precision === "month_day") {
    return `${String(date.month).padStart(2, "0")}-${String(date.day).padStart(2, "0")}`;
  }
  if (date?.precision === "full") {
    return `${String(date.year).padStart(4, "0")}-${String(date.month).padStart(2, "0")}-${String(
      date.day,
    ).padStart(2, "0")}`;
  }
  throw new TypeError("runtime character has no approved birth date");
}

/**
 * Validate a compact Character runtime pack and select the same observable values that the
 * Bevy consumer reports. The function is pure: it reads only the supplied JSON object.
 */
export function characterPackPresentation(pack) {
  if (!pack || typeof pack !== "object") {
    throw new TypeError("invalid character pack");
  }
  if (pack.pack_format_version !== PACK_FORMAT_VERSION) {
    throw new TypeError("unsupported character pack format version");
  }
  if (!Array.isArray(pack.characters) || pack.characters.length === 0) {
    throw new TypeError("character pack requires at least one character");
  }
  for (const profile of pack.characters) {
    if (!profile || typeof profile.id !== "string" || profile.id.length === 0) {
      throw new TypeError("invalid character pack character");
    }
  }
  const characterIds = new Set(pack.characters.map((profile) => profile.id));
  for (const profile of pack.characters) {
    validateReferences(profile, characterIds);
  }

  const profile = pack.characters[0];
  const displayName = requiredString(profile, "display_name", "invalid character display name");
  const accentColor = profile.presentation?.palette?.[ACCENT_SLOT];
  if (typeof accentColor !== "string" || !CANONICAL_COLOR.test(accentColor)) {
    throw new TypeError("runtime presentation has no approved accent colour");
  }
  const openness = traitScore(profile.personality?.openness);
  const band = opennessBand(openness);
  const alignment = profile.alignment?.[ALIGNMENT_AXIS];
  const alignmentLabel = requiredString(
    alignment,
    "label",
    "runtime character has no approved alignment value",
  );
  const innerLife = requiredString(
    profile.inner_life?.[INNER_LIFE_ID],
    "content",
    "runtime character has no approved inner-life record",
  );
  const voice = requiredString(
    profile.voice?.[VOICE_ID],
    "content",
    "runtime character has no approved voice record",
  );
  const dateContext = requiredString(
    profile.date_context?.[DATE_CUE_ID],
    "content",
    "runtime character has no approved date-context cue",
  );

  return {
    packId: requiredString(pack, "id", "invalid character pack id"),
    packVersion: requiredString(pack, "version", "invalid character pack version"),
    license: requiredString(pack.license, "license", "invalid character pack licence"),
    id: requiredString(profile, "id", "invalid character id"),
    displayName,
    accentColor,
    greeting: greetingFor(alignmentLabel, band),
    opennessBand: band,
    alignmentLabel,
    birthDate: birthDateLabel(profile.birth_date),
    innerLife,
    voice,
    dateContext,
  };
}
