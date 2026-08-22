# Domain module in PixiJS

This PixiJS v8 example imports the canonical compiled JSON tracer and Weave World reference seed,
decodes their tagged domain values, and renders both with `Application` and `Text`. It reads the
same values as the Bevy example and has no dependency on the Weave Editor.

```bash
cd examples/domain-module-pixijs
npm ci
npm test
npm run build
```

Run `npm run dev` for the browser view. The pure value-reader tests intentionally run in Node so
CI can verify the portable boundary without a GPU or browser.
