use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use weave_core::ir::{BuiltinPatternIr, PatternDrawMethodIr, PatternSystemIr, ValueLiteral};

use crate::builtins::{
    IChingSystem, elder_futhark_definition, i_ching_definition, tarot_definition,
};
use crate::{
    DataPatternSystem, DrawMethod, PatternDefinition, PatternElement, PatternError, PatternSystem,
    PatternValue, ReversalPolicy, SpreadDefinition,
};

/// Build one executable pattern from compiler IR.
///
/// Built-ins and authored data cross the same trait boundary. Specialized algorithms are chosen
/// here, keeping the story runtime independent of individual meaning systems.
pub fn system_from_ir(
    id: &str,
    ir: &PatternSystemIr,
) -> Result<Arc<dyn PatternSystem>, PatternError> {
    validate_ir_configuration(id, ir)?;
    let mut definition = match ir.builtin {
        Some(BuiltinPatternIr::Tarot) => tarot_definition(),
        Some(BuiltinPatternIr::IChing) => i_ching_definition(),
        Some(BuiltinPatternIr::ElderFuthark) => elder_futhark_definition(),
        None => authored_definition(id, ir)?,
    };
    definition.id = id.to_owned();
    definition.default_method = lower_method(&ir.draw_method);
    definition.allow_duplicates = ir.allow_duplicates;
    definition.reversal_policy = if ir.reversals {
        ReversalPolicy::Half
    } else {
        ReversalPolicy::Never
    };
    for (name, spread) in &ir.spreads {
        definition.spreads.insert(
            name.clone(),
            SpreadDefinition {
                name: name.clone(),
                positions: spread.positions.clone(),
            },
        );
    }

    if ir.builtin == Some(BuiltinPatternIr::IChing) {
        return IChingSystem::new(definition)
            .map(|system| Arc::new(system) as Arc<dyn PatternSystem>);
    }
    DataPatternSystem::new(definition).map(|system| Arc::new(system) as Arc<dyn PatternSystem>)
}

fn authored_definition(id: &str, ir: &PatternSystemIr) -> Result<PatternDefinition, PatternError> {
    if ir.collections.len() != 1 {
        return Err(PatternError::InvalidAuthoredData {
            system: id.to_owned(),
            message: format!(
                "expected exactly one element collection, found {}",
                ir.collections.len()
            ),
        });
    }
    let Some(collection) = ir.collections.values().next() else {
        return Err(PatternError::InvalidAuthoredData {
            system: id.to_owned(),
            message: "no element collection was compiled".to_owned(),
        });
    };
    let mut element_ids = BTreeSet::new();
    let mut elements = Vec::with_capacity(collection.elements.len());
    for (index, element) in collection.elements.iter().enumerate() {
        let fields = element
            .fields
            .iter()
            .map(|(name, value)| (name.clone(), lower_value(value)))
            .collect::<BTreeMap<_, _>>();
        let display_name = match fields.get("name") {
            Some(PatternValue::String(name) | PatternValue::Symbol(name)) => name,
            _ => {
                return Err(PatternError::InvalidAuthoredData {
                    system: id.to_owned(),
                    message: format!("element {} has no string or symbol `name`", index + 1),
                });
            }
        };
        if !fields.contains_key("meaning") {
            return Err(PatternError::InvalidAuthoredData {
                system: id.to_owned(),
                message: format!("element {} has no `meaning` field", index + 1),
            });
        }
        let element_id = identifier_from_name(display_name, index);
        if !element_ids.insert(element_id.clone()) {
            return Err(PatternError::InvalidAuthoredData {
                system: id.to_owned(),
                message: format!("element names collapse to duplicate id `{element_id}`"),
            });
        }
        let mut reversed_fields = BTreeMap::new();
        if let Some(reversed_meaning) = fields.get("reversed_meaning") {
            reversed_fields.insert("meaning".to_owned(), reversed_meaning.clone());
        }
        elements.push(PatternElement {
            id: element_id,
            fields,
            reversible: !reversed_fields.is_empty(),
            reversed_fields,
        });
    }
    let spreads = ir
        .spreads
        .iter()
        .map(|(name, spread)| {
            (
                name.clone(),
                SpreadDefinition {
                    name: name.clone(),
                    positions: spread.positions.clone(),
                },
            )
        })
        .collect();
    Ok(PatternDefinition {
        version: crate::PATTERN_MODEL_VERSION,
        id: id.to_owned(),
        name: id.to_owned(),
        elements,
        spreads,
        default_method: lower_method(&ir.draw_method),
        allow_duplicates: ir.allow_duplicates,
        reversal_policy: if ir.reversals {
            ReversalPolicy::Half
        } else {
            ReversalPolicy::Never
        },
    })
}

