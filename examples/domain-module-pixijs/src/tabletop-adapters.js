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

/** Deterministic presentation colour, matching the Bevy consumer. */
export function presentationColor(id) {
  let hash = 2_166_136_261;
  for (const byte of new TextEncoder().encode(id)) {
    hash ^= byte;
    hash = Math.imul(hash, 16_777_619) >>> 0;
  }
  return `hsl(${hash % 360}, 52%, 58%)`;
}

/** Validate the public projection shared with the PixiJS app. */
export function validateProjection(projection) {
  if (projection?.projection_format_version !== 1) {
    reject("unsupported tabletop consumer projection version");
  }
  if (!Array.isArray(projection.adapters) || projection.adapters.length !== ADAPTER_CONSUMERS.length) {
    reject("tabletop consumer projection does not list every adapter");
  }
  for (const adapter of projection.adapters) {
    if (typeof adapter.key !== "string" || !Array.isArray(adapter.steps)) {
      reject("invalid tabletop consumer adapter entry");
    }
    requireDigest(adapter.final_state_sha256, `${adapter.key} final state`);
    if (!/^hsl\(\d+, 52%, 58%\)$/u.test(adapter.presentation_color ?? "")) {
      reject(`${adapter.key} presentation colour is invalid`);
    }
    for (const step of adapter.steps) {
      if (typeof step.id !== "string" || !Array.isArray(step.events)) {
        reject(`${adapter.key} step is invalid`);
      }
      step.events.forEach((event, index) => {
        if (event.sequence !== index || typeof event.kind !== "string" || typeof event.capability !== "string") {
          reject(`${adapter.key} event sequence is invalid`);
        }
      });
    }
  }
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
      `${adapter.steps.length} steps · ${adapter.event_count} events`,
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
export function validateFixtureBundle(descriptor, bundle) {
  const { manifest, creation, preview, initialState, finalState, steps } = bundle;
  if (manifest?.manifest_format_version !== 1) {
    reject("unsupported tabletop manifest version");
  }
  if (manifest.id !== descriptor.id || manifest.version !== descriptor.version) {
    reject("tabletop manifest coordinate does not match the consumer");
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
  let expectedBefore = steps[0].receipt.before_state_sha256;
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
    if (request.operation !== receipt.operation || request.capability !== receipt.capability) {
      reject(`${step.id}: request and receipt operation differ`);
    }
    requireDigest(request.expected_state_sha256, `${step.id} expected state`);
    requireDigest(receipt.request_sha256, `${step.id} request hash`);
    requireDigest(receipt.before_state_sha256, `${step.id} before state`);
    requireDigest(receipt.after_state_sha256, `${step.id} after state`);
    if (receipt.before_state_sha256 !== request.expected_state_sha256) {
      reject(`${step.id}: request does not expect the receipt's preceding state`);
    }
    if (index > 0 && receipt.before_state_sha256 !== expectedBefore) {
      reject(`${step.id}: state hash chain is broken`);
    }
    expectedBefore = receipt.after_state_sha256;
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
      requireDigest(event.payload_sha256, `${step.id} event payload`);
      if (event.visibility === "public" && event.payload == null) {
        reject(`${step.id}: public event has no payload`);
      }
    });
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
export function publicFixtureView(descriptor, bundle) {
  const steps = bundle.steps.map((step) => ({
    id: step.id,
    operation: step.request.operation,
    events: step.receipt.events
      .filter((event) => event.visibility === "public")
      .map((event) => ({
        sequence: event.sequence,
        kind: event.kind,
        capability: event.capability,
        payload: decodeDomainValue(event.payload),
      })),
  }));
  const publicEventCount = steps.reduce((total, step) => total + step.events.length, 0);
  return {
    key: descriptor.key,
    id: bundle.manifest.id,
    version: bundle.manifest.version,
    source: "fixture_tree",
    character_name: bundle.creation.name,
    steps,
    event_count: bundle.steps.reduce((total, step) => total + step.receipt.events.length, 0),
    public_event_count: publicEventCount,
    final_state_sha256: bundle.steps.at(-1).receipt.after_state_sha256,
    presentation_color: presentationColor(bundle.manifest.id),
    observable: flattenDomainValue(bundle.finalState.value),
  };
}
