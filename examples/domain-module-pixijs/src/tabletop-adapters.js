//! Pure tabletop adapter consumer contract shared by the PixiJS app and Node tests.
//!
//! This module has no Node or DOM imports. It describes every adapter's replay data, flattens the
//! same tagged domain values as the Bevy consumer, and renders adapter events with an injected
//! PixiJS `Text` constructor.

import { decodeDomainValue } from "./domain-values.js";

export const ADAPTER_CONSUMERS = [
  {
    key: "plug-and-play",
    id: "org.weave.tabletop.plug_and_play",
    version: "1.0.0",
    kind: "fixture_tree",
    manifest: "plug-and-play.tabletop-adapter.json",
    creation: "creation.tabletop-creation.json",
    preview: "preview.tabletop-creation-preview.json",
    initialState: "state.tabletop-state.json",
    finalState: "final-state.tabletop-state.json",
    receiptSuffix: ".tabletop-receipt.json",
    expectedSteps: 7,
  },
  {
    key: "dungeonpunk",
    id: "org.weave.tabletop.dungeonpunk",
    version: "1.0.0",
    kind: "fixture_tree",
    manifest: "dungeonpunk.tabletop-adapter.json",
    creation: "creation.tabletop-creation.json",
    preview: "preview.tabletop-creation-preview.json",
    initialState: "state.tabletop-state.json",
    finalState: "final-state.tabletop-state.json",
    receiptSuffix: ".tabletop-receipt.json",
    expectedSteps: 11,
  },
  {
    key: "freehack",
    id: "org.weave.tabletop.freehack",
    version: "1.0.0",
    kind: "freehack_public",
    manifest: "freehack.tabletop-adapter.json",
    creation: "creation.tabletop-creation.json",
    publicState: "public-state.freehack-public-state.json",
    publicReceiptSuffix: ".public-receipt.json",
    // Public steps follow the checked-in generation order. The two steps without a public
    // receipt are host-only and never enter the runtime bundle.
    publicOrder: [
      "preview_crossing_probability",
      "cross_wet_gantry",
      "recall_tide_gate",
      "open_gantry_turn",
      "submit_ira",
      "cancel_ira",
      "resubmit_ira",
      "submit_moss",
      "resolve_gantry_turn",
      "create_crossing_track",
      "advance_crossing_track",
      "open_route_turn",
      "submit_ira_route",
      "timeout_route",
    ],
    expectedSteps: 14,
  },
  {
    key: "open-tale",
    id: "org.weave.tabletop.open_tale",
    version: "1.0.0",
    kind: "fixture_tree",
    manifest: "open-tale.tabletop-adapter.json",
    creation: "creation.tabletop-creation.json",
    preview: "preview.tabletop-creation-preview.json",
    initialState: "initial-state.tabletop-state.json",
    finalState: "final-state.tabletop-state.json",
    receiptSuffix: ".tabletop-receipt.json",
    index: "playthrough/index.json",
    expectedSteps: 11,
  },
  {
    key: "spark",
    id: "org.weave.tabletop.spark",
    version: "1.0.0",
    kind: "canonical_project",
    project: "project/project.json",
    expected: "project/expected-output.json",
    expectedSteps: 6,
  },
  {
    key: "wicked-ones",
    id: "org.weave.tabletop.wicked_ones",
    version: "1.0.0",
    kind: "canonical_project",
    project: "project/project.json",
    expected: "project/expected-output.json",
    expectedSteps: 1,
  },
  {
    key: "open-adventure",
    id: "org.weave.tabletop.open_adventure",
    version: "1.0.0",
    kind: "canonical_project",
    project: "project/project.json",
    expected: "project/expected-output.json",
    expectedSteps: 2,
  },
];

const HEX64 = /^[0-9a-f]{64}$/u;
const VISIBILITIES = new Set(["public", "authoring", "host_only"]);

/** Throw a TypeError with a stable, redaction-safe message. */
export function reject(message) {
  throw new TypeError(message);
}

