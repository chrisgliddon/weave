//! Editor-independent Language Server Protocol support for Weave.

mod analysis;
mod text;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use analysis::{
    Analysis, Outline, ReferenceTarget, Symbol, SymbolKind as WeaveSymbolKind, analyze,
};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};
use walkdir::{DirEntry, WalkDir};
use weave_core::{Diagnostic as WeaveDiagnostic, Severity, Span};

#[derive(Debug, Clone)]
struct DocumentState {
    text: String,
    version: Option<i32>,
    open: bool,
    analysis: Analysis,
}

impl DocumentState {
    fn new(text: String, version: Option<i32>, open: bool) -> Self {
        let analysis = analyze(&text);
        Self {
            text,
            version,
            open,
            analysis,
        }
    }
}

#[derive(Debug, Default)]
struct Project {
    documents: HashMap<Uri, DocumentState>,
    roots: Vec<PathBuf>,
}

impl Project {
    fn add_root(&mut self, root: PathBuf) {
        if !self.roots.contains(&root) {
            self.roots.push(root.clone());
        }
        self.load_root(&root);
    }

    fn remove_root(&mut self, root: &Path) {
        self.roots.retain(|candidate| candidate != root);
        self.documents.retain(|uri, document| {
            document.open
                || uri
                    .to_file_path()
                    .is_none_or(|path| !path.starts_with(root))
        });
    }

    fn load_root(&mut self, root: &Path) {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(relevant_entry)
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "weave")
            })
        {
            self.load_path(entry.path());
        }
    }

    fn load_path(&mut self, path: &Path) {
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let Some(uri) = Uri::from_file_path(path) else {
            return;
        };
        if self
            .documents
            .get(&uri)
            .is_some_and(|document| document.open)
        {
            return;
        }
        self.documents
            .insert(uri, DocumentState::new(text, None, false));
    }

    fn matching_symbols<'a>(
        &'a self,
        target: &ReferenceTarget,
        preferred: &Uri,
    ) -> Vec<(&'a Uri, &'a DocumentState, &'a Symbol)> {
        let mut symbols = self
            .documents
            .iter()
            .flat_map(|(uri, document)| {
                document
                    .analysis
                    .symbols
                    .iter()
                    .filter(|symbol| target.matches(symbol))
                    .map(move |symbol| (uri, document, symbol))
            })
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| {
            (left.0 != preferred, left.0.as_str(), left.2.selection.start).cmp(&(
                right.0 != preferred,
                right.0.as_str(),
                right.2.selection.start,
            ))
        });
        symbols
    }

    fn target_at(&self, uri: &Uri, position: Position) -> Option<(ReferenceTarget, Span)> {
        let document = self.documents.get(uri)?;
        let byte = text::byte_offset(&document.text, position).ok()?;
        Some((
            document.analysis.target_at(byte)?,
            document.analysis.selection_at(byte)?,
        ))
    }

    fn diagnostics(&self) -> Vec<(Uri, Vec<Diagnostic>, Option<i32>)> {
        self.documents
            .iter()
            .map(|(uri, document)| {
                let diagnostics = document
                    .analysis
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| !self.resolved_by_workspace(uri, diagnostic))
                    .map(|diagnostic| lsp_diagnostic(&document.text, diagnostic))
                    .collect();
                (uri.clone(), diagnostics, document.version)
            })
            .collect()
    }

    fn resolved_by_workspace(&self, uri: &Uri, diagnostic: &WeaveDiagnostic) -> bool {
        if diagnostic.code.0 == "W2012" {
            return self.documents.values().any(|document| {
                document
                    .analysis
                    .symbols
                    .iter()
                    .any(|symbol| symbol.kind == WeaveSymbolKind::Knot)
            });
        }
        if !matches!(
            diagnostic.code.0,
            "W2017" | "W2019" | "W2021" | "W2023" | "W2024" | "W2028" | "W2029"
        ) {
            return false;
        }
        let Some(span) = diagnostic.span else {
            return false;
        };
        let Some(document) = self.documents.get(uri) else {
            return false;
        };
        let relevant = document
            .analysis
            .references
            .iter()
            .filter(|reference| {
                ranges_overlap(reference.span, span)
                    && reference_applies_to_diagnostic(&reference.target, diagnostic.code.0)
            })
            .collect::<Vec<_>>();
        !relevant.is_empty()
            && relevant.iter().all(|reference| {
                self.documents.values().any(|candidate| {
                    candidate
                        .analysis
                        .symbols
                        .iter()
                        .any(|symbol| reference.target.matches(symbol))
                })
            })
    }
}

/// Weave language server backend.
#[derive(Debug)]
pub struct Backend {
    client: Client,
    project: RwLock<Project>,
}

