const STORY_IR_VERSION = 4;

export function decodeDomainValue(encoded) {
  if (!encoded || typeof encoded !== "object" || typeof encoded.kind !== "string") {
    throw new TypeError("invalid compiled domain value");
  }
  switch (encoded.kind) {
    case "null":
      return null;
    case "bool":
      if (typeof encoded.value === "boolean") return encoded.value;
      break;
    case "number":
      if (typeof encoded.value === "number" && Number.isFinite(encoded.value)) {
        return encoded.value;
      }
      break;
    case "string":
    case "symbol":
      if (typeof encoded.value === "string") return encoded.value;
      break;
    case "list":
      if (Array.isArray(encoded.value)) return encoded.value.map(decodeDomainValue);
      break;
    case "object":
      if (encoded.value && typeof encoded.value === "object" && !Array.isArray(encoded.value)) {
        return Object.fromEntries(
          Object.entries(encoded.value).map(([name, value]) => [name, decodeDomainValue(value)]),
        );
      }
      break;
    default:
      break;
  }
  throw new TypeError("invalid compiled domain value");
}

export function readModuleExport(story, alias, path) {
  if (story?.version !== STORY_IR_VERSION) {
    throw new TypeError(`unsupported story IR version; expected ${STORY_IR_VERSION}`);
  }
  if (!Array.isArray(path) || path.length === 0) {
    throw new TypeError("a module export path is required");
  }
  const encoded = story.modules?.[alias]?.exports?.[path[0]]?.value;
  let value = decodeDomainValue(encoded);
  for (const field of path.slice(1)) {
    if (!value || typeof value !== "object" || Array.isArray(value) || !(field in value)) {
      throw new TypeError("compiled domain export path is unavailable");
    }
    value = value[field];
  }
  return value;
}
