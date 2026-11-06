#!/usr/bin/env node
//! Headless PixiJS-boundary consumer for the Open Tale fixture.
//
//! This module reads the portable JSON fixtures with Node built-ins, checks that every receipt
//! drives the recorded event sequence, and exposes a small `renderEvents` helper. A PixiJS v8
//! host supplies `PIXI` and an `Application`; the workspace does not install PixiJS here.

import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(root, relativePath), "utf8"));
}

export function validateFixtures() {
  const manifest = readJson("open-tale.tabletop-adapter.json");
  if (manifest.id !== "org.weave.tabletop.open_tale") {
    throw new Error(`unexpected adapter id: ${manifest.id}`);
  }
  if (manifest.provenance?.license !== "Apache-2.0") {
    throw new Error("adapter provenance is not Apache-2.0");
  }

  const index = readJson("playthrough/index.json");
  const lines = [];
  for (const step of index) {
    for (const path of [
      step.request_json,
      step.request_ron,
      step.receipt_json,
      step.receipt_ron,
    ]) {
      if (!existsSync(resolve(root, path))) {
        throw new Error(`missing fixture artifact: ${path}`);
      }
    }

    const request = readJson(step.request_json);
    const receipt = readJson(step.receipt_json);
    if (receipt.operation !== request.operation) {
      throw new Error(`${step.id}: receipt operation does not match request`);
    }
    if (receipt.capability !== request.capability) {
      throw new Error(`${step.id}: receipt capability does not match request`);
    }
    if (
      receipt.adapter.id !== request.adapter.id ||
      receipt.adapter.version !== request.adapter.version ||
      receipt.adapter.content_sha256 !== request.adapter.content_sha256
    ) {
      throw new Error(`${step.id}: receipt adapter coordinate does not match request`);
    }
    const kinds = receipt.events.map((event) => event.kind);
    if (JSON.stringify(kinds) !== JSON.stringify(step.event_kinds)) {
      throw new Error(`${step.id}: event sequence does not match the fixture index`);
    }
    for (const event of receipt.events) {
      if (!/^[0-9a-f]{64}$/.test(event.payload_sha256)) {
        throw new Error(`${step.id}: event payload hash is not a SHA-256 digest`);
      }
    }
    lines.push(`${step.id}: ${kinds.join(", ")}`);
  }
  return { manifest, index, lines };
}

export function renderEvents(PIXI, app, lines) {
  const style = { fill: 0xffffff, fontSize: 14, fontFamily: "monospace" };
  lines.forEach((line, index) => {
    const text = new PIXI.Text({ text: line, style });
    text.y = index * 18;
    app.stage.addChild(text);
  });
  return app;
}

function main() {
  const { lines } = validateFixtures();
  for (const line of lines) {
    console.log(line);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
