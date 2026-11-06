import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { characterPackPresentation } from "../src/character-pack.js";

const runtimePackUrl = new URL(
  "../../character-pack-bevy/fixtures/runtime-pack.character-pack.json",
  import.meta.url,
);

async function runtimePack() {
  return JSON.parse(await readFile(runtimePackUrl, "utf8"));
}

test("reads the checked portable Character runtime pack", async () => {
  const presentation = characterPackPresentation(await runtimePack());
  assert.deepEqual(presentation, {
    packId: "org.weave.example.character_pack.thistlewood",
    packVersion: "1.0.0",
    license: "MIT",
    id: "org.weave.example.character.mira_thistlewood",
    displayName: "Mira Thistlewood",
    accentColor: "#7BC96F",
    greeting: "Morning. I brought a spare map in case the path surprises us.",
    opennessBand: "high",
    alignmentLabel: "Seeking",
    birthDate: "2001-06-21",
    innerLife:
      "Keeps a small garden because patient work makes a place feel shared.",
    voice: "Speaks plainly, then leaves room for the other person to decide.",
    dateContext:
      "Midsummer evenings stretched late enough to finish a long walk before dusk.",
  });
});

test("renders the same observable values as the Bevy consumer", async () => {
  const presentation = characterPackPresentation(await runtimePack());
  assert.equal(presentation.displayName, "Mira Thistlewood");
  assert.equal(presentation.accentColor, "#7BC96F");
  assert.equal(presentation.opennessBand, "high");
  assert.equal(presentation.alignmentLabel, "Seeking");
  assert.equal(presentation.birthDate, "2001-06-21");
  assert.match(presentation.greeting, /spare map/u);
});

test("rejects an unsupported pack version", async () => {
  const pack = await runtimePack();
  pack.pack_format_version = 2;
  assert.throws(
    () => characterPackPresentation(pack),
    /unsupported character pack format version/u,
  );
});

test("rejects a missing relationship reference", async () => {
  const pack = await runtimePack();
  const profile = pack.characters[0];
  profile.relationships = {
    stray: {
      source_character_id: profile.id,
      target_character_id: "org.weave.example.character.absent",
      kind: "org.weave.relationship.friend",
      origin: "authored",
    },
  };
  assert.throws(
    () => characterPackPresentation(pack),
    /relationship target is absent from the pack/u,
  );
});

test("rejects a noncanonical presentation colour", async () => {
  const pack = await runtimePack();
  pack.characters[0].presentation.palette.accent = "#7bc96f";
  assert.throws(
    () => characterPackPresentation(pack),
    /runtime presentation has no approved accent colour/u,
  );
});
