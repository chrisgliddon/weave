import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { decodeDomainValue, readModuleExport } from "../src/domain-values.js";
import {
  alignmentCharacterPresentation,
  characterPresentation,
  expressionCharacterPresentation,
  identityCharacterPresentation,
  projectionCharacterPresentation,
  relationshipCharacterPresentation,
  temporalCharacterPresentation,
} from "../src/character-presentation.js";
import { composedWorldPresentation } from "../src/world-presentation.js";
import {
  dungeonpunkPresentation,
  freehackPresentation,
  tabletopPresentation,
} from "../src/tabletop-presentation.js";

const storyUrl = new URL("../../domain-modules/contract/tracer.story.json", import.meta.url);
const worldStoryUrls = [
  "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.json",
  "../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.json",
  "../../domain-modules/weave-world/corpus/stories/maldives.story.json",
  "../../domain-modules/weave-world/reference-place.story.json",
].map((path) => new URL(path, import.meta.url));
const composedWorldUrl = new URL(
  "../../domain-modules/weave-world/composed-setting.story.json",
  import.meta.url,
);
const characterUrl = new URL(
  "../../domain-modules/weave-character/ari-vale.story.json",
  import.meta.url,
);
const temporalCharacterUrl = new URL(
  "../../domain-modules/weave-character/context/runtime/ari-vale-temporal.story.json",
  import.meta.url,
);
const tabletopStoryUrl = new URL(
  "../../tabletop-adapters/plug-and-play/runtime/ember-vale.story.json",
  import.meta.url,
);
const tabletopReceiptUrl = new URL(
  "../../tabletop-adapters/plug-and-play/runtime.tabletop-receipt.json",
  import.meta.url,
);
const dungeonpunkStoryUrl = new URL(
  "../../tabletop-adapters/dungeonpunk/runtime/vesper-ash.story.json",
  import.meta.url,
);
const dungeonpunkReceiptUrl = new URL(
  "../../tabletop-adapters/dungeonpunk/runtime.tabletop-receipt.json",
  import.meta.url,
);
const freehackStoryUrl = new URL(
  "../../tabletop-adapters/freehack/runtime/tavi-quill.story.json",
  import.meta.url,
);
const freehackReceiptUrl = new URL(
  "../../tabletop-adapters/freehack/public-receipt.freehack-public-receipt.json",
  import.meta.url,
);

test("reads the exact portable tracer exports", async () => {
  const story = JSON.parse(await readFile(storyUrl, "utf8"));
  assert.equal(readModuleExport(story, "constellation", ["phase"]), "twilight");
  assert.equal(
    readModuleExport(story, "constellation", ["observation", "label"]),
    "Glasswing constellation",
  );
  assert.equal(readModuleExport(story, "constellation", ["observation", "intensity"]), 0.625);
});

test("reads Plug-And-Play character state and a visibility-projected receipt", async () => {
  const [story, receipt] = await Promise.all(
    [tabletopStoryUrl, tabletopReceiptUrl].map(async (url) =>
      JSON.parse(await readFile(url, "utf8")),
    ),
  );
  const presentation = tabletopPresentation(story, receipt);
  assert.equal(presentation.adapter.id, "org.weave.tabletop.plug_and_play");
  assert.equal(presentation.name, "Ember Vale");
  assert.ok(Number.isInteger(presentation.attributes.agility));
  assert.ok(Number.isInteger(presentation.fortune));
  assert.ok(Number.isInteger(presentation.survivability));
  assert.match(presentation.requestSha256, /^[0-9a-f]{64}$/u);
  assert.match(presentation.hiddenEntropySha256, /^[0-9a-f]{64}$/u);
  assert.equal(typeof presentation.check.outcome, "string");
  assert.equal(typeof presentation.check.rerolled, "boolean");
});

test("rejects a tabletop receipt that exposes host-only entropy", async () => {
  const [story, receipt] = await Promise.all(
    [tabletopStoryUrl, tabletopReceiptUrl].map(async (url) =>
      JSON.parse(await readFile(url, "utf8")),
    ),
  );
  const entropy = receipt.events.find((event) => event.kind === "entropy_trace");
  entropy.payload = { kind: "object", value: {} };
  assert.throws(
    () => tabletopPresentation(story, receipt),
    /invalid portable tabletop presentation/u,
  );
});

