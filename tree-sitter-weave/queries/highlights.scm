(comment) @comment

(string) @string
(choice_label) @string
(escape_sequence) @string.escape
(number) @number
(boolean) @boolean
(null) @constant.builtin

[
  "module"
  "id"
  "version"
  "pack"
  "grammar"
  "pattern"
  "spread"
  "builtin"
  "draw"
  "reversals"
  "duplicates"
  "positions"
  "VAR"
  "LIST"
  "FLAG"
  "STATE"
  "SET"
  "PUSH"
  "REMOVE"
  "THREAD"
  "if"
  "else"
] @keyword

[
  "uniform"
  "three_coin"
  "yarrow_stalks"
] @constant.builtin

(draw_method) @constant.builtin
"END" @constant.builtin

(knot_header name: (identifier) @function)
(module_declaration name: (identifier) @module)
(grammar_declaration name: (identifier) @module)
(grammar_rule name: (identifier) @property)
(pattern_declaration name: (identifier) @type)
(pattern_collection name: (identifier) @variable.member)
(spread_declaration name: (identifier) @function.method)
(pattern_field name: (identifier) @property)

[
  (variable_declaration name: (identifier) @variable)
  (list_declaration name: (identifier) @variable)
  (flag_declaration name: (identifier) @variable)
  (state_declaration name: (identifier) @variable)
]

(assignment_statement name: (identifier) @variable)
(list_mutation_statement name: (identifier) @variable)
(path_expression root: (identifier) @variable)
(path_expression member: (identifier) @property)

(grammar_reference grammar: (identifier) @module)
(grammar_reference rule: (identifier) @property)
(divert_statement target: (identifier) @function)
(thread_statement target: (identifier) @function)
(inline_divert target: (identifier) @function)

(choice_marker) @operator

[
  "->"
  "<-"
  "="
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "!"
  "and"
  "or"
  "not"
  "in"
  "&&"
  "||"
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ":"
  "."
  "#"
  "==="
] @punctuation.delimiter
