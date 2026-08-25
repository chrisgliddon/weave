import { Application, Text } from "pixi.js";
import tracerStory from "../../domain-modules/contract/tracer.story.json";
import britishColumbiaStory from "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.json";
import hokkaidoStory from "../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.json";
import maldivesStory from "../../domain-modules/weave-world/corpus/stories/maldives.story.json";
import newZealandStory from "../../domain-modules/weave-world/reference-place.story.json";
import composedWorldStory from "../../domain-modules/weave-world/composed-setting.story.json";
import characterStory from "../../domain-modules/weave-character/ari-vale.story.json";
import temporalCharacterStory from "../../domain-modules/weave-character/context/runtime/ari-vale-temporal.story.json";
import tabletopStory from "../../tabletop-adapters/plug-and-play/runtime/ember-vale.story.json";
import tabletopReceipt from "../../tabletop-adapters/plug-and-play/runtime.tabletop-receipt.json";
import dungeonpunkStory from "../../tabletop-adapters/dungeonpunk/runtime/vesper-ash.story.json";
import dungeonpunkReceipt from "../../tabletop-adapters/dungeonpunk/runtime.tabletop-receipt.json";
import freehackStory from "../../tabletop-adapters/freehack/runtime/tavi-quill.story.json";
import freehackReceipt from "../../tabletop-adapters/freehack/public-receipt.freehack-public-receipt.json";

import {
  alignmentCharacterPresentation,
  characterPresentation,
  expressionCharacterPresentation,
  identityCharacterPresentation,
  projectionCharacterPresentation,
  temporalCharacterPresentation,
} from "./character-presentation.js";
import { readModuleExport } from "./domain-values.js";
import { composedWorldPresentation } from "./world-presentation.js";
import {
  dungeonpunkPresentation,
  freehackPresentation,
  tabletopPresentation,
} from "./tabletop-presentation.js";

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
  const expressionCharacter = expressionCharacterPresentation(characterStory);
  const identityPresentation = identityCharacterPresentation(characterStory);
  const alignmentCharacter = alignmentCharacterPresentation(characterStory);
  const projectionCharacter = projectionCharacterPresentation(characterStory);
  const temporalCharacter = temporalCharacterPresentation(temporalCharacterStory);
  const tabletop = tabletopPresentation(tabletopStory, tabletopReceipt);
  const dungeonpunk = dungeonpunkPresentation(dungeonpunkStory, dungeonpunkReceipt);
  const freehack = freehackPresentation(freehackStory, freehackReceipt);

  const app = new Application();
  await app.init({
    width: 720,
    height: 640,
    background: composedWorld.background,
    antialias: true,
    autoDensity: true,
    resolution: Math.min(window.devicePixelRatio, 2),
  });
  canvasHost.append(app.canvas);

  const reading = new Text({
    text: `${worldLines.join("\n")}\n${composedWorld.harbor} · ${composedWorld.behavior} · ${composedWorld.primaryBiome}\n\n${character.displayName} · ${identityPresentation.pronounSubject} · ${identityPresentation.accent} · ${identityPresentation.visualTone}\n${expressionCharacter.term.surface} · ${expressionCharacter.preference.polarity} ${expressionCharacter.preference.target}\n${expressionCharacter.voice.instruction} · ${expressionCharacter.template.id}\ncreativity ${character.creativity} · derived OCEAN openness ${character.oceanOpenness}\n${alignmentCharacter.values.map((value) => value.label).join(" · ")} · alignment write-back ${alignmentCharacter.canonicalPersonalityWriteBack}\n${projectionCharacter.values.map((value) => value.label).join(" · ")} · projection HEXACO write-back ${projectionCharacter.writeBack.hexaco}\n${temporalCharacter.cues.length} reviewed temporal cues · presentation write-back ${identityPresentation.canonicalPersonalityWriteBack}\n\n${tabletop.name} · A ${tabletop.attributes.agility} Bn ${tabletop.attributes.brains} Bw ${tabletop.attributes.brawn} W ${tabletop.attributes.wits}\n${tabletop.check.outcome} (${tabletop.check.total}) · Fortune ${tabletop.fortune} · Survivability ${tabletop.survivability}\n${dungeonpunk.name} · STR ${dungeonpunk.attributes.strength} DEX ${dungeonpunk.attributes.dexterity} CON ${dungeonpunk.attributes.constitution}\nStruggle ${dungeonpunk.roll.outcome} (${dungeonpunk.roll.selected}) · HP ${dungeonpunk.hp} · Stress ${dungeonpunk.stress} · XP ${dungeonpunk.xp}\n${freehack.name} · ${freehack.archetype} · Focus ${freehack.modifiers.focus} · Fatigue ${freehack.fatigue}\nFreehack ${freehack.check.outcome} (${freehack.check.magnitude}) · gantry ${freehack.gantryStatus} · ${freehack.publicMemoryCount} public memories\n${label} · ${phase} · ${intensity}`,
    style: {
      align: "center",
      fill: composedWorld.foreground,
      fontFamily: "ui-rounded, system-ui, sans-serif",
      fontSize: 19,
      fontWeight: "600",
      lineHeight: 42,
    },
  });
  reading.anchor.set(0.5);
  reading.position.set(app.screen.width / 2, app.screen.height / 2);
  app.stage.addChild(reading);
  status.textContent =
    "PixiJS read portable World, Character, Plug-And-Play, Dungeonpunk, and Freehack exports; Freehack uses only its explicit public story/receipt schema, tabletop play retains exact adapter hashes and redacted audit boundaries, projection labels retain review lineage and no write-back authority, and temporal fact/cue lineage stays separate.";
} catch (error) {
  status.textContent = `The portable domain export could not be displayed: ${error.message ?? error}`;
}