/** Require a SHA-256 hex digest. */
export function requireDigest(value, label) {
  if (typeof value !== "string" || !HEX64.test(value)) {
    reject(`${label} is not a SHA-256 digest`);
  }
  return value;
}

/** Compare an artifact's adapter coordinate with the manifest. */
export function requireCoordinate(artifact, manifest, label) {
  if (
    artifact?.adapter?.id !== manifest.id ||
    artifact?.adapter?.version !== manifest.version ||
    artifact?.adapter?.schema_version !== manifest.schema_version
  ) {
    reject(`${label} adapter coordinate does not match the manifest`);
  }
  if (typeof artifact.adapter.content_sha256 !== "string" || !HEX64.test(artifact.adapter.content_sha256)) {
    reject(`${label} adapter content hash is not a SHA-256 digest`);
  }
}

/** Decode a tagged domain value and flatten its scalar leaves in sorted path order. */
export function flattenDomainValue(encoded) {
  return flattenPlain(decodeDomainValue(encoded));
}

class RawNumber {
  constructor(text) {
    this.text = text;
  }
}

function f64Text(value) {
  if (Object.is(value, -0)) return "-0.0";
  if (Number.isInteger(value)) return `${value}.0`;
  return String(value).replace("e+", "e").replace(/e(-?)0+(\d+)/u, "e$1$2");
}

function canonicalAdapter(adapter) {
  return {
    id: adapter.id,
    version: adapter.version,
    schema_version: adapter.schema_version,
    content_sha256: adapter.content_sha256,
  };
}

function canonicalDomain(encoded) {
  switch (encoded?.kind) {
    case "null":
      return { kind: "null" };
    case "bool":
      return { kind: "bool", value: encoded.value };
    case "number":
      return { kind: "number", value: new RawNumber(f64Text(encoded.value)) };
    case "string":
      return { kind: "string", value: encoded.value };
    case "symbol":
      return { kind: "symbol", value: encoded.value };
    case "list":
      return { kind: "list", value: encoded.value.map(canonicalDomain) };
    case "object": {
      const value = {};
      for (const key of Object.keys(encoded.value).sort()) {
        value[key] = canonicalDomain(encoded.value[key]);
      }
      return { kind: "object", value };
    }
    default:
      reject("invalid compiled domain value");
  }
}

