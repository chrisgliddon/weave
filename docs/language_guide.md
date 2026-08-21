# Weave Language Guide

This is the normative Phase 2 language specification for Weave source files. Examples in the README are informative; when wording differs, this document defines parser, checker, formatter, compiler, and runtime behavior.

## 1. Source files

- File extension: `.weave`.
- Encoding: UTF-8 without a required byte-order mark.
- Newlines: LF and CRLF are accepted. The formatter writes LF.
- Identifiers and keywords are case-sensitive.
- Source order is significant for narrative content and choice presentation. Map serialization is deterministic and sorted by key.
- Grammar, pattern, and global declarations must appear before the first knot. Once a knot begins, only knots may follow.
- A file must contain at least one knot. The first knot is the default entry point.

Line comments begin with `//` outside string literals and continue to the end of the line. Comments and blank lines do not execute. The formatter preserves comments and semantic source order.

## 2. Lexical rules

An identifier begins with an ASCII letter or `_` and continues with ASCII letters, digits, or `_`:

```text
[A-Za-z_][A-Za-z0-9_]*
```

ASCII-only identifiers keep serialized names and editor integrations portable. Narrative text and string contents may contain arbitrary Unicode.

Reserved uppercase statement keywords are:

```text
VAR LIST FLAG STATE SET PUSH REMOVE THREAD END
```

Reserved lowercase expression keywords are:

```text
true false null and or not else
```

Strings use double quotes. The escapes `\\`, `\"`, `\n`, `\r`, and `\t` are supported. An unknown escape is an error. Numbers are finite signed decimal `f64` values; `NaN` and infinities are not source literals.

## 3. File structure

A document contains zero or more declarations followed by one or more knots:

```weave
grammar names {
    first: ["Aldric", "Mira"]
    full: "#first# the Brave"
}

VAR visits = 0
LIST inventory = []
FLAG met_teller = false
STATE quest = dormant [dormant, active, complete]

=== arrival ===
Welcome, #names.full#.
-> END
```

Global declarations execute when a story is created. Declarations inside knots execute when reached. All variables are story-scoped; knots do not create lexical variable scopes.

## 4. Grammar declarations

A grammar contains named rules. A rule is one string or a non-empty list of strings:

```weave
grammar weather {
    mood: ["hushed", "electric", "sombre"]
    scene: "The room was #mood#."
}
```

Each expansion selects one alternative uniformly using the story random-number generator. A seed supplied to the runtime makes the sequence reproducible.

Inside a rule, `#rule#` refers to another rule in the same grammar. `#grammar.rule#` is always qualified and may be used from grammar rules or narrative text. Narrative text may not use an unqualified grammar reference because no implicit grammar is active there.

Grammar expansion is recursive. Missing references are static errors. A runtime expansion depth greater than 64 is an error, which prevents cycles such as `a: "#a#"` from overflowing the stack.

## 5. Pattern declarations

Pattern systems produce structured semantic objects. A declaration either selects one built-in data set or authors exactly one collection.

Built-in declarations use `builtin` and may customize the default algorithm, reversal policy, duplicate policy, or spreads:

```weave
pattern tarot {
    builtin: tarot
    draw: uniform
    reversals: true
    duplicates: false
    spread three_card { positions: [past, present, future] }
}
```

The built-in identifiers and their default methods are:

| Identifier | Contents | Default method | Built-in spread |
|---|---|---|---|
| `tarot` | 78 cards | `uniform` | `three_card`: `past`, `present`, `future` |
| `i_ching` | 64 King Wen hexagrams | `three_coin` | none |
| `elder_futhark` | 24 runes | `uniform` | `three_rune`: `past`, `present`, `future` |

I-Ching also accepts `draw: yarrow_stalks`. Tarot accepts `draw: weighted_by_weight`, where major arcana have twice the built-in weight of minor arcana. Unsupported method/built-in combinations are compile errors.

Authored systems contain records with unique `name` values and a `meaning` field:

```weave
pattern weather_omens {
    omens: [
        (name: "Storm Crow", meaning: ill_tidings, severity: 3),
        (name: "Sun Dog", meaning: good_fortune, severity: 1),
        (name: "Frost Wolf", meaning: harsh_winter, severity: 4),
    ]
    draw: weighted_by_severity
    spread day_omen { positions: [dawn, noon, dusk] }
}
```

Record values may be strings, finite numbers, booleans, `null`, or symbols. `uniform` selects every record with equal probability. `weighted_by_<field>` requires that field to be a positive finite number on every record. A spread has a unique name and one or more unique positions. Draws do not repeat an element within a spread unless `duplicates: true` is set.

`reversals: true` gives every reversible element an equal upright/reversed chance. Tarot defines reversal semantics for every card. Elder Futhark only reverses runes that explicitly carry a reversed meaning. An authored record becomes reversible by defining `reversed_meaning`; on a reversed draw, that value replaces `meaning`. Enabling reversals without any reversal semantics is a compile error.

