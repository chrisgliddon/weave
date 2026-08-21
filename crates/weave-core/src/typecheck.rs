//! Static name, type, and control-flow analysis.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::{
    BinaryOperator, Declaration, Document, Expr, GrammarEntry, Item, ListOperation, Literal,
    PatternEntry, Span, Spanned, Statement, UnaryOperator, VariableKind,
};
use crate::diagnostic::{Diagnostic, Severity};
use crate::template::{TemplatePart, parse_template};

/// Static Weave value type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Type could not be narrowed statically.
    Any,
    /// Null value.
    Null,
    /// Boolean value.
    Bool,
    /// Finite number.
    Number,
    /// UTF-8 string.
    String,
    /// Semantic atom.
    Symbol,
    /// Homogeneous ordered list.
    List(Box<Type>),
    /// Structured string-keyed value.
    Object,
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => formatter.write_str("Any"),
            Self::Null => formatter.write_str("Null"),
            Self::Bool => formatter.write_str("Bool"),
            Self::Number => formatter.write_str("Number"),
            Self::String => formatter.write_str("String"),
            Self::Symbol => formatter.write_str("Symbol"),
            Self::List(element) => write!(formatter, "List<{element}>"),
            Self::Object => formatter.write_str("Object"),
        }
    }
}

/// Result of checking a source document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeCheckResult {
    /// All discovered errors and warnings.
    pub diagnostics: Vec<Diagnostic>,
    /// Story variable types after inference.
    pub variables: BTreeMap<String, Type>,
}