function canonicalStringify(value) {
  if (value instanceof RawNumber) return value.text;
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalStringify).join(",")}]`;
  return `{${Object.keys(value)
    .map((key) => `${JSON.stringify(key)}:${canonicalStringify(value[key])}`)
    .join(",")}}`;
}

/** Canonical JSON for one resolver payload, matching Rust's serde_json serialization. */
export function canonicalPayloadJson(payload) {
  return canonicalStringify(canonicalDomain(payload));
}

/** Canonical JSON for one mutable tabletop state. */
export function canonicalStateJson(state) {
  return canonicalStringify({
    state_format_version: state.state_format_version,
    adapter: canonicalAdapter(state.adapter),
    owner_id: state.owner_id,
    definition_sha256: state.definition_sha256,
    revision: state.revision,
    entropy: {
      algorithm: state.entropy.algorithm,
      seed: state.entropy.seed,
      cursor: state.entropy.cursor,
    },
    value: canonicalDomain(state.value),
  });
}

/** Canonical JSON for one resolver request. */
export function canonicalRequestJson(request) {
  return canonicalStringify({
    resolver_format_version: request.resolver_format_version,
    request_id: request.request_id,
    adapter: canonicalAdapter(request.adapter),
    operation: request.operation,
    capability: request.capability,
    definition_sha256: request.definition_sha256,
    definition: canonicalDomain(request.definition),
    expected_state_sha256: request.expected_state_sha256,
    input: canonicalDomain(request.input),
  });
}

/** Flatten a plain JSON value into stable, sorted `path=value` lines. */
export function flattenPlain(value, path = "", lines = []) {
  if (Array.isArray(value)) {
    if (value.length === 0) {
      lines.push(joinPath(path, "[]"));
    }
    value.forEach((item, index) => flattenPlain(item, `${path}[${index}]`, lines));
    return lines;
  }
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort();
    if (keys.length === 0) {
      lines.push(joinPath(path, "{}"));
    }
    for (const key of keys) {
      flattenPlain(value[key], path.length === 0 ? key : `${path}.${key}`, lines);
    }
    return lines;
  }
  lines.push(joinPath(path, formatScalar(value)));
  return lines;
}

function joinPath(path, value) {
  return path.length === 0 ? value : `${path}=${value}`;
}

function formatScalar(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    return Number.isInteger(value) ? String(value) : value.toFixed(4);
  }
  return String(value);
}

/** Compact manifest contract used by the public projection. */
export function manifestContract(manifest) {
  return {
    manifest_format_version: manifest.manifest_format_version,
    operations: Object.fromEntries(
      Object.entries(manifest.operations).map(([id, operation]) => [
        id,
        {
          required_capability: operation.required_capability,
          event_kinds: operation.event_kinds,
        },
      ]),
    ),
    event_types: Object.keys(manifest.event_types).sort(),
    runtime_visibility: manifest.visibility.runtime,
  };
}

/** Deterministic presentation colour, matching the Bevy consumer. */
export function presentationColor(id) {
  let hash = 2_166_136_261;
  for (const byte of new TextEncoder().encode(id)) {
    hash ^= byte;
    hash = Math.imul(hash, 16_777_619) >>> 0;
  }
  return `hsl(${hash % 360}, 52%, 58%)`;
}

/**
 * Validate the public projection shared with the PixiJS app.
 *
 * When `hash` is supplied, every public event payload digest is recomputed with the same
 * canonicalization Rust uses. The projection contains only public event counts and public data.
 */
export function validateProjection(projection, hash) {
  if (projection?.projection_format_version !== 2) {
    reject("unsupported tabletop consumer projection version");
  }
  if (!Array.isArray(projection.adapters) || projection.adapters.length !== ADAPTER_CONSUMERS.length) {
    reject("tabletop consumer projection does not list every adapter");
  }
  projection.adapters.forEach((adapter, index) => {
    const descriptor = ADAPTER_CONSUMERS[index];
    if (adapter.key !== descriptor.key) {
      reject(`projection adapter ${index} is not ${descriptor.key}`);
    }
    if (adapter.id !== descriptor.id || adapter.version !== descriptor.version) {
      reject(`${adapter.key} projection coordinate does not match the consumer`);
    }
    if (!["fixture_tree", "freehack_public", "canonical_project"].includes(adapter.source)) {
      reject(`${adapter.key} projection source is invalid`);
    }
    if ("event_count" in adapter) {
      reject(`${adapter.key} projection must not publish hidden event totals`);
    }
    if (!Array.isArray(adapter.steps) || adapter.steps.length !== descriptor.expectedSteps) {
      reject(`${adapter.key} projection does not contain the exact step count`);
    }
    if (adapter.presentation_color !== presentationColor(descriptor.id)) {
      reject(`${adapter.key} presentation colour is not deterministic`);
    }
    requireDigest(adapter.initial_state_sha256, `${adapter.key} initial state`);
    requireDigest(adapter.final_state_sha256, `${adapter.key} final state`);
    if (!Array.isArray(adapter.observable) || !adapter.observable.every((line) => typeof line === "string")) {
      reject(`${adapter.key} observable values are invalid`);
    }
    const contract = adapter.contract;
    if (
      contract?.manifest_format_version !== 1 ||
      typeof contract.operations !== "object" ||
      !Array.isArray(contract.event_types) ||
      !Array.isArray(contract.runtime_visibility) ||
      !contract.runtime_visibility.includes("public")
    ) {
      reject(`${adapter.key} declared contract is invalid`);
    }

    let publicEventCount = 0;
    const stepIds = new Set();
    adapter.steps.forEach((step, stepIndex) => {
      if (typeof step.id !== "string" || step.id.length === 0) {
        reject(`${adapter.key} step ${stepIndex} has no id`);
      }
      if (stepIds.has(step.id)) {
        reject(`${adapter.key} step ${step.id} is duplicated`);
      }
      stepIds.add(step.id);
      const declaration = contract.operations[step.operation];
      if (!declaration) {
        reject(`${adapter.key} ${step.id}: operation is not declared by the manifest`);
      }
      if (declaration.required_capability !== step.capability) {
        reject(`${adapter.key} ${step.id}: capability is not declared for the operation`);
      }
      requireDigest(step.request_sha256, `${adapter.key} ${step.id} request`);
      requireDigest(step.before_state_sha256, `${adapter.key} ${step.id} before state`);
      requireDigest(step.after_state_sha256, `${adapter.key} ${step.id} after state`);
      if (stepIndex === 0) {
        if (step.before_state_sha256 !== adapter.initial_state_sha256) {
          reject(`${adapter.key} ${step.id}: first step does not start from the initial state`);
        }
      } else if (
        adapter.source !== "freehack_public" &&
        step.before_state_sha256 !== adapter.steps[stepIndex - 1].after_state_sha256
      ) {
        reject(`${adapter.key} ${step.id}: public state-hash chain is broken`);
      }
      if (!Array.isArray(step.events)) {
        reject(`${adapter.key} ${step.id}: step has no events`);
      }
      step.events.forEach((event, eventIndex) => {
        if (event.sequence !== eventIndex) {
          reject(`${adapter.key} ${step.id}: public events are not renumbered`);
        }
        if (
          typeof event.kind !== "string" ||
          !declaration.event_kinds.includes(event.kind) ||
          !contract.event_types.includes(event.kind)
        ) {
          reject(`${adapter.key} ${step.id}: event kind is not declared`);
        }
        if (event.capability !== step.capability) {
          reject(`${adapter.key} ${step.id}: event capability is not declared`);
        }
        requireDigest(event.payload_sha256, `${adapter.key} ${step.id} event payload`);
        if (!event.payload || typeof event.payload !== "object") {
          reject(`${adapter.key} ${step.id}: public event has no payload`);
        }
        if (hash && hash(canonicalPayloadJson(event.payload)) !== event.payload_sha256) {
          reject(`${adapter.key} ${step.id}: event payload digest does not match`);
        }
        publicEventCount += 1;
      });
    });
    if (adapter.public_event_count !== publicEventCount) {
      reject(`${adapter.key} public event count does not match the rendered events`);
    }
    if (
      adapter.source !== "freehack_public" &&
      adapter.steps.at(-1).after_state_sha256 !== adapter.final_state_sha256
    ) {
      reject(`${adapter.key} final state hash does not match the last step`);
    }
    if (adapter.source !== "canonical_project" && typeof adapter.character_name !== "string") {
      reject(`${adapter.key} character name is missing`);
    }
  });
  return projection;
}

/** Render every adapter as a labelled column of event and state lines. */
export function renderAdapterRuns(PIXI, app, adapters) {
  const width = 100;
  adapters.forEach((adapter, index) => {
    const x = 8 + index * width;
    const lines = [
      `${adapter.key} ${adapter.version}`,
      adapter.id,
      adapter.character_name ?? "synthetic project",
      `${adapter.steps.length} steps · ${adapter.public_event_count} public events`,
      `color ${adapter.presentation_color}`,
      `final ${adapter.final_state_sha256.slice(0, 16)}`,
    ];
    for (const step of adapter.steps) {
      for (const event of step.events) {
        lines.push(`${step.id}: ${event.kind}`);
      }
    }
    for (const line of adapter.observable.slice(0, 8)) {
      lines.push(line);
    }
    lines.forEach((line, lineIndex) => {
      const text = new PIXI.Text({
        text: line,
        style: {
          fill: adapter.presentation_color,
          fontFamily: "monospace",
          fontSize: 9,
          wordWrap: true,
          wordWrapWidth: width - 8,
        },
      });
      text.x = x;
      text.y = 8 + lineIndex * 13;
      app.stage.addChild(text);
    });
  });
  return app;
}

/** Validate one fixture-tree bundle before it is used or projected. */
export function validateFixtureBundle(descriptor, bundle, hash) {
  if (typeof hash !== "function") {
    reject("fixture validation requires a SHA-256 digest function");
  }
  const { manifest, creation, preview, initialState, finalState, steps } = bundle;
  if (manifest?.manifest_format_version !== 1) {
    reject("unsupported tabletop manifest version");
  }
  if (manifest.id !== descriptor.id || manifest.version !== descriptor.version) {
    reject("tabletop manifest coordinate does not match the consumer");
  }
  if (
    typeof manifest.operations !== "object" ||
    typeof manifest.event_types !== "object" ||
    !Array.isArray(manifest.visibility?.authority_host)
  ) {
    reject("tabletop manifest contract is incomplete");
  }
  if (creation?.creation_format_version !== 1) {
    reject("unsupported tabletop creation version");
  }
  if (typeof creation.name !== "string" || creation.name.length === 0) {
    reject("tabletop creation has no character name");
  }
  requireCoordinate(preview, manifest, "creation preview");
  requireCoordinate(initialState, manifest, "initial state");
  requireCoordinate(finalState, manifest, "final state");
  if (initialState.state_format_version !== 1 || finalState.state_format_version !== 1) {
    reject("unsupported tabletop state version");
  }
  requireDigest(initialState.definition_sha256, "initial definition");
  if (JSON.stringify(preview.initial_state) !== JSON.stringify(initialState)) {
    reject("initial state does not match the creation preview");
  }
  if (!Array.isArray(steps) || steps.length === 0) {
    reject("playthrough has no steps");
  }

  const initialHash = hash(canonicalStateJson(initialState));
  let expectedBefore = initialHash;
  steps.forEach((step, index) => {
    const { request, receipt } = step;
    if (request.resolver_format_version !== 1 || receipt.resolver_format_version !== 1) {
      reject(`${step.id}: unsupported resolver version`);
    }
    requireCoordinate(request, manifest, `${step.id} request`);
    requireCoordinate(receipt, manifest, `${step.id} receipt`);
    if (
      request.adapter.id !== receipt.adapter.id ||
      request.adapter.version !== receipt.adapter.version ||
      request.adapter.schema_version !== receipt.adapter.schema_version ||
      request.adapter.content_sha256 !== receipt.adapter.content_sha256
    ) {
      reject(`${step.id}: request and receipt coordinates differ`);
    }
    if (request.operation !== receipt.operation) {
      reject(`${step.id}: request and receipt operation differ`);
    }
    if (request.capability !== receipt.capability) {
      reject(`${step.id}: request and receipt capability differ`);
    }
    const declaration = manifest.operations[request.operation];
    if (!declaration) {
      reject(`${step.id}: operation is not declared by the manifest`);
    }
    if (declaration.required_capability !== request.capability) {
      reject(`${step.id}: capability is not declared for the operation`);
    }
    requireDigest(request.expected_state_sha256, `${step.id} expected state`);
    requireDigest(receipt.request_sha256, `${step.id} request hash`);
    requireDigest(receipt.before_state_sha256, `${step.id} before state`);
    requireDigest(receipt.after_state_sha256, `${step.id} after state`);
    if (hash(canonicalRequestJson(request)) !== receipt.request_sha256) {
      reject(`${step.id}: request digest does not match the receipt`);
    }
    if (receipt.before_state_sha256 !== request.expected_state_sha256) {
      reject(`${step.id}: request does not expect the receipt's preceding state`);
    }
    if (receipt.before_state_sha256 !== expectedBefore) {
      reject(`${step.id}: state hash chain is broken`);
    }
    const afterHash = hash(canonicalStateJson(receipt.after_state));
    if (afterHash !== receipt.after_state_sha256) {
      reject(`${step.id}: after-state digest does not match the receipt`);
    }
    expectedBefore = afterHash;
    requireCoordinate(receipt.after_state, manifest, `${step.id} after state`);
    if (receipt.after_state.state_format_version !== 1) {
      reject(`${step.id}: unsupported tabletop state version`);
    }
    if (receipt.after_state.revision !== receipt.before_revision + 1) {
      reject(`${step.id}: state revision did not advance exactly once`);
    }
    const entropyDelta =
      receipt.after_state.entropy.cursor - receipt.entropy_before.cursor;
    if (entropyDelta !== receipt.entropy_consumed || entropyDelta < 0) {
      reject(`${step.id}: entropy cursor does not match consumption`);
    }
    if (!Array.isArray(receipt.events)) {
      reject(`${step.id}: receipt has no events`);
    }
    receipt.events.forEach((event, eventIndex) => {
      if (
        event.sequence !== eventIndex ||
        typeof event.kind !== "string" ||
        typeof event.capability !== "string" ||
        !VISIBILITIES.has(event.visibility)
      ) {
        reject(`${step.id}: event envelope is invalid`);
      }
      if (!declaration.event_kinds.includes(event.kind)) {
        reject(`${step.id}: event kind is not declared for the operation`);
      }
      if (!Object.hasOwn(manifest.event_types, event.kind)) {
        reject(`${step.id}: event kind has no declared payload type`);
      }
      if (!manifest.visibility.authority_host.includes(event.visibility)) {
        reject(`${step.id}: event visibility is outside the manifest policy`);
      }
      if (event.capability !== request.capability) {
        reject(`${step.id}: event capability is not declared for the operation`);
      }
      requireDigest(event.payload_sha256, `${step.id} event payload`);
      if (event.payload == null) {
        reject(`${step.id}: authority event payload is redacted`);
      }
      if (hash(canonicalPayloadJson(event.payload)) !== event.payload_sha256) {
        reject(`${step.id}: event payload digest does not match`);
      }
      if (event.visibility === "public" && event.payload == null) {
        reject(`${step.id}: public event has no payload`);
      }
    });
    if (index + 1 === steps.length && expectedBefore !== hash(canonicalStateJson(finalState))) {
      reject("final state digest does not match the last replayed receipt");
    }
  });
  if (JSON.stringify(steps.at(-1).receipt.after_state) !== JSON.stringify(finalState)) {
    reject("final state does not match the last replayed receipt");
  }
  if (descriptor.expectedSteps !== steps.length) {
    reject(`${descriptor.key}: expected ${descriptor.expectedSteps} steps, found ${steps.length}`);
  }
  return bundle;
}

