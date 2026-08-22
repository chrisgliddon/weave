# Pattern systems

Pattern systems turn deterministic entropy into structured meaning. The language-facing contract is documented in [`language_guide.md`](language_guide.md); this guide explains authoring, runtime shape, and the Rust extension boundary.

## Built-ins

```weave
pattern tarot_reading {
    builtin: tarot
    draw: weighted_by_weight
    reversals: true
}

pattern changes {
    builtin: i_ching
    draw: three_coin
}

pattern runes {
    builtin: elder_futhark
}
```

- Tarot contains 22 major and 56 minor arcana. `three_card` draws `past`, `present`, and `future` without replacement. Every card has explicit reversal semantics.
- I-Ching contains all 64 King Wen hexagrams. `three_coin` consumes three fair coin values per line. `yarrow_stalks` uses the traditional old-yin/young-yang/young-yin/old-yang probabilities `1/16`, `5/16`, `7/16`, and `3/16`. Six lines are generated bottom-up; changing lines produce a structured `transformed` hexagram.
- Elder Futhark contains 24 runes in traditional order. `three_rune` draws `past`, `present`, and `future` without replacement. Only runes with an explicit `reversed_meaning` can reverse.

The source and licensing basis for canonical names, ordering, and symbols is recorded in the public [`ATTRIBUTION.md`](https://github.com/chrisgliddon/weave/blob/main/crates/weave-patterns/ATTRIBUTION.md). Weave's concise semantic keywords are original summaries.

## Custom systems

```weave
pattern weather_omens {
    omens: [
        (name: "Storm Crow", meaning: ill_tidings, severity: 3),
        (name: "Sun Dog", meaning: good_fortune, severity: 1),
        (name: "Frost Wolf", meaning: harsh_winter, severity: 4),
    ]
    draw: weighted_by_severity
    duplicates: false
    spread day_omen { positions: [dawn, noon, dusk] }
}

VAR omen = weather_omens.spread.day_omen.draw()
```

Every record needs a unique string or symbol `name` and a `meaning`. A weighted field must be positive and numeric on every element. When duplicates are disabled, a spread cannot have more positions than elements. These conditions are checked before IR is produced and are validated again when executable data crosses the Rust boundary.

Add `reversed_meaning` to records that can reverse, then set `reversals: true` on the pattern. Upright-only records remain upright even in that system.

Reusable third-party data belongs in a versioned, data-only [community pattern package](community_patterns.md). Packages add licensing, provenance, compatibility, integrity, discovery, and moderation metadata while lowering to this same runtime model.

## Result objects

A single draw exposes its semantic fields directly:

```weave
VAR rune = runes.draw()
The rune is {rune.name}: {rune.meaning}.
```

It also includes `id` and `reversed`. A spread is keyed by declared position, and every entry adds `position`:

```weave
VAR cards = tarot_reading.spread.three_card.draw()
Past: {cards.past.name}; future: {cards.future.name}.
```

Results can be stored in `VAR`, assigned with `SET`, interpolated, and compared in conditions. One story seed controls grammar choices and pattern draws as a single deterministic sequence.

## Rust extension boundary

`weave-patterns` separates immutable definitions from mutable session state:

```rust
pub trait PatternSystem: Send + Sync + std::fmt::Debug {
    fn definition(&self) -> &PatternDefinition;

    fn draw(
        &self,
        request: &DrawRequest,
        state: &mut PatternState,
        random: &mut dyn RandomSource,
    ) -> Result<DrawResult, PatternError>;
}
```

`PatternDefinition`, elements, spreads, methods, values, state, requests, and results are Serde data. A host may share one immutable system across sessions; each session owns its own `PatternState` and entropy source. `PatternError` reports stable `Pxxxx` categories for invalid definitions, unknown spreads, insufficient unique elements, invalid weights, unsupported methods, and authored-data failures.

Compiler IR is converted by `system_from_ir`. Built-in and authored systems both return `Arc<dyn PatternSystem>`, so `weave-runtime` has no built-in-specific execution branches. `StoryState` serializes pattern draw counts and last-draw identities alongside narrative state. Restoring against changed data drops stale pattern identities; Bevy hot reload reinitializes the session when compiled pattern definitions differ.

## Bevy events

`PatternDrawn` owns the system, element, position, name, meaning, reversal flag, and complete field map. An observer can handle it without borrowing runtime internals:

```rust,no_run
use bevy::prelude::*;
use weave_bevy::prelude::*;

fn on_pattern(draw: On<PatternDrawn>) {
    println!("{} {:?}: {:?}", draw.element, draw.position, draw.meaning);
}
```

Spread events follow position order and precede the narrative boundary produced by the same command.