impl TypeCheckResult {
    /// Whether checking produced no errors.
    #[must_use]
    pub fn is_success(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

#[derive(Debug, Clone)]
struct VariableInfo {
    kind: VariableKind,
    value_type: Type,
    allowed_states: BTreeSet<String>,
    span: Span,
}

#[derive(Debug, Clone, Default)]
struct GrammarInfo {
    rules: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
struct PatternInfo {
    collections: BTreeSet<String>,
    spreads: BTreeMap<String, BTreeSet<String>>,
}

struct Checker {
    diagnostics: Vec<Diagnostic>,
    variables: BTreeMap<String, VariableInfo>,
    grammars: BTreeMap<String, GrammarInfo>,
    patterns: BTreeMap<String, PatternInfo>,
    knots: BTreeSet<String>,
}

impl Checker {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            variables: BTreeMap::new(),
            grammars: BTreeMap::new(),
            patterns: BTreeMap::new(),
            knots: BTreeSet::new(),
        }
    }

    fn check(mut self, document: &Document) -> TypeCheckResult {
        self.index_declarations(document);
        self.index_variables(document);
        self.refine_variables(document);
        self.check_declarations(document);

        TypeCheckResult {
            variables: self
                .variables
                .iter()
                .map(|(name, info)| (name.clone(), info.value_type.clone()))
                .collect(),
            diagnostics: self.diagnostics,
        }
    }

    fn index_declarations(&mut self, document: &Document) {
        let mut knot_count = 0;
        for item in &document.items {
            match &item.node {
                Item::Grammar(grammar) => {
                    let name = &grammar.node.name;
                    if self.grammars.contains_key(name) {
                        self.duplicate("W2001", "grammar", name, grammar.span);
                        continue;
                    }
                    let mut info = GrammarInfo::default();
                    for entry in &grammar.node.entries {
                        if let GrammarEntry::Rule(rule) = &entry.node {
                            if !info.rules.insert(rule.name.clone()) {
                                self.duplicate("W2002", "grammar rule", &rule.name, entry.span);
                            }
                            if rule.alternatives.is_empty() {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "W2003",
                                        format!(
                                            "grammar rule `{}.{}` has no alternatives",
                                            name, rule.name
                                        ),
                                    )
                                    .with_span(entry.span),
                                );
                            }
                        }
                    }
                    self.grammars.insert(name.clone(), info);
                }
                Item::Pattern(pattern) => {
                    let name = &pattern.node.name;
                    if self.patterns.contains_key(name) {
                        self.duplicate("W2004", "pattern", name, pattern.span);
                        continue;
                    }
                    let mut info = PatternInfo::default();
                    for entry in &pattern.node.entries {
                        match &entry.node {
                            PatternEntry::Collection(collection) => {
                                if !info.collections.insert(collection.name.clone()) {
                                    self.duplicate(
                                        "W2005",
                                        "pattern collection",
                                        &collection.name,
                                        entry.span,
                                    );
                                }
                                if collection.elements.is_empty() {
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            "W2006",
                                            format!(
                                                "pattern collection `{}.{}` has no elements",
                                                name, collection.name
                                            ),
                                        )
                                        .with_span(entry.span),
                                    );
                                }
                                for element in &collection.elements {
                                    let mut fields = BTreeSet::new();
                                    for (field, _) in &element.fields {
                                        if !fields.insert(field.clone()) {
                                            self.duplicate(
                                                "W2007",
                                                "pattern field",
                                                field,
                                                entry.span,
                                            );
                                        }
                                    }
                                }
                            }
                            PatternEntry::Spread(spread) => {
                                if info.spreads.contains_key(&spread.name) {
                                    self.duplicate(
                                        "W2008",
                                        "pattern spread",
                                        &spread.name,
                                        entry.span,
                                    );
                                }
                                let positions =
                                    spread.positions.iter().cloned().collect::<BTreeSet<_>>();
                                if positions.len() != spread.positions.len() {
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            "W2009",
                                            format!(
                                                "spread `{}.{}` has duplicate positions",
                                                name, spread.name
                                            ),
                                        )
                                        .with_span(entry.span),
                                    );
                                }
                                if positions.is_empty() {
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            "W2010",
                                            format!(
                                                "spread `{}.{}` has no positions",
                                                name, spread.name
                                            ),
                                        )
                                        .with_span(entry.span),
                                    );
                                }
                                info.spreads.insert(spread.name.clone(), positions);
                            }
                            PatternEntry::Comment(_) | PatternEntry::Blank => {}
                        }
                    }
                    self.patterns.insert(name.clone(), info);
                }
                Item::Knot(knot) => {
                    knot_count += 1;
                    if !self.knots.insert(knot.node.name.clone()) {
                        self.duplicate("W2011", "knot", &knot.node.name, knot.span);
                    }
                }
                Item::Global(_) | Item::Comment(_) | Item::Blank => {}
            }
        }
        if knot_count == 0 {
            self.diagnostics.push(
                Diagnostic::error("W2012", "a Weave document must contain at least one knot")
                    .with_help("add a knot such as `=== start ===`"),
            );
        }
    }

    fn index_variables(&mut self, document: &Document) {
        for item in &document.items {
            match &item.node {
                Item::Global(statement) => self.index_statement_variables(statement),
                Item::Knot(knot) => self.index_block_variables(&knot.node.body),
                _ => {}
            }
        }
    }

    fn index_block_variables(&mut self, block: &[Spanned<Statement>]) {
        for statement in block {
            self.index_statement_variables(statement);
        }
    }

    fn index_statement_variables(&mut self, statement: &Spanned<Statement>) {
        match &statement.node {
            Statement::Declare(declaration) => self.register_variable(declaration, statement.span),
            Statement::Choice(choice) => self.index_block_variables(&choice.body),
            Statement::Conditional(conditional) => {
                for branch in &conditional.branches {
                    self.index_block_variables(&branch.body);
                }
                if let Some(fallback) = &conditional.fallback {
                    self.index_block_variables(fallback);
                }
            }
            _ => {}
        }
    }

    fn register_variable(&mut self, declaration: &Declaration, span: Span) {
        if let Some(previous) = self.variables.get(&declaration.name) {
            if previous.kind != declaration.kind {
                self.diagnostics.push(
                    Diagnostic::error(
                        "W2013",
                        format!("`{}` is redeclared as a different kind", declaration.name),
                    )
                    .with_span(span)
                    .with_help(format!(
                        "the first declaration at {}:{} uses {:?}",
                        previous.span.line, previous.span.column, previous.kind
                    )),
                );
            }
            return;
        }
        let value_type = match declaration.kind {
            VariableKind::Variable => Type::Any,
            VariableKind::List => Type::List(Box::new(Type::Any)),
            VariableKind::Flag => Type::Bool,
            VariableKind::State => Type::Symbol,
        };
        self.variables.insert(
            declaration.name.clone(),
            VariableInfo {
                kind: declaration.kind,
                value_type,
                allowed_states: declaration.allowed_states.iter().cloned().collect(),
                span,
            },
        );
    }

    fn refine_variables(&mut self, document: &Document) {
        for item in &document.items {
            match &item.node {
                Item::Global(statement) => self.refine_statement(statement),
                Item::Knot(knot) => self.refine_block(&knot.node.body),
                _ => {}
            }
        }
    }

    fn refine_block(&mut self, block: &[Spanned<Statement>]) {
        for statement in block {
            self.refine_statement(statement);
        }
    }

    fn refine_statement(&mut self, statement: &Spanned<Statement>) {
        match &statement.node {
            Statement::Declare(declaration) => {
                let inferred = self.infer_expression(&declaration.value);
                if let Some(info) = self.variables.get_mut(&declaration.name) {
                    match declaration.kind {
                        VariableKind::Variable if info.value_type == Type::Any => {
                            info.value_type = inferred;
                        }
                        VariableKind::List if matches!(inferred, Type::List(_)) => {
                            info.value_type = inferred;
                        }
                        _ => {}
                    }
                }
            }
            Statement::Choice(choice) => self.refine_block(&choice.body),
            Statement::Conditional(conditional) => {
                for branch in &conditional.branches {
                    self.refine_block(&branch.body);
                }
                if let Some(fallback) = &conditional.fallback {
                    self.refine_block(fallback);
                }
            }
            _ => {}
        }
    }

    fn check_declarations(&mut self, document: &Document) {
        for item in &document.items {
            match &item.node {
                Item::Grammar(grammar) => {
                    for entry in &grammar.node.entries {
                        if let GrammarEntry::Rule(rule) = &entry.node {
                            for alternative in &rule.alternatives {
                                self.check_template(
                                    alternative,
                                    entry.span,
                                    Some(&grammar.node.name),
                                );
                            }
                        }
                    }
                }
                Item::Global(statement) => self.check_statement(statement),
                Item::Knot(knot) => self.check_block(&knot.node.body),
                _ => {}
            }
        }
    }

    fn check_block(&mut self, block: &[Spanned<Statement>]) {
        let mut terminal = false;
        for statement in block {
            let trivia = matches!(statement.node, Statement::Comment(_) | Statement::Blank);
            if terminal && !trivia {
                self.diagnostics.push(
                    Diagnostic::warning("W2901", "statement is unreachable after a divert or END")
                        .with_span(statement.span),
                );
            }
            self.check_statement(statement);
            if matches!(statement.node, Statement::Divert(_) | Statement::End) {
                terminal = true;
            }
        }
    }

    fn check_statement(&mut self, statement: &Spanned<Statement>) {
        match &statement.node {
            Statement::Text(text) => self.check_template(text, statement.span, None),
            Statement::Declare(declaration) => self.check_declaration(declaration, statement.span),
            Statement::Assign { name, value } => self.check_assignment(name, value, statement.span),
            Statement::MutateList {
                operation,
                name,
                value,
            } => self.check_list_mutation(*operation, name, value, statement.span),
            Statement::Choice(choice) => {
                self.check_template(&choice.text, statement.span, None);
                if let Some(condition) = &choice.condition {
                    self.require_boolean(condition, "choice condition");
                }
                self.check_block(&choice.body);
                if let Some(target) = &choice.divert {
                    self.check_target(target, statement.span, true);
                }
            }
            Statement::Conditional(conditional) => {
                for branch in &conditional.branches {
                    self.require_boolean(&branch.condition, "conditional branch");
                    self.check_block(&branch.body);
                }
                if let Some(fallback) = &conditional.fallback {
                    self.check_block(fallback);
                }
            }
            Statement::Divert(target) => self.check_target(target, statement.span, false),
            Statement::Thread(target) => self.check_target(target, statement.span, false),
            Statement::End | Statement::Comment(_) | Statement::Blank => {}
        }
    }

    fn check_declaration(&mut self, declaration: &Declaration, span: Span) {
        let inferred = self.infer_expression(&declaration.value);
        match declaration.kind {
            VariableKind::Variable => {
                if let Some(expected) = self
                    .variables
                    .get(&declaration.name)
                    .map(|info| info.value_type.clone())
                {
                    self.require_compatible(
                        &inferred,
                        &expected,
                        declaration.value.span,
                        "variable initializer",
                    );
                }
            }
            VariableKind::List => {
                if !matches!(inferred, Type::List(_) | Type::Any) {
                    self.type_error(
                        "LIST initializer",
                        "List",
                        &inferred,
                        declaration.value.span,
                    );
                }
            }
            VariableKind::Flag => {
                self.require_compatible(
                    &inferred,
                    &Type::Bool,
                    declaration.value.span,
                    "FLAG initializer",
                );
            }
            VariableKind::State => self.check_state_declaration(declaration, span, &inferred),
        }
    }

    fn check_state_declaration(&mut self, declaration: &Declaration, span: Span, inferred: &Type) {
        if declaration.allowed_states.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2014",
                    format!("STATE `{}` has no allowed values", declaration.name),
                )
                .with_span(span),
            );
        }
        let unique = declaration.allowed_states.iter().collect::<BTreeSet<_>>();
        if unique.len() != declaration.allowed_states.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2015",
                    format!("STATE `{}` repeats an allowed value", declaration.name),
                )
                .with_span(span),
            );
        }
        self.require_compatible(
            inferred,
            &Type::Symbol,
            declaration.value.span,
            "STATE initializer",
        );
        if let Some(symbol) = self.static_symbol(&declaration.value)
            && !declaration.allowed_states.contains(&symbol)
        {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2016",
                    format!(
                        "initial state `{symbol}` is not allowed for `{}`",
                        declaration.name
                    ),
                )
                .with_span(declaration.value.span),
            );
        }
    }

    fn check_assignment(&mut self, name: &str, value: &Spanned<Expr>, span: Span) {
        let Some(info) = self.variables.get(name).cloned() else {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2017",
                    format!("assignment target `{name}` is not declared"),
                )
                .with_span(span),
            );
            return;
        };
        let inferred = self.infer_expression(value);
        self.require_compatible(&inferred, &info.value_type, value.span, "assignment");
        if info.kind == VariableKind::State
            && let Some(symbol) = self.static_symbol(value)
            && !info.allowed_states.contains(&symbol)
        {
            self.diagnostics.push(
                Diagnostic::error("W2018", format!("state `{name}` does not allow `{symbol}`"))
                    .with_span(value.span),
            );
        }
    }

    fn check_list_mutation(
        &mut self,
        operation: ListOperation,
        name: &str,
        value: &Spanned<Expr>,
        span: Span,
    ) {
        let Some(info) = self.variables.get(name).cloned() else {
            self.diagnostics.push(
                Diagnostic::error("W2019", format!("list target `{name}` is not declared"))
                    .with_span(span),
            );
            return;
        };
        let Type::List(element) = info.value_type else {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2020",
                    format!(
                        "{} target `{name}` is not a list",
                        list_operation_name(operation)
                    ),
                )
                .with_span(span),
            );
            return;
        };
        let inferred = self.infer_expression(value);
        self.require_compatible(
            &inferred,
            &element,
            value.span,
            list_operation_name(operation),
        );
    }

    fn check_target(&mut self, target: &str, span: Span, allow_end: bool) {
        if (allow_end && target == "END") || self.knots.contains(target) {
            return;
        }
        self.diagnostics.push(
            Diagnostic::error("W2021", format!("unknown flow target `{target}`"))
                .with_span(span)
                .with_help("declare a knot with that name or divert to END"),
        );
    }

    fn check_template(&mut self, source: &str, span: Span, current_grammar: Option<&str>) {
        let parts = match parse_template(source, span) {
            Ok(parts) => parts,
            Err(mut diagnostics) => {
                self.diagnostics.append(&mut diagnostics);
                return;
            }
        };
        for part in parts {
            match part.node {
                TemplatePart::Text(_) => {}
                TemplatePart::GrammarRef { grammar, rule } => {
                    self.check_grammar_reference(
                        grammar.as_deref(),
                        &rule,
                        current_grammar,
                        part.span,
                    );
                }
                TemplatePart::Expression(expression) => {
                    self.infer_expression(&expression);
                }
            }
        }
    }

    fn check_grammar_reference(
        &mut self,
        grammar: Option<&str>,
        rule: &str,
        current_grammar: Option<&str>,
        span: Span,
    ) {
        let grammar = grammar.or(current_grammar);
        let Some(grammar) = grammar else {
            self.diagnostics.push(
                Diagnostic::error(
                    "W2022",
                    format!("grammar reference `#{rule}#` must be qualified here"),
                )
                .with_span(span)
                .with_help(format!("use `#grammar.{rule}#` outside a grammar block")),
            );
            return;
        };
        let Some(info) = self.grammars.get(grammar) else {
            self.diagnostics.push(
                Diagnostic::error("W2023", format!("unknown grammar `{grammar}`")).with_span(span),
            );
            return;
        };
        if !info.rules.contains(rule) {
            self.diagnostics.push(
                Diagnostic::error("W2024", format!("grammar `{grammar}` has no rule `{rule}`"))
                    .with_span(span),
            );
        }
    }

    fn require_boolean(&mut self, expression: &Spanned<Expr>, context: &str) {
        let inferred = self.infer_expression(expression);
        self.require_compatible(&inferred, &Type::Bool, expression.span, context);
    }

    fn infer_expression(&mut self, expression: &Spanned<Expr>) -> Type {
        match &expression.node {
            Expr::Literal(literal) => literal_type(literal),
            Expr::List(values) => self.infer_list(values, expression.span),
            Expr::Path(path) => self.infer_path(path, expression.span),
            Expr::GrammarRef { grammar, rule } => {
                self.check_grammar_reference(grammar.as_deref(), rule, None, expression.span);
                Type::String
            }
            Expr::Call { path, arguments } => self.infer_call(path, arguments, expression.span),
            Expr::Unary { operator, operand } => {
                let operand_type = self.infer_expression(operand);
                match operator {
                    UnaryOperator::Not => {
                        self.require_compatible(
                            &operand_type,
                            &Type::Bool,
                            operand.span,
                            "not operand",
                        );
                        Type::Bool
                    }
                    UnaryOperator::Negate => {
                        self.require_compatible(
                            &operand_type,
                            &Type::Number,
                            operand.span,
                            "negation operand",
                        );
                        Type::Number
                    }
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => self.infer_binary(left, *operator, right, expression.span),
        }
    }

    fn infer_list(&mut self, values: &[Spanned<Expr>], span: Span) -> Type {
        let mut element_type = Type::Any;
        for value in values {
            let inferred = self.infer_expression(value);
            if element_type == Type::Any {
                element_type = inferred;
            } else if !types_compatible(&element_type, &inferred) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "W2025",
                        format!(
                            "list mixes incompatible element types `{element_type}` and `{inferred}`"
                        ),
                    )
                    .with_span(value.span),
                );
                element_type = Type::Any;
            }
        }
        let _ = span;
        Type::List(Box::new(element_type))
    }

    fn infer_path(&mut self, path: &[String], span: Span) -> Type {
        let Some(root) = path.first() else {
            self.diagnostics
                .push(Diagnostic::error("W2026", "empty value path").with_span(span));
            return Type::Any;
        };
        if path.len() == 1 {
            return self
                .variables
                .get(root)
                .map_or(Type::Symbol, |info| info.value_type.clone());
        }
        if let Some(info) = self.variables.get(root) {
            if matches!(info.value_type, Type::Object | Type::Any) {
                return Type::Any;
            }
            self.diagnostics.push(
                Diagnostic::error(
                    "W2027",
                    format!("cannot access a field on `{}`", info.value_type),
                )
                .with_span(span),
            );
            return Type::Any;
        }
        if self.patterns.contains_key(root) {
            return Type::Any;
        }
        self.diagnostics.push(
            Diagnostic::error("W2028", format!("unknown path root `{root}`")).with_span(span),
        );
        Type::Any
    }

    fn infer_call(&mut self, path: &[String], arguments: &[Spanned<Expr>], span: Span) -> Type {
        for argument in arguments {
            self.infer_expression(argument);
        }
        let valid = match path {
            [pattern, spread_keyword, spread, draw]
                if spread_keyword == "spread" && draw == "draw" =>
            {
                self.patterns
                    .get(pattern)
                    .is_some_and(|info| info.spreads.contains_key(spread))
            }
            _ => false,
        };
        if !valid {
            self.diagnostics.push(
                Diagnostic::error("W2029", format!("unsupported call `{}`", path.join(".")))
                    .with_span(span)
                    .with_help("Phase 1 only reserves `pattern.spread.name.draw()`"),
            );
        } else if !arguments.is_empty() {
            self.diagnostics.push(
                Diagnostic::error("W2030", "pattern draw does not accept arguments")
                    .with_span(span),
            );
        }
        Type::Any
    }

    fn infer_binary(
        &mut self,
        left: &Spanned<Expr>,
        operator: BinaryOperator,
        right: &Spanned<Expr>,
        span: Span,
    ) -> Type {
        let left_type = self.infer_expression(left);
        if operator == BinaryOperator::And || operator == BinaryOperator::Or {
            self.require_compatible(&left_type, &Type::Bool, left.span, "Boolean operand");
            let right_type = self.infer_expression(right);
            self.require_compatible(&right_type, &Type::Bool, right.span, "Boolean operand");
            return Type::Bool;
        }
        let right_type = self.infer_expression(right);
        match operator {
            BinaryOperator::Add => {
                if types_compatible(&left_type, &Type::Number)
                    && types_compatible(&right_type, &Type::Number)
                {
                    Type::Number
                } else if types_compatible(&left_type, &Type::String)
                    && types_compatible(&right_type, &Type::String)
                {
                    Type::String
                } else if left_type == Type::Any || right_type == Type::Any {
                    Type::Any
                } else {
                    self.binary_type_error(operator, &left_type, &right_type, span);
                    Type::Any
                }
            }
            BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => {
                self.require_compatible(&left_type, &Type::Number, left.span, "arithmetic operand");
                self.require_compatible(
                    &right_type,
                    &Type::Number,
                    right.span,
                    "arithmetic operand",
                );
                Type::Number
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => Type::Bool,
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                let ordered = matches!(
                    left_type,
                    Type::Number | Type::String | Type::Symbol | Type::Any
                ) && types_compatible(&left_type, &right_type);
                if !ordered {
                    self.binary_type_error(operator, &left_type, &right_type, span);
                }
                Type::Bool
            }
            BinaryOperator::In => {
                let valid = match &right_type {
                    Type::List(element) => types_compatible(&left_type, element),
                    Type::String => types_compatible(&left_type, &Type::String),
                    Type::Any => true,
                    _ => false,
                };
                if !valid {
                    self.binary_type_error(operator, &left_type, &right_type, span);
                }
                Type::Bool
            }
            BinaryOperator::And | BinaryOperator::Or => Type::Bool,
        }
    }

    fn static_symbol(&self, expression: &Spanned<Expr>) -> Option<String> {
        match &expression.node {
            Expr::Literal(Literal::Symbol(symbol)) => Some(symbol.clone()),
            Expr::Path(path) if path.len() == 1 && !self.variables.contains_key(&path[0]) => {
                Some(path[0].clone())
            }
            _ => None,
        }
    }

    fn require_compatible(&mut self, actual: &Type, expected: &Type, span: Span, context: &str) {
        if !types_compatible(actual, expected) {
            self.type_error(context, &expected.to_string(), actual, span);
        }
    }

    fn type_error(&mut self, context: &str, expected: &str, actual: &Type, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                "W2031",
                format!("{context} requires `{expected}`, found `{actual}`"),
            )
            .with_span(span),
        );
    }

    fn binary_type_error(
        &mut self,
        operator: BinaryOperator,
        left: &Type,
        right: &Type,
        span: Span,
    ) {
        self.diagnostics.push(
            Diagnostic::error(
                "W2032",
                format!(
                    "operator `{}` does not support `{left}` and `{right}`",
                    operator_name(operator)
                ),
            )
            .with_span(span),
        );
    }

    fn duplicate(&mut self, code: &'static str, kind: &str, name: &str, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, format!("duplicate {kind} `{name}`")).with_span(span));
    }
}

