//! This crate provides Weave language support for the [tree-sitter] parsing library.
//!
//! Typically, you will use the [`LANGUAGE`] constant to add this language to a
//! tree-sitter [`Parser`], and then use the parser to parse some code:
//!
//! ```
//! let code = "=== start ===\nHello, {name}.\n-> END\n";
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_weave::LANGUAGE;
//! parser
//!     .set_language(&language.into())
//!     .expect("Error loading Weave parser");
//! let tree = parser.parse(code, None).unwrap();
//! assert!(!tree.root_node().has_error());
//! ```
//!
//! [`Parser`]: https://docs.rs/tree-sitter/0.26.12/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_weave() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_weave) };

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(with_highlights_query)]
/// The syntax highlighting query for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(with_injections_query)]
/// The language injection query for this grammar.
pub const INJECTIONS_QUERY: &str = include_str!("../../queries/injections.scm");

#[cfg(with_locals_query)]
/// The local variable query for this grammar.
pub const LOCALS_QUERY: &str = include_str!("../../queries/locals.scm");

#[cfg(with_tags_query)]
/// The symbol tagging query for this grammar.
pub const TAGS_QUERY: &str = include_str!("../../queries/tags.scm");

#[cfg(test)]
mod tests {
    const REPRESENTATIVE_SOURCE: &str = r#"
grammar greetings {
    opening: ["Hello", "Welcome"]
}

pattern cards {
    builtin: tarot
    reversals: true
    spread reading { positions: [past, future] }
}

VAR visits = 0
=== start ===
#greetings.opening#, traveler.
* [Draw a card] {if visits >= 0} -> ending

=== ending ===
-> END
"#;

    #[test]
    fn parses_representative_source_without_errors() {
        let mut parser = tree_sitter::Parser::new();
        let language = super::LANGUAGE.into();
        parser
            .set_language(&language)
            .expect("Error loading Weave parser");

        let tree = parser
            .parse(REPRESENTATIVE_SOURCE, None)
            .expect("parser should return a syntax tree");

        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn editor_queries_compile() {
        let language = super::LANGUAGE.into();

        for (name, source) in [
            ("highlights", super::HIGHLIGHTS_QUERY),
            ("locals", super::LOCALS_QUERY),
            ("tags", super::TAGS_QUERY),
        ] {
            tree_sitter::Query::new(&language, source)
                .unwrap_or_else(|error| panic!("{name} query should compile: {error}"));
        }
    }
}