/** Extract the runtime-visible portion of a validated fixture-tree bundle. */
export function publicFixtureView(descriptor, bundle, hash) {
  const steps = bundle.steps
    .map((step) => ({
      id: step.id,
      operation: step.request.operation,
      capability: step.receipt.capability,
      request_sha256: step.receipt.request_sha256,
      before_state_sha256: step.receipt.before_state_sha256,
      after_state_sha256: step.receipt.after_state_sha256,
      events: step.receipt.events
        .filter((event) => event.visibility === "public")
        .map((event, sequence) => ({
          sequence,
          kind: event.kind,
          capability: event.capability,
          payload: event.payload,
          payload_sha256: event.payload_sha256,
        })),
    }))
    .filter((step) => step.events.length > 0);
  const publicEventCount = steps.reduce((total, step) => total + step.events.length, 0);
  return {
    key: descriptor.key,
    id: bundle.manifest.id,
    version: bundle.manifest.version,
    source: "fixture_tree",
    character_name: bundle.creation.name,
    public_event_count: publicEventCount,
    initial_state_sha256: hash(canonicalStateJson(bundle.initialState)),
    final_state_sha256: steps.at(-1).after_state_sha256,
    presentation_color: presentationColor(bundle.manifest.id),
    contract: manifestContract(bundle.manifest),
    steps,
    observable: flattenDomainValue(bundle.finalState.value),
  };
}
