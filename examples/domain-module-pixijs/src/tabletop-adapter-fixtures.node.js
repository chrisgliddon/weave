//! Node-only loader for every tabletop adapter's checked-in replay data.
//!
//! The browser bundle consumes the deterministic public projection written by
//! `scripts/tabletop-public-fixtures.mjs`. Tests use this loader to read the original fixture files,
//! validate the complete replay record, and prove that bad fixtures are rejected.

import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  ADAPTER_CONSUMERS,
  canonicalPayloadJson,
  canonicalStateJson,
  flattenPlain,
  manifestContract,
  presentationColor,
  publicFixtureView,
  reject,
  requireCoordinate,
  requireDigest,
  validateFixtureBundle,
} from "./tabletop-adapters.js";

const pinnedProjectionPath = join(
  dirname(fileURLToPath(import.meta.url)),
  "tabletop-fixtures.generated.json",
);

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function pinnedManifest(descriptor) {
  const projection = readJson(pinnedProjectionPath);
  const adapter = projection.adapters?.find((entry) => entry.key === descriptor.key);
  if (!adapter?.contract || adapter.id !== descriptor.id || adapter.version !== descriptor.version) {
    reject(`${descriptor.key}: pinned manifest contract is missing or stale`);
  }
  return {
    manifest_format_version: adapter.contract.manifest_format_version,
    id: adapter.id,
    version: adapter.version,
    operations: adapter.contract.operations,
    event_types: Object.fromEntries(
      adapter.contract.event_types.map((kind) => [kind, true]),
    ),
    visibility: {
      authority_host: ["public", "authoring", "host_only"],
      runtime: adapter.contract.runtime_visibility,
    },
  };
}

function sha256Text(text) {
  return createHash("sha256").update(text).digest("hex");
}

function sha256Json(value) {
  return sha256Text(JSON.stringify(value));
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
    reject(`operation ${operation} capability is not declared for the operation`);
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
  validateFixtureBundle(descriptor, bundle, sha256Text);
  if (descriptor.index) {
    verifyIndex(descriptor, directory, steps);
  }
  return publicFixtureView(descriptor, bundle, sha256Text);
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
  const authorityState = readJson(join(directory, "authority-state.tabletop-state.json"));
  const initialHash = sha256Text(canonicalStateJson(authorityState));
  const publicState = readJson(join(directory, descriptor.publicState));
  if (publicState?.projection_format_version !== 1) {
    reject("unsupported Freehack public state version");
  }
  requireCoordinate(publicState, manifest, "Freehack public state");

  let contentSha256 = publicState.adapter.content_sha256;
  const steps = descriptor.publicOrder.map((id, stepIndex) => {
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

    const authority = readJson(
      join(directory, "playthrough", `${id}.authority-receipt.json`),
    );
    if (authority?.projection_format_version !== 1 || !authority.receipt) {
      reject(`${id}: authority receipt is missing or unsupported`);
    }
    const source = authority.receipt;
    requireCoordinate(source, manifest, `${id} authority receipt`);
    if (source.operation !== receipt.operation || source.capability !== receipt.capability) {
      reject(`${id}: authority receipt operation differs from the public receipt`);
    }
    const sourceEvents = source.events
      .filter((event) => event.visibility === "public")
      .map((event, sequence) => {
        if (event.payload == null) {
          reject(`${id}: authority public event has no payload`);
        }
        if (sha256Text(canonicalPayloadJson(event.payload)) !== event.payload_sha256) {
          reject(`${id}: authority public event payload digest does not match`);
        }
        return {
          sequence,
          kind: event.kind,
          capability: event.capability,
          payload: event.payload,
          payload_sha256: event.payload_sha256,
        };
      });
    if (sourceEvents.length !== receipt.events.length) {
      reject(`${id}: public receipt does not match its authority receipt`);
    }
    receipt.events.forEach((event, index) => {
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
      const anchored = sourceEvents[index];
      if (
        anchored.kind !== event.kind ||
        anchored.capability !== event.capability ||
        JSON.stringify(anchored.payload) !== JSON.stringify(event.payload)
      ) {
        reject(`${id}: public receipt event differs from its authority receipt`);
      }
    });
    requireDigest(source.request_sha256, `${id} authority request`);
    requireDigest(source.before_state_sha256, `${id} authority before state`);
    requireDigest(source.after_state_sha256, `${id} authority after state`);
    if (stepIndex === 0 && source.before_state_sha256 !== initialHash) {
      reject(`${id}: first public step does not start from the authority initial state`);
    }
    return {
      id,
      operation: receipt.operation,
      capability: receipt.capability,
      request_sha256: source.request_sha256,
      before_state_sha256: source.before_state_sha256,
      after_state_sha256: source.after_state_sha256,
      events: sourceEvents,
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
  const publicEventCount = steps.reduce((total, step) => total + step.events.length, 0);
  return {
    key: descriptor.key,
    id: manifest.id,
    version: manifest.version,
    source: "freehack_public",
    character_name: creation.name,
    public_event_count: publicEventCount,
    initial_state_sha256: initialHash,
    final_state_sha256: steps.at(-1).public_state_sha256,
    presentation_color: presentationColor(manifest.id),
    contract: manifestContract(manifest),
    steps: steps.map(({ public_state, public_state_sha256, ...step }) => step),
    observable: flattenPlain(publicState),
  };
}

function loadCanonicalProject(descriptor, directory) {
  const manifest = pinnedManifest(descriptor);
  requireManifest(descriptor, manifest);
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
    requireOperation(manifest, step.operation, step.capability);
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
    const declaration = manifest.operations[step.operation];
    for (const kind of step.event_kinds) {
      if (!declaration.event_kinds.includes(kind) || !Object.hasOwn(manifest.event_types, kind)) {
        reject(`${descriptor.key}: expected event kind is not declared for the operation`);
      }
    }
    const publicKinds = step.event_kinds.filter(
      (kind) => kind !== "entropy_trace" && !kind.endsWith("_authority"),
    );
    publicEvents += publicKinds.length;
    return {
      id: step.request_id,
      operation: step.operation,
      capability: step.capability,
      request_sha256: step.request_sha256,
      before_state_sha256: step.before_state_sha256,
      after_state_sha256: step.after_state_sha256,
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
    public_event_count: publicEvents,
    initial_state_sha256: expected.initial_state_sha256,
    final_state_sha256: expected.final_state_sha256,
    presentation_color: presentationColor(project.adapter_id),
    contract: manifestContract(manifest),
    steps,
    observable: [],
  };
}
