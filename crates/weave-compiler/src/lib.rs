//! Compiler from `.weave` source to the versioned runtime IR.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use semver::Version;
use weave_domain::{
    DomainCatalog, DomainError, DomainValue, ExportSource, ResolvedDomainModule,
    resolve_module_order,
};

use weave_core::ast::{
    BinaryOperator, Declaration, Document, Expr, GrammarEntry, Item, ListOperation, Literal,
    ModuleDecl, ModuleEntry, PatternDrawMethod, PatternEntry, Spanned, Statement, UnaryOperator,
    VariableKind,
};
use weave_core::ir::{
    BinaryOperatorIr, BuiltinPatternIr, ChoiceIr, ConditionalBranchIr, ConditionalIr,
    DeclarationIr, DomainExportIr, DomainExportSourceIr, DomainModuleIr, DomainValueIr, Expression,
    GrammarIr, Instruction, InstructionKind, KnotIr, ListOperationIr, PatternCollectionIr,
    PatternDrawMethodIr, PatternElementIr, PatternSystemIr, SpreadIr, StoryIr, Template,
    TemplatePartIr, UnaryOperatorIr, ValueLiteral, VariableKindIr,
};
use weave_core::{
    Diagnostic, DomainModuleSignature, PatternSignature, Severity, TemplatePart, has_errors,
    parse_document, parse_template, type_check_with_extensions,
};

/// Compiler version corresponding to the current IR.
pub const COMPILER_IR_VERSION: u32 = weave_core::ir::IR_VERSION;

/// Stable identifier published in the machine-readable JSON Schema.
pub const JSON_SCHEMA_ID: &str = "urn:weave:schema:story-ir:3";

/// Source metadata supplied by an embedding application.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileOptions {
    /// Optional path or display name embedded in the IR.
    pub source_name: Option<String>,
}

/// Successful compilation and its non-fatal diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStory {
    /// Immutable runtime representation.
    pub story: StoryIr,
    /// Compiler warnings. Successful compilation never contains errors here.
    pub diagnostics: Vec<Diagnostic>,
    /// Exact validated artifacts selected for editor and build inspection.
    pub domain_modules: BTreeMap<String, ResolvedDomainModule>,
}

/// One or more source diagnostics that prevented compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// Complete diagnostic list, in source order where possible.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            diagnostic.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Error for CompileError {}

