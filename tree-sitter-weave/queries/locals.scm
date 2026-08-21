(source_file) @local.scope
(knot) @local.scope

[
  (variable_declaration name: (identifier) @local.definition)
  (list_declaration name: (identifier) @local.definition)
  (flag_declaration name: (identifier) @local.definition)
  (state_declaration name: (identifier) @local.definition)
]

(assignment_statement name: (identifier) @local.reference)
(list_mutation_statement name: (identifier) @local.reference)
(path_expression root: (identifier) @local.reference)