test("reads Dungeonpunk state and a visibility-projected Struggle receipt", async () => {
  const [story, receipt] = await Promise.all(
    [dungeonpunkStoryUrl, dungeonpunkReceiptUrl].map(async (url) =>
      JSON.parse(await readFile(url, "utf8")),
    ),
  );
  const presentation = dungeonpunkPresentation(story, receipt);
  assert.equal(presentation.adapter.id, "org.weave.tabletop.dungeonpunk");
  assert.equal(presentation.name, "Vesper Ash");
  assert.deepEqual(presentation.attributes, {
    charisma: 0,
    constitution: 1,
    dexterity: 1,
    intelligence: 1,
    strength: 2,
    wisdom: 0,
  });
  assert.equal(presentation.fate, 1);
  assert.equal(presentation.none, 0);
  assert.equal(presentation.roll.outcome, "failure");
  assert.equal(presentation.roll.selection, "highest");
  assert.equal(presentation.roll.helped, true);
  assert.equal(presentation.roll.push, true);
  assert.match(presentation.requestSha256, /^[0-9a-f]{64}$/u);
  assert.match(presentation.hiddenEntropySha256, /^[0-9a-f]{64}$/u);
});

test("rejects a Dungeonpunk receipt that exposes host-only entropy", async () => {
  const [story, receipt] = await Promise.all(
    [dungeonpunkStoryUrl, dungeonpunkReceiptUrl].map(async (url) =>
      JSON.parse(await readFile(url, "utf8")),
    ),
  );
  const entropy = receipt.events.find((event) => event.kind === "entropy_trace");
  entropy.payload = { kind: "object", value: {} };
  assert.throws(
    () => dungeonpunkPresentation(story, receipt),
    /invalid portable Dungeonpunk presentation/u,
  );
});

test("reads only the explicit Freehack public story and receipt", async () => {
  const [storySource, receiptSource] = await Promise.all(
    [freehackStoryUrl, freehackReceiptUrl].map((url) => readFile(url, "utf8")),
  );
  for (const marker of [
    "request_sha256",
    "payload_sha256",
    "entropy",
    "opposition",
    "draw_index",
    "signed_result",
    "_authority",
    "sealed_current",
    "hidden_watch",
    "sealed_route",
  ]) {
    assert.equal(`${storySource}\n${receiptSource}`.includes(marker), false, marker);
  }
  const presentation = freehackPresentation(
    JSON.parse(storySource),
    JSON.parse(receiptSource),
  );
  assert.equal(presentation.adapter.id, "org.weave.tabletop.freehack");
  assert.equal(presentation.name, "Tavi Quill");
  assert.equal(presentation.archetype, "courier");
  assert.equal(presentation.modifiers.focus, 4);
  assert.ok(Number.isInteger(presentation.modifiers.balance));
  assert.equal("clearance" in presentation.modifiers, false);
  assert.equal(presentation.fatigue, 0);
  assert.equal(presentation.tracks.fatigue.target_count, 3);
  assert.equal(presentation.tracks.crossing_ready.completed, true);
  assert.equal(presentation.tracks.crossing_ready.target_count, 1);
  assert.equal(presentation.gantryStatus, "resolved");
  assert.equal(presentation.publicMemoryCount, 2);
  assert.equal(presentation.check.outcome, "success");
  assert.equal(presentation.check.magnitude, 1);
  assert.equal(presentation.check.support_total, 4);
  assert.match(presentation.publicStateSha256, /^[0-9a-f]{64}$/u);
});

test("rejects a Freehack player receipt with an authority-shaped field", async () => {
  const [story, receipt] = await Promise.all(
    [freehackStoryUrl, freehackReceiptUrl].map(async (url) =>
      JSON.parse(await readFile(url, "utf8")),
    ),
  );
  receipt.request_sha256 = "0".repeat(64);
  assert.throws(
    () => freehackPresentation(story, receipt),
    /invalid portable Freehack public presentation/u,
  );
});

test("reads four exact portable world seeds", async () => {
  const stories = await Promise.all(
    worldStoryUrls.map(async (url) => JSON.parse(await readFile(url, "utf8"))),
  );
  assert.deepEqual(
    stories.map((story) => ({
      preset: readModuleExport(story, "world", ["seed", "identity", "preset"]),
      climate: readModuleExport(story, "world", ["seed", "climate", "band"]),
      hazards: readModuleExport(story, "world", ["seed", "hazards", "tendencies"]),
    })),
    [
      {
        preset: "british_columbia_temperate_forest",
        climate: "temperate_oceanic",
        hazards: ["heavy_precipitation", "seasonal_freeze", "slope_instability"],
      },
      {
        preset: "hokkaido_japan",
        climate: "humid_continental",
        hazards: ["seasonal_freeze"],
      },
      {
        preset: "maldives",
        climate: "tropical_oceanic",
        hazards: ["coastal_inundation"],
      },
      {
        preset: "aotearoa_new_zealand",
        climate: "temperate_oceanic",
        hazards: ["heavy_precipitation", "slope_instability"],
      },
    ],
  );
});

