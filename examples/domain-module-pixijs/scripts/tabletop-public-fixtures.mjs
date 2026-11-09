#!/usr/bin/env node
//! Build the deterministic public projection consumed by the PixiJS app.
//!
//! The script reads the checked-in fixture trees and canonical projects with Node built-ins,
//! validates every replay record, filters authority-only events, and writes only public event
//! payloads and public state. It performs no network access.

import { writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { loadAllAdapterRuns } from "../src/tabletop-adapter-fixtures.node.js";
import { validateProjection } from "../src/tabletop-adapters.js";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const fixtureRoot = resolve(scriptDirectory, "../../tabletop-adapters");
const outputPath = resolve(scriptDirectory, "../src/tabletop-fixtures.generated.json");

const adapters = loadAllAdapterRuns(fixtureRoot);
const projection = validateProjection({
  projection_format_version: 1,
  generated_by: "scripts/tabletop-public-fixtures.mjs",
  source: "checked-in tabletop adapter fixture trees and canonical projects",
  adapters,
});

writeFileSync(outputPath, `${JSON.stringify(projection, null, 2)}\n`);
console.log(
  `Wrote ${adapters.length} adapters and ${adapters.reduce(
    (total, adapter) => total + adapter.event_count,
    0,
  )} events to ${outputPath}`,
);
