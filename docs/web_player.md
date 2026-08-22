# Browser and WASM

`weave-web` exposes the standalone deterministic runtime to JavaScript through WebAssembly. It accepts compiled story JSON, returns structured frames, and has no Bevy, GPUI, DOM, storage, cookie, or network dependency.

## Build the no-bundler example

```bash
cargo install wasm-bindgen-cli --version 0.2.127 --locked --root target/web-tools
./scripts/build-web-player.sh
python3 -m http.server 4173 --directory examples/web-player
```

Open `http://127.0.0.1:4173`. The example uses ordinary headings, status text, buttons, and grouped choice controls. The host page owns `localStorage`; the WASM module only serializes and restores versioned state strings.

## JavaScript flow

```js
const player = new StoryPlayer(compiledStoryJson, 73);
const frame = player.continue();
const choices = player.choices();
player.choose(0);
player.restartWithSeed(99);
const saved = player.serializeState();
player.restoreState(saved);
```

Equal story JSON, seed, and choices produce equal grammar and pattern draws. Seeds cross the JavaScript boundary as unsigned 32-bit integers and expand into the runtime's deterministic 64-bit seed.

The complete [browser example guide](https://github.com/chrisgliddon/weave/blob/main/examples/web-player/README.md) documents platform requirements, state ownership, the Playwright interaction test, and current release size.

## PixiJS domain-module consumer

The separate [PixiJS v8 example](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-pixijs) imports canonical IR-4 tracer and composed World JSON, validates the version and tagged values, and renders selected module exports without WASM or editor dependencies. Its exact host test proves that the composed climate/biome changes the canvas colors while the authored beacon rule, harbor, road, and coastal override change the travel presentation:

```bash
npm --prefix examples/domain-module-pixijs ci
npm --prefix examples/domain-module-pixijs test
npm --prefix examples/domain-module-pixijs run build
```
