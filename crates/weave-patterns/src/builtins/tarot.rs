use std::collections::BTreeMap;

use crate::{
    DrawMethod, PATTERN_MODEL_VERSION, PatternDefinition, PatternElement, PatternValue,
    ReversalPolicy, SpreadDefinition,
};

const MAJOR_ARCANA: [(&str, &str, &str, &str, &str); 22] = [
    (
        "the_fool",
        "The Fool",
        "new_beginnings",
        "recklessness",
        "air",
    ),
    (
        "the_magician",
        "The Magician",
        "focused_will",
        "manipulation",
        "air",
    ),
    (
        "the_high_priestess",
        "The High Priestess",
        "intuition",
        "hidden_motives",
        "water",
    ),
    (
        "the_empress",
        "The Empress",
        "abundance",
        "creative_stagnation",
        "earth",
    ),
    (
        "the_emperor",
        "The Emperor",
        "authority",
        "rigidity",
        "fire",
    ),
    (
        "the_hierophant",
        "The Hierophant",
        "tradition",
        "empty_conformity",
        "earth",
    ),
    ("the_lovers", "The Lovers", "union", "misalignment", "air"),
    (
        "the_chariot",
        "The Chariot",
        "determination",
        "lost_direction",
        "water",
    ),
    ("strength", "Strength", "courage", "self_doubt", "fire"),
    (
        "the_hermit",
        "The Hermit",
        "introspection",
        "isolation",
        "earth",
    ),
    (
        "wheel_of_fortune",
        "Wheel of Fortune",
        "turning_cycles",
        "resisting_change",
        "fire",
    ),
    ("justice", "Justice", "balance", "unfairness", "air"),
    (
        "the_hanged_man",
        "The Hanged Man",
        "surrender",
        "needless_delay",
        "water",
    ),
    (
        "death",
        "Death",
        "transformation",
        "fear_of_transition",
        "water",
    ),
    ("temperance", "Temperance", "integration", "excess", "fire"),
    (
        "the_devil",
        "The Devil",
        "attachment",
        "breaking_bonds",
        "earth",
    ),
    (
        "the_tower",
        "The Tower",
        "sudden_change",
        "avoided_reckoning",
        "fire",
    ),
    ("the_star", "The Star", "hope", "discouragement", "air"),
    (
        "the_moon",
        "The Moon",
        "uncertainty",
        "confusion_lifting",
        "water",
    ),
    ("the_sun", "The Sun", "vitality", "dimmed_joy", "fire"),
    (
        "judgement",
        "Judgement",
        "awakening",
        "self_reproach",
        "fire",
    ),
    (
        "the_world",
        "The World",
        "completion",
        "unfinished_work",
        "earth",
    ),
];

const SUITS: [(&str, &str, &str); 4] = [
    ("wands", "fire", "action"),
    ("cups", "water", "feeling"),
    ("swords", "air", "thought"),
    ("pentacles", "earth", "resources"),
];

const RANKS: [(&str, &str, &str); 14] = [
    ("ace", "Ace", "opportunity"),
    ("two", "Two", "choice"),
    ("three", "Three", "growth"),
    ("four", "Four", "stability"),
    ("five", "Five", "challenge"),
    ("six", "Six", "harmony"),
    ("seven", "Seven", "assessment"),
    ("eight", "Eight", "movement"),
    ("nine", "Nine", "culmination"),
    ("ten", "Ten", "completion"),
    ("page", "Page", "curiosity"),
    ("knight", "Knight", "pursuit"),
    ("queen", "Queen", "understanding"),
    ("king", "King", "stewardship"),
];

/// Complete 78-card tarot definition with semantic and reversal fields.
#[must_use]
pub fn tarot_definition() -> PatternDefinition {
    let mut elements = Vec::with_capacity(78);
    for (number, (id, name, meaning, reversed, element)) in MAJOR_ARCANA.iter().enumerate() {
        elements.push(tarot_element(
            id,
            name,
            PatternValue::Number(number as f64),
            "major",
            "major_arcana",
            meaning,
            reversed,
            element,
            2.0,
        ));
    }
    for (suit, element, domain) in SUITS {
        for (rank, display_rank, theme) in RANKS {
            let id = format!("{rank}_of_{suit}");
            let display_suit = uppercase_first(suit);
            let name = format!("{display_rank} of {display_suit}");
            let meaning = format!("{theme}_in_{domain}");
            let reversed = format!("blocked_{meaning}");
            elements.push(tarot_element(
                &id,
                &name,
                PatternValue::Symbol(rank.to_owned()),
                "minor",
                suit,
                &meaning,
                &reversed,
                element,
                1.0,
            ));
        }
    }
    PatternDefinition {
        version: PATTERN_MODEL_VERSION,
        id: "tarot".to_owned(),
        name: "Tarot".to_owned(),
        elements,
        spreads: BTreeMap::from([(
            "three_card".to_owned(),
            SpreadDefinition {
                name: "three_card".to_owned(),
                positions: vec!["past".to_owned(), "present".to_owned(), "future".to_owned()],
            },
        )]),
        default_method: DrawMethod::Uniform,
        allow_duplicates: false,
        reversal_policy: ReversalPolicy::Half,
    }
}

