import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  ADAPTER_CONSUMERS,
  renderAdapterRuns,
  validateProjection,
} from "../src/tabletop-adapters.js";
import {
  loadAdapterRun,
  loadAllAdapterRuns,
} from "../src/tabletop-adapter-fixtures.node.js";

const testDirectory = dirname(fileURLToPath(import.meta.url));
const fixtureRoot = resolve(testDirectory, "../../tabletop-adapters");
const generatedPath = resolve(testDirectory, "../src/tabletop-fixtures.generated.json");

function descriptor(key) {
  const found = ADAPTER_CONSUMERS.find((adapter) => adapter.key === key);
  assert.ok(found, `missing descriptor ${key}`);
  return found;
}

function withAdapterCopy(key, check) {
  const root = mkdtempSync(join(tmpdir(), "weave-tabletop-consumer-"));
  try {
    cpSync(join(fixtureRoot, key), join(root, key), { recursive: true });
    check(root, join(root, key));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

test("loads every checked-in adapter with the recorded step and event counts", () => {
  const runs = loadAllAdapterRuns(fixtureRoot);
  assert.deepEqual(
    runs.map((run) => run.key),
    ADAPTER_CONSUMERS.map((adapter) => adapter.key),
  );
  assert.deepEqual(
    runs.map((run) => run.steps.length),
    [7, 11, 14, 11, 6, 1, 2],
  );
  assert.deepEqual(
    runs.map((run) => run.public_event_count),
    [7, 20, 15, 28, 8, 1, 2],
  );
  for (const run of runs) {
    assert.match(run.final_state_sha256, /^[0-9a-f]{64}$/u);
    assert.match(run.presentation_color, /^hsl\(\d+, 52%, 58%\)$/u);
    assert.ok(run.event_count >= run.public_event_count);
    assert.ok(run.steps.every((step) => typeof step.id === "string"));
  }
  for (const run of runs.slice(0, 4)) {
    assert.ok(run.observable.length > 0, `${run.key} observable values`);
  }
});

test("Open Tale replays its recorded eleven-step playthrough with public events", () => {
  const run = loadAdapterRun(descriptor("open-tale"), fixtureRoot);
  assert.equal(run.character_name, "Alma Reed");
  assert.equal(run.steps.length, 11);
  assert.equal(run.steps[0].id, "01_cross_the_ford");
  assert.equal(run.steps.at(-1).id, "11_inspect_character");
  assert.equal(run.public_event_count, 28);
  assert.ok(run.observable.includes("experience=12"));
  assert.ok(run.observable.includes("health=10"));
  assert.ok(run.observable.includes("combats.reed_watch.status=concluded"));
});

test("Freehack exposes only its public receipts and public final state", () => {
  const run = loadAdapterRun(descriptor("freehack"), fixtureRoot);
  assert.equal(run.character_name, "Tavi Quill");
  assert.equal(run.steps.length, 14);
  assert.equal(run.public_event_count, 15);
  for (const step of run.steps) {
    for (const event of step.events) {
      assert.equal("visibility" in event, false);
      assert.equal("payload_sha256" in event, false);
      assert.ok(event.payload);
    }
  }
  const publicState = readJson(
    join(fixtureRoot, "freehack", "public-state.freehack-public-state.json"),
  );
  const expectedHash = createHash("sha256")
    .update(JSON.stringify(publicState))
    .digest("hex");
  assert.equal(run.final_state_sha256, expectedHash);
  assert.ok(run.observable.some((line) => line.startsWith("memories[")));
});

test("the generated public projection matches a fresh fixture load", () => {
  const projection = validateProjection(readJson(generatedPath));
  const runs = loadAllAdapterRuns(fixtureRoot);
  assert.equal(projection.adapters.length, runs.length);
  projection.adapters.forEach((adapter, index) => {
    const run = runs[index];
    assert.equal(adapter.key, run.key);
    assert.deepEqual(adapter.steps, run.steps);
    assert.equal(adapter.event_count, run.event_count);
    assert.equal(adapter.public_event_count, run.public_event_count);
    assert.equal(adapter.final_state_sha256, run.final_state_sha256);
    assert.equal(adapter.presentation_color, run.presentation_color);
    assert.deepEqual(adapter.observable, run.observable);
  });
});

test("PixiJS rendering covers every adapter and every public event", () => {
  const projection = validateProjection(readJson(generatedPath));
  const rendered = [];
  class Text {
    constructor({ text, style }) {
      assert.equal(typeof text, "string");
      assert.equal(typeof style.fill, "string");
      rendered.push({ text, style, x: undefined, y: undefined });
    }
  }
  const children = [];
  const app = {
    stage: {
      addChild(node) {
        children.push(node);
      },
    },
  };

  assert.equal(renderAdapterRuns({ Text }, app, projection.adapters), app);
  assert.equal(children.length, rendered.length);
  assert.ok(children.length > 0);
  for (const adapter of projection.adapters) {
    assert.ok(
      rendered.some((node) => node.text.startsWith(`${adapter.key} `)),
      `${adapter.key} header`,
    );
    for (const step of adapter.steps) {
      for (const event of step.events) {
        assert.ok(
          rendered.some((node) => node.text.includes(event.kind)),
          `${adapter.key} ${event.kind}`,
        );
      }
    }
  }
  for (const node of children) {
    assert.ok(Number.isInteger(node.x) && Number.isInteger(node.y));
  }
});

test("rejects a fixture with an unsupported state version", () => {
  const plugAndPlay = descriptor("plug-and-play");
  withAdapterCopy(plugAndPlay.key, (root, directory) => {
    const path = join(directory, plugAndPlay.initialState);
    const state = readJson(path);
    state.state_format_version = 2;
    writeFileSync(path, JSON.stringify(state));
    assert.throws(
      () => loadAdapterRun(plugAndPlay, root),
      /unsupported tabletop state version/u,
    );
  });
});

test("rejects a fixture with a mismatched adapter coordinate", () => {
  const plugAndPlay = descriptor("plug-and-play");
  withAdapterCopy(plugAndPlay.key, (root, directory) => {
    const path = join(directory, "playthrough", "canal_chase.tabletop-request.json");
    const request = readJson(path);
    request.adapter.version = "9.9.9";
    writeFileSync(path, JSON.stringify(request));
    assert.throws(
      () => loadAdapterRun(plugAndPlay, root),
      /adapter coordinate/u,
    );
  });
});

test("rejects a fixture whose final state does not match the last receipt", () => {
  const openTale = descriptor("open-tale");
  withAdapterCopy(openTale.key, (root, directory) => {
    const path = join(directory, openTale.finalState);
    const state = readJson(path);
    state.revision += 1;
    writeFileSync(path, JSON.stringify(state));
    assert.throws(
      () => loadAdapterRun(openTale, root),
      /final state does not match/u,
    );
  });
});

test("rejects a Freehack public receipt that carries an authority field", () => {
  const freehack = descriptor("freehack");
  withAdapterCopy(freehack.key, (root, directory) => {
    const path = join(
      directory,
      "playthrough",
      "create_crossing_track.public-receipt.json",
    );
    const receipt = readJson(path);
    receipt.events[0].visibility = "public";
    writeFileSync(path, JSON.stringify(receipt));
    assert.throws(
      () => loadAdapterRun(freehack, root),
      /public event schema is invalid/u,
    );
  });
});

test("rejects a canonical project with a broken expected state chain", () => {
  const spark = descriptor("spark");
  withAdapterCopy(spark.key, (root, directory) => {
    const path = join(directory, spark.expected);
    const expected = readJson(path);
    expected.steps[0].before_state_sha256 = "0".repeat(64);
    writeFileSync(path, JSON.stringify(expected));
    assert.throws(
      () => loadAdapterRun(spark, root),
      /state-hash chain/u,
    );
  });
});