There are two call forms:

```weave
VAR rune = runes.draw()
VAR omen = weather_omens.spread.day_omen.draw()
```

A single draw returns the element fields directly, plus `id` and `reversed`. A spread returns an object keyed by position; each positioned element also has `position`. For example:

```weave
{omen.dawn.meaning == ill_tidings:
    A crow circles the village.
- else:
    The dawn is quiet.
}
```

Tarot elements expose `name`, `arcana`, `suit`, `rank`, `meaning`, `element`, and `reversed_meaning`. I-Ching results expose number, name, meaning, trigrams, six line values, changing lines, and a structured `transformed` hexagram. Elder Futhark results expose name, glyph, transliteration, and meaning.

Pattern results have static type `Any` because authored fields are open-ended. At runtime they are deterministic objects driven by the same seeded entropy stream as grammar expansion. Immutable definitions live in compiled story data; draw counts and last-draw identities live in separately serialized story state.

## 6. Values and types

Weave has these runtime types:

| Type | Examples | Notes |
|---|---|---|
| `Null` | `null` | Absence of a value |
| `Bool` | `true`, `false` | Required by conditions and flags |
| `Number` | `0`, `-2`, `3.5` | Finite `f64` |
| `String` | `"hello"` | UTF-8 text |
| `Symbol` | `active`, `new_beginnings` | Stable semantic atom |
| `List<T>` | `["key", "map"]` | Ordered, homogeneous when statically knowable |
| `Object` | Pattern draw results | String-keyed structured data |
| `Any` | Pattern draw result | Statically unknown, dynamically checked |

A bare one-segment identifier resolves to a declared variable when one exists; otherwise it is a symbol literal. A dotted value path must begin with a declared variable. Pattern names are only valid in the recognized draw calls, and their result must be stored before fields are read. This rule preserves concise comparisons such as `quest == active` while still diagnosing misspelled object paths.

## 7. Variables, lists, flags, and state machines

Declarations use explicit kinds:

```weave
VAR score = 0
LIST inventory = ["map"]
FLAG gate_open = false
STATE quest = dormant [dormant, active, complete]
```

- `VAR` accepts any expression. Reaching it assigns the evaluated value, including when revisiting a knot.
- `LIST` requires a list value.
- `FLAG` requires a Boolean value.
- `STATE` requires an initial symbol followed by a non-empty, unique list of allowed symbols containing the initial value.

Assignment and list mutation are statements:

```weave
SET score = score + 1
SET gate_open = true
SET quest = active
PUSH inventory, "silver key"
REMOVE inventory, "map"
```

`SET` requires an existing declaration and a compatible value. Assigning a state machine to a symbol outside its allowed set is a runtime error even when the source value has dynamic type. `PUSH` appends. `REMOVE` deletes the first equal item and is a no-op when absent.

Redeclaring a name with an incompatible kind or type is a static error. A declaration in one knot is visible to all knots for static analysis and at runtime after it has executed. Reading a declaration before execution is a structured runtime error unless it was global.

## 8. Knots and flow

A knot header is three equals signs, an identifier, and three equals signs:

```weave
=== arrival ===
The story starts here.
```

Knot names are unique. Execution starts at the first knot unless the host jumps elsewhere.

An unconditional divert replaces the current flow stack:

```weave
-> next_knot
```

`-> END` ends the story. A divert target must be an existing knot or `END`. Statements after an unconditional divert in the same block are unreachable and produce a warning.

A thread calls another knot and resumes after it finishes:

```weave
<- shared_warning
The caller resumes here.
```

When a threaded knot reaches the end of its content without diverting, its frame returns to the caller. A divert inside the thread still replaces the entire flow stack. Direct or indirect thread recursion deeper than 256 frames is a runtime error.

## 9. Narrative text and interpolation

Any non-empty knot line that is not a structural statement is narrative text and produces one line event. Quote characters in narrative lines are ordinary visible characters.

Text supports grammar expansion and expression interpolation:

```weave
#weather.scene#
You have {score} coins and the quest is {quest}.
```

Expansion order is:

1. Grammar references are expanded recursively from left to right.
2. Expression interpolations are evaluated from left to right.
3. Values are converted to display text. Lists use `[a, b]`; symbols display without punctuation; objects have deterministic key order.

To emit literal `{`, `}`, or `#`, escape it with a backslash. Unbalanced or empty interpolation markers are compile errors.

## 10. Choices

`*` introduces a once-only choice. `+` introduces a sticky choice that is offered on every visit:

```weave
* [Take the lantern] -> cellar
+ [Wait a little longer]
    Time passes.
    -> arrival
```

An optional condition follows the label:

```weave
* [Unlock the door] {if "silver key" in inventory} -> vault
```

