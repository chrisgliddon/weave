use std::collections::{BTreeMap, BTreeSet};

use crate::PATTERN_MODEL_VERSION;
use crate::model::{
    DrawMethod, DrawRequest, DrawResult, DrawnElement, PatternDefinition, PatternError,
    PatternState, PatternSystem, PatternValue, RandomSource, ReversalPolicy, valid_identifier,
};

/// Generic executor for immutable built-in or author-defined element data.
#[derive(Debug, Clone)]
pub struct DataPatternSystem {
    definition: PatternDefinition,
}

impl DataPatternSystem {
    /// Validate and own one immutable definition.
    pub fn new(definition: PatternDefinition) -> Result<Self, PatternError> {
        validate_definition(&definition)?;
        Ok(Self { definition })
    }
}

impl PatternSystem for DataPatternSystem {
    fn definition(&self) -> &PatternDefinition {
        &self.definition
    }

    fn draw(
        &self,
        request: &DrawRequest,
        state: &mut PatternState,
        random: &mut dyn RandomSource,
    ) -> Result<DrawResult, PatternError> {
        let positions = match &request.spread {
            Some(spread) => self
                .definition
                .spreads
                .get(spread)
                .ok_or_else(|| PatternError::UnknownSpread {
                    system: self.definition.id.clone(),
                    spread: spread.clone(),
                })?
                .positions
                .iter()
                .cloned()
                .map(Some)
                .collect::<Vec<_>>(),
            None => vec![None],
        };
        let allow_duplicates = request
            .allow_duplicates
            .unwrap_or(self.definition.allow_duplicates);
        if !allow_duplicates && positions.len() > self.definition.elements.len() {
            return Err(PatternError::InsufficientElements {
                system: self.definition.id.clone(),
                requested: positions.len(),
                available: self.definition.elements.len(),
            });
        }
        let method = request
            .method
            .clone()
            .unwrap_or_else(|| self.definition.default_method.clone());
        if matches!(method, DrawMethod::ThreeCoin | DrawMethod::YarrowStalks) {
            return Err(PatternError::UnsupportedMethod {
                system: self.definition.id.clone(),
                method,
            });
        }
        let reversal_policy = request
            .reversal_policy
            .unwrap_or(self.definition.reversal_policy);
        let mut available = (0..self.definition.elements.len()).collect::<Vec<_>>();
        let mut entries = Vec::with_capacity(positions.len());
        for position in positions {
            let selected = choose_index(
                &self.definition,
                &method,
                &available,
                allow_duplicates,
                random,
            )?;
            let element = &self.definition.elements[selected];
            if !allow_duplicates
                && let Some(index) = available
                    .iter()
                    .position(|candidate| *candidate == selected)
            {
                available.remove(index);
            }
            let reversed = reversal_policy == ReversalPolicy::Half
                && element.reversible
                && random.next_u64() & 1 == 1;
            let mut fields = element.fields.clone();
            if reversed {
                fields.extend(element.reversed_fields.clone());
            }
            entries.push(DrawnElement {
                id: element.id.clone(),
                position,
                reversed,
                fields,
            });
        }
        state.draws = state.draws.saturating_add(1);
        state.last_draw = entries.iter().map(|entry| entry.id.clone()).collect();
        Ok(DrawResult {
            system: self.definition.id.clone(),
            spread: request.spread.clone(),
            method,
            entries,
            metadata: BTreeMap::new(),
        })
    }
}

fn choose_index(
    definition: &PatternDefinition,
    method: &DrawMethod,
    available: &[usize],
    allow_duplicates: bool,
    random: &mut dyn RandomSource,
) -> Result<usize, PatternError> {
    let candidates = if allow_duplicates {
        (0..definition.elements.len()).collect::<Vec<_>>()
    } else {
        available.to_vec()
    };
    match method {
        DrawMethod::Uniform => Ok(candidates[random.next_u64() as usize % candidates.len()]),
        DrawMethod::Weighted { field } => {
            let mut weights = Vec::with_capacity(candidates.len());
            let mut total = 0.0;
            for index in &candidates {
                let element = &definition.elements[*index];
                let Some(PatternValue::Number(weight)) = element.fields.get(field) else {
                    return Err(PatternError::InvalidWeight {
                        element: element.id.clone(),
                        field: field.clone(),
                    });
                };
                if !weight.is_finite() || *weight <= 0.0 {
                    return Err(PatternError::InvalidWeight {
                        element: element.id.clone(),
                        field: field.clone(),
                    });
                }
                total += weight;
                weights.push(*weight);
            }
            let unit = random.next_u64() as f64 / u64::MAX as f64;
            let mut threshold = unit * total;
            for (index, weight) in candidates.iter().zip(weights) {
                if threshold < weight {
                    return Ok(*index);
                }
                threshold -= weight;
            }
            candidates
                .last()
                .copied()
                .ok_or_else(|| PatternError::EmptyPattern(definition.id.clone()))
        }
        DrawMethod::ThreeCoin | DrawMethod::YarrowStalks => Err(PatternError::UnsupportedMethod {
            system: definition.id.clone(),
            method: method.clone(),
        }),
    }
}

