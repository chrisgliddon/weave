import { decodeDomainValue, readModuleExport } from "./domain-values.js";

const SHA256 = /^[0-9a-f]{64}$/u;

export function tabletopPresentation(story, receipt) {
  const adapter = readModuleExport(story, "rules", ["adapter"]);
  const definition = readModuleExport(story, "rules", ["definition"]);
  const state = readModuleExport(story, "rules", ["state"]);
  const capabilities = readModuleExport(story, "rules", ["capabilities"]);
  const publicCheck = receipt?.events?.find((event) => event.kind === "check_resolved");
  const entropyTrace = receipt?.events?.find((event) => event.kind === "entropy_trace");
  if (
    typeof adapter?.id !== "string" ||
    adapter.version !== "1.0.0" ||
    !SHA256.test(adapter.content_sha256) ||
    typeof definition?.name !== "string" ||
    !Number.isInteger(definition?.effective_attributes?.agility) ||
    !Number.isInteger(definition?.effective_attributes?.brains) ||
    !Number.isInteger(definition?.effective_attributes?.brawn) ||
    !Number.isInteger(definition?.effective_attributes?.wits) ||
    !Number.isInteger(state?.fortune_remaining) ||
    !Number.isInteger(state?.survivability_current) ||
    typeof state?.wounded !== "boolean" ||
    capabilities?.checks_and_conflicts !== true ||
    capabilities?.encounters !== true ||
    receipt?.resolver_format_version !== 1 ||
    receipt?.adapter?.id !== adapter.id ||
    receipt?.adapter?.version !== adapter.version ||
    receipt?.adapter?.content_sha256 !== adapter.content_sha256 ||
    receipt?.operation !== "check" ||
    !SHA256.test(receipt?.request_sha256 ?? "") ||
    publicCheck?.visibility !== "public" ||
    !SHA256.test(publicCheck?.payload_sha256 ?? "") ||
    publicCheck?.payload == null ||
    entropyTrace?.visibility !== "host_only" ||
    !SHA256.test(entropyTrace?.payload_sha256 ?? "") ||
    entropyTrace?.payload !== null
  ) {
    throw new TypeError("invalid portable tabletop presentation");
  }
  const check = decodeDomainValue(publicCheck.payload);
  if (
    !Number.isInteger(check?.die) ||
    !Number.isInteger(check?.total) ||
    typeof check?.outcome !== "string" ||
    typeof check?.critical !== "boolean" ||
    typeof check?.fumble !== "boolean" ||
    typeof check?.rerolled !== "boolean"
  ) {
    throw new TypeError("invalid portable tabletop check event");
  }
  return {
    adapter,
    name: definition.name,
    attributes: definition.effective_attributes,
    fortune: state.fortune_remaining,
    survivability: state.survivability_current,
    wounded: state.wounded,
    check,
    requestSha256: receipt.request_sha256,
    hiddenEntropySha256: entropyTrace.payload_sha256,
  };
}

export function dungeonpunkPresentation(story, receipt) {
  const adapter = readModuleExport(story, "rules", ["adapter"]);
  const definition = readModuleExport(story, "rules", ["definition"]);
  const state = readModuleExport(story, "rules", ["state"]);
  const capabilities = readModuleExport(story, "rules", ["capabilities"]);
  const rollEvent = receipt?.events?.find((event) => event.kind === "roll_resolved");
  const entropyTrace = receipt?.events?.find((event) => event.kind === "entropy_trace");
  const attributes = definition?.attributes;
  if (
    adapter?.id !== "org.weave.tabletop.dungeonpunk" ||
    adapter.version !== "1.0.0" ||
    !SHA256.test(adapter.content_sha256 ?? "") ||
    typeof definition?.name !== "string" ||
    !["strength", "dexterity", "constitution", "intelligence", "charisma", "wisdom"].every(
      (attribute) => Number.isInteger(attributes?.[attribute]),
    ) ||
    definition?.constants?.fate !== 1 ||
    definition?.constants?.none !== 0 ||
    !Number.isInteger(state?.hp_current) ||
    !Number.isInteger(state?.stress) ||
    !Number.isInteger(state?.xp) ||
    typeof state?.encumbered !== "boolean" ||
    capabilities?.checks_and_conflicts !== true ||
    capabilities?.clocks !== true ||
    receipt?.resolver_format_version !== 1 ||
    receipt?.adapter?.id !== adapter.id ||
    receipt?.adapter?.version !== adapter.version ||
    receipt?.adapter?.content_sha256 !== adapter.content_sha256 ||
    receipt?.operation !== "struggle" ||
    !SHA256.test(receipt?.request_sha256 ?? "") ||
    rollEvent?.visibility !== "public" ||
    !SHA256.test(rollEvent?.payload_sha256 ?? "") ||
    rollEvent?.payload == null ||
    entropyTrace?.visibility !== "host_only" ||
    !SHA256.test(entropyTrace?.payload_sha256 ?? "") ||
    entropyTrace?.payload !== null
  ) {
    throw new TypeError("invalid portable Dungeonpunk presentation");
  }
  const roll = decodeDomainValue(rollEvent.payload);
  if (
    !Number.isInteger(roll?.dice_pool) ||
    !Array.isArray(roll?.dice) ||
    roll.dice.length === 0 ||
    !roll.dice.every(Number.isInteger) ||
    !Number.isInteger(roll?.selected) ||
    !["highest", "lowest"].includes(roll?.selection) ||
    !["success", "twist", "failure"].includes(roll?.outcome) ||
    typeof roll?.helped !== "boolean" ||
    typeof roll?.push !== "boolean" ||
    typeof roll?.failure_xp_gained !== "boolean"
  ) {
    throw new TypeError("invalid portable Dungeonpunk Struggle event");
  }
  return {
    adapter,
    name: definition.name,
    attributes,
    fate: definition.constants.fate,
    none: definition.constants.none,
    hp: state.hp_current,
    stress: state.stress,
    xp: state.xp,
    encumbered: state.encumbered,
    roll,
    requestSha256: receipt.request_sha256,
    hiddenEntropySha256: entropyTrace.payload_sha256,
  };
}

