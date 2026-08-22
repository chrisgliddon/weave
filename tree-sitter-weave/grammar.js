/**
 * @file Tree-sitter grammar for the Weave narrative scripting language
 * @author Weave Contributors
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  OR: 1,
  AND: 2,
  EQUALITY: 3,
  COMPARISON: 4,
  ADDITIVE: 5,
  MULTIPLICATIVE: 6,
  UNARY: 7,
  CALL: 8,
};

export default grammar({
  name: "weave",

  extras: $ => [/[ \t]/, $.comment],

  externals: $ => [$._newline, $._indent, $._dedent],

  word: $ => $.identifier,

  supertypes: $ => [$.expression],

  conflicts: $ => [[$.choice_statement]],

  rules: {
    source_file: $ => repeat(choice($._top_level_item, $._newline)),

    _top_level_item: $ => choice(
      $.module_declaration,
      $.grammar_declaration,
      $.pattern_declaration,
      $.variable_declaration,
      $.list_declaration,
      $.flag_declaration,
      $.state_declaration,
      $.knot,
    ),

    module_declaration: $ => seq(
      "module",
      field("name", $.identifier),
      "{",
      repeat(choice($._module_entry, $._newline)),
      "}",
    ),

    _module_entry: $ => choice(
      $.module_id_setting,
      $.module_version_setting,
      $.module_pack_setting,
    ),

    module_id_setting: $ => seq("id", ":", field("value", $.string)),

    module_version_setting: $ => seq("version", ":", field("value", $.string)),

    module_pack_setting: $ => seq("pack", ":", field("value", $.string)),

    grammar_declaration: $ => seq(
      "grammar",
      field("name", $.identifier),
      "{",
      repeat(choice($.grammar_rule, $._newline)),
      "}",
    ),

    grammar_rule: $ => seq(
      field("name", $.identifier),
      ":",
      field("value", choice($.string, $.grammar_string_list)),
      optional(","),
    ),

    grammar_string_list: $ => seq(
      "[",
      optional(commaSep1($.string)),
      optional(","),
      "]",
    ),

    pattern_declaration: $ => seq(
      "pattern",
      field("name", $.identifier),
      "{",
      repeat(choice($._pattern_entry, $._newline)),
      "}",
    ),

    _pattern_entry: $ => choice(
      $.builtin_setting,
      $.draw_setting,
      $.reversals_setting,
      $.duplicates_setting,
      $.pattern_collection,
      $.spread_declaration,
    ),

    builtin_setting: $ => seq("builtin", ":", field("value", $.identifier)),

    draw_setting: $ => seq("draw", ":", field("value", $.draw_method)),

    draw_method: $ => choice(
      "uniform",
      "three_coin",
      "yarrow_stalks",
      token(/weighted_by_[A-Za-z_][A-Za-z0-9_]*/),
    ),

    reversals_setting: $ => seq("reversals", ":", field("value", $.boolean)),

    duplicates_setting: $ => seq("duplicates", ":", field("value", $.boolean)),

    pattern_collection: $ => seq(
      field("name", $.identifier),
      ":",
      "[",
      repeat(choice(seq($.pattern_element, optional(",")), $._newline)),
      "]",
      optional(","),
    ),

    pattern_element: $ => seq("(", commaSep1($.pattern_field), ")"),

    pattern_field: $ => seq(
      field("name", $.identifier),
      ":",
      field("value", $._pattern_literal),
    ),

    _pattern_literal: $ => choice(
      $.string,
      $.number,
      $.boolean,
      $.null,
      $.identifier,
    ),

    spread_declaration: $ => seq(
      "spread",
      field("name", $.identifier),
      "{",
      "positions",
      ":",
      field("positions", $.symbol_list),
      "}",
    ),

    knot: $ => prec.right(seq(
      $.knot_header,
      repeat(choice($._statement, $._newline)),
      optional($._simple_statement),
    )),

    knot_header: $ => seq(keyword("==="), field("name", $.identifier), keyword("===")),

    _statement: $ => choice(
      $.choice_statement,
      $.conditional_statement,
      seq($._simple_statement, $._newline),
    ),

    _simple_statement: $ => choice(
      $.variable_declaration,
      $.list_declaration,
      $.flag_declaration,
      $.state_declaration,
      $.assignment_statement,
      $.list_mutation_statement,
      $.divert_statement,
      $.thread_statement,
      $.narrative_statement,
    ),

    choice_statement: $ => seq(
      field("kind", $.choice_marker),
      field("label", $.choice_label),
      optional(field("condition", $.choice_condition)),
      optional(field("divert", $.inline_divert)),
      optional(seq(repeat1($._newline), field("body", $.block))),
    ),

    choice_marker: _ => choice(keyword("*"), keyword("+")),

    choice_label: $ => seq(
      "[",
      repeat(choice(
        $.choice_text,
        $.escape_sequence,
        $.grammar_reference,
        $.interpolation,
      )),
      "]",
    ),

    choice_text: _ => token(prec(-1, /[^\]#{}\\\r\n]+/)),

    choice_condition: $ => seq("{", "if", $.expression, "}"),

    inline_divert: $ => seq(keyword("->"), field("target", choice($.identifier, "END"))),

    conditional_statement: $ => seq(
      "{",
      field("condition", $.expression),
      ":",
      repeat1($._newline),
      optional(field("body", $.block)),
      repeat($.conditional_branch),
      optional($.else_branch),
      "}",
    ),

    conditional_branch: $ => seq(
      keyword("-"),
      field("condition", $.expression),
      ":",
      optional(seq(repeat1($._newline), field("body", $.block))),
    ),

    else_branch: $ => seq(
      keyword("-"),
      "else",
      ":",
      optional(seq(repeat1($._newline), field("body", $.block))),
    ),

    block: $ => seq(
      $._indent,
      repeat(choice($._statement, $._newline)),
      $._dedent,
    ),

    variable_declaration: $ => declaration($, "VAR"),

    list_declaration: $ => declaration($, "LIST"),

    flag_declaration: $ => declaration($, "FLAG"),

    state_declaration: $ => seq(
      keyword("STATE"),
      field("name", $.identifier),
      "=",
      field("value", $.expression),
      field("allowed", $.symbol_list),
    ),

    assignment_statement: $ => seq(
      keyword("SET"),
      field("name", $.identifier),
      "=",
      field("value", $.expression),
    ),

    list_mutation_statement: $ => seq(
      field("operation", choice(keyword("PUSH"), keyword("REMOVE"))),
      field("name", $.identifier),
      ",",
      field("value", $.expression),
    ),

    divert_statement: $ => seq(
      keyword("->"),
      field("target", choice($.identifier, "END")),
    ),

    thread_statement: $ => seq(
      choice(keyword("<-"), keyword("THREAD")),
      field("target", $.identifier),
    ),

    narrative_statement: $ => repeat1(choice(
      $.narrative_text,
      $.escape_sequence,
      $.grammar_reference,
      $.interpolation,
    )),

    narrative_text: _ => token(/[^#{}\\\r\n \t]+/),

    interpolation: $ => seq("{", $.expression, "}"),

    grammar_reference: $ => choice(
      seq("#", field("rule", $.identifier), "#"),
      seq(
        "#",
        field("grammar", $.identifier),
        ".",
        field("rule", $.identifier),
        "#",
      ),
    ),

    expression: $ => choice(
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.path_expression,
      $.list,
      $.grammar_reference,
      $.string,
      $.number,
      $.boolean,
      $.null,
      $.parenthesized_expression,
    ),

    binary_expression: $ => choice(
      ...[
        ["or", PREC.OR],
        ["||", PREC.OR],
        ["and", PREC.AND],
        ["&&", PREC.AND],
        ["==", PREC.EQUALITY],
        ["!=", PREC.EQUALITY],
        ["<", PREC.COMPARISON],
        ["<=", PREC.COMPARISON],
        [">", PREC.COMPARISON],
        [">=", PREC.COMPARISON],
        ["in", PREC.COMPARISON],
        ["+", PREC.ADDITIVE],
        ["-", PREC.ADDITIVE],
        ["*", PREC.MULTIPLICATIVE],
        ["/", PREC.MULTIPLICATIVE],
        ["%", PREC.MULTIPLICATIVE],
      ].map(([operator, precedence]) => prec.left(precedence, seq(
        field("left", $.expression),
        field("operator", operator),
        field("right", $.expression),
      ))),
    ),

    unary_expression: $ => prec(PREC.UNARY, seq(
      field("operator", choice("!", "-", "not")),
      field("operand", $.expression),
    )),

    call_expression: $ => prec(PREC.CALL, seq(
      field("function", $.path_expression),
      field("arguments", $.argument_list),
    )),

    argument_list: $ => seq(
      "(",
      optional(commaSep1($.expression)),
      optional(","),
      ")",
    ),

    path_expression: $ => seq(
      field("root", $.identifier),
      repeat(seq(".", field("member", $.identifier))),
    ),

    parenthesized_expression: $ => seq("(", $.expression, ")"),

    list: $ => seq(
      "[",
      optional(commaSep1($.expression)),
      optional(","),
      "]",
    ),

    symbol_list: $ => seq(
      "[",
      optional(commaSep1($.identifier)),
      optional(","),
      "]",
    ),

    string: $ => seq(
      "\"",
      repeat(choice(
        $.string_content,
        $.escape_sequence,
        $.grammar_reference,
        $.interpolation,
      )),
      "\"",
    ),

    string_content: _ => token.immediate(prec(-1, /[^"#{}\\\r\n]+/)),

    escape_sequence: _ => token.immediate(seq("\\", /[^\r\n]/)),

    number: _ => token(/\d+(\.\d+)?([eE][+-]?\d+)?/),

    boolean: _ => choice("true", "false"),

    null: _ => "null",

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: _ => token(seq("//", /[^\r\n]*/)),

  },
});

function declaration($, declarationKeyword) {
  return seq(
    keyword(declarationKeyword),
    field("name", $.identifier),
    "=",
    field("value", $.expression),
  );
}

function keyword(value) {
  return token(prec(2, value));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