fn validate_definition(definition: &PatternDefinition) -> Result<(), PatternError> {
    if definition.version != PATTERN_MODEL_VERSION {
        return Err(PatternError::UnsupportedVersion {
            found: definition.version,
            expected: PATTERN_MODEL_VERSION,
        });
    }
    if !valid_identifier(&definition.id) {
        return Err(PatternError::InvalidIdentifier {
            kind: "pattern",
            value: definition.id.clone(),
        });
    }
    if definition.elements.is_empty() {
        return Err(PatternError::EmptyPattern(definition.id.clone()));
    }
    let mut elements = BTreeSet::new();
    for element in &definition.elements {
        if !valid_identifier(&element.id) {
            return Err(PatternError::InvalidIdentifier {
                kind: "element",
                value: element.id.clone(),
            });
        }
        if !elements.insert(element.id.clone()) {
            return Err(PatternError::DuplicateElement {
                system: definition.id.clone(),
                element: element.id.clone(),
            });
        }
        for field in element.fields.keys().chain(element.reversed_fields.keys()) {
            if !valid_identifier(field) {
                return Err(PatternError::InvalidIdentifier {
                    kind: "field",
                    value: field.clone(),
                });
            }
        }
    }
    let mut spreads = BTreeSet::new();
    for (key, spread) in &definition.spreads {
        if key != &spread.name || !valid_identifier(key) {
            return Err(PatternError::InvalidIdentifier {
                kind: "spread",
                value: key.clone(),
            });
        }
        if !spreads.insert(key.clone()) {
            return Err(PatternError::DuplicateSpread {
                system: definition.id.clone(),
                spread: key.clone(),
            });
        }
        if spread.positions.is_empty() {
            return Err(PatternError::EmptySpread {
                system: definition.id.clone(),
                spread: key.clone(),
            });
        }
        let mut positions = BTreeSet::new();
        for position in &spread.positions {
            if !valid_identifier(position) {
                return Err(PatternError::InvalidIdentifier {
                    kind: "position",
                    value: position.clone(),
                });
            }
            if !positions.insert(position.clone()) {
                return Err(PatternError::DuplicatePosition {
                    system: definition.id.clone(),
                    spread: key.clone(),
                    position: position.clone(),
                });
            }
        }
        if !definition.allow_duplicates && spread.positions.len() > definition.elements.len() {
            return Err(PatternError::InsufficientElements {
                system: definition.id.clone(),
                requested: spread.positions.len(),
                available: definition.elements.len(),
            });
        }
    }
    if let DrawMethod::Weighted { field } = &definition.default_method {
        for element in &definition.elements {
            let Some(PatternValue::Number(weight)) = element.fields.get(field) else {
                return Err(PatternError::InvalidWeight {
                    element: element.id.clone(),
                    field: field.clone(),
                });
            };
            if !weight.is_finite() || *weight <= 0.0 {
                return Err(PatternError::InvalidWeight {
                    element: element.id.clone(),
                    field: field.clone(),
                });
            }
        }
    }
    if definition.reversal_policy == ReversalPolicy::Half
        && !definition.elements.iter().any(|element| element.reversible)
    {
        return Err(PatternError::InvalidReversalConfiguration(
            definition.id.clone(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PatternElement, SeededRandom, SpreadDefinition};

    fn definition() -> PatternDefinition {
        PatternDefinition {
            version: PATTERN_MODEL_VERSION,
            id: "omens".to_owned(),
            name: "Omens".to_owned(),
            elements: [
                ("crow", "ill_tidings", 4.0),
                ("sun_dog", "good_fortune", 1.0),
                ("wolf", "harsh_winter", 2.0),
            ]
            .into_iter()
            .map(|(id, meaning, weight)| PatternElement {
                id: id.to_owned(),
                fields: BTreeMap::from([
                    ("name".to_owned(), PatternValue::String(id.to_owned())),
                    (
                        "meaning".to_owned(),
                        PatternValue::Symbol(meaning.to_owned()),
                    ),
                    ("severity".to_owned(), PatternValue::Number(weight)),
                ]),
                reversed_fields: BTreeMap::new(),
                reversible: false,
            })
            .collect(),
            spreads: BTreeMap::from([(
                "day".to_owned(),
                SpreadDefinition {
                    name: "day".to_owned(),
                    positions: vec!["dawn".to_owned(), "noon".to_owned(), "dusk".to_owned()],
                },
            )]),
            default_method: DrawMethod::Weighted {
                field: "severity".to_owned(),
            },
            allow_duplicates: false,
            reversal_policy: ReversalPolicy::Never,
        }
    }

    #[test]
    fn seeded_spreads_are_deterministic_and_unique() {
        let system = DataPatternSystem::new(definition()).expect("valid definition");
        let request = DrawRequest {
            spread: Some("day".to_owned()),
            ..DrawRequest::default()
        };
        let mut first_random = SeededRandom::new(17, 0);
        let mut second_random = SeededRandom::new(17, 0);
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
                .map(|entry| &entry.id)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn invalid_weight_returns_a_structured_error() {
        let mut invalid = definition();
        invalid.elements[0]
            .fields
            .insert("severity".to_owned(), PatternValue::Number(0.0));
        assert!(matches!(
            DataPatternSystem::new(invalid),
            Err(PatternError::InvalidWeight { .. })
        ));
    }

    #[test]
    fn state_is_separate_and_serializable() {
        let system = DataPatternSystem::new(definition()).expect("valid definition");
        let mut state = PatternState::default();
        let mut random = SeededRandom::new(1, 0);
        system
            .draw(&DrawRequest::default(), &mut state, &mut random)
            .expect("draw succeeds");
        let encoded = ron::to_string(&state).expect("serialize state");
        let restored: PatternState = ron::from_str(&encoded).expect("deserialize state");
        assert_eq!(restored, state);
        assert_eq!(state.draws, 1);
    }
}
