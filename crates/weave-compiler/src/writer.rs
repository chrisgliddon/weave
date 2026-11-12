//! Writer tooling that works on source: stable line ids and engine exports.
//!
//! These functions never execute a story. The Unreal export reads speakers and
//! channels in source order, which is an approximation of what a playthrough
//! sees; [`UnrealExport::warnings`] names every line whose speaker or channel
//! that approximation cannot pin down.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use weave_core::ast::{Cue, Document, Spanned, Statement, StringsEntry};
use weave_core::markup::{
    LineIdEntry, NARRATION_CHANNEL, Piece, Rendered, Tag, collect_line_ids, extract_tags,
    fold_tags, parse_string_value, render, render_line,
};
use weave_core::{Diagnostic, Span, TemplateMode, TemplatePart, parse_template_with};

/// One narrative line located in source, with its effective id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineLocation {
    /// Knot that contains the line.
    pub knot: String,
    /// Effective line id.
    pub id: String,
    /// Whether the id is written with `^id:` in the source.
    pub explicit: bool,
    /// Location of the line.
    pub span: Span,
}

/// List every narrative line and its effective id, in source order.
pub fn list_line_ids(source: &str) -> Result<Vec<LineLocation>, Vec<Diagnostic>> {
    let document = weave_core::parse_document(source)?;
    Ok(document
        .knots()
        .flat_map(|knot| {
            collect_line_ids(&knot.node.name, &knot.node.body)
                .into_iter()
                .map(|entry| LineLocation {
                    knot: knot.node.name.clone(),
                    id: entry.id,
                    explicit: entry.explicit,
                    span: entry.span,
                })
        })
        .collect())
}

/// The result of [`stamp_line_ids`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamped {
    /// The rewritten source.
    pub source: String,
    /// Number of lines that gained an explicit id.
    pub count: usize,
    /// Lines left unstamped because appending a tag would change their text,
    /// such as a line ending in `\`: `(default id, source line)`.
    pub skipped: Vec<(String, usize)>,
}

/// Append ` ^id:<default>` to every narrative line that has no explicit id.
///
/// Stamping is idempotent and never renumbers: each line keeps the id it
/// already had. The source must compile cleanly first, so a stamped file never
/// gains a duplicate id. Every stamp is verified by parsing the result; a line
/// whose stamp would not read back as its explicit id is left alone and
/// reported in [`Stamped::skipped`].
pub fn stamp_line_ids(source: &str) -> Result<Stamped, Vec<Diagnostic>> {
    let document = weave_core::parse_document(source)?;
    let checked = weave_core::type_check(&document);
    if weave_core::has_errors(&checked.diagnostics) {
        return Err(checked.diagnostics);
    }
    let candidates = document
        .knots()
        .flat_map(|knot| collect_line_ids(&knot.node.name, &knot.node.body))
        .filter(|entry| !entry.explicit)
        .map(|entry: LineIdEntry| {
            (
                entry.span.start + entry.raw.len(),
                entry.id,
                entry.span.line,
            )
        })
        .collect::<Vec<_>>();
    let mut skipped = Vec::new();
    loop {
        let insertions = candidates
            .iter()
            .filter(|(_, id, _)| !skipped.iter().any(|(skip, _)| skip == id))
            .collect::<Vec<_>>();
        let output = insert_stamps(source, &insertions);
        // Verify: every stamped line must now carry its id explicitly, and no
        // other line may change.
        let after = list_line_ids(&output)?;
        let before = list_line_ids(source)?;
        let failed = insertions
            .iter()
            .filter(|(_, id, _)| !after.iter().any(|line| line.explicit && &line.id == id))
            .map(|(_, id, line)| (id.clone(), *line))
            .collect::<Vec<_>>();
        if failed.is_empty() {
            if after.len() != before.len()
                || after.iter().zip(&before).any(|(new, old)| new.id != old.id)
            {
                return Err(vec![Diagnostic::error(
                    "W2321",
                    "stamping would change line ids; nothing was written",
                )]);
            }
            return Ok(Stamped {
                source: output,
                count: insertions.len(),
                skipped,
            });
        }
        skipped.extend(failed);
    }
}

