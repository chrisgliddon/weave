use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_core::ir::{DomainValueIr, StoryIr};
use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};

const SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari-vale.weave");
const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/weave-character/module.weave-module.json");
const PACK: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari_vale.weave-domain.json");
const CHECKED_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari-vale.story.ron");
const CHECKED_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/ari-vale.story.json");

fn catalog() -> DomainCatalog {
    DomainCatalog::from_artifacts(
        [ModuleManifest::from_json(MANIFEST).expect("Character manifest")],
        [DomainPack::from_json(PACK).expect("Character pack")],
    )
    .expect("Character catalog")
}

fn options() -> CompileOptions {
    CompileOptions {
        source_name: Some("examples/domain-modules/weave-character/ari-vale.weave".to_owned()),
    }
}

#[test]
fn complete_character_compiles_to_exact_portable_story_ir() {
    let first = compile_with_modules(SOURCE, &options(), &catalog()).expect("Character compiles");
    let second = compile_with_modules(SOURCE, &options(), &catalog()).expect("repeat compile");
    let ron = to_ron(&first.story).expect("Character RON");
    let json = to_json(&first.story).expect("Character JSON");
    assert_eq!(ron, to_ron(&second.story).unwrap());
    assert_eq!(json, to_json(&second.story).unwrap());
    assert_eq!(ron, CHECKED_RON);
    assert_eq!(json, CHECKED_JSON);

    let from_ron: StoryIr = ron::from_str(&ron).expect("decode Character RON");
    let from_json: StoryIr = serde_json::from_str(&json).expect("decode Character JSON");
    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron, first.story);
    let character = &from_ron.modules["character"];
    assert_eq!(character.id, "org.weave.character");
    assert_eq!(character.pack_id, "ari_vale");
    assert_eq!(
        character.value(&["profile", "identity", "id"]),
        Some(&DomainValueIr::String(
            "org.weave.character.ari_vale".to_owned()
        ))
    );
    assert_eq!(
        character.value(&["profile", "identity", "display_name", "value"]),
        Some(&DomainValueIr::String("Ari Vale, Wayfinder".to_owned()))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "hexaco",
            "openness",
            "creativity",
            "projection_score"
        ]),
        Some(&DomainValueIr::Number(0.86))
    );
    assert_eq!(
        character.value(&["profile", "ocean", "openness", "score"]),
        Some(&DomainValueIr::Number(0.83))
    );
    assert_eq!(
        character.value(&["profile", "ocean", "lossy"]),
        Some(&DomainValueIr::Bool(true))
    );
    assert_eq!(character.authored_overrides.len(), 1);
    assert_eq!(
        character.authored_overrides[0].path,
        ["profile", "identity", "display_name", "value"].map(str::to_owned)
    );
}

#[test]
fn character_schema_paths_are_static_and_derived_evidence_is_read_only() {
    let unknown = SOURCE.replace(
        "character.profile.hexaco.openness.creativity.projection_score",
        "character.profile.hexaco.openness.imaginary_facet.projection_score",
    );
    let error = compile_with_modules(&unknown, &options(), &catalog())
        .expect_err("unknown facet must fail statically");
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "D143")
    );

    let writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.ocean.openness.score: 0.1",
    );
    let error = compile_with_modules(&writeback, &options(), &catalog())
        .expect_err("derived writeback must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");
    assert!(error.diagnostics[0].message.contains("read-only path"));

    let canonical_writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.hexaco.openness.creativity.projection_score: 0.1",
    );
    let error = compile_with_modules(&canonical_writeback, &options(), &catalog())
        .expect_err("canonical evidence bypass must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");
}
