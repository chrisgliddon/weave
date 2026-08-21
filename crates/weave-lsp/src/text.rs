use tower_lsp_server::ls_types::{Position, Range, TextDocumentContentChangeEvent};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextEditError {
    #[error("line {0} is outside the document")]
    Line(u32),
    #[error("UTF-16 character {character} is outside line {line}")]
    Character { line: u32, character: u32 },
    #[error("UTF-16 character {character} splits a surrogate pair on line {line}")]
    Surrogate { line: u32, character: u32 },
    #[error("change range ends before it starts")]
    ReversedRange,
}

pub fn byte_offset(text: &str, position: Position) -> Result<usize, TextEditError> {
    let line_start = line_start(text, position.line).ok_or(TextEditError::Line(position.line))?;
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |offset| line_start + offset);
    let line = &text[line_start..line_end];
    let target = position.character;
    let mut units = 0_u32;
    for (offset, character) in line.char_indices() {
        if units == target {
            return Ok(line_start + offset);
        }
        let next = units + character.len_utf16() as u32;
        if target < next {
            return Err(TextEditError::Surrogate {
                line: position.line,
                character: target,
            });
        }
        units = next;
    }
    if units == target {
        Ok(line_end)
    } else {
        Err(TextEditError::Character {
            line: position.line,
            character: target,
        })
    }
}

pub fn position_at(text: &str, byte_offset: usize) -> Position {
    let mut offset = byte_offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    let prefix = &text[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = text[line_start..offset].encode_utf16().count() as u32;
    Position::new(line, character)
}

pub fn full_range(text: &str) -> Range {
    Range::new(Position::new(0, 0), position_at(text, text.len()))
}

pub fn apply_changes(
    text: &mut String,
    changes: &[TextDocumentContentChangeEvent],
) -> Result<(), TextEditError> {
    for change in changes {
        if let Some(range) = change.range {
            let start = byte_offset(text, range.start)?;
            let end = byte_offset(text, range.end)?;
            if end < start {
                return Err(TextEditError::ReversedRange);
            }
            text.replace_range(start..end, &change.text);
        } else {
            text.clone_from(&change.text);
        }
    }
    Ok(())
}

fn line_start(text: &str, target: u32) -> Option<usize> {
    if target == 0 {
        return Some(0);
    }
    let mut line = 0_u32;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            line += 1;
            if line == target {
                return Some(index + 1);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_positions_without_splitting_surrogates() {
        let source = "a😀b\nsecond";
        assert_eq!(byte_offset(source, Position::new(0, 1)), Ok(1));
        assert!(matches!(
            byte_offset(source, Position::new(0, 2)),
            Err(TextEditError::Surrogate { .. })
        ));
        assert_eq!(byte_offset(source, Position::new(0, 3)), Ok(5));
        assert_eq!(position_at(source, 5), Position::new(0, 3));
        assert_eq!(position_at(source, source.len()), Position::new(1, 6));
    }

    #[test]
    fn applies_sequential_incremental_and_full_changes() {
        let mut source = "hello world".to_owned();
        apply_changes(
            &mut source,
            &[
                TextDocumentContentChangeEvent {
                    range: Some(Range::new(Position::new(0, 6), Position::new(0, 11))),
                    range_length: Some(5),
                    text: "Weave".to_owned(),
                },
                TextDocumentContentChangeEvent {
                    range: Some(Range::new(Position::new(0, 0), Position::new(0, 5))),
                    range_length: Some(5),
                    text: "Hello".to_owned(),
                },
            ],
        )
        .expect("valid changes");
        assert_eq!(source, "Hello Weave");

        apply_changes(
            &mut source,
            &[TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "replacement".to_owned(),
            }],
        )
        .expect("full replacement");
        assert_eq!(source, "replacement");
    }
}
