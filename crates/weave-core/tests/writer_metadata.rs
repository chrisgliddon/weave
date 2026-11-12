//! Tags, line ids, channels, inline markup, and string tables.

use weave_core::{Diagnostic, parse_document, type_check};

/// Every diagnostic code from parsing then checking `source`.
fn codes(source: &str) -> Vec<&'static str> {
    let diagnostics: Vec<Diagnostic> = match parse_document(source) {
        Ok(document) => type_check(&document).diagnostics,
        Err(diagnostics) => diagnostics,
    };
    diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code.0)
        .collect()
}

fn assert_code(source: &str, code: &str) {
    let found = codes(source);
    assert!(
        found.contains(&code),
        "expected {code} for {source:?}, got {found:?}"
    );
}

fn assert_clean(source: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics for {source:?}, got {found:?}"
    );
}

fn story(body: &str) -> String {
    format!("=== start ===\n{body}\n-> END\n")
}

#[test]
fn accepts_the_complete_writer_example() {
    assert_clean(include_str!(
        "../../../examples/writer-metadata/lighthouse.weave"
    ));
}

#[test]
fn accepts_literal_angle_brackets_and_carets() {
    assert_clean(&story(
        "If 3 < 4 then <3 is a heart. x^2 and \\^tag and \\<b\\> stay.",
    ));
    assert_clean(&story("Hello ^mood:warm ^loud there."));
    assert_clean(&story("Wait. <pause 0.25> Now <em>go <b>fast</b></em>."));
}

#[test]
fn reports_each_tag_diagnostic() {
    assert_code(&story("Hi ^9bad"), "W1050");
    assert_code(&story("Hi ^"), "W1050");
    assert_code(&story("Hi ^mood:a ^mood:b"), "W1051");
    assert_code(&story("Hi ^id:a ^id:b"), "W1051");
    assert_code(&story("Hi ^id:9x"), "W1052");
    assert_code(&story("Hi ^id:a..b"), "W1052");
    assert_code(&story("* [Go] ^id:x\n    Gone."), "W1053");
    assert_code("STRINGS ui\n    a = \"A\" ^id:x\n", "W1053");
}

#[test]
fn reports_each_markup_diagnostic() {
    assert_code(&story("Hi <b"), "W1060");
    assert_code(&story("Hi <b x>there</b>"), "W1061");
    assert_code(&story("Hi <pause 0>"), "W1062");
    assert_code(&story("Hi <pause 61>"), "W1062");
    assert_code(&story("Hi <pause -1>"), "W1062");
    assert_code(&story("Hi <pause 1e3>"), "W1062");
    assert_code(&story("Hi <b>there</i>"), "W1063");
    assert_code(&story("Hi there</b>"), "W1063");
    assert_code(&story("Hi <b>there"), "W1064");
}

#[test]
fn markup_is_literal_in_choice_labels_and_interpolated_values() {
    assert_clean(&story("* [Pick <b>this</b>]\n    Done."));
    // An interpolated value is never parsed for markup, so this checks clean.
    assert_clean("VAR name = \"<b>x</b>\"\n=== start ===\nHi {name}.\n-> END\n");
}

#[test]
fn reports_each_channel_diagnostic() {
    assert_clean(
        "channel radio { animated: false, cps: 30, tint: \"green\" }\n=== start ===\n@to radio\nHi.\n@to\nBack.\n-> END\n",
    );
    assert_code(
        "channel a {}\nchannel a {}\n=== start ===\nHi.\n-> END\n",
        "W2300",
    );
    assert_code(&story("@to nowhere\nHi."), "W2301");
    assert_code(
        "channel a { animated: 3 }\n=== start ===\nHi.\n-> END\n",
        "W2302",
    );
    assert_code(
        "channel a { cps: 0 }\n=== start ===\nHi.\n-> END\n",
        "W2302",
    );
    assert_code(
        "channel a { cps: true }\n=== start ===\nHi.\n-> END\n",
        "W2302",
    );
    assert_code(
        "channel a { cps: 3, cps: 4 }\n=== start ===\nHi.\n-> END\n",
        "W2303",
    );
}

#[test]
fn reports_each_string_table_diagnostic() {
    assert_clean(
        "STRINGS ui\n    title = \"<b>Go</b> {raw} #raw#\" ^screen:title\n    menu.1 = \"One\"\n=== start ===\nHi.\n-> END\n",
    );
    assert_code(
        "STRINGS ui\n    a = \"A\"\nSTRINGS ui\n    b = \"B\"\n",
        "W2310",
    );
    assert_code("STRINGS ui\n    a = \"A\"\n    a = \"B\"\n", "W2311");
    assert_code("STRINGS ui\n    a = \"A\" trailing\n", "W1070");
    assert_code("STRINGS ui\n    a = \"<b>A\"\n", "W1064");
}

#[test]
fn reports_duplicate_line_ids_across_knots() {
    assert_code(
        "=== a ===\nOne. ^id:same\n-> b\n=== b ===\nTwo. ^id:same\n-> END\n",
        "W2320",
    );
    // An explicit id equal to another line's default id also collides.
    assert_code("=== a ===\nOne.\nTwo. ^id:a.1\n-> END\n", "W2320");
}

#[test]
fn string_values_accept_markup_escapes() {
    assert_clean("STRINGS ui\n    a = \"a \\< b \\> c \\^d\"\n=== start ===\nHi.\n-> END\n");
}

#[test]
fn a_lone_backslash_does_not_hide_the_next_tag() {
    let document = parse_document(&story("Ends in \\ ^id:start.x")).expect("parses");
    assert!(type_check(&document).diagnostics.is_empty());
    let ids = weave_core::markup::collect_line_ids(
        "start",
        &document.knots().next().expect("knot").node.body,
    );
    assert!(ids[0].explicit, "{ids:?}");
    assert_eq!(ids[0].id, "start.x");
}