impl Backend {
    /// Create a language server backend for a tower-lsp client connection.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            project: RwLock::new(Project::default()),
        }
    }

    async fn publish_diagnostics(&self) {
        let diagnostics = self.project.read().await.diagnostics();
        for (uri, diagnostics, version) in diagnostics {
            self.client
                .publish_diagnostics(uri, diagnostics, version)
                .await;
        }
    }

    async fn reload_uri(&self, uri: &Uri) {
        let Some(path) = uri.to_file_path() else {
            return;
        };
        let mut project = self.project.write().await;
        if path.exists() {
            project.load_path(&path);
        } else if !project
            .documents
            .get(uri)
            .is_some_and(|document| document.open)
        {
            project.documents.remove(uri);
        }
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let roots = initialization_roots(&params);
        {
            let mut project = self.project.write().await;
            for root in roots {
                project.add_root(root);
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "#".to_owned(),
                        ".".to_owned(),
                        ">".to_owned(),
                        "<".to_owned(),
                    ]),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "weave-lsp".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Weave language server initialized")
            .await;
        self.publish_diagnostics().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.project.write().await.documents.insert(
            document.uri,
            DocumentState::new(document.text, Some(document.version), true),
        );
        self.publish_diagnostics().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let error = {
            let mut project = self.project.write().await;
            let Some(document) = project.documents.get_mut(&uri) else {
                return;
            };
            if document.version.is_some_and(|current| version <= current) {
                return;
            }
            match text::apply_changes(&mut document.text, &params.content_changes) {
                Ok(()) => {
                    document.version = Some(version);
                    document.analysis = analyze(&document.text);
                    None
                }
                Err(error) => Some(error.to_string()),
            }
        };
        if let Some(error) = error {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("ignored malformed document change: {error}"),
                )
                .await;
        }
        self.publish_diagnostics().await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            let mut project = self.project.write().await;
            if let Some(document) = project.documents.get_mut(&params.text_document.uri) {
                document.text = text;
                document.analysis = analyze(&document.text);
            }
        }
        self.publish_diagnostics().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().map(|path| path.into_owned());
        {
            let mut project = self.project.write().await;
            if let Some(path) = path.filter(|path| path.exists()) {
                if let Ok(text) = fs::read_to_string(path) {
                    project
                        .documents
                        .insert(uri.clone(), DocumentState::new(text, None, false));
                }
            } else {
                project.documents.remove(&uri);
            }
        }
        self.publish_diagnostics().await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            self.reload_uri(&change.uri).await;
        }
        self.publish_diagnostics().await;
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let mut project = self.project.write().await;
        for removed in params.event.removed {
            if let Some(path) = removed.uri.to_file_path() {
                project.remove_root(&path);
            }
        }
        for added in params.event.added {
            if let Some(path) = added.uri.to_file_path() {
                project.add_root(path.into_owned());
            }
        }
        drop(project);
        self.publish_diagnostics().await;
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let project = self.project.read().await;
        let Some(document) = project.documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let mut symbols = document
            .analysis
            .outlines
            .iter()
            .map(|outline| document_symbol(&document.text, outline))
            .collect::<Vec<_>>();
        symbols.extend(
            document
                .analysis
                .symbols
                .iter()
                .filter(|symbol| symbol.kind == WeaveSymbolKind::Variable)
                .map(|symbol| DocumentSymbol {
                    name: symbol.name.clone(),
                    detail: Some(symbol.detail.clone()),
                    kind: lsp_symbol_kind(symbol.kind),
                    tags: None,
                    #[allow(deprecated)]
                    deprecated: None,
                    range: span_range(&document.text, symbol.span),
                    selection_range: span_range(&document.text, symbol.selection),
                    children: None,
                }),
        );
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params;
        let project = self.project.read().await;
        let Some((target, _)) = project.target_at(&position.text_document.uri, position.position)
        else {
            return Ok(None);
        };
        let locations = project
            .matching_symbols(&target, &position.text_document.uri)
            .into_iter()
            .map(|(uri, document, symbol)| Location {
                uri: uri.clone(),
                range: span_range(&document.text, symbol.selection),
            })
            .collect::<Vec<_>>();
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let position = params.text_document_position;
        let project = self.project.read().await;
        let Some((target, _)) = project.target_at(&position.text_document.uri, position.position)
        else {
            return Ok(None);
        };
        let definitions = project.matching_symbols(&target, &position.text_document.uri);
        if definitions.is_empty() {
            return Ok(None);
        }
        let mut locations = Vec::new();
        if params.context.include_declaration {
            locations.extend(definitions.iter().map(|(uri, document, symbol)| Location {
                uri: (*uri).clone(),
                range: span_range(&document.text, symbol.selection),
            }));
        }
        for (uri, document) in &project.documents {
            locations.extend(
                document
                    .analysis
                    .references
                    .iter()
                    .filter(|reference| {
                        definitions
                            .iter()
                            .any(|(_, _, symbol)| reference.target.matches(symbol))
                    })
                    .map(|reference| Location {
                        uri: uri.clone(),
                        range: span_range(&document.text, reference.span),
                    }),
            );
        }
        sort_locations(&mut locations, &position.text_document.uri);
        Ok(Some(locations))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params;
        let project = self.project.read().await;
        let Some((target, selection)) =
            project.target_at(&position.text_document.uri, position.position)
        else {
            return Ok(None);
        };
        let Some((_, _, symbol)) = project
            .matching_symbols(&target, &position.text_document.uri)
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "```weave\n{} {}\n```\n\n{}",
                    symbol.kind.label(),
                    symbol.name,
                    symbol.detail
                ),
            }),
            range: project
                .documents
                .get(&position.text_document.uri)
                .map(|document| span_range(&document.text, selection)),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let position = params.text_document_position;
        let project = self.project.read().await;
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        for (label, detail) in KEYWORD_COMPLETIONS {
            seen.insert((*label).to_owned());
            items.push(CompletionItem {
                label: (*label).to_owned(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some((*detail).to_owned()),
                ..CompletionItem::default()
            });
        }
        for document in project.documents.values() {
            for symbol in &document.analysis.symbols {
                if seen.insert(symbol.name.clone()) {
                    items.push(CompletionItem {
                        label: symbol.name.clone(),
                        kind: Some(completion_kind(symbol.kind)),
                        detail: Some(symbol.detail.clone()),
                        ..CompletionItem::default()
                    });
                }
            }
        }
        if let Some(document) = project.documents.get(&position.text_document.uri)
            && let Ok(byte) = text::byte_offset(&document.text, position.position)
            && line_prefix(&document.text, byte)
                .trim_start()
                .starts_with("->")
        {
            items.retain(|item| {
                item.kind == Some(CompletionItemKind::FUNCTION) || item.label == "END"
            });
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let project = self.project.read().await;
        let Some((target, selection)) =
            project.target_at(&params.text_document.uri, params.position)
        else {
            return Ok(None);
        };
        if project
            .matching_symbols(&target, &params.text_document.uri)
            .is_empty()
        {
            return Ok(None);
        }
        let Some(document) = project.documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::Range(span_range(
            &document.text,
            selection,
        ))))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        if !valid_identifier(&params.new_name) {
            return Err(Error::invalid_params(
                "Weave names must start with a letter or underscore and contain only ASCII letters, digits, or underscores",
            ));
        }
        let position = params.text_document_position;
        let project = self.project.read().await;
        let Some((target, _)) = project.target_at(&position.text_document.uri, position.position)
        else {
            return Ok(None);
        };
        let definitions = project.matching_symbols(&target, &position.text_document.uri);
        if definitions.is_empty() {
            return Ok(None);
        }
        let mut changes = HashMap::new();
        for (uri, document) in &project.documents {
            let mut edits = document
                .analysis
                .references
                .iter()
                .filter(|reference| {
                    definitions
                        .iter()
                        .any(|(_, _, symbol)| reference.target.matches(symbol))
                })
                .map(|reference| TextEdit {
                    range: span_range(&document.text, reference.span),
                    new_text: params.new_name.clone(),
                })
                .collect::<Vec<_>>();
            edits.extend(
                definitions
                    .iter()
                    .filter(|(definition_uri, _, _)| *definition_uri == uri)
                    .map(|(_, _, symbol)| TextEdit {
                        range: span_range(&document.text, symbol.selection),
                        new_text: params.new_name.clone(),
                    }),
            );
            edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
            edits.dedup_by(|left, right| left.range == right.range);
            if !edits.is_empty() {
                changes.insert(uri.clone(), edits);
            }
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let project = self.project.read().await;
        let Some(document) = project.documents.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let Ok(formatted) = weave_fmt::format_source(&document.text) else {
            return Ok(None);
        };
        if formatted == document.text {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: text::full_range(&document.text),
            new_text: formatted,
        }]))
    }
}

