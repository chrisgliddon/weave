(knot_header name: (identifier) @name) @definition.function
(module_declaration name: (identifier) @name) @definition.module
(grammar_declaration name: (identifier) @name) @definition.module
(grammar_rule name: (identifier) @name) @definition.field
(pattern_declaration name: (identifier) @name) @definition.type
(scene_declaration name: (identifier) @name) @definition.type
(stage_declaration name: (identifier) @name) @definition.type
(stage_variant name: (identifier) @name) @definition.field
(pattern_collection name: (identifier) @name) @definition.field
(spread_declaration name: (identifier) @name) @definition.method

[
  (variable_declaration name: (identifier) @name) @definition.var
  (list_declaration name: (identifier) @name) @definition.var
  (flag_declaration name: (identifier) @name) @definition.var
  (state_declaration name: (identifier) @name) @definition.var
]

(divert_statement target: (identifier) @name) @reference.call
(thread_statement target: (identifier) @name) @reference.call
(inline_divert target: (identifier) @name) @reference.call
(directive_statement argument: (identifier) @name) @reference.call