test("composed world rules, places, and environment alter PixiJS presentation", async () => {
  const story = JSON.parse(await readFile(composedWorldUrl, "utf8"));
  assert.deepEqual(composedWorldPresentation(story), {
    harbor: "Emberwake Harbor",
    road: "Lantern Road",
    primaryBiome: "temperate_conifer_forest",
    climateBand: "humid_continental",
    coastal: true,
    beaconRule: true,
    behavior: "beacon escort via Lantern Road",
    background: "#102825",
    foreground: "#d9f4e3",
  });
  assert.equal(story.modules.world.authored_overrides.length, 46);
  assert.deepEqual(story.modules.world.authored_overrides[0].path, [
    "places",
    "emberwake_harbor",
    "attributes",
    "booleans",
    "has_beacon",
  ]);
});

test("reads the complete portable Character profile", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  assert.deepEqual(characterPresentation(story), {
    id: "org.weave.character.ari_vale",
    displayName: "Ari Vale, Wayfinder",
    factorScores: {
      honestyHumility: 0.72,
      emotionality: 0.57,
      extraversion: 0.68,
      agreeableness: 0.63,
      conscientiousness: 0.78,
      openness: 0.83,
    },
    creativity: 0.86,
    oceanOpenness: 0.83,
    oceanIsLossy: true,
    oceanIsIndependentEvidence: false,
  });
});

test("reads typed identity presentation and reviewed catalog coordinates", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const presentation = identityCharacterPresentation(story);
  assert.equal(presentation.pronounSubject, "they");
  assert.equal(presentation.accent, "#D6A24A");
  assert.equal(presentation.avatarPath, "presentation/assets/ari-vale-avatar.svg");
  assert.equal(presentation.avatarAltText, "Geometric cedar and gold wayfinder avatar.");
  assert.equal(presentation.visualTone, "river_glass");
  assert.equal(presentation.catalog.id, "org.weave.character.presentation.glasswind");
  assert.equal(presentation.catalog.version, "1.0.0");
  assert.match(presentation.catalog.sha256, /^[0-9a-f]{64}$/);
  assert.match(presentation.proposalSha256, /^[0-9a-f]{64}$/);
  assert.match(presentation.reviewSha256, /^[0-9a-f]{64}$/);
  assert.equal(presentation.avatarLocked, true);
  assert.equal(presentation.canonicalPersonalityWriteBack, false);
});

test("rejects unsafe Character presentation assets without disclosing the path", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const rejected = "../private-avatar.svg";
  story.modules.character.exports.profile.value.value.presentation.value.assets.value.authored_avatar.value.path.value =
    rejected;
  assert.throws(
    () => identityCharacterPresentation(story),
    (error) => error instanceof TypeError && !error.message.includes(rejected),
  );
});

test("reads only approved alignment values with exact portable fingerprints", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const alignment = alignmentCharacterPresentation(story);
  assert.equal(alignment.viewId, "org.weave.alignment.wayfinder_compass");
  assert.equal(alignment.pack.id, "org.weave.alignment.wayfinder_compass");
  assert.equal(alignment.pack.version, "1.0.0");
  assert.match(alignment.pack.sha256, /^[0-9a-f]{64}$/);
  assert.match(alignment.reviewSha256, /^[0-9a-f]{64}$/);
  assert.match(alignment.appliedSha256, /^[0-9a-f]{64}$/);
  assert.equal(alignment.canonicalPersonalityWriteBack, false);
  assert.deepEqual(
    alignment.values.map(({ id, labelId, label, decision }) => ({
      id,
      labelId,
      label,
      decision,
    })),
    [
      { id: "horizon", labelId: "seeking", label: "Seeking", decision: "reviewed" },
      { id: "reciprocity", labelId: "mutual", label: "Mutual", decision: "edited" },
      {
        id: "structure",
        labelId: "adapting",
        label: "Adapting",
        decision: "overridden",
      },
    ],
  );
  assert.ok(
    alignment.values.every(
      (value) =>
        value.coverageMicros === 1_000_000 &&
        value.inputPaths.length > 0 &&
        value.id !== "signal" &&
        value.id !== "tempo",
    ),
  );
});