/// A serialization failure after successful source compilation.
#[derive(Debug, thiserror::Error)]
pub enum SerializeError {
    /// RON serialization failed.
    #[error("could not serialize RON: {0}")]
    Ron(#[from] ron::Error),
    /// JSON serialization failed.
    #[error("could not serialize JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parse, statically check, and lower source into runtime IR.
pub fn compile(source: &str, options: &CompileOptions) -> Result<CompiledStory, CompileError> {
    compile_with_extensions(source, options, &BTreeMap::new(), &DomainCatalog::new())
}

/// Compile source with validated, data-only external pattern definitions in scope.
///
/// The definitions are embedded into the resulting story IR. Runtime consumers therefore do not
/// access the package registry or execute package-provided code.
pub fn compile_with_patterns(
    source: &str,
    options: &CompileOptions,
    external_patterns: &BTreeMap<String, PatternSystemIr>,
) -> Result<CompiledStory, CompileError> {
    compile_with_extensions(source, options, external_patterns, &DomainCatalog::new())
}

/// Compile source with an explicit catalog of declarative domain artifacts.
pub fn compile_with_modules(
    source: &str,
    options: &CompileOptions,
    domain_catalog: &DomainCatalog,
) -> Result<CompiledStory, CompileError> {
    compile_with_extensions(source, options, &BTreeMap::new(), domain_catalog)
}

/// Compile source with all validated external data surfaces in scope.
pub fn compile_with_extensions(
    source: &str,
    options: &CompileOptions,
    external_patterns: &BTreeMap<String, PatternSystemIr>,
    domain_catalog: &DomainCatalog,
) -> Result<CompiledStory, CompileError> {
    let mut external_diagnostics = Vec::new();
    for (name, pattern) in external_patterns {
        if let Err(error) = weave_patterns::system_from_ir(name, pattern) {
            external_diagnostics.push(Diagnostic::error(
                "W3001",
                format!("external pattern `{name}` is invalid: {error}"),
            ));
        }
    }
    if has_errors(&external_diagnostics) {
        return Err(CompileError {
            diagnostics: external_diagnostics,
        });
    }

    let document = parse_document(source).map_err(|diagnostics| CompileError { diagnostics })?;
    let pattern_signatures = external_patterns
        .iter()
        .map(|(name, pattern)| {
            let mut spreads = pattern.spreads.keys().cloned().collect::<Vec<_>>();
            match pattern.builtin {
                Some(BuiltinPatternIr::Tarot) => spreads.push("three_card".to_owned()),
                Some(BuiltinPatternIr::ElderFuthark) => spreads.push("three_rune".to_owned()),
                Some(BuiltinPatternIr::IChing) | None => {}
            }
            (name.clone(), PatternSignature::new(spreads))
        })
        .collect::<BTreeMap<_, _>>();
    let domain_modules = resolve_domain_activations(&document, domain_catalog)?;
    let module_signatures = domain_modules
        .iter()
        .map(|(alias, resolved)| {
            (
                alias.clone(),
                DomainModuleSignature::from_manifest(&resolved.manifest),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let checked = type_check_with_extensions(&document, &pattern_signatures, &module_signatures);
    if has_errors(&checked.diagnostics) {
        return Err(CompileError {
            diagnostics: checked.diagnostics,
        });
    }

    let mut lowerer = Lowerer::new(options.source_name.clone());
    let mut story = lowerer.lower_document(&document);
    if has_errors(&lowerer.diagnostics) {
        return Err(CompileError {
            diagnostics: lowerer.diagnostics,
        });
    }

    let mut diagnostics = checked
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .collect::<Vec<_>>();
    diagnostics.append(&mut lowerer.diagnostics);
    story.patterns.extend(external_patterns.clone());
    story.modules = domain_modules
        .iter()
        .map(|(alias, resolved)| (alias.clone(), lower_domain_module(resolved)))
        .collect();
    Ok(CompiledStory {
        story,
        diagnostics,
        domain_modules,
    })
}

fn resolve_domain_activations(
    document: &Document,
    catalog: &DomainCatalog,
) -> Result<BTreeMap<String, ResolvedDomainModule>, CompileError> {
    let current_weave = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| CompileError {
        diagnostics: vec![Diagnostic::error(
            "D109",
            "the compiler has an invalid embedded compatibility version",
        )],
    })?;
    let mut resolved = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for module in document.modules() {
        let Some((module_id, module_requirement, pack_id, pack_requirement)) =
            activation_fields(module, &mut diagnostics)
        else {
            continue;
        };
        match catalog.resolve(
            module_id,
            module_requirement,
            pack_id,
            pack_requirement,
            &current_weave,
        ) {
            Ok(activation) => {
                if resolved
                    .insert(module.node.alias.clone(), activation)
                    .is_some()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "D110",
                            format!(
                                "domain module alias `{}` is activated more than once",
                                module.node.alias
                            ),
                        )
                        .with_span(module.span),
                    );
                }
            }
            Err(error) => diagnostics.push(domain_diagnostic(&error, module.span)),
        }
    }

    if diagnostics.is_empty() && !resolved.is_empty() {
        let manifests = resolved
            .values()
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();
        if let Err(error) = resolve_module_order(&manifests, &current_weave) {
            let span = document.modules().next().map(|module| module.span);
            let mut diagnostic = Diagnostic::error(domain_error_code(&error), error.to_string());
            if let Some(span) = span {
                diagnostic = diagnostic.with_span(span);
            }
            diagnostics.push(diagnostic);
        }
    }

    if diagnostics.is_empty() {
        Ok(resolved)
    } else {
        Err(CompileError { diagnostics })
    }
}

fn activation_fields<'a>(
    module: &'a Spanned<ModuleDecl>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(&'a str, &'a str, &'a str, &'a str)> {
    let mut id = None;
    let mut version = None;
    let mut pack = None;
    for entry in &module.node.entries {
        let (slot, field): (&mut Option<&str>, &str) = match &entry.node {
            ModuleEntry::Id(value) => (&mut id, value),
            ModuleEntry::Version(value) => (&mut version, value),
            ModuleEntry::Pack(value) => (&mut pack, value),
            ModuleEntry::Comment(_) | ModuleEntry::Blank => continue,
        };
        if slot.replace(field).is_some() {
            diagnostics.push(
                Diagnostic::error("D112", "domain activation repeats a required field")
                    .with_span(entry.span)
                    .with_help("declare `id`, `version`, and `pack` exactly once"),
            );
        }
    }
    if id.is_none() || version.is_none() || pack.is_none() {
        diagnostics.push(
            Diagnostic::error(
                "D113",
                "domain activation requires `id`, `version`, and `pack` fields",
            )
            .with_span(module.span),
        );
        return None;
    }
    let (Some(id), Some(version), Some(pack)) = (id, version, pack) else {
        return None;
    };
    let Some((pack_id, pack_requirement)) = pack.rsplit_once('@') else {
        diagnostics.push(
            Diagnostic::error(
                "D152",
                "domain pack selector must use `pack_id@version_requirement`",
            )
            .with_span(module.span),
        );
        return None;
    };
    if pack_id.is_empty() || pack_requirement.is_empty() {
        diagnostics.push(
            Diagnostic::error("D152", "domain pack selector has an empty component")
                .with_span(module.span),
        );
        return None;
    }
    Some((id, version, pack_id, pack_requirement))
}

