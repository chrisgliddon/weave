use std::collections::BTreeMap;

use crate::{
    DataPatternSystem, DrawMethod, DrawRequest, DrawResult, DrawnElement, PATTERN_MODEL_VERSION,
    PatternDefinition, PatternElement, PatternError, PatternState, PatternSystem, PatternValue,
    RandomSource, ReversalPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigram {
    Heaven,
    Lake,
    Fire,
    Thunder,
    Wind,
    Water,
    Mountain,
    Earth,
}

impl Trigram {
    const fn name(self) -> &'static str {
        match self {
            Self::Heaven => "heaven",
            Self::Lake => "lake",
            Self::Fire => "fire",
            Self::Thunder => "thunder",
            Self::Wind => "wind",
            Self::Water => "water",
            Self::Mountain => "mountain",
            Self::Earth => "earth",
        }
    }

    const fn lines(self) -> [bool; 3] {
        match self {
            Self::Heaven => [true, true, true],
            Self::Lake => [true, true, false],
            Self::Fire => [true, false, true],
            Self::Thunder => [true, false, false],
            Self::Wind => [false, true, true],
            Self::Water => [false, true, false],
            Self::Mountain => [false, false, true],
            Self::Earth => [false, false, false],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct HexagramSpec {
    name: &'static str,
    meaning: &'static str,
    upper: Trigram,
    lower: Trigram,
}

use Trigram::{Earth, Fire, Heaven, Lake, Mountain, Thunder, Water, Wind};

const HEXAGRAMS: [HexagramSpec; 64] = [
    hex("Qian — Creative Force", "creative_force", Heaven, Heaven),
    hex("Kun — Receptive Earth", "receptive_devotion", Earth, Earth),
    hex(
        "Zhun — Difficulty at the Start",
        "beginning_difficulty",
        Water,
        Thunder,
    ),
    hex(
        "Meng — Youthful Inexperience",
        "beginner_learning",
        Mountain,
        Water,
    ),
    hex("Xu — Waiting", "patient_waiting", Water, Heaven),
    hex("Song — Dispute", "principled_conflict", Heaven, Water),
    hex("Shi — The Army", "disciplined_collective", Earth, Water),
    hex("Bi — Alliance", "mutual_support", Water, Earth),
    hex(
        "Xiao Chu — Small Restraint",
        "gentle_restraint",
        Wind,
        Heaven,
    ),
    hex("Lu — Treading", "careful_conduct", Heaven, Lake),
    hex("Tai — Peace", "harmony", Earth, Heaven),
    hex("Pi — Stagnation", "stagnation", Heaven, Earth),
    hex("Tong Ren — Fellowship", "fellowship", Heaven, Fire),
    hex(
        "Da You — Great Possession",
        "great_possession",
        Fire,
        Heaven,
    ),
    hex("Qian — Humility", "humility", Earth, Mountain),
    hex("Yu — Enthusiasm", "enthusiasm", Thunder, Earth),
    hex("Sui — Following", "adaptation", Lake, Thunder),
    hex("Gu — Repairing Decay", "repair", Mountain, Wind),
    hex("Lin — Approach", "approach", Earth, Lake),
    hex("Guan — Contemplation", "contemplation", Wind, Earth),
    hex("Shi He — Biting Through", "decisive_action", Fire, Thunder),
    hex("Bi — Adornment", "grace", Mountain, Fire),
    hex("Bo — Splitting Apart", "dissolution", Mountain, Earth),
    hex("Fu — Return", "renewal", Earth, Thunder),
    hex("Wu Wang — Without Falsehood", "innocence", Heaven, Thunder),
    hex(
        "Da Chu — Great Restraint",
        "great_restraint",
        Mountain,
        Heaven,
    ),
    hex("Yi — Nourishment", "nourishment", Mountain, Thunder),
    hex("Da Guo — Great Excess", "critical_excess", Lake, Wind),
    hex("Kan — Repeated Water", "navigating_danger", Water, Water),
    hex("Li — Clinging Fire", "clarity", Fire, Fire),
    hex("Xian — Influence", "mutual_influence", Lake, Mountain),
    hex("Heng — Endurance", "endurance", Thunder, Wind),
    hex("Dun — Retreat", "retreat", Heaven, Mountain),
    hex("Da Zhuang — Great Power", "great_power", Thunder, Heaven),
    hex("Jin — Advance", "progress", Fire, Earth),
    hex("Ming Yi — Light Obscured", "light_obscured", Earth, Fire),
    hex("Jia Ren — The Household", "family_order", Wind, Fire),
    hex("Kui — Opposition", "opposition", Fire, Lake),
    hex("Jian — Obstruction", "obstruction", Water, Mountain),
    hex("Xie — Release", "release", Thunder, Water),
    hex("Sun — Decrease", "decrease", Mountain, Lake),
    hex("Yi — Increase", "increase", Wind, Thunder),
    hex("Guai — Breakthrough", "breakthrough", Lake, Heaven),
    hex("Gou — Encounter", "encounter", Heaven, Wind),
    hex("Cui — Gathering", "gathering", Lake, Earth),
    hex("Sheng — Rising", "ascent", Earth, Wind),
    hex("Kun — Confinement", "oppression", Lake, Water),
    hex("Jing — The Well", "replenishment", Water, Wind),
    hex("Ge — Transformation", "transformation", Lake, Fire),
    hex("Ding — The Cauldron", "cultivation", Fire, Wind),
    hex("Zhen — Arousing Thunder", "awakening", Thunder, Thunder),
    hex("Gen — Still Mountain", "stillness", Mountain, Mountain),
    hex(
        "Jian — Gradual Development",
        "gradual_development",
        Wind,
        Mountain,
    ),
    hex("Gui Mei — Marrying Maiden", "unequal_union", Thunder, Lake),
    hex("Feng — Abundance", "abundance", Thunder, Fire),
    hex("Lu — The Traveler", "travel", Fire, Mountain),
    hex("Xun — Gentle Wind", "gentle_penetration", Wind, Wind),
    hex("Dui — Joyful Lake", "joy", Lake, Lake),
    hex("Huan — Dispersion", "dispersal", Wind, Water),
    hex("Jie — Limitation", "limits", Water, Lake),
    hex("Zhong Fu — Inner Truth", "inner_truth", Wind, Lake),
    hex("Xiao Guo — Small Excess", "small_excess", Thunder, Mountain),
    hex("Ji Ji — Already Complete", "completion", Water, Fire),
    hex("Wei Ji — Not Yet Complete", "not_yet_complete", Fire, Water),
];

const fn hex(
    name: &'static str,
    meaning: &'static str,
    upper: Trigram,
    lower: Trigram,
) -> HexagramSpec {
    HexagramSpec {
        name,
        meaning,
        upper,
        lower,
    }
}

/// Complete King Wen sequence with stable trigram and line data.
#[must_use]
pub fn i_ching_definition() -> PatternDefinition {
    PatternDefinition {
        version: PATTERN_MODEL_VERSION,
        id: "i_ching".to_owned(),
        name: "I Ching".to_owned(),
        elements: HEXAGRAMS
            .iter()
            .enumerate()
            .map(|(index, spec)| hexagram_element(index + 1, *spec))
            .collect(),
        spreads: BTreeMap::new(),
        default_method: DrawMethod::ThreeCoin,
        allow_duplicates: false,
        reversal_policy: ReversalPolicy::Never,
    }
}

fn hexagram_element(number: usize, spec: HexagramSpec) -> PatternElement {
    let lines = line_states(spec)
        .into_iter()
        .map(|yang| PatternValue::Symbol(if yang { "yang" } else { "yin" }.to_owned()))
        .collect();
    PatternElement::new(
        format!("hexagram_{number}"),
        BTreeMap::from([
            ("lines".to_owned(), PatternValue::List(lines)),
            (
                "lower_trigram".to_owned(),
                PatternValue::Symbol(spec.lower.name().to_owned()),
            ),
            (
                "meaning".to_owned(),
                PatternValue::Symbol(spec.meaning.to_owned()),
            ),
            (
                "name".to_owned(),
                PatternValue::String(spec.name.to_owned()),
            ),
            ("number".to_owned(), PatternValue::Number(number as f64)),
            (
                "upper_trigram".to_owned(),
                PatternValue::Symbol(spec.upper.name().to_owned()),
            ),
        ]),
    )
}

fn line_states(spec: HexagramSpec) -> [bool; 6] {
    let lower = spec.lower.lines();
    let upper = spec.upper.lines();
    [lower[0], lower[1], lower[2], upper[0], upper[1], upper[2]]
}

/// I-Ching executor for line-generating methods.
#[derive(Debug, Clone)]
pub struct IChingSystem {
    definition: PatternDefinition,
}

impl IChingSystem {
    pub(crate) fn new(definition: PatternDefinition) -> Result<Self, PatternError> {
        let _ = DataPatternSystem::new(definition.clone())?;
        Ok(Self { definition })
    }
}

impl PatternSystem for IChingSystem {
    fn definition(&self) -> &PatternDefinition {
        &self.definition
    }

    fn draw(
        &self,
        request: &DrawRequest,
        state: &mut PatternState,
        random: &mut dyn RandomSource,
    ) -> Result<DrawResult, PatternError> {
        if let Some(spread) = &request.spread {
            return Err(PatternError::UnknownSpread {
                system: self.definition.id.clone(),
                spread: spread.clone(),
            });
        }
        let method = request
            .method
            .clone()
            .unwrap_or_else(|| self.definition.default_method.clone());
        if !matches!(method, DrawMethod::ThreeCoin | DrawMethod::YarrowStalks) {
            return Err(PatternError::UnsupportedMethod {
                system: self.definition.id.clone(),
                method,
            });
        }
        let line_values = std::array::from_fn(|_| generate_line(&method, random));
        let primary_lines = line_values.map(|line| matches!(line, 7 | 9));
        let transformed_lines = std::array::from_fn(|index| match line_values[index] {
            6 => true,
            9 => false,
            _ => primary_lines[index],
        });
        let primary_number =
            number_for_lines(primary_lines).ok_or_else(|| PatternError::InvalidAuthoredData {
                system: self.definition.id.clone(),
                message: "generated line structure is absent from the King Wen sequence".to_owned(),
            })?;
        let transformed_number = number_for_lines(transformed_lines).ok_or_else(|| {
            PatternError::InvalidAuthoredData {
                system: self.definition.id.clone(),
                message: "transformed line structure is absent from the King Wen sequence"
                    .to_owned(),
            }
        })?;
        let primary = &self.definition.elements[primary_number - 1];
        let transformed = &self.definition.elements[transformed_number - 1];
        let changing_lines = line_values
            .iter()
            .enumerate()
            .filter(|(_, line)| matches!(line, 6 | 9))
            .map(|(index, _)| PatternValue::Number((index + 1) as f64))
            .collect::<Vec<_>>();
        let raw_lines = line_values
            .into_iter()
            .map(|line| PatternValue::Number(f64::from(line)))
            .collect::<Vec<_>>();
        let mut fields = primary.fields.clone();
        fields.insert("lines".to_owned(), PatternValue::List(raw_lines.clone()));
        fields.insert(
            "line_states".to_owned(),
            PatternValue::List(
                primary_lines
                    .into_iter()
                    .map(|yang| PatternValue::Symbol(if yang { "yang" } else { "yin" }.to_owned()))
                    .collect(),
            ),
        );
        fields.insert(
            "changing_lines".to_owned(),
            PatternValue::List(changing_lines.clone()),
        );
        fields.insert(
            "transformed".to_owned(),
            PatternValue::Object(transformed.fields.clone()),
        );
        state.draws = state.draws.saturating_add(1);
        state.last_draw = vec![primary.id.clone()];
        Ok(DrawResult {
            system: self.definition.id.clone(),
            spread: None,
            method,
            entries: vec![DrawnElement {
                id: primary.id.clone(),
                position: None,
                reversed: false,
                fields,
            }],
            metadata: BTreeMap::from([
                (
                    "changing_lines".to_owned(),
                    PatternValue::List(changing_lines),
                ),
                ("lines".to_owned(), PatternValue::List(raw_lines)),
                (
                    "transformed_number".to_owned(),
                    PatternValue::Number(transformed_number as f64),
                ),
            ]),
        })
    }
}

fn generate_line(method: &DrawMethod, random: &mut dyn RandomSource) -> u8 {
    match method {
        DrawMethod::ThreeCoin => {
            let first = 2 + (random.next_u64() & 1) as u8;
            let second = 2 + (random.next_u64() & 1) as u8;
            let third = 2 + (random.next_u64() & 1) as u8;
            first + second + third
        }
        DrawMethod::YarrowStalks => match random.next_u64() % 16 {
            0 => 6,
            1..=5 => 7,
            6..=12 => 8,
            _ => 9,
        },
        DrawMethod::Uniform | DrawMethod::Weighted { .. } => unreachable!("method prevalidated"),
    }
}

fn number_for_lines(lines: [bool; 6]) -> Option<usize> {
    HEXAGRAMS
        .iter()
        .position(|spec| line_states(*spec) == lines)
        .map(|index| index + 1)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::SeededRandom;

    #[derive(Default)]
    struct ZeroRandom;

    impl RandomSource for ZeroRandom {
        fn next_u64(&mut self) -> u64 {
            0
        }
    }

    #[test]
    fn king_wen_sequence_is_complete_and_unique() {
        let definition = i_ching_definition();
        assert_eq!(definition.elements.len(), 64);
        assert_eq!(
            definition
                .elements
                .iter()
                .filter_map(|hexagram| match hexagram.fields.get("name") {
                    Some(PatternValue::String(name)) => Some(name.as_str()),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .len(),
            64
        );
        assert_eq!(
            HEXAGRAMS
                .iter()
                .map(|spec| line_states(*spec))
                .collect::<BTreeSet<_>>()
                .len(),
            64
        );
        assert_eq!(
            definition.elements[0].fields.get("name"),
            Some(&PatternValue::String("Qian — Creative Force".to_owned()))
        );
        assert_eq!(
            definition.elements[63].fields.get("name"),
            Some(&PatternValue::String(
                "Wei Ji — Not Yet Complete".to_owned()
            ))
        );
    }

    #[test]
    fn old_yin_lines_transform_receptive_into_creative() {
        let system = IChingSystem::new(i_ching_definition()).expect("valid I Ching");
        let result = system
            .draw(
                &DrawRequest::default(),
                &mut PatternState::default(),
                &mut ZeroRandom,
            )
            .expect("draw succeeds");
        assert_eq!(result.entries[0].name(), Some("Kun — Receptive Earth"));
        assert_eq!(
            result.entries[0].meaning(),
            Some(&PatternValue::Symbol("receptive_devotion".to_owned()))
        );
        let Some(PatternValue::Object(transformed)) = result.entries[0].fields.get("transformed")
        else {
            panic!("transformed hexagram is structured");
        };
        assert_eq!(
            transformed.get("name"),
            Some(&PatternValue::String("Qian — Creative Force".to_owned()))
        );
        assert_eq!(
            transformed.get("meaning"),
            Some(&PatternValue::Symbol("creative_force".to_owned()))
        );
        assert_eq!(
            result.entries[0].fields.get("changing_lines"),
            Some(&PatternValue::List(
                (1..=6)
                    .map(|line| PatternValue::Number(f64::from(line)))
                    .collect()
            ))
        );
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        let system = IChingSystem::new(i_ching_definition()).expect("valid I Ching");
        for method in [DrawMethod::ThreeCoin, DrawMethod::YarrowStalks] {
            let request = DrawRequest {
                method: Some(method),
                ..DrawRequest::default()
            };
            let mut first = SeededRandom::new(404, 0);
            let mut second = SeededRandom::new(404, 0);
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
    fn line_algorithms_follow_their_expected_distributions() {
        let samples = 32_000_f64;
        for (method, expected) in [
            (DrawMethod::ThreeCoin, [0.125, 0.375, 0.375, 0.125]),
            (DrawMethod::YarrowStalks, [0.0625, 0.3125, 0.4375, 0.1875]),
        ] {
            let mut random = SeededRandom::new(8_675_309, 0);
            let mut counts = [0_u32; 4];
            for _ in 0..samples as usize {
                counts[usize::from(generate_line(&method, &mut random) - 6)] += 1;
            }
            for (count, probability) in counts.into_iter().zip(expected) {
                let observed = f64::from(count) / samples;
                assert!(
                    (observed - probability).abs() < 0.015,
                    "{method:?}: {observed} versus {probability}"
                );
            }
        }
    }
}
