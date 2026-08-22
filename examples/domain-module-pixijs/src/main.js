import { Application, Text } from "pixi.js";
import tracerStory from "../../domain-modules/contract/tracer.story.json";
import britishColumbiaStory from "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.json";
import hokkaidoStory from "../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.json";
import maldivesStory from "../../domain-modules/weave-world/corpus/stories/maldives.story.json";
import newZealandStory from "../../domain-modules/weave-world/reference-place.story.json";
import composedWorldStory from "../../domain-modules/weave-world/composed-setting.story.json";
import characterStory from "../../domain-modules/weave-character/ari-vale.story.json";

import { characterPresentation } from "./character-presentation.js";
import { readModuleExport } from "./domain-values.js";
import { composedWorldPresentation } from "./world-presentation.js";

const status = document.querySelector("#status");
const canvasHost = document.querySelector("#canvas");

try {
  const label = readModuleExport(tracerStory, "constellation", ["observation", "label"]);
  const intensity = readModuleExport(tracerStory, "constellation", ["observation", "intensity"]);
  const phase = readModuleExport(tracerStory, "constellation", ["phase"]);
  const worldLines = [britishColumbiaStory, hokkaidoStory, maldivesStory, newZealandStory].map(
    (story) => {
      const name = readModuleExport(story, "world", ["seed", "identity", "display_name"]);
      const climate = readModuleExport(story, "world", ["seed", "climate", "band"]);
      const biome = readModuleExport(story, "world", ["seed", "primary_biome"]);
      return `${name} · ${climate} · ${biome}`;
    },
  );
  const composedWorld = composedWorldPresentation(composedWorldStory);
  const character = characterPresentation(characterStory);

  const app = new Application();
  await app.init({
    width: 720,
    height: 500,
    background: composedWorld.background,
    antialias: true,
    autoDensity: true,
    resolution: Math.min(window.devicePixelRatio, 2),
  });
  canvasHost.append(app.canvas);

  const reading = new Text({
    text: `${worldLines.join("\n")}\n${composedWorld.harbor} · ${composedWorld.behavior} · ${composedWorld.primaryBiome}\n\n${character.displayName} · creativity ${character.creativity} · derived OCEAN openness ${character.oceanOpenness}\n${label} · ${phase} · ${intensity}`,
    style: {
      align: "center",
      fill: composedWorld.foreground,
      fontFamily: "ui-rounded, system-ui, sans-serif",
      fontSize: 21,
      fontWeight: "600",
      lineHeight: 42,
    },
  });
  reading.anchor.set(0.5);
  reading.position.set(app.screen.width / 2, app.screen.height / 2);
  app.stage.addChild(reading);
  status.textContent =
    "PixiJS read portable World and Character exports; the OCEAN value remains visibly derived and lossy.";
} catch (error) {
  status.textContent = `The portable domain export could not be displayed: ${error.message ?? error}`;
}
