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