/// Check declarations, references, expressions, and control flow.
#[must_use]
pub fn type_check(document: &Document) -> TypeCheckResult {
    Checker::new().check(document)
}

fn literal_type(literal: &Literal) -> Type {
    match literal {
        Literal::Null => Type::Null,
        Literal::Bool(_) => Type::Bool,
        Literal::Number(_) => Type::Number,
        Literal::String(_) => Type::String,
        Literal::Symbol(_) => Type::Symbol,
    }
}

fn types_compatible(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (Type::Any, _) | (_, Type::Any) => true,
        (Type::List(left), Type::List(right)) => types_compatible(left, right),
        _ => left == right,
    }
}

fn list_operation_name(operation: ListOperation) -> &'static str {
    match operation {
        ListOperation::Push => "PUSH",
        ListOperation::Remove => "REMOVE",
    }
}

fn operator_name(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::In => "in",
        BinaryOperator::And => "and",
        BinaryOperator::Or => "or",
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_document;

    use super::*;

    #[test]
    fn accepts_a_typed_branching_story() {
        let source = r#"grammar greeting {
    line: ["Hello", "Welcome"]
}

VAR visits = 0
LIST inventory = ["map"]
FLAG paid = false
STATE quest = dormant [dormant, active, complete]

=== start ===
SET visits = visits + 1
#greeting.line#, visitor.
* [Continue] {if visits > 0} -> ending

=== ending ===
SET quest = active
-> END
"#;
        let document = parse_document(source).unwrap_or_default();
        let result = type_check(&document);
        assert!(result.is_success(), "{:?}", result.diagnostics);
        assert_eq!(result.variables.get("visits"), Some(&Type::Number));
    }

    #[test]
    fn reports_unknown_targets_and_wrong_flag_types() {
        let source = r#"FLAG paid = "yes"

=== start ===
-> missing
"#;
        let document = parse_document(source).unwrap_or_default();
        let result = type_check(&document);
        assert!(!result.is_success());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.0 == "W2021")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.0 == "W2031")
        );
    }
}