export function freehackPresentation(story, receipt) {
  const adapter = readModuleExport(story, "freehack", ["adapter"]);
  const capabilities = readModuleExport(story, "freehack", ["capabilities"]);
  const character = readModuleExport(story, "freehack", ["character"]);
  const snapshot = readModuleExport(story, "freehack", ["snapshot"]);
  const tracks = readModuleExport(story, "freehack", ["tracks"]);
  const serializedPublicTransport = JSON.stringify({ story, receipt });
  const forbiddenMarkers = [
    "request_sha256",
    "payload_sha256",
    "entropy",
    "opposition",
    "draw_index",
    "signed_result",
    "_authority",
    "sealed_current",
    "Sealed Current",
    "hidden_watch",
    "sealed_route",
    "Private action marker",
  ];
  const publicCheck = receipt?.events?.find((event) => event.kind === "check_resolved");
  if (
    forbiddenMarkers.some((marker) => serializedPublicTransport.includes(marker)) ||
    adapter?.id !== "org.weave.tabletop.freehack" ||
    adapter.version !== "1.0.0" ||
    !SHA256.test(adapter.content_sha256 ?? "") ||
    capabilities?.checks_and_conflicts !== true ||
    capabilities?.resources_and_conditions !== true ||
    capabilities?.scenes !== true ||
    typeof character?.name !== "string" ||
    typeof character?.archetype_id !== "string" ||
    !Number.isInteger(character?.modifiers?.focus) ||
    !Number.isInteger(character?.modifiers?.balance) ||
    "clearance" in (character?.modifiers ?? {}) ||
    !Array.isArray(character?.feature_ids) ||
    !character.feature_ids.every((id) => typeof id === "string") ||
    !Array.isArray(character?.inventory_ids) ||
    !character.inventory_ids.every((id) => typeof id === "string") ||
    !Number.isInteger(tracks?.fatigue?.progress) ||
    !Number.isInteger(tracks?.fatigue?.target_count) ||
    typeof tracks?.fatigue?.interval !== "string" ||
    typeof tracks?.fatigue?.consequence !== "string" ||
    typeof tracks?.fatigue?.completed !== "boolean" ||
    !["open", "resolved", "timed_out"].includes(snapshot?.gantry_status) ||
    !Number.isInteger(snapshot?.memory_count) ||
    receipt?.projection_format_version !== 1 ||
    receipt?.adapter?.id !== adapter.id ||
    receipt?.adapter?.version !== adapter.version ||
    receipt?.adapter?.content_sha256 !== adapter.content_sha256 ||
    receipt?.operation !== "resolve_check" ||
    !SHA256.test(receipt?.public_state_sha256 ?? "") ||
    receipt?.public_state?.projection_format_version !== 1 ||
    receipt?.public_state?.adapter?.id !== adapter.id ||
    !Array.isArray(receipt?.events) ||
    receipt.events.length === 0 ||
    receipt.events.some(
      (event, index) =>
        event?.sequence !== index ||
        typeof event?.kind !== "string" ||
        event.kind.endsWith("_authority") ||
        "visibility" in event ||
        "payload_sha256" in event,
    ) ||
    publicCheck == null
  ) {
    throw new TypeError("invalid portable Freehack public presentation");
  }
  const check = decodeDomainValue(publicCheck.payload);
  if (
    !["success", "failure"].includes(check?.outcome) ||
    !Number.isInteger(check?.magnitude) ||
    !Number.isInteger(check?.support_total) ||
    "opposition_total" in check ||
    "draw_index" in check ||
    "signed_result" in check
  ) {
    throw new TypeError("invalid portable Freehack public check event");
  }
  return {
    adapter,
    name: character.name,
    archetype: character.archetype_id,
    modifiers: character.modifiers,
    featureIds: character.feature_ids,
    inventoryIds: character.inventory_ids,
    fatigue: tracks.fatigue.progress,
    tracks,
    gantryStatus: snapshot.gantry_status,
    publicMemoryCount: snapshot.memory_count,
    check,
    publicStateSha256: receipt.public_state_sha256,
  };
}