const KEYWORD_COMPLETIONS: &[(&str, &str)] = &[
    ("VAR", "declare a story variable"),
    ("LIST", "declare a story list"),
    ("FLAG", "declare a Boolean flag"),
    ("STATE", "declare a state machine"),
    ("GRAMMAR", "declare a generative grammar"),
    ("PATTERN", "declare a pattern system"),
    ("END", "end story execution"),
];

fn initialization_roots(params: &InitializeParams) -> Vec<PathBuf> {
    if let Some(folders) = &params.workspace_folders {
        return folders
            .iter()
            .filter_map(|folder| folder.uri.to_file_path().map(|path| path.into_owned()))
            .collect();
    }
    #[allow(deprecated)]
    params
        .root_uri
        .as_ref()
        .and_then(Uri::to_file_path)
        .map(|path| vec![path.into_owned()])
        .unwrap_or_default()
}

fn relevant_entry(entry: &DirEntry) -> bool {
    !entry.file_type().is_dir()
        || !matches!(
            entry.file_name().to_str(),
            Some(".git" | ".dex" | "target" | "node_modules")
        )
}

fn lsp_diagnostic(source: &str, diagnostic: &WeaveDiagnostic) -> Diagnostic {
    let message = diagnostic.help.as_ref().map_or_else(
        || diagnostic.message.clone(),
        |help| format!("{}\nHelp: {help}", diagnostic.message),
    );
    Diagnostic {
        range: diagnostic.span.map_or_else(
            || Range::new(Position::new(0, 0), Position::new(0, 0)),
            |span| span_range(source, span),
        ),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        source: Some("weave".to_owned()),
        message,
        ..Diagnostic::default()
    }
}

