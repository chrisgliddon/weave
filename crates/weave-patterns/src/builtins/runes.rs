use std::collections::BTreeMap;

use crate::{
    DrawMethod, PATTERN_MODEL_VERSION, PatternDefinition, PatternElement, PatternValue,
    ReversalPolicy, SpreadDefinition,
};

type RuneSpec = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);

const RUNES: [RuneSpec; 24] = [
    (
        "fehu",
        "Fehu",
        "ᚠ",
        "f",
        "prosperity",
        Some("material_loss"),
    ),
    ("uruz", "Uruz", "ᚢ", "u", "vitality", Some("weakness")),
    (
        "thurisaz",
        "Thurisaz",
        "ᚦ",
        "th",
        "defense",
        Some("impulsiveness"),
    ),
    (
        "ansuz",
        "Ansuz",
        "ᚨ",
        "a",
        "communication",
        Some("misunderstanding"),
    ),
    ("raidho", "Raidho", "ᚱ", "r", "journey", Some("disruption")),
    ("kenaz", "Kenaz", "ᚲ", "k", "insight", Some("obscurity")),
    ("gebo", "Gebo", "ᚷ", "g", "exchange", None),
    ("wunjo", "Wunjo", "ᚹ", "w", "joy", Some("alienation")),
    ("hagalaz", "Hagalaz", "ᚺ", "h", "disruption", None),
    ("nauthiz", "Nauthiz", "ᚾ", "n", "necessity", None),
    ("isa", "Isa", "ᛁ", "i", "stillness", None),
    ("jera", "Jera", "ᛃ", "j", "harvest", None),
    ("eihwaz", "Eihwaz", "ᛇ", "ï", "endurance", None),
    (
        "perthro",
        "Perthro",
        "ᛈ",
        "p",
        "mystery",
        Some("concealed_truth"),
    ),
    (
        "algiz",
        "Algiz",
        "ᛉ",
        "z",
        "protection",
        Some("vulnerability"),
    ),
    ("sowilo", "Sowilo", "ᛊ", "s", "illumination", None),
    ("tiwaz", "Tiwaz", "ᛏ", "t", "justice", Some("imbalance")),
    ("berkano", "Berkano", "ᛒ", "b", "growth", Some("stagnation")),
    ("ehwaz", "Ehwaz", "ᛖ", "e", "partnership", None),
    ("mannaz", "Mannaz", "ᛗ", "m", "humanity", Some("isolation")),
    ("laguz", "Laguz", "ᛚ", "l", "flow", Some("blockage")),
    ("ingwaz", "Ingwaz", "ᛜ", "ng", "potential", None),
    ("dagaz", "Dagaz", "ᛞ", "d", "breakthrough", None),
    ("othala", "Othala", "ᛟ", "o", "inheritance", None),
];

/// Complete 24-rune Elder Futhark definition in traditional rune order.
#[must_use]
pub fn elder_futhark_definition() -> PatternDefinition {
    let elements = RUNES
        .into_iter()
        .map(|(id, name, glyph, transliteration, meaning, reversed)| {
            let mut fields = BTreeMap::from([
                ("glyph".to_owned(), PatternValue::String(glyph.to_owned())),
                (
                    "meaning".to_owned(),
                    PatternValue::Symbol(meaning.to_owned()),
                ),
                ("name".to_owned(), PatternValue::String(name.to_owned())),
                (
                    "transliteration".to_owned(),
                    PatternValue::String(transliteration.to_owned()),
                ),
            ]);
            let mut reversed_fields = BTreeMap::new();
            if let Some(reversed) = reversed {
                fields.insert(
                    "reversed_meaning".to_owned(),
                    PatternValue::Symbol(reversed.to_owned()),
                );
                reversed_fields.insert(
                    "meaning".to_owned(),
                    PatternValue::Symbol(reversed.to_owned()),
                );
            }
            PatternElement {
                id: id.to_owned(),
                fields,
                reversed_fields,
                reversible: reversed.is_some(),
            }
        })
        .collect();
    PatternDefinition {
        version: PATTERN_MODEL_VERSION,
        id: "elder_futhark".to_owned(),
        name: "Elder Futhark".to_owned(),
        elements,
        spreads: BTreeMap::from([(
            "three_rune".to_owned(),
            SpreadDefinition {
                name: "three_rune".to_owned(),
                positions: vec!["past".to_owned(), "present".to_owned(), "future".to_owned()],
            },
        )]),
        default_method: DrawMethod::Uniform,
        allow_duplicates: false,
        reversal_policy: ReversalPolicy::Half,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{DataPatternSystem, DrawRequest, PatternState, PatternSystem, SeededRandom};

    use super::*;

    #[test]
    fn elder_futhark_is_complete_unique_and_round_trips() {
        let definition = elder_futhark_definition();
        assert_eq!(definition.elements.len(), 24);
        assert_eq!(
            definition
                .elements
                .iter()
                .map(|rune| &rune.id)
                .collect::<BTreeSet<_>>()
                .len(),
            24
        );
        assert!(
            definition
                .elements
                .iter()
                .all(|rune| rune.fields.contains_key("transliteration"))
        );
        let encoded = ron::to_string(&definition).expect("definition serializes");
        let restored: PatternDefinition = ron::from_str(&encoded).expect("definition decodes");
        assert_eq!(restored, definition);
    }

    #[test]
    fn single_and_three_rune_draws_are_deterministic() {
        let system = DataPatternSystem::new(elder_futhark_definition()).expect("valid runes");
        for spread in [None, Some("three_rune".to_owned())] {
            let request = DrawRequest {
                spread,
                ..DrawRequest::default()
            };
            let mut first = SeededRandom::new(91, 0);
            let mut second = SeededRandom::new(91, 0);
            let left = system
                .draw(&request, &mut PatternState::default(), &mut first)
                .expect("draw succeeds");
            let right = system
                .draw(&request, &mut PatternState::default(), &mut second)
                .expect("draw succeeds");
            assert_eq!(left, right);
        }
    }

    #[test]
    fn reversal_is_only_enabled_for_explicit_semantics() {
        let definition = elder_futhark_definition();
        for rune in definition.elements {
            assert_eq!(
                rune.reversible,
                rune.fields.contains_key("reversed_meaning")
            );
        }
    }
}