#[allow(clippy::too_many_arguments)]
fn tarot_element(
    id: &str,
    name: &str,
    rank: PatternValue,
    arcana: &str,
    suit: &str,
    meaning: &str,
    reversed_meaning: &str,
    element: &str,
    weight: f64,
) -> PatternElement {
    PatternElement {
        id: id.to_owned(),
        fields: BTreeMap::from([
            ("arcana".to_owned(), PatternValue::Symbol(arcana.to_owned())),
            (
                "element".to_owned(),
                PatternValue::Symbol(element.to_owned()),
            ),
            (
                "meaning".to_owned(),
                PatternValue::Symbol(meaning.to_owned()),
            ),
            ("name".to_owned(), PatternValue::String(name.to_owned())),
            ("rank".to_owned(), rank),
            (
                "reversed_meaning".to_owned(),
                PatternValue::Symbol(reversed_meaning.to_owned()),
            ),
            ("suit".to_owned(), PatternValue::Symbol(suit.to_owned())),
            ("weight".to_owned(), PatternValue::Number(weight)),
        ]),
        reversed_fields: BTreeMap::from([(
            "meaning".to_owned(),
            PatternValue::Symbol(reversed_meaning.to_owned()),
        )]),
        reversible: true,
    }
}

fn uppercase_first(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_ascii_uppercase().to_string() + characters.as_str()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{DataPatternSystem, DrawRequest, PatternState, PatternSystem, SeededRandom};

    use super::*;

    #[test]
    fn tarot_has_complete_unique_arcana_and_required_fields() {
        let definition = tarot_definition();
        assert_eq!(definition.elements.len(), 78);
        assert_eq!(
            definition
                .elements
                .iter()
                .filter(|card| card.fields.get("arcana")
                    == Some(&PatternValue::Symbol("major".to_owned())))
                .count(),
            22
        );
        assert_eq!(
            definition
                .elements
                .iter()
                .filter(|card| card.fields.get("arcana")
                    == Some(&PatternValue::Symbol("minor".to_owned())))
                .count(),
            56
        );
        assert_eq!(
            definition
                .elements
                .iter()
                .map(|card| &card.id)
                .collect::<BTreeSet<_>>()
                .len(),
            78
        );
        for card in &definition.elements {
            for field in [
                "name",
                "arcana",
                "suit",
                "rank",
                "meaning",
                "element",
                "reversed_meaning",
            ] {
                assert!(card.fields.contains_key(field), "{} lacks {field}", card.id);
            }
        }
    }

    #[test]
    fn three_card_draw_is_seeded_unique_and_position_ordered() {
        let system = DataPatternSystem::new(tarot_definition()).expect("valid tarot");
        let request = DrawRequest {
            spread: Some("three_card".to_owned()),
            ..DrawRequest::default()
        };
        let mut first_random = SeededRandom::new(22, 0);
        let mut second_random = SeededRandom::new(22, 0);
        let first = system
            .draw(&request, &mut PatternState::default(), &mut first_random)
            .expect("draw succeeds");
        let second = system
            .draw(&request, &mut PatternState::default(), &mut second_random)
            .expect("draw succeeds");
        assert_eq!(first, second);
        assert_eq!(
            first
                .entries
                .iter()
                .map(|entry| entry.position.as_deref())
                .collect::<Vec<_>>(),
            [Some("past"), Some("present"), Some("future")]
        );
        assert_eq!(
            first
                .entries
                .iter()
                .map(|entry| &entry.id)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn weighted_and_reversed_tarot_draws_are_executable() {
        let system = DataPatternSystem::new(tarot_definition()).expect("valid tarot");
        let request = DrawRequest {
            spread: Some("three_card".to_owned()),
            method: Some(DrawMethod::Weighted {
                field: "weight".to_owned(),
            }),
            ..DrawRequest::default()
        };
        let mut random = SeededRandom::new(12, 0);
        let result = system
            .draw(&request, &mut PatternState::default(), &mut random)
            .expect("weighted draw succeeds");
        assert_eq!(result.entries.len(), 3);
        assert!(result.entries.iter().any(|entry| entry.reversed));
        for entry in result.entries {
            assert!(entry.fields.contains_key("meaning"));
            assert!(entry.fields.contains_key("reversed_meaning"));
        }
    }
}
