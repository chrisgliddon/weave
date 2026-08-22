import { Application, Text } from "pixi.js";
import tracerStory from "../../domain-modules/contract/tracer.story.json";
import worldStory from "../../domain-modules/weave-world/reference-place.story.json";

import { readModuleExport } from "./domain-values.js";

const status = document.querySelector("#status");
const canvasHost = document.querySelector("#canvas");

try {
  const label = readModuleExport(tracerStory, "constellation", ["observation", "label"]);
  const intensity = readModuleExport(tracerStory, "constellation", ["observation", "intensity"]);
  const phase = readModuleExport(tracerStory, "constellation", ["phase"]);
  const worldName = readModuleExport(worldStory, "world", ["seed", "identity", "display_name"]);
  const primaryBiome = readModuleExport(worldStory, "world", ["seed", "primary_biome"]);
  const waterSetting = readModuleExport(worldStory, "world", ["seed", "water", "setting"]);

  const app = new Application();
  await app.init({
    width: 720,
    height: 360,
    background: "#111326",
    antialias: true,
    autoDensity: true,
    resolution: Math.min(window.devicePixelRatio, 2),
  });
  canvasHost.append(app.canvas);

  const reading = new Text({
    text: `${worldName}\n${primaryBiome}\n${waterSetting}\n\n${label} · ${phase} · ${intensity}`,
    style: {
      align: "center",
      fill: "#f2ecff",
      fontFamily: "ui-rounded, system-ui, sans-serif",
      fontSize: 26,
      fontWeight: "600",
      lineHeight: 48,
    },
  });
  reading.anchor.set(0.5);
  reading.position.set(app.screen.width / 2, app.screen.height / 2);
  app.stage.addChild(reading);
  status.textContent =
    "PixiJS read the same compiled tracer and Weave World JSON exports as the Bevy example.";
} catch (error) {
  status.textContent = `The portable domain export could not be displayed: ${error.message ?? error}`;
}