fn domain_diagnostic(error: &DomainError, span: weave_core::Span) -> Diagnostic {
    Diagnostic::error(domain_error_code(error), error.to_string())
        .with_span(span)
        .with_help("check the activation and explicit domain artifacts supplied to this build")
}

fn domain_error_code(error: &DomainError) -> &'static str {
    match error {
        DomainError::UnsupportedContract { .. } => "D100",
        DomainError::UnsupportedPackFormat { .. } => "D101",
        DomainError::UnsupportedCapability { .. } => "D102",
        DomainError::ModuleVersionNotInstalled | DomainError::IncompatibleWeave { .. } => "D103",
        DomainError::PackVersionNotInstalled | DomainError::IncompatibleModule { .. } => "D104",
        DomainError::DuplicateManifestArtifact
        | DomainError::DuplicatePackArtifact
        | DomainError::DuplicateModule { .. }
        | DomainError::NamespaceCollision { .. } => "D110",
        DomainError::MissingDependency { .. }
        | DomainError::IncompatibleDependency { .. }
        | DomainError::DependencyCycle => "D120",
        DomainError::InvalidField { path, .. } if is_provenance_path(path) => "D130",
        DomainError::UnknownExport { .. }
        | DomainError::MissingExport { .. }
        | DomainError::UnknownType { .. }
        | DomainError::RecursiveType { .. }
        | DomainError::TypeMismatch { .. }
        | DomainError::InvalidField { .. } => "D140",
        DomainError::ModuleNotInstalled => "D150",
        DomainError::PackNotInstalled => "D151",
        DomainError::InvalidArtifactVersion { .. }
        | DomainError::InvalidActivationRequirement { .. }
        | DomainError::InvalidJson { .. }
        | DomainError::InvalidRon
        | DomainError::Serialize => "D152",
    }
}

fn is_provenance_path(path: &str) -> bool {
    path == "authors"
        || path.starts_with("authors[")
        || path == "license"
        || path == "license_url"
        || path == "provenance"
        || path.starts_with("provenance.")
}

fn lower_domain_module(resolved: &ResolvedDomainModule) -> DomainModuleIr {
    let exports = resolved
        .pack
        .values
        .iter()
        .filter_map(|(name, value)| {
            let declaration = resolved.manifest.exports.get(name)?;
            Some((
                name.clone(),
                DomainExportIr {
                    source: match declaration.source {
                        ExportSource::Pack => DomainExportSourceIr::Pack,
                        ExportSource::State => DomainExportSourceIr::State,
                    },
                    value: lower_domain_value(value),
                },
            ))
        })
        .collect();
    DomainModuleIr {
        id: resolved.manifest.id.clone(),
        version: resolved.manifest.version.clone(),
        pack_id: resolved.pack.id.clone(),
        pack_version: resolved.pack.version.clone(),
        exports,
    }
}

