# Weave WASM player example

This no-bundler example compiles the standalone runtime to `wasm32-unknown-unknown`, loads versioned Story IR JSON, and renders lines and choices with ordinary accessible HTML controls.

Build all generated artifacts from the repository root:

```bash
cargo install wasm-bindgen-cli --version 0.2.127 --locked --root target/web-tools
./scripts/build-web-player.sh
python3 -m http.server 4173 --directory examples/web-player
```

Open `http://127.0.0.1:4173`. The page requires a current evergreen browser with WebAssembly, ES modules, and `fetch`. It has no Bevy, GPUI, Node runtime, bundler, network API, or desktop dependency.

With Python Playwright and Chromium installed, run `python3 scripts/test-web-player.py` from a second terminal while that server is active for the full interaction, storage, console, accessibility-role, and mobile-overflow smoke test.

## JavaScript API

```js
const player = new StoryPlayer(compiledStoryJson, 73);
const frame = player.continue();
const choices = player.choices();
player.choose(0);
player.restart();
player.restartWithSeed(99);
const saved = player.serializeState();
player.restoreState(saved);
```

Seeds cross the JavaScript boundary as unsigned 32-bit integers and are expanded into the runtime's deterministic 64-bit seed. Equal story JSON, seed, and choices produce equal grammar and pattern draws. No ambient browser randomness is consulted.

The WASM module never reads or writes cookies, storage, the DOM, or the network. It only accepts immutable story JSON and returns a versioned state string. The example's JavaScript explicitly owns `localStorage`, so an embedding host can instead keep state in memory, IndexedDB, a server, or nowhere.

With Rust 1.93 and `wasm-bindgen` 0.2.127, the release build is 831,909 bytes of uncompressed WASM plus 12,734 bytes of generated JavaScript. The build script remeasures both artifacts after binding generation. HTTP compression is deployment-specific.

Run native and WASM tests with:

```bash
cargo test -p weave-web
cargo test -p weave-web --target wasm32-unknown-unknown
```