Choice bodies are indented relative to the choice. Four spaces are canonical; tabs are accepted as four spaces and rewritten by the formatter. A choice may have a body, an inline divert, or both. When both exist, the body executes before the divert.

Adjacent choices at the same indentation form one choice set and are presented in source order. Conditions are evaluated when the set is reached. A once-only choice is recorded by a stable compiler-generated identifier when selected, then omitted on later visits. If no choice remains eligible, execution continues after the choice set.

Nested choices use additional indentation:

```weave
* [Ask the price]
    "Five silver."
    * [Pay] -> reading
    * [Leave] -> leaving
```

## 11. Conditions

Conditional narrative uses braces and ordered branches:

```weave
{score > 10:
    The purse is heavy.
- score > 0:
    A few coins remain.
- else:
    The purse is empty.
}
```

The first condition that evaluates to `true` executes. The `else` branch is optional and must be last. Branch bodies use the same statements and indentation rules as knot bodies. Conditions must have static type `Bool` or `Any`; a non-Boolean dynamic result is a runtime error.

## 12. Expressions

Operators, from highest to lowest precedence, are:

1. Parentheses, lists, paths, calls, grammar references.
2. Unary `not`, `!`, and numeric `-`.
3. `*`, `/`, `%`.
4. `+`, `-`.
5. `<`, `<=`, `>`, `>=`, and `in`.
6. `==`, `!=`.
7. `and`, `&&`.
8. `or`, `||`.

Binary operators evaluate left to right within one precedence level. `and` and `or` short-circuit.

- Arithmetic operators require numbers, except `String + String`, which concatenates.
- Ordering compares numbers, strings, or symbols of the same type.
- Equality compares any two values; values of different concrete types are unequal.
- `value in list` tests equality against list members. `string in string` tests substring membership.
- Division or remainder by zero is a runtime error.
- A function call is only valid for a declared `pattern.draw()` or `pattern.spread.name.draw()` path. Pattern draws accept no arguments.

## 13. Static analysis

The checker performs at least these validations before IR is produced:

- Duplicate grammar, rule, pattern, spread, knot, or variable names.
- Missing grammar rules and missing divert/thread targets.
- Declaration initializer and assignment type compatibility.
- Boolean conditions.
- Valid state-machine definitions and statically known transitions.
- Valid built-in names, method combinations, pattern collection records, weights, reversal configuration, and spread positions/capacity.
- Unsupported call paths.
- Unreachable statements after `-> target` or `-> END` in one block.

All diagnostics have a stable code, severity, message, and source span. Errors prevent output. Warnings do not. Tools must not panic on malformed UTF-8 input bytes, malformed syntax, unknown IR versions, or invalid runtime data.

## 14. Serialized IR

The compiler lowers checked source AST into a distinct runtime IR. IR version 2 includes:

- Sorted grammar and executable pattern maps, including built-in identity and draw configuration.
- Source-ordered knot instructions and choices.
- Parsed template segments and expressions.
- Stable once-choice identifiers.
- Source spans for actionable runtime errors.

RON is the normative output. JSON uses the same model for interoperability, but schema publication and compatibility guarantees beyond the shared version field are a Phase 4 deliverable.

The runtime rejects an unsupported `version` before executing any instruction.

## 15. Complete Phase 2 example

```weave
grammar names {
    first: ["Aldric", "Mira", "Theron", "Lyssia"]
    last: ["the Bold", "Whisperwind", "Ironhand"]
    full: "#first# #last#"
}

grammar atmosphere {
    mood: ["sombre", "electric", "anticipatory", "hushed"]
    scene: "The tavern was #mood#."
}

pattern cards {
    builtin: tarot
    reversals: true
}

VAR visits = 0
LIST inventory = []
FLAG paid = false
STATE reading = unseen [unseen, offered, complete]
VAR cards_on_table = cards.spread.three_card.draw()

=== arrival ===
SET visits = visits + 1
#atmosphere.scene#
A fortune teller waves you over. "Welcome, #names.full#."

* [Ask for a reading] {if reading != complete}
    SET reading = offered
    -> offer
+ [Leave] -> END

=== offer ===
"Five silver," she says.
* [Pay]
    SET paid = true
    SET reading = complete
    -> reading
* [Decline] -> END

=== reading ===
{visits > 1:
    "You came back," she says.
- else:
    She turns over {cards_on_table.present.name}.
}
Its meaning is {cards_on_table.present.meaning}.
-> END
```

## 16. Invalid examples

Unknown divert target:

```weave
=== start ===
-> nowhere
```

Invalid flag initializer:

```weave
FLAG enabled = "yes"

=== start ===
-> END
```

Invalid state transition:

```weave
STATE quest = dormant [dormant, active, complete]

=== start ===
SET quest = missing
```

Unqualified grammar reference in narrative text:

```weave
grammar names { first: "Mira" }

=== start ===
Hello, #first#.
```

Each example must produce a source-spanned error and no compiled story.