fn lower_domain_value(value: &DomainValue) -> DomainValueIr {
    match value {
        DomainValue::Null => DomainValueIr::Null,
        DomainValue::Bool(value) => DomainValueIr::Bool(*value),
        DomainValue::Number(value) => DomainValueIr::Number(*value),
        DomainValue::String(value) => DomainValueIr::String(value.clone()),
        DomainValue::Symbol(value) => DomainValueIr::Symbol(value.clone()),
        DomainValue::List(values) => {
            DomainValueIr::List(values.iter().map(lower_domain_value).collect())
        }
        DomainValue::Object(fields) => DomainValueIr::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), lower_domain_value(value)))
                .collect(),
        ),
    }
}

/// Serialize an IR story as canonical, human-readable RON.
pub fn to_ron(story: &StoryIr) -> Result<String, SerializeError> {
    let config = ron::ser::PrettyConfig::new()
        .new_line("\n")
        .indentor("    ")
        .struct_names(true)
        .enumerate_arrays(false)
        .compact_arrays(false)
        .compact_maps(false);
    let mut output = ron::ser::to_string_pretty(story, config)?;
    output.push('\n');
    Ok(output)
}

/// Serialize an IR story as stable, pretty JSON.
pub fn to_json(story: &StoryIr) -> Result<String, SerializeError> {
    let mut output = serde_json::to_string_pretty(story)?;
    output.push('\n');
    Ok(output)
}

/// Generate the canonical JSON Schema for the current runtime IR.
pub fn json_schema() -> Result<String, SerializeError> {
    let schema = schemars::schema_for!(StoryIr);
    let mut value = serde_json::to_value(schema)?;
    if let Some(root) = value.as_object_mut() {
        root.insert(
            "$id".to_owned(),
            serde_json::Value::String(JSON_SCHEMA_ID.to_owned()),
        );
        root.insert(
            "title".to_owned(),
            serde_json::Value::String("Weave Story IR v3".to_owned()),
        );
        root.insert(
            "x-weave-ir-version".to_owned(),
            serde_json::Value::from(COMPILER_IR_VERSION),
        );
        if let Some(version) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut("version"))
            .and_then(serde_json::Value::as_object_mut)
        {
            version.insert(
                "const".to_owned(),
                serde_json::Value::from(COMPILER_IR_VERSION),
            );
        }
    }
    sort_json_keys(&mut value);
    let mut output = serde_json::to_string_pretty(&value)?;
    output.push('\n');
    Ok(output)
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                sort_json_keys(value);
            }
            object.sort_keys();
        }
        _ => {}
    }
}

/// Compile source directly to canonical RON.
pub fn compile_to_ron(
    source: &str,
    options: &CompileOptions,
) -> Result<(String, Vec<Diagnostic>), CompileOrSerializeError> {
    let compiled = compile(source, options)?;
    let output = to_ron(&compiled.story)?;
    Ok((output, compiled.diagnostics))
}