fn insert_stamps(source: &str, insertions: &[&(usize, String, usize)]) -> String {
    let mut ordered = insertions.to_vec();
    ordered.sort_by_key(|insertion| std::cmp::Reverse(insertion.0));
    let mut output = source.to_owned();
    for (position, id, _) in ordered {
        output.insert_str(*position, &format!(" ^id:{id}"));
    }
    output
}

/// One file produced by an export, relative to the output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    /// Relative path using `/` separators.
    pub path: String,
    /// UTF-8 file contents.
    pub contents: String,
}

/// Files and advisories produced by [`unreal_export`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnrealExport {
    /// Files to write, in a stable order.
    pub files: Vec<ExportFile>,
    /// Lines whose speaker or channel comes from source order rather than an
    /// explicit cue in the same knot.
    pub warnings: Vec<String>,
}

/// One exported narrative line.
#[derive(Debug, Clone, PartialEq)]
struct ExportLine {
    id: String,
    knot: String,
    channel: String,
    speaker: Option<String>,
    rendered: Rendered,
    tags: Vec<String>,
    meta: BTreeMap<String, String>,
    /// Unique voice file name, assigned once every line is known.
    vo_file: String,
}

impl ExportLine {
    fn base_vo_file(&self) -> String {
        format!(
            "VO_{}_{}",
            self.speaker.as_deref().unwrap_or(NARRATION_CHANNEL),
            self.id.replace('.', "_")
        )
    }
}

/// Build Unreal string-table CSVs, DataTable-ready line rows, and voice scripts.
///
/// The source must compile cleanly; pass the same text given to the compiler.
pub fn unreal_export(source: &str) -> Result<UnrealExport, Vec<Diagnostic>> {
    let document = weave_core::parse_document(source)?;
    let checked = weave_core::type_check(&document);
    if weave_core::has_errors(&checked.diagnostics) {
        return Err(checked.diagnostics);
    }
    let mut export = UnrealExport::default();
    string_tables(&document, &mut export)?;
    let mut lines = export_lines(&document, &mut export.warnings)?;
    assign_voice_files(&mut lines, &mut export.warnings);
    export.files.push(ExportFile {
        path: "Lines.csv".to_owned(),
        contents: lines_csv(&lines),
    });
    export.files.push(ExportFile {
        path: "Lines.json".to_owned(),
        contents: lines_json(&lines),
    });
    let mut by_speaker: BTreeMap<String, Vec<&ExportLine>> = BTreeMap::new();
    for line in &lines {
        by_speaker
            .entry(
                line.speaker
                    .clone()
                    .unwrap_or_else(|| NARRATION_CHANNEL.to_owned()),
            )
            .or_default()
            .push(line);
    }
    for (speaker, lines) in by_speaker {
        export.files.push(ExportFile {
            path: format!("vo/{speaker}.md"),
            contents: voice_script(&speaker, &lines),
        });
    }
    collision_warnings(&lines, &export.files, &mut export.warnings);
    Ok(export)
}