fn document_symbol(source: &str, outline: &Outline) -> DocumentSymbol {
    DocumentSymbol {
        name: outline.name.clone(),
        detail: Some(outline.detail.clone()),
        kind: lsp_symbol_kind(outline.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: span_range(source, outline.span),
        selection_range: span_range(source, outline.selection),
        children: (!outline.children.is_empty()).then(|| {
            outline
                .children
                .iter()
                .map(|child| document_symbol(source, child))
                .collect()
        }),
    }
}

const fn lsp_symbol_kind(kind: WeaveSymbolKind) -> SymbolKind {
    match kind {
        WeaveSymbolKind::Knot => SymbolKind::FUNCTION,
        WeaveSymbolKind::Grammar => SymbolKind::NAMESPACE,
        WeaveSymbolKind::GrammarRule => SymbolKind::STRING,
        WeaveSymbolKind::Pattern => SymbolKind::CLASS,
        WeaveSymbolKind::Collection => SymbolKind::ARRAY,
        WeaveSymbolKind::Spread => SymbolKind::STRUCT,
        WeaveSymbolKind::Variable => SymbolKind::VARIABLE,
    }
}

const fn completion_kind(kind: WeaveSymbolKind) -> CompletionItemKind {
    match kind {
        WeaveSymbolKind::Knot => CompletionItemKind::FUNCTION,
        WeaveSymbolKind::Grammar => CompletionItemKind::MODULE,
        WeaveSymbolKind::GrammarRule => CompletionItemKind::VALUE,
        WeaveSymbolKind::Pattern => CompletionItemKind::CLASS,
        WeaveSymbolKind::Collection => CompletionItemKind::FIELD,
        WeaveSymbolKind::Spread => CompletionItemKind::STRUCT,
        WeaveSymbolKind::Variable => CompletionItemKind::VARIABLE,
    }
}

fn span_range(source: &str, span: Span) -> Range {
    Range::new(
        text::position_at(source, span.start),
        text::position_at(source, span.end),
    )
}

fn ranges_overlap(left: Span, right: Span) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn reference_applies_to_diagnostic(target: &ReferenceTarget, code: &str) -> bool {
    match (code, target) {
        ("W2017" | "W2019", ReferenceTarget::Exact(key)) => key.starts_with("variable:"),
        ("W2021", ReferenceTarget::Exact(key)) => key.starts_with("knot:"),
        ("W2023", ReferenceTarget::Exact(key)) => key.starts_with("grammar:"),
        ("W2024", ReferenceTarget::GrammarRule { .. }) => true,
        ("W2028", ReferenceTarget::Value(_)) => true,
        ("W2029", ReferenceTarget::Exact(key)) => {
            key.starts_with("pattern:") || key.starts_with("spread:")
        }
        _ => false,
    }
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn line_prefix(source: &str, byte: usize) -> &str {
    let start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    &source[start..byte]
}

fn sort_locations(locations: &mut [Location], preferred: &Uri) {
    locations.sort_by(|left, right| {
        (
            &left.uri != preferred,
            left.uri.as_str(),
            left.range.start.line,
            left.range.start.character,
        )
            .cmp(&(
                &right.uri != preferred,
                right.uri.as_str(),
                right.range.start.line,
                right.range.start.character,
            ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_weave_rename_identifiers() {
        assert!(valid_identifier("next_scene_2"));
        assert!(!valid_identifier("2nd_scene"));
        assert!(!valid_identifier("next-scene"));
        assert!(!valid_identifier(""));
    }
}