fn validate_ir_configuration(id: &str, ir: &PatternSystemIr) -> Result<(), PatternError> {
    let invalid_method = match (ir.builtin, &ir.draw_method) {
        (Some(BuiltinPatternIr::Tarot), PatternDrawMethodIr::Uniform) => None,
        (Some(BuiltinPatternIr::Tarot), PatternDrawMethodIr::WeightedBy { field })
            if field == "weight" =>
        {
            None
        }
        (
            Some(BuiltinPatternIr::IChing),
            PatternDrawMethodIr::ThreeCoin | PatternDrawMethodIr::YarrowStalks,
        ) => None,
        (Some(BuiltinPatternIr::ElderFuthark), PatternDrawMethodIr::Uniform) | (None, _) => None,
        (Some(_), method) => Some(method),
    };
    if let Some(method) = invalid_method {
        return Err(PatternError::UnsupportedMethod {
            system: id.to_owned(),
            method: lower_method(method),
        });
    }
    if ir.builtin.is_some() && !ir.collections.is_empty() {
        return Err(PatternError::InvalidAuthoredData {
            system: id.to_owned(),
            message: "built-in data cannot be mixed with authored collections".to_owned(),
        });
    }
    if ir.builtin == Some(BuiltinPatternIr::IChing) && !ir.spreads.is_empty() {
        return Err(PatternError::InvalidAuthoredData {
            system: id.to_owned(),
            message: "I-Ching line generation does not support spreads".to_owned(),
        });
    }
    Ok(())
}

fn lower_method(method: &PatternDrawMethodIr) -> DrawMethod {
    match method {
        PatternDrawMethodIr::Uniform => DrawMethod::Uniform,
        PatternDrawMethodIr::WeightedBy { field } => DrawMethod::Weighted {
            field: field.clone(),
        },
        PatternDrawMethodIr::ThreeCoin => DrawMethod::ThreeCoin,
        PatternDrawMethodIr::YarrowStalks => DrawMethod::YarrowStalks,
    }
}

fn lower_value(value: &ValueLiteral) -> PatternValue {
    match value {
        ValueLiteral::Null => PatternValue::Null,
        ValueLiteral::Bool(value) => PatternValue::Bool(*value),
        ValueLiteral::Number(value) => PatternValue::Number(*value),
        ValueLiteral::String(value) => PatternValue::String(value.clone()),
        ValueLiteral::Symbol(value) => PatternValue::Symbol(value.clone()),
    }
}

fn identifier_from_name(name: &str, index: usize) -> String {
    let mut id = String::new();
    let mut underscore = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
            underscore = false;
        } else if !id.is_empty() && !underscore {
            id.push('_');
            underscore = true;
        }
    }
    while id.ends_with('_') {
        id.pop();
    }
    if id.is_empty() || id.starts_with(|character: char| character.is_ascii_digit()) {
        format!("element_{index}_{id}")
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use weave_core::ir::{
        PatternCollectionIr, PatternDrawMethodIr, PatternElementIr, PatternSystemIr, SpreadIr,
        ValueLiteral,
    };

    use super::*;
    use crate::{DrawRequest, PatternState, SeededRandom};

    #[test]
    fn authored_ir_uses_the_public_pattern_boundary() {
        let ir = PatternSystemIr {
            builtin: None,
            collections: BTreeMap::from([(
                "omens".to_owned(),
                PatternCollectionIr {
                    elements: vec![PatternElementIr {
                        fields: BTreeMap::from([
                            ("name".to_owned(), ValueLiteral::String("Crow".to_owned())),
                            (
                                "meaning".to_owned(),
                                ValueLiteral::Symbol("warning".to_owned()),
                            ),
                        ]),
                    }],
                },
            )]),
            spreads: BTreeMap::from([(
                "single".to_owned(),
                SpreadIr {
                    positions: vec!["sign".to_owned()],
                },
            )]),
            draw_method: PatternDrawMethodIr::Uniform,
            allow_duplicates: false,
            reversals: false,
        };
        let system = system_from_ir("omens", &ir).expect("valid authored system");
        let result = system
            .draw(
                &DrawRequest::default(),
                &mut PatternState::default(),
                &mut SeededRandom::new(4, 0),
            )
            .expect("draw succeeds");
        assert_eq!(result.entries[0].name(), Some("Crow"));
    }
}