fn string_tables(document: &Document, export: &mut UnrealExport) -> Result<(), Vec<Diagnostic>> {
    for table in document.string_tables() {
        let mut csv = String::new();
        push_csv_row(&mut csv, &["Key", "SourceString", "Comment"]);
        for entry in &table.node.entries {
            let StringsEntry::Entry(item) = &entry.node else {
                continue;
            };
            let rendered = render(&parse_string_value(&item.raw, item.value_span)?);
            // Always rich text, so every row reads the same way: escaped `<`
            // and `&` stay text instead of turning into markup on import.
            let source_string = rendered.rich;
            let comment = tags_source(&item.tags);
            if starts_like_formula(&source_string) {
                export.warnings.push(format!(
                    "string `{}.{}` starts with `=`, `+`, `-`, or `@`; spreadsheet apps may run such CSV cells as formulas, so open the CSVs as text or import them straight into the engine",
                    table.node.name, item.key
                ));
            }
            push_csv_row(&mut csv, &[&item.key, &source_string, &comment]);
        }
        export.files.push(ExportFile {
            path: format!("StringTable_{}.csv", table.node.name),
            contents: csv,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placement {
    speaker: Option<String>,
    channel: String,
    speaker_known: bool,
    channel_known: bool,
}

fn export_lines(
    document: &Document,
    warnings: &mut Vec<String>,
) -> Result<Vec<ExportLine>, Vec<Diagnostic>> {
    let mut lines = Vec::new();
    let mut placement = Placement {
        speaker: None,
        channel: NARRATION_CHANNEL.to_owned(),
        speaker_known: true,
        channel_known: true,
    };
    // Only the opening knot starts from a known placement, and only when no
    // divert or thread can enter it again later with a different one.
    let first_reentered = document.knots().next().is_some_and(|first| {
        document
            .knots()
            .any(|knot| enters(&knot.node.body, &first.node.name))
    });
    for (index, knot) in document.knots().enumerate() {
        if index > 0 || first_reentered {
            placement.speaker_known = false;
            placement.channel_known = false;
        }
        let ids = collect_line_ids(&knot.node.name, &knot.node.body);
        let mut walker = Walker {
            knot: &knot.node.name,
            ids: ids.into_iter(),
            lines: &mut lines,
            unsure: Vec::new(),
        };
        placement = walker.block(&knot.node.body, placement)?;
        if let Some(first) = walker.unsure.first() {
            warnings.push(format!(
                "knot `{}`: {} line(s) take their speaker or channel from source order (first `{}` on line {}); start the knot with `@speaker` and `@to` to make them exact",
                knot.node.name,
                walker.unsure.len(),
                first.0,
                first.1
            ));
        }
    }
    Ok(lines)
}

struct Walker<'a> {
    knot: &'a str,
    ids: std::vec::IntoIter<LineIdEntry>,
    lines: &'a mut Vec<ExportLine>,
    unsure: Vec<(String, usize)>,
}

impl Walker<'_> {
    fn block(
        &mut self,
        statements: &[Spanned<Statement>],
        mut placement: Placement,
    ) -> Result<Placement, Vec<Diagnostic>> {
        for statement in statements {
            match &statement.node {
                Statement::Text(raw) => {
                    let id = self.ids.next().map(|entry| entry.id).unwrap_or_default();
                    if !(placement.speaker_known && placement.channel_known) {
                        self.unsure.push((id.clone(), statement.span.line));
                    }
                    let tagged = extract_tags(raw, statement.span)?;
                    let tags = fold_tags(&tagged.tags, true)?;
                    self.lines.push(ExportLine {
                        id,
                        knot: self.knot.to_owned(),
                        channel: placement.channel.clone(),
                        speaker: placement.speaker.clone(),
                        rendered: script_text(&tagged.text)?,
                        tags: tags.tags,
                        meta: tags.meta,
                        vo_file: String::new(),
                    });
                }
                Statement::Cue(Cue::Speaker(speaker)) => {
                    placement.speaker.clone_from(speaker);
                    placement.speaker_known = true;
                }
                Statement::Cue(Cue::To(channel)) => {
                    placement.channel = channel
                        .clone()
                        .unwrap_or_else(|| NARRATION_CHANNEL.to_owned());
                    placement.channel_known = true;
                }
                Statement::Choice(choice) => {
                    // A choice set may be skipped when no choice is eligible, so
                    // the placement before it is one possible outcome.
                    let after = self.block(&choice.body, placement.clone())?;
                    placement = merge(&placement, &after);
                }
                Statement::Thread(_) => {
                    // A thread runs another knot inline, and its cues stay in effect.
                    placement.speaker_known = false;
                    placement.channel_known = false;
                }
                Statement::Conditional(conditional) => {
                    let mut outcomes = Vec::new();
                    for branch in &conditional.branches {
                        outcomes.push(self.block(&branch.body, placement.clone())?);
                    }
                    match &conditional.fallback {
                        Some(fallback) => outcomes.push(self.block(fallback, placement.clone())?),
                        None => outcomes.push(placement.clone()),
                    }
                    let mut merged = outcomes[0].clone();
                    for outcome in &outcomes[1..] {
                        merged = merge(&merged, outcome);
                    }
                    placement = merged;
                }
                _ => {}
            }
        }
        Ok(placement)
    }
}

/// Whether any divert or thread in `statements` targets `knot`.
fn enters(statements: &[Spanned<Statement>], knot: &str) -> bool {
    statements.iter().any(|statement| match &statement.node {
        Statement::Divert(target) | Statement::Thread(target) => target == knot,
        Statement::Choice(choice) => {
            choice.divert.as_deref() == Some(knot) || enters(&choice.body, knot)
        }
        Statement::Conditional(conditional) => {
            conditional
                .branches
                .iter()
                .any(|branch| enters(&branch.body, knot))
                || conditional
                    .fallback
                    .as_deref()
                    .is_some_and(|fallback| enters(fallback, knot))
        }
        _ => false,
    })
}

/// Give every line a voice file name of its own. Names come from the speaker
/// and line id with dots as underscores, so different ids can meet (`intro.1`
/// and `intro_1`); a later line that would reuse a name, ignoring case, gets
/// `_2`, `_3`, and so on, with a warning so the writer can pick a clearer id.
fn assign_voice_files(lines: &mut [ExportLine], warnings: &mut Vec<String>) {
    let mut taken: BTreeMap<String, String> = BTreeMap::new();
    for line in lines.iter_mut() {
        let base = line.base_vo_file();
        let mut name = base.clone();
        let mut suffix = 2;
        while taken.contains_key(&name.to_lowercase()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        if name != base {
            warnings.push(format!(
                "line `{}` would share the voice file name `{base}` with line `{}`; it is exported as `{name}`, or give it a different `^id:`",
                line.id,
                taken[&base.to_lowercase()]
            ));
        }
        taken.insert(name.to_lowercase(), line.id.clone());
        line.vo_file = name;
    }
}

/// Warn about names that would overwrite each other: files and row names that
/// differ only in case (one file on case-insensitive filesystems, one row in
/// an Unreal DataTable), and cells a spreadsheet would run as a formula.
fn collision_warnings(lines: &[ExportLine], files: &[ExportFile], warnings: &mut Vec<String>) {
    let mut rows: BTreeMap<String, &str> = BTreeMap::new();
    for line in lines {
        if let Some(other) = rows.insert(line.id.to_lowercase(), &line.id)
            && other != line.id
        {
            warnings.push(format!(
                "line ids `{other}` and `{}` differ only in case; Unreal row names ignore case",
                line.id
            ));
        }
    }
    let mut paths: BTreeMap<String, &str> = BTreeMap::new();
    for file in files {
        if let Some(other) = paths.insert(file.path.to_lowercase(), &file.path) {
            warnings.push(format!(
                "`{other}` and `{}` differ only in case and collide on case-insensitive filesystems",
                file.path
            ));
        }
    }
    let formula = lines
        .iter()
        .find(|line| starts_like_formula(&line.rendered.text))
        .map(|line| line.id.clone());
    if let Some(id) = formula {
        warnings.push(format!(
            "line `{id}` starts with `=`, `+`, `-`, or `@`; spreadsheet apps may run such CSV cells as formulas, so open the CSVs as text or import them straight into the engine"
        ));
    }
    if lines
        .iter()
        .any(|line| line.speaker.as_deref() == Some(NARRATION_CHANNEL))
    {
        warnings.push(format!(
            "a speaker named `{NARRATION_CHANNEL}` shares `vo/{NARRATION_CHANNEL}.md` with unvoiced narration"
        ));
    }
}

/// Combine two possible placements: values that differ become uncertain.
/// The later value is kept as the best guess, matching source order.
fn merge(first: &Placement, second: &Placement) -> Placement {
    Placement {
        speaker: second.speaker.clone(),
        channel: second.channel.clone(),
        speaker_known: first.speaker_known
            && second.speaker_known
            && first.speaker == second.speaker,
        channel_known: first.channel_known
            && second.channel_known
            && first.channel == second.channel,
    }
}

/// Render a narrative line for a script: markup is applied, and grammar
/// references and interpolations stay as readable placeholders.
fn script_text(text: &str) -> Result<Rendered, Vec<Diagnostic>> {
    let span = Span::new(0, text.len(), 1, 1);
    let parts = parse_template_with(text, span, TemplateMode::Markup)?;
    let pieces = parts
        .into_iter()
        .map(|part| match part.node {
            TemplatePart::Text(text) => Piece::Text(text),
            TemplatePart::GrammarRef { grammar, rule } => Piece::Text(match grammar {
                Some(grammar) => format!("#{grammar}.{rule}#"),
                None => format!("#{rule}#"),
            }),
            TemplatePart::Expression(_) => {
                Piece::Text(text[part.span.start..part.span.end].to_owned())
            }
            TemplatePart::Pause(seconds) => Piece::Pause(seconds),
            TemplatePart::StyleOpen(name) => Piece::Open(name),
            TemplatePart::StyleClose(name) => Piece::Close(name),
        })
        .collect::<Vec<_>>();
    Ok(render_line(&pieces))
}

const LINE_COLUMNS: [&str; 9] = [
    "Name", "Knot", "Channel", "Speaker", "Text", "Rich", "Tags", "Meta", "VoFile",
];

fn line_fields(line: &ExportLine) -> [String; 9] {
    [
        line.id.clone(),
        line.knot.clone(),
        line.channel.clone(),
        line.speaker.clone().unwrap_or_default(),
        line.rendered.text.clone(),
        line.rendered.rich.clone(),
        line.tags.join(" "),
        line.meta
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(";"),
        line.vo_file.clone(),
    ]
}

fn lines_csv(lines: &[ExportLine]) -> String {
    let mut csv = String::new();
    push_csv_row(&mut csv, &LINE_COLUMNS);
    for line in lines {
        let fields = line_fields(line);
        let fields = fields.iter().map(String::as_str).collect::<Vec<_>>();
        push_csv_row(&mut csv, &fields);
    }
    csv
}

fn lines_json(lines: &[ExportLine]) -> String {
    let rows = lines
        .iter()
        .map(|line| {
            LINE_COLUMNS
                .iter()
                .zip(line_fields(line))
                .map(|(column, value)| ((*column).to_owned(), serde_json::Value::String(value)))
                .collect::<serde_json::Map<_, _>>()
        })
        .map(serde_json::Value::Object)
        .collect::<Vec<_>>();
    let mut output =
        serde_json::to_string_pretty(&serde_json::Value::Array(rows)).unwrap_or_default();
    output.push('\n');
    output
}

fn voice_script(speaker: &str, lines: &[&ExportLine]) -> String {
    let mut output = format!(
        "# Voice script: {speaker}\n\nLines in story source order. Pauses are marked as `[pause N s]`.\n\n| ID | Line | Direction | File |\n|---|---|---|---|\n"
    );
    for line in lines {
        let mut direction = line.tags.clone();
        direction.extend(
            line.meta
                .iter()
                .map(|(key, value)| format!("{key}: {value}")),
        );
        let _ = writeln!(
            output,
            "| {} | {} | {} | {} |",
            markdown_cell(&line.id),
            markdown_cell(&spoken_text(&line.rendered)),
            markdown_cell(&direction.join(", ")),
            markdown_cell(&line.vo_file),
        );
    }
    output
}

/// Plain text with pauses written inline for a voice actor, each marker set
/// off by spaces, as in `Wait. [pause 1 s] Go.`.
fn spoken_text(rendered: &Rendered) -> String {
    fn marker(output: &mut String, pause: f64) {
        if !output.is_empty() && !output.ends_with(char::is_whitespace) {
            output.push(' ');
        }
        let _ = write!(output, "[pause {pause} s]");
    }
    let mut output = String::new();
    let mut beats = rendered.beats.iter().peekable();
    for (index, character) in rendered.text.chars().enumerate() {
        let mut marked = false;
        while let Some(beat) = beats.next_if(|beat| beat.at == index) {
            marker(&mut output, beat.pause);
            marked = true;
        }
        if marked && !character.is_whitespace() {
            output.push(' ');
        }
        output.push(character);
    }
    for beat in beats {
        marker(&mut output, beat.pause);
    }
    output
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('`', "\\`")
        .replace("\r\n", "<br>")
        .replace(['\n', '\r'], "<br>")
}

fn tags_source(tags: &[Spanned<Tag>]) -> String {
    tags.iter()
        .map(|tag| tag.node.to_source())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a spreadsheet would treat a cell starting this way as a formula.
fn starts_like_formula(text: &str) -> bool {
    text.starts_with(['=', '+', '-', '@', '\t', '\r'])
}

fn push_csv_row(output: &mut String, fields: &[&str]) {
    let row = fields
        .iter()
        .map(|field| format!("\"{}\"", field.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    output.push_str(&row);
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"channel dialog { cps: 40 }

stage mara {
    name: "Mara"
}

STRINGS ui
    menu.start = "<b>Begin</b>, \"friend\"" ^button ^sound:click
    menu.quit = "Leave"

=== intro ===
@to dialog
@speaker mara
Hello <pause 0.5>there, {visits}. ^warm ^id:intro.hello
* [Go]
    @speaker
    A door | opens.
Back again.
-> END
"#;

    #[test]
    fn stamps_missing_ids_once_and_keeps_existing_ones() {
        let Stamped {
            source: stamped,
            count,
            ..
        } = stamp_line_ids(SOURCE).expect("stamp");
        assert_eq!(count, 2);
        assert!(stamped.contains("A door | opens. ^id:intro.2\n"));
        assert!(stamped.contains("Back again. ^id:intro.3\n"));
        assert!(stamped.contains("^warm ^id:intro.hello\n"));
        let Stamped {
            source: again,
            count,
            ..
        } = stamp_line_ids(&stamped).expect("restamp");
        assert_eq!(count, 0);
        assert_eq!(again, stamped);
        let before = list_line_ids(SOURCE).expect("ids");
        let after = list_line_ids(&stamped).expect("ids");
        assert_eq!(
            before.iter().map(|line| &line.id).collect::<Vec<_>>(),
            after.iter().map(|line| &line.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exports_string_tables_lines_and_voice_scripts() {
        let export = unreal_export(SOURCE).expect("export");
        let file = |path: &str| {
            export
                .files
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.contents.clone())
                .unwrap_or_else(|| panic!("missing {path}"))
        };
        assert_eq!(
            file("StringTable_ui.csv"),
            "\"Key\",\"SourceString\",\"Comment\"\n\"menu.start\",\"<b>Begin</>, &quot;friend&quot;\",\"^button ^sound:click\"\n\"menu.quit\",\"Leave\",\"\"\n"
        );
        let csv = file("Lines.csv");
        assert!(csv.starts_with(
            "\"Name\",\"Knot\",\"Channel\",\"Speaker\",\"Text\",\"Rich\",\"Tags\",\"Meta\",\"VoFile\"\n"
        ));
        assert!(csv.contains(
            "\"intro.hello\",\"intro\",\"dialog\",\"mara\",\"Hello there, {visits}.\",\"Hello there, {visits}.\",\"warm\",\"\",\"VO_mara_intro_hello\"\n"
        ));
        assert!(csv.contains("\"intro.2\",\"intro\",\"dialog\",\"\",\"A door | opens.\""));
        let json: serde_json::Value = serde_json::from_str(&file("Lines.json")).expect("json");
        assert_eq!(json[0]["Name"], "intro.hello");
        assert_eq!(json[1]["VoFile"], "VO_narration_intro_2");
        let mara = file("vo/mara.md");
        assert!(mara.contains(
            "| intro.hello | Hello [pause 0.5 s] there, {visits}. | warm | VO_mara_intro_hello |"
        ));
        let narration = file("vo/narration.md");
        assert!(narration.contains("A door \\| opens."));
        // After an optional choice that changes the speaker, the next line is uncertain.
        assert_eq!(export.warnings.len(), 1, "{:?}", export.warnings);
        assert!(export.warnings[0].contains("intro.3"));
    }

    #[test]
    fn skips_nothing_and_stays_idempotent_after_a_trailing_backslash() {
        let source = "=== intro ===\nEnds with backslash \\\nNext.\n-> END\n";
        let first = stamp_line_ids(source).expect("stamp");
        assert_eq!(first.count, 2, "{first:?}");
        assert!(first.skipped.is_empty());
        let second = stamp_line_ids(&first.source).expect("restamp");
        assert_eq!(second.count, 0);
        assert_eq!(second.source, first.source);
    }

    #[test]
    fn threads_and_reentered_openings_make_speakers_uncertain() {
        let threaded = "stage mara {\n    name: \"Mara\"\n}\n=== intro ===\nOpening.\n<- aside\nAfter.\n-> END\n=== aside ===\n@speaker mara\nAside.\n";
        let export = unreal_export(threaded).expect("export");
        assert!(
            export
                .warnings
                .iter()
                .any(|warning| warning.contains("knot `intro`")),
            "{:?}",
            export.warnings
        );
        let looped = "stage mara {\n    name: \"Mara\"\n}\nVAR n = 0\n=== intro ===\nFirst.\n@speaker mara\nSecond.\nSET n = n + 1\n{n < 2:\n    -> intro\n}\n-> END\n";
        let export = unreal_export(looped).expect("export");
        assert!(
            export
                .warnings
                .iter()
                .any(|warning| warning.contains("knot `intro`")),
            "{:?}",
            export.warnings
        );
    }

    #[test]
    fn warns_when_voice_file_names_collide() {
        let source = "=== intro ===\nOne.\nTwo. ^id:intro_1\n-> END\n";
        let export = unreal_export(source).expect("export");
        assert!(
            export
                .warnings
                .iter()
                .any(|warning| warning.contains("VO_narration_intro_1_2")),
            "{:?}",
            export.warnings
        );
        let csv = &export
            .files
            .iter()
            .find(|file| file.path == "Lines.csv")
            .expect("lines")
            .contents;
        assert!(csv.contains("\"VO_narration_intro_1\"\n"), "{csv}");
        assert!(csv.contains("\"VO_narration_intro_1_2\"\n"), "{csv}");

        let formula = unreal_export(
            "STRINGS ui\n    a = \"=SUM(1,2)\"\n=== intro ===\n@speaker\n-1 is odd.\n-> END\n",
        )
        .expect("export");
        assert_eq!(
            formula
                .warnings
                .iter()
                .filter(|warning| warning.contains("formulas"))
                .count(),
            2,
            "{:?}",
            formula.warnings
        );
    }

    #[test]
    fn voice_scripts_space_pause_markers_and_escape_html() {
        let source = "=== intro ===\nWait. <pause 1> Go.\nSee \\<img src=x\\> now.\n-> END\n";
        let export = unreal_export(source).expect("export");
        let script = &export
            .files
            .iter()
            .find(|file| file.path == "vo/narration.md")
            .expect("script")
            .contents;
        assert!(script.contains("Wait. [pause 1 s] Go."), "{script}");
        assert!(script.contains("&lt;img src=x&gt;"), "{script}");
        let table = unreal_export("STRINGS ui\n    a = \"A\"\n=== intro ===\nHi.\n-> END\n")
            .expect("export");
        assert!(
            table.files[0]
                .contents
                .starts_with("\"Key\",\"SourceString\",\"Comment\"")
        );
    }
}