test("reads explainable projection labels with exact review lineage and no write-back", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const projections = projectionCharacterPresentation(story);
  assert.equal(projections.characterId, "org.weave.character.ari_vale");
  assert.deepEqual(
    projections.values.map(({ id, kind, label, decision, lossy, locked }) => ({
      id,
      kind,
      label,
      decision,
      lossy,
      locked,
    })),
    [
      {
        id: "narrative_role",
        kind: "narrative_role",
        label: "Signal Keeper",
        decision: "reviewed",
        lossy: false,
        locked: true,
      },
      {
        id: "personality_lens",
        kind: "categorical_personality",
        label: "Open Explorer",
        decision: "derived",
        lossy: true,
        locked: false,
      },
      {
        id: "social_role",
        kind: "social_role",
        label: "Question Host",
        decision: "reviewed",
        lossy: false,
        locked: false,
      },
      {
        id: "vocation",
        kind: "vocation",
        label: "Route Archivist",
        decision: "reviewed",
        lossy: false,
        locked: false,
      },
    ],
  );
  assert.ok(
    projections.values.every(
      (value) =>
        value.independentEvidence === false &&
        value.explanation.length > 0 &&
        value.rationale.length > 0 &&
        value.inputPaths.length > 0 &&
        value.pack.id === "org.weave.projection.glasswind_lenses" &&
        value.pack.version === "1.0.0" &&
        /^[0-9a-f]{64}$/u.test(value.pack.sha256) &&
        /^[0-9a-f]{64}$/u.test(value.proposalSha256) &&
        /^[0-9a-f]{64}$/u.test(value.reviewSha256),
    ),
  );
  assert.deepEqual(projections.writeBack, {
    alignment: false,
    birth: false,
    hexaco: false,
    identity: false,
    ocean: false,
    relationships: false,
    ruleset: false,
  });
});

test("queries layered Character relationships without treating affinity as canon", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const relationships = relationshipCharacterPresentation(story);
  assert.equal(relationships.kindPack.id, "org.weave.relationship.reference");
  assert.equal(relationships.kindPack.version, "1.0.0");
  assert.match(relationships.kindPack.sha256, /^[0-9a-f]{64}$/u);
  assert.deepEqual({ ...relationships, kindPack: undefined }, {
    canonicalPersonalityWriteBack: false,
    graphFormatVersion: 1,
    kindPack: undefined,
    edges: [
      {
        id: "mentor_sable",
        sourceCharacterId: "org.weave.character.ari_vale",
        targetCharacterId: "org.weave.character.sable_reed",
        kind: "org.weave.relationship.mentor",
        origin: "authored",
        review: "not_required",
        lock: "unlocked",
        affinityScoreMicros: undefined,
        evidenceCount: 0,
      },
    ],
  });
});

test("reads observable normalized expression, voice, preference, and exact template links", async () => {
  const story = JSON.parse(await readFile(characterUrl, "utf8"));
  const expression = expressionCharacterPresentation(story);
  assert.deepEqual(
    { ...expression, template: { ...expression.template, pack: undefined } },
    {
      term: {
        id: "trailmark",
        surface: "trailmark",
        normalized: "trailmark",
        origin: "pack_assigned",
      },
      preference: {
        id: "clear_questions",
        target: "clear questions",
        polarity: "prefer",
      },
      voice: {
        id: "prefer_clear_questions",
        instruction: "Ask one clear question after a short observation.",
        medium: "both",
        effect: "prefer",
      },
      template: {
        id: "arrival_greeting",
        scenarioId: "arrival",
        pack: undefined,
      },
      canonicalPersonalityWriteBack: false,
    },
  );
  assert.equal(expression.template.pack.id, "org.weave.expression.glasswind");
  assert.equal(expression.template.pack.version, "1.0.0");
  assert.match(expression.template.pack.sha256, /^[0-9a-f]{64}$/u);
});

test("reads reviewed temporal cues with separate fact and fictional-cue lineage", async () => {
  const story = JSON.parse(await readFile(temporalCharacterUrl, "utf8"));
  const context = temporalCharacterPresentation(story);
  assert.equal(context.canonicalPersonalityWriteBack, false);
  assert.deepEqual(context.acceptedRecordIds, [
    "apollo_11_lunar_landing",
    "calendar_midsummer_period",
    "world_coastal_fog_cycle",
  ]);
  assert.equal(context.cues.length, 3);
  assert.ok(
    context.cues.every(
      (cue) => cue.factSourceIds.length > 0 && cue.cueSourceIds.length > 0,
    ),
  );
  assert.deepEqual(
    context.cues.find((cue) => cue.recordId === "apollo_11_lunar_landing"),
    {
      id: "cue_afa4e07801cd5172193f83f1",
      recordId: "apollo_11_lunar_landing",
      kind: "value",
      content:
        "Consider whether this fictional character values difficult work whose meaning becomes visible only when many people share one horizon.",
      decision: "accepted",
      factSourceIds: ["apollo_11_wikidata"],
      cueSourceIds: ["weave_historical_cues"],
      relevance: 0.787075,
      uncertainty: "exact",
      sensitivity: "moderate",
    },
  );
});

test("rejects invalid tagged values without echoing them", () => {
  const rejected = "credential-shaped-value";
  assert.throws(
    () => decodeDomainValue({ kind: "number", value: rejected }),
    (error) => error instanceof TypeError && !error.message.includes(rejected),
  );
});