/// Failure from the convenience compile-and-serialize API.
#[derive(Debug, thiserror::Error)]
pub enum CompileOrSerializeError {
    /// Source compilation failed.
    #[error("{0}")]
    Compile(#[from] CompileError),
    /// Output serialization failed.
    #[error("{0}")]
    Serialize(#[from] SerializeError),
}

struct Lowerer {
    source_name: Option<String>,
    diagnostics: Vec<Diagnostic>,
    current_grammar: Option<String>,
    current_knot: String,
}

impl Lowerer {
    fn new(source_name: Option<String>) -> Self {
        Self {
            source_name,
            diagnostics: Vec::new(),
            current_grammar: None,
            current_knot: String::new(),
        }
    }

    fn lower_document(&mut self, document: &Document) -> StoryIr {
        let entry = document
            .knots()
            .next()
            .map(|knot| knot.node.name.clone())
            .unwrap_or_else(|| "start".to_owned());
        let mut story = StoryIr::new(entry);
        story.source_name = self.source_name.clone();

        for item in &document.items {
            match &item.node {
                Item::Module(_) => {}
                Item::Grammar(grammar) => {
                    self.current_grammar = Some(grammar.node.name.clone());
                    let mut rules = BTreeMap::new();
                    for entry in &grammar.node.entries {
                        if let GrammarEntry::Rule(rule) = &entry.node {
                            let alternatives = rule
                                .alternatives
                                .iter()
                                .map(|alternative| self.lower_template(alternative, entry.span))
                                .collect();
                            rules.insert(rule.name.clone(), alternatives);
                        }
                    }
                    story
                        .grammars
                        .insert(grammar.node.name.clone(), GrammarIr { rules });
                    self.current_grammar = None;
                }
                Item::Pattern(pattern) => {
                    let mut builtin = None;
                    let mut collections = BTreeMap::new();
                    let mut spreads = BTreeMap::new();
                    let mut draw_method = None;
                    let mut allow_duplicates = false;
                    let mut reversals = false;
                    for entry in &pattern.node.entries {
                        match &entry.node {
                            PatternEntry::Builtin(name) => {
                                builtin = match name.as_str() {
                                    "tarot" => Some(BuiltinPatternIr::Tarot),
                                    "i_ching" => Some(BuiltinPatternIr::IChing),
                                    "elder_futhark" => Some(BuiltinPatternIr::ElderFuthark),
                                    _ => None,
                                };
                            }
                            PatternEntry::DrawMethod(method) => {
                                draw_method = Some(match method {
                                    PatternDrawMethod::Uniform => PatternDrawMethodIr::Uniform,
                                    PatternDrawMethod::WeightedBy(field) => {
                                        PatternDrawMethodIr::WeightedBy {
                                            field: field.clone(),
                                        }
                                    }
                                    PatternDrawMethod::ThreeCoin => PatternDrawMethodIr::ThreeCoin,
                                    PatternDrawMethod::YarrowStalks => {
                                        PatternDrawMethodIr::YarrowStalks
                                    }
                                });
                            }
                            PatternEntry::Reversals(value) => reversals = *value,
                            PatternEntry::Duplicates(value) => allow_duplicates = *value,
                            PatternEntry::Collection(collection) => {
                                let elements = collection
                                    .elements
                                    .iter()
                                    .map(|element| PatternElementIr {
                                        fields: element
                                            .fields
                                            .iter()
                                            .map(|(name, value)| {
                                                (name.clone(), lower_literal(value))
                                            })
                                            .collect(),
                                    })
                                    .collect();
                                collections.insert(
                                    collection.name.clone(),
                                    PatternCollectionIr { elements },
                                );
                            }
                            PatternEntry::Spread(spread) => {
                                spreads.insert(
                                    spread.name.clone(),
                                    SpreadIr {
                                        positions: spread.positions.clone(),
                                    },
                                );
                            }
                            PatternEntry::Comment(_) | PatternEntry::Blank => {}
                        }
                    }
                    story.patterns.insert(
                        pattern.node.name.clone(),
                        PatternSystemIr {
                            builtin,
                            collections,
                            spreads,
                            draw_method: draw_method.unwrap_or_else(|| {
                                if builtin == Some(BuiltinPatternIr::IChing) {
                                    PatternDrawMethodIr::ThreeCoin
                                } else {
                                    PatternDrawMethodIr::Uniform
                                }
                            }),
                            allow_duplicates,
                            reversals,
                        },
                    );
                }
                Item::Global(statement) => {
                    if let Some(instruction) = self.lower_statement(statement, "global") {
                        story.globals.push(instruction);
                    }
                }
                Item::Knot(knot) => {
                    self.current_knot.clone_from(&knot.node.name);
                    let content = self.lower_block(&knot.node.body, "root");
                    story
                        .knots
                        .insert(knot.node.name.clone(), KnotIr { content });
                }
                Item::Comment(_) | Item::Blank => {}
            }
        }
        story
    }

    fn lower_block(&mut self, statements: &[Spanned<Statement>], path: &str) -> Vec<Instruction> {
        let mut semantic_index = 0;
        let mut lowered = Vec::new();
        for statement in statements {
            let statement_path = format!("{path}.{semantic_index}");
            if let Some(instruction) = self.lower_statement(statement, &statement_path) {
                semantic_index += 1;
                lowered.push(instruction);
            }
        }
        lowered
    }

    fn lower_statement(
        &mut self,
        statement: &Spanned<Statement>,
        path: &str,
    ) -> Option<Instruction> {
        let kind = match &statement.node {
            Statement::Text(text) => {
                InstructionKind::Text(self.lower_template(text, statement.span))
            }
            Statement::Declare(declaration) => {
                InstructionKind::Declare(lower_declaration(declaration))
            }
            Statement::Assign { name, value } => InstructionKind::Assign {
                name: name.clone(),
                value: lower_expression(&value.node),
            },
            Statement::MutateList {
                operation,
                name,
                value,
            } => InstructionKind::MutateList {
                operation: match operation {
                    ListOperation::Push => ListOperationIr::Push,
                    ListOperation::Remove => ListOperationIr::Remove,
                },
                name: name.clone(),
                value: lower_expression(&value.node),
            },
            Statement::Choice(choice) => {
                let id = format!("{}:{path}", self.current_knot);
                let body = self.lower_block(&choice.body, &format!("{path}.body"));
                InstructionKind::Choice(ChoiceIr {
                    id,
                    once: choice.once,
                    text: self.lower_template(&choice.text, statement.span),
                    condition: choice
                        .condition
                        .as_ref()
                        .map(|condition| lower_expression(&condition.node)),
                    body,
                    divert: choice.divert.clone(),
                })
            }
            Statement::Conditional(conditional) => {
                let branches = conditional
                    .branches
                    .iter()
                    .enumerate()
                    .map(|(index, branch)| ConditionalBranchIr {
                        condition: lower_expression(&branch.condition.node),
                        body: self.lower_block(&branch.body, &format!("{path}.branch.{index}")),
                    })
                    .collect();
                let fallback = conditional
                    .fallback
                    .as_ref()
                    .map(|fallback| self.lower_block(fallback, &format!("{path}.fallback")));
                InstructionKind::Conditional(ConditionalIr { branches, fallback })
            }
            Statement::Divert(target) => InstructionKind::Divert(target.clone()),
            Statement::Thread(target) => InstructionKind::Thread(target.clone()),
            Statement::End => InstructionKind::End,
            Statement::Comment(_) | Statement::Blank => return None,
        };
        Some(Instruction {
            span: statement.span,
            kind,
        })
    }

    fn lower_template(&mut self, source: &str, span: weave_core::Span) -> Template {
        match parse_template(source, span) {
            Ok(parts) => Template {
                parts: parts
                    .into_iter()
                    .map(|part| match part.node {
                        TemplatePart::Text(text) => TemplatePartIr::Text(text),
                        TemplatePart::GrammarRef { grammar, rule } => TemplatePartIr::GrammarRef {
                            grammar: grammar.or_else(|| self.current_grammar.clone()),
                            rule,
                        },
                        TemplatePart::Expression(expression) => {
                            TemplatePartIr::Expression(lower_expression(&expression.node))
                        }
                    })
                    .collect(),
            },
            Err(mut diagnostics) => {
                self.diagnostics.append(&mut diagnostics);
                Template::default()
            }
        }
    }
}

fn lower_declaration(declaration: &Declaration) -> DeclarationIr {
    DeclarationIr {
        kind: match declaration.kind {
            VariableKind::Variable => VariableKindIr::Variable,
            VariableKind::List => VariableKindIr::List,
            VariableKind::Flag => VariableKindIr::Flag,
            VariableKind::State => VariableKindIr::State,
        },
        name: declaration.name.clone(),
        value: lower_expression(&declaration.value.node),
        allowed_states: declaration.allowed_states.clone(),
    }
}

fn lower_expression(expression: &Expr) -> Expression {
    match expression {
        Expr::Literal(value) => Expression::Literal(lower_literal(value)),
        Expr::List(values) => Expression::List(
            values
                .iter()
                .map(|value| lower_expression(&value.node))
                .collect(),
        ),
        Expr::Path(path) => Expression::Path(path.clone()),
        Expr::GrammarRef { grammar, rule } => Expression::GrammarRef {
            grammar: grammar.clone(),
            rule: rule.clone(),
        },
        Expr::Call { path, arguments } => Expression::Call {
            path: path.clone(),
            arguments: arguments
                .iter()
                .map(|argument| lower_expression(&argument.node))
                .collect(),
        },
        Expr::Unary { operator, operand } => Expression::Unary {
            operator: match operator {
                UnaryOperator::Not => UnaryOperatorIr::Not,
                UnaryOperator::Negate => UnaryOperatorIr::Negate,
            },
            operand: Box::new(lower_expression(&operand.node)),
        },
        Expr::Binary {
            left,
            operator,
            right,
        } => Expression::Binary {
            left: Box::new(lower_expression(&left.node)),
            operator: match operator {
                BinaryOperator::Add => BinaryOperatorIr::Add,
                BinaryOperator::Subtract => BinaryOperatorIr::Subtract,
                BinaryOperator::Multiply => BinaryOperatorIr::Multiply,
                BinaryOperator::Divide => BinaryOperatorIr::Divide,
                BinaryOperator::Remainder => BinaryOperatorIr::Remainder,
                BinaryOperator::Equal => BinaryOperatorIr::Equal,
                BinaryOperator::NotEqual => BinaryOperatorIr::NotEqual,
                BinaryOperator::Less => BinaryOperatorIr::Less,
                BinaryOperator::LessEqual => BinaryOperatorIr::LessEqual,
                BinaryOperator::Greater => BinaryOperatorIr::Greater,
                BinaryOperator::GreaterEqual => BinaryOperatorIr::GreaterEqual,
                BinaryOperator::In => BinaryOperatorIr::In,
                BinaryOperator::And => BinaryOperatorIr::And,
                BinaryOperator::Or => BinaryOperatorIr::Or,
            },
            right: Box::new(lower_expression(&right.node)),
        },
    }
}

fn lower_literal(literal: &Literal) -> ValueLiteral {
    match literal {
        Literal::Null => ValueLiteral::Null,
        Literal::Bool(value) => ValueLiteral::Bool(*value),
        Literal::Number(value) => ValueLiteral::Number(*value),
        Literal::String(value) => ValueLiteral::String(value.clone()),
        Literal::Symbol(value) => ValueLiteral::Symbol(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_source_without_partial_output() {
        let error = compile("=== start ===\n-> missing\n", &CompileOptions::default())
            .expect_err("unknown target should fail");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.0 == "W2021")
        );
    }

    #[test]
    fn output_is_stable_and_round_trips() {
        let source =
            "grammar words {\nvalue: [\"one\", \"two\"]\n}\n=== start ===\n#words.value#\n-> END\n";
        let first = compile(source, &CompileOptions::default()).expect("source should compile");
        let second = compile(source, &CompileOptions::default()).expect("source should compile");
        let first_ron = to_ron(&first.story).expect("RON should serialize");
        assert_eq!(
            first_ron,
            to_ron(&second.story).expect("RON should serialize")
        );
        let decoded: StoryIr = ron::from_str(&first_ron).expect("RON should deserialize");
        assert_eq!(decoded, first.story);
    }

    #[test]
    fn lowers_builtin_and_authored_patterns_into_executable_ir() {
        let source = r#"
pattern cards {
    builtin: tarot
    reversals: true
}
pattern changes {
    builtin: i_ching
}
pattern omens {
    signs: [(name: "Crow", meaning: warning, severity: 2)]
    draw: weighted_by_severity
    duplicates: true
}
VAR card = cards.draw()
VAR change = changes.draw()
VAR omen = omens.draw()
=== start ===
{card.name} {change.name} {omen.name}
"#;
        let compiled = compile(source, &CompileOptions::default()).expect("source compiles");
        assert_eq!(
            compiled.story.patterns["cards"].builtin,
            Some(BuiltinPatternIr::Tarot)
        );
        assert_eq!(
            compiled.story.patterns["changes"].draw_method,
            PatternDrawMethodIr::ThreeCoin
        );
        assert_eq!(
            compiled.story.patterns["omens"].draw_method,
            PatternDrawMethodIr::WeightedBy {
                field: "severity".to_owned()
            }
        );
        assert!(compiled.story.patterns["omens"].allow_duplicates);

        let ron = to_ron(&compiled.story).expect("patterns serialize");
        let decoded: StoryIr = ron::from_str(&ron).expect("patterns deserialize");
        assert_eq!(decoded, compiled.story);
    }
}
