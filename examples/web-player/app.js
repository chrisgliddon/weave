import init, { StoryPlayer } from "./pkg/weave_web.js";

const STORAGE_KEY = "weave.web-player.state.v1";
const elements = {
  transcript: document.querySelector("#transcript"),
  status: document.querySelector("#status"),
  choices: document.querySelector("#choice-fieldset"),
  choiceList: document.querySelector("#choice-list"),
  continue: document.querySelector("#continue"),
  restart: document.querySelector("#restart"),
  seed: document.querySelector("#seed"),
  applySeed: document.querySelector("#apply-seed"),
  save: document.querySelector("#save"),
  load: document.querySelector("#load"),
  clear: document.querySelector("#clear"),
};

let player;
let storyJson;

function savedState() {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function setStatus(message) {
  elements.status.textContent = message;
}

function setReady(ready) {
  elements.continue.disabled = !ready;
  elements.restart.disabled = !ready;
  elements.applySeed.disabled = !ready;
  elements.save.disabled = !ready;
  elements.load.disabled = !ready || savedState() === null;
}

function unpack(field) {
  return field && Object.hasOwn(field, "value") ? field.value : field;
}

function drawLabel(draw) {
  const entry = draw.entries?.[0];
  if (!entry) return `Pattern draw from ${draw.system}`;
  const name = unpack(entry.fields?.name) ?? entry.id;
  const meaning = unpack(entry.fields?.meaning);
  return meaning ? `${name} · ${String(meaning).replaceAll("_", " ")}` : String(name);
}

function appendLine(text, draws = []) {
  const item = document.createElement("li");
  item.textContent = text;
  for (const draw of draws) {
    const note = document.createElement("span");
    note.className = "draw-note";
    note.textContent = `Signal drawn: ${drawLabel(draw)}`;
    item.append(note);
  }
  elements.transcript.append(item);
}

function hideChoices() {
  elements.choiceList.replaceChildren();
  elements.choices.hidden = true;
}

function showChoices(choices) {
  elements.choiceList.replaceChildren();
  choices.forEach((choice, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "choice-button";
    button.textContent = choice.text;
    button.addEventListener("click", () => selectChoice(index, choice.text));
    elements.choiceList.append(button);
  });
  elements.choices.hidden = false;
  elements.continue.disabled = true;
  elements.choiceList.querySelector("button")?.focus();
}

function continueStory() {
  try {
    const frame = player.continue();
    if (frame.kind === "line") {
      appendLine(frame.text, frame.draws);
      setStatus("Line delivered");
    } else if (frame.kind === "choices") {
      showChoices(frame.choices);
      setStatus("Waiting for your choice");
    } else {
      elements.continue.disabled = true;
      setStatus("Story complete");
    }
  } catch (error) {
    setStatus(`Runtime error: ${error.message ?? error}`);
    elements.continue.disabled = true;
  }
}

function selectChoice(index, label) {
  try {
    player.choose(index);
    hideChoices();
    appendLine(`Choice — ${label}`);
    elements.continue.disabled = false;
    continueStory();
  } catch (error) {
    setStatus(`Choice failed: ${error.message ?? error}`);
  }
}

function restart(seed = Number(elements.seed.value)) {
  const safeSeed = Number.isInteger(seed) && seed >= 0 && seed <= 0xffffffff ? seed : 0;
  elements.seed.value = String(safeSeed);
  player.restartWithSeed(safeSeed);
  elements.transcript.replaceChildren();
  hideChoices();
  elements.continue.disabled = false;
  setStatus(`Ready · seed ${safeSeed}`);
  continueStory();
}

elements.continue.addEventListener("click", continueStory);
elements.restart.addEventListener("click", () => restart());
elements.applySeed.addEventListener("click", () => restart());
elements.save.addEventListener("click", () => {
  try {
    localStorage.setItem(STORAGE_KEY, player.serializeState());
    elements.load.disabled = false;
    setStatus("State saved by this page");
  } catch (error) {
    setStatus(`State was not saved: ${error.message ?? error}`);
  }
});
elements.load.addEventListener("click", () => {
  const saved = savedState();
  if (!saved) return;
  try {
    player.restoreState(saved);
    elements.transcript.replaceChildren();
    hideChoices();
    const choices = player.choices();
    if (choices.length > 0) {
      showChoices(choices);
    } else {
      elements.continue.disabled = !player.canContinue();
    }
    setStatus("Saved runtime state restored");
  } catch (error) {
    setStatus(`Save rejected: ${error.message ?? error}`);
  }
});
elements.clear.addEventListener("click", () => {
  try {
    localStorage.removeItem(STORAGE_KEY);
    elements.load.disabled = true;
    setStatus("Saved state cleared");
  } catch (error) {
    setStatus(`Saved state was not cleared: ${error.message ?? error}`);
  }
});

try {
  await init();
  storyJson = await fetch("./story.json").then((response) => {
    if (!response.ok) throw new Error(`Story request failed with ${response.status}`);
    return response.text();
  });
  player = new StoryPlayer(storyJson, Number(elements.seed.value));
  setReady(true);
  restart(Number(elements.seed.value));
} catch (error) {
  setReady(false);
  setStatus(`Player failed to load: ${error.message ?? error}`);
}
