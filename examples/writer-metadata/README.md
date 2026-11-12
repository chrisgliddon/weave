# Writer metadata example

`lighthouse.weave` is a short scene that uses every writer-metadata feature: tags, stable line ids,
two channels (`dialog` types on at 40 characters a second; `sign` appears all at once), speakers,
pauses, style spans, and a `STRINGS` table of interface text.

The checked-in outputs are regenerated from the source:

```bash
weavec examples/writer-metadata/lighthouse.weave --format json \
  -o examples/writer-metadata/lighthouse.story.json
weavec export --unreal examples/writer-metadata/unreal examples/writer-metadata/lighthouse.weave
```

Play it with a fixed choice plan:

```bash
weavec play examples/writer-metadata/lighthouse.weave --plan 0
```

```text
[shore.1] (narration) Wind pushes the grass flat along the cliff path.
[shore.sign] (sign) GULL POINT LIGHT Keeper on duty
[shore.3] (dialog) keeper: You came up in this weather? Brave, or lost.
[shore.4] (dialog) visitor: A little of both.
 > 0: Ask about the lamp
   1: Turn back
[shore.5] (dialog) visitor: Why is the lamp dark tonight?
[shore.wick] (dialog) keeper: The wick is spent. I was waiting for someone to fetch oil.
[stores.1] (dialog) keeper: Oil is in the shed. Mind the third step.
[stores.lit] (sign) LAMP LIT
[stores.3] (dialog) visitor: There. Now the boats can find the harbour.
-- ended
```

The compiled JSON is also the fixture for the C ABI tests in `crates/weave-ffi/tests/abi.rs`.
