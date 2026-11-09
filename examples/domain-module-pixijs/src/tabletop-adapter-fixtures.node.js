//! Node-only loader for every tabletop adapter's checked-in replay data.
//!
//! The browser bundle consumes the deterministic public projection written by
//! `scripts/tabletop-public-fixtures.mjs`. Tests use this loader to read the original fixture files,
//! validate the complete replay record, and prove that bad fixtures are rejected.

import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import {
  ADAPTER_CONSUMERS,
  flattenPlain,
  presentationColor,
  publicFixtureView,
  reject,
  requireCoordinate,
  requireDigest,
  validateFixtureBundle,
} from "./tabletop-adapters.js";

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function sha256Json(value) {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function requireManifest(descriptor, manifest) {
  if (manifest?.manifest_format_version !== 1) {
    reject("unsupported tabletop manifest version");
  }
  if (manifest.id !== descriptor.id || manifest.version !== descriptor.version) {
    reject("tabletop manifest coordinate does not match the consumer");
  }
}

function requireOperation(manifest, operation, capability) {
  const declaration = manifest.operations?.[operation];
  if (!declaration) {
    reject(`operation ${operation} is not declared by the adapter manifest`);
  }
  if (declaration.required_capability !== capability) {
    reject(`operation ${operation} capability does not match the adapter manifest`);
  }
}

/** Load and validate every adapter's checked-in replay data. */
export function loadAllAdapterRuns(root) {
  return ADAPTER_CONSUMERS.map((descriptor) => loadAdapterRun(descriptor, root));
}

/** Load and validate one adapter's checked-in replay data. */
export function loadAdapterRun(descriptor, root) {
  const directory = join(root, descriptor.key);
  if (descriptor.kind === "canonical_project") {
    return loadCanonicalProject(descriptor, directory);
  }
  if (descriptor.kind === "freehack_public") {
    return loadFreehackPublic(descriptor, directory);
  }
  return loadFixtureTree(descriptor, directory);
}

function loadFixtureTree(descriptor, directory) {
  const manifest = readJson(join(directory, descriptor.manifest));
  requireManifest(descriptor, manifest);
  const creation = readJson(join(directory, descriptor.creation));
  const preview = readJson(join(directory, descriptor.preview));
  const initialState = readJson(join(directory, descriptor.initialState));
  const finalState = readJson(join(directory, descriptor.finalState));
  const steps = loadFixtureSteps(descriptor, directory, initialState);
  const bundle = { manifest, creation, preview, initialState, finalState, steps };
  validateFixtureBundle(descriptor, bundle);
  if (descriptor.index) {
    verifyIndex(descriptor, directory, steps);
  }
  return publicFixtureView(descriptor, bundle);
}

function loadFixtureSteps(descriptor, directory, initialState) {
  const playthrough = join(directory, "playthrough");
  const names = readdirSync(playthrough)
    .filter((name) => name.endsWith(".tabletop-request.json"))
    .sort();
  const steps = names.map((name) => {
    const id = name.slice(0, -".tabletop-request.json".length);
    const request = readJson(join(playthrough, name));
    const receipt = readJson(join(playthrough, `${id}${descriptor.receiptSuffix}`));
    return { id, request, receipt };
  });
  if (steps.length === 0) {
    reject(`${descriptor.key}: playthrough has no request files`);
  }

  const first = steps.findIndex(
    (step) =>
      step.receipt.before_revision === initialState.revision &&
      JSON.stringify(step.receipt.entropy_before) === JSON.stringify(initialState.entropy),
  );
  if (first < 0) {
    reject(`${descriptor.key}: no playthrough step starts from the initial state`);
  }
  const ordered = [steps[first]];
  while (ordered.length < steps.length) {
    const previous = ordered.at(-1).receipt.after_state_sha256;
    const next = steps.find(
      (step) =>
        !ordered.includes(step) && step.request.expected_state_sha256 === previous,
    );
    if (!next) {
      reject(`${descriptor.key}: playthrough state-hash chain is broken`);
    }
    ordered.push(next);
  }
  return ordered;
}

function verifyIndex(descriptor, directory, steps) {
  const index = readJson(join(directory, descriptor.index));
  if (!Array.isArray(index) || index.length !== steps.length) {
    reject(`${descriptor.key}: playthrough index length does not match the replay`);
  }
  index.forEach((entry, position) => {
    if (typeof entry.request_json !== "string" || !Array.isArray(entry.event_kinds)) {
      reject(`${descriptor.key}: playthrough index entry is invalid`);
    }
    const expectedId = entry.request_json
      .split("/")
      .at(-1)
      .replace(".tabletop-request.json", "");
    if (steps[position].id !== expectedId) {
      reject(`${descriptor.key}: playthrough index order does not match the replay`);
    }
    const kinds = steps[position].receipt.events.map((event) => event.kind);
    if (JSON.stringify(entry.event_kinds) !== JSON.stringify(kinds)) {
      reject(`${descriptor.key}: playthrough index event sequence does not match the receipt`);
    }
  });
}

function loadFreehackPublic(descriptor, directory) {
  const manifest = readJson(join(directory, descriptor.manifest));
  requireManifest(descriptor, manifest);
  const creation = readJson(join(directory, descriptor.creation));
  if (creation?.creation_format_version !== 1 || typeof creation.name !== "string") {
    reject("unsupported tabletop creation version");
  }
  const publicState = readJson(join(directory, descriptor.publicState));
  if (publicState?.projection_format_version !== 1) {
    reject("unsupported Freehack public state version");
  }
  requireCoordinate(publicState, manifest, "Freehack public state");

  let contentSha256 = publicState.adapter.content_sha256;
  const steps = descriptor.publicOrder.map((id) => {
    const receipt = readJson(
      join(directory, "playthrough", `${id}${descriptor.publicReceiptSuffix}`),
    );
    if (receipt?.projection_format_version !== 1) {
      reject(`${id}: unsupported Freehack public receipt version`);
    }
    requireCoordinate(receipt, manifest, `${id} public receipt`);
    requireOperation(manifest, receipt.operation, receipt.capability);
    if (receipt.adapter.content_sha256 !== contentSha256) {
      reject(`${id}: public receipt adapter content differs from the public state`);
    }
    contentSha256 = receipt.adapter.content_sha256;
    requireDigest(receipt.public_state_sha256, `${id} public state`);
    if (sha256Json(receipt.public_state) !== receipt.public_state_sha256) {
      reject(`${id}: public state hash does not match the receipt`);
    }
    requireCoordinate(receipt.public_state, manifest, `${id} public state`);
    if (!Array.isArray(receipt.events) || receipt.events.length === 0) {
      reject(`${id}: public receipt has no events`);
    }
    const events = receipt.events.map((event, index) => {
      if (
        event.sequence !== index ||
        typeof event.kind !== "string" ||
        typeof event.capability !== "string" ||
        event.visibility !== undefined ||
        event.payload_sha256 !== undefined ||
        event.payload == null
      ) {
        reject(`${id}: public event schema is invalid`);
      }
      return {
        sequence: event.sequence,
        kind: event.kind,
        capability: event.capability,
        payload: event.payload,
      };
    });
    return {
      id,
      operation: receipt.operation,
      events,
      public_state: receipt.public_state,
      public_state_sha256: receipt.public_state_sha256,
    };
  });

  if (steps.length !== descriptor.expectedSteps) {
    reject(`${descriptor.key}: expected ${descriptor.expectedSteps} steps`);
  }
  if (JSON.stringify(steps.at(-1).public_state) !== JSON.stringify(publicState)) {
    reject("Freehack final public state does not match the checked-in public state");
  }
  const eventCount = steps.reduce((total, step) => total + step.events.length, 0);
  return {
    key: descriptor.key,
    id: manifest.id,
    version: manifest.version,
    source: "freehack_public",
    character_name: creation.name,
    steps: steps.map(({ public_state, public_state_sha256, ...step }) => step),
    event_count: eventCount,
    public_event_count: eventCount,
    final_state_sha256: steps.at(-1).public_state_sha256,
    presentation_color: presentationColor(manifest.id),
    observable: flattenPlain(publicState),
  };
}

function loadCanonicalProject(descriptor, directory) {
  const project = readJson(join(directory, descriptor.project));
  const expected = readJson(join(directory, descriptor.expected));
  if (project?.project_format_version !== 1 || expected?.project_format_version !== 1) {
    reject(`${descriptor.key}: unsupported canonical project version`);
  }
  if (project.adapter_id !== descriptor.id || project.adapter_version !== descriptor.version) {
    reject(`${descriptor.key}: canonical project selects a different adapter`);
  }
  if (expected.adapter_id !== descriptor.id || expected.adapter_version !== descriptor.version) {
    reject(`${descriptor.key}: expected output selects a different adapter`);
  }
  if (
    project.source?.source_class !== "synthetic_original" ||
    project.source?.license !== "MIT" ||
    expected.replay !== "exact"
  ) {
    reject(`${descriptor.key}: canonical project source is not the reviewed synthetic MIT source`);
  }
  if (!Array.isArray(project.scripted_path) || project.scripted_path.length !== descriptor.expectedSteps) {
    reject(`${descriptor.key}: scripted path length is not the reviewed length`);
  }
  if (!Array.isArray(expected.steps) || expected.steps.length !== project.scripted_path.length) {
    reject(`${descriptor.key}: expected output step count does not match the scripted path`);
  }
  requireDigest(expected.initial_state_sha256, `${descriptor.key} initial state`);
  requireDigest(expected.final_state_sha256, `${descriptor.key} final state`);

  let before = expected.initial_state_sha256;
  let recordedEvents = 0;
  let publicEvents = 0;
  const steps = expected.steps.map((step, index) => {
    const operation = project.scripted_path[index];
    if (
      step.request_id !== operation.request_id ||
      step.operation !== operation.operation ||
      step.capability !== operation.capability
    ) {
      reject(`${descriptor.key}: expected step does not match the scripted path`);
    }
    requireDigest(step.request_sha256, `${step.request_id} request`);
    requireDigest(step.before_state_sha256, `${step.request_id} before state`);
    requireDigest(step.receipt_sha256, `${step.request_id} receipt`);
    requireDigest(step.after_state_sha256, `${step.request_id} after state`);
    if (step.before_state_sha256 !== before) {
      reject(`${descriptor.key}: expected state-hash chain is broken`);
    }
    before = step.after_state_sha256;
    if (step.revision !== index + 1) {
      reject(`${descriptor.key}: expected revision does not advance exactly once`);
    }
    if (!Array.isArray(step.event_kinds) || step.event_kinds.length === 0) {
      reject(`${descriptor.key}: expected step has no event kinds`);
    }
    recordedEvents += step.event_kinds.length;
    const publicKinds = step.event_kinds.filter(
      (kind) => kind !== "entropy_trace" && !kind.endsWith("_authority"),
    );
    publicEvents += publicKinds.length;
    return {
      id: step.request_id,
      operation: step.operation,
      events: publicKinds.map((kind, sequence) => ({
        sequence,
        kind,
        capability: step.capability,
      })),
    };
  });
  if (before !== expected.final_state_sha256) {
    reject(`${descriptor.key}: final state hash does not match the expected chain`);
  }
  return {
    key: descriptor.key,
    id: project.adapter_id,
    version: project.adapter_version,
    source: "canonical_project",
    character_name: null,
    steps,
    event_count: recordedEvents,
    public_event_count: publicEvents,
    final_state_sha256: expected.final_state_sha256,
    presentation_color: presentationColor(project.adapter_id),
    observable: [],
  };
}
