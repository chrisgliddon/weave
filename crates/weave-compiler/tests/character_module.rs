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
const TEMPORAL_SOURCE: &str = include_str!(
    "../../../examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave"
);
const TEMPORAL_MANIFEST: &str = include_str!(
    "../../../examples/domain-modules/weave-character/context/runtime/module.weave-module.json"
);
const TEMPORAL_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-character/context/runtime/ari_vale_temporal.weave-domain.json"
);
const TEMPORAL_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.story.ron"
);
const TEMPORAL_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.story.json"
);

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

fn temporal_catalog() -> DomainCatalog {
    DomainCatalog::from_artifacts(
        [ModuleManifest::from_json(TEMPORAL_MANIFEST).expect("temporal Character manifest")],
        [DomainPack::from_json(TEMPORAL_PACK).expect("temporal Character pack")],
    )
    .expect("temporal Character catalog")
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
    assert_eq!(character.version, "1.6.0");
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
    assert_eq!(
        character.value(&["profile", "alignment", "canonical_personality_write_back"]),
        Some(&DomainValueIr::Bool(false))
    );
    assert_eq!(
        character.value(&["profile", "alignment", "pack", "id"]),
        Some(&DomainValueIr::String(
            "org.weave.alignment.wayfinder_compass".to_owned()
        ))
    );
    assert_eq!(
        character.value(&["profile", "alignment", "pack", "version"]),
        Some(&DomainValueIr::String("1.0.0".to_owned()))
    );
    assert!(matches!(
        character.value(&["profile", "alignment", "pack", "sha256"]),
        Some(DomainValueIr::String(value)) if value.len() == 64
    ));
    assert_eq!(
        character.value(&["profile", "relationships", "kind_pack", "id"]),
        Some(&DomainValueIr::String(
            "org.weave.relationship.reference".to_owned()
        ))
    );
    assert_eq!(
        character.value(&["profile", "relationships", "kind_pack", "version"]),
        Some(&DomainValueIr::String("1.0.0".to_owned()))
    );
    assert!(matches!(
        character.value(&["profile", "relationships", "kind_pack", "sha256"]),
        Some(DomainValueIr::String(value)) if value.len() == 64
    ));
    assert_eq!(
        character.value(&["profile", "expression", "lexicon", "trailmark", "surface",]),
        Some(&DomainValueIr::String("trailmark".to_owned()))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "expression",
            "preferences",
            "clear_questions",
            "target",
        ]),
        Some(&DomainValueIr::String("clear questions".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "expression", "canonical_personality_write_back",]),
        Some(&DomainValueIr::Bool(false))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "personality_lens",
            "label",
        ]),
        Some(&DomainValueIr::String("Open Explorer".to_owned()))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "personality_lens",
            "lossy",
        ]),
        Some(&DomainValueIr::Bool(true))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "personality_lens",
            "decision",
        ]),
        Some(&DomainValueIr::Symbol("derived".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "projections", "values", "vocation", "label",]),
        Some(&DomainValueIr::String("Route Archivist".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "projections", "values", "social_role", "label",]),
        Some(&DomainValueIr::String("Question Host".to_owned()))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "narrative_role",
            "label",
        ]),
        Some(&DomainValueIr::String("Signal Keeper".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "projections", "values", "narrative_role", "lock",]),
        Some(&DomainValueIr::Symbol("locked".to_owned()))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "narrative_role",
            "pack",
            "id",
        ]),
        Some(&DomainValueIr::String(
            "org.weave.projection.glasswind_lenses".to_owned()
        ))
    );
    assert!(matches!(
        character.value(&[
            "profile",
            "projections",
            "values",
            "narrative_role",
            "review_sha256",
        ]),
        Some(DomainValueIr::String(value)) if value.len() == 64
    ));
    let Some(DomainValueIr::List(input_paths)) = character.value(&[
        "profile",
        "projections",
        "values",
        "narrative_role",
        "input_paths",
    ]) else {
        panic!("projection input paths are absent");
    };
    assert_eq!(input_paths.len(), 3);
    for target in [
        "alignment",
        "birth",
        "hexaco",
        "identity",
        "ocean",
        "relationships",
        "ruleset",
    ] {
        assert_eq!(
            character.value(&["profile", "projections", "write_back", target]),
            Some(&DomainValueIr::Bool(false))
        );
    }
    assert_eq!(
        character.value(&[
            "profile",
            "relationships",
            "edges",
            "mentor_sable",
            "target_character_id",
        ]),
        Some(&DomainValueIr::String(
            "org.weave.character.sable_reed".to_owned()
        ))
    );
    assert_eq!(
        character.value(&[
            "profile",
            "relationships",
            "edges",
            "mentor_sable",
            "origin",
        ]),
        Some(&DomainValueIr::Symbol("authored".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "alignment", "values", "horizon", "label_id"]),
        Some(&DomainValueIr::String("seeking".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "alignment", "values", "reciprocity", "decision"]),
        Some(&DomainValueIr::Symbol("edited".to_owned()))
    );
    assert_eq!(
        character.value(&["profile", "alignment", "values", "structure", "decision"]),
        Some(&DomainValueIr::Symbol("overridden".to_owned()))
    );
    assert!(
        character
            .value(&["profile", "alignment", "values", "signal"])
            .is_none()
    );
    assert!(
        character
            .value(&["profile", "alignment", "values", "tempo"])
            .is_none()
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

    let alignment_writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.alignment.values.horizon.label_id: \"anchored\"",
    );
    let error = compile_with_modules(&alignment_writeback, &options(), &catalog())
        .expect_err("reviewed alignment bypass must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");
    assert!(error.diagnostics[0].message.contains("read-only path"));

    let relationship_writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.relationships.edges.mentor_sable.kind: \"org.weave.relationship.rival\"",
    );
    let error = compile_with_modules(&relationship_writeback, &options(), &catalog())
        .expect_err("relationship workflow bypass must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");

    let expression_writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.expression.lexicon.trailmark.surface: \"changed\"",
    );
    let error = compile_with_modules(&expression_writeback, &options(), &catalog())
        .expect_err("expression workflow bypass must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");

    let projection_writeback = SOURCE.replace(
        "override profile.identity.display_name.value: \"Ari Vale, Wayfinder\"",
        "override profile.projections.values.narrative_role.label: \"Changed\"",
    );
    let error = compile_with_modules(&projection_writeback, &options(), &catalog())
        .expect_err("projection review bypass must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");
}

#[test]
fn reviewed_temporal_context_compiles_with_separate_lineage_and_no_writeback() {
    let options = CompileOptions {
        source_name: Some(
            "examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave"
                .to_owned(),
        ),
    };
    let compilation = compile_with_modules(TEMPORAL_SOURCE, &options, &temporal_catalog())
        .expect("temporal Character compiles");
    assert_eq!(to_json(&compilation.story).unwrap(), TEMPORAL_JSON);
    assert_eq!(to_ron(&compilation.story).unwrap(), TEMPORAL_RON);
    let character = &compilation.story.modules["character"];
    assert_eq!(
        character.value(&[
            "profile",
            "date_context",
            "canonical_personality_write_back"
        ]),
        Some(&DomainValueIr::Bool(false))
    );
    let Some(DomainValueIr::List(cues)) = character.value(&["profile", "date_context", "cues"])
    else {
        panic!("reviewed date-context cues are absent");
    };
    assert_eq!(cues.len(), 3);
    for cue in cues {
        let DomainValueIr::Object(fields) = cue else {
            panic!("date-context cue is not an object");
        };
        assert!(matches!(
            fields.get("fact_source_ids"),
            Some(DomainValueIr::List(values)) if !values.is_empty()
        ));
        assert!(matches!(
            fields.get("cue_source_ids"),
            Some(DomainValueIr::List(values)) if !values.is_empty()
        ));
    }

    let forbidden = TEMPORAL_SOURCE.replace(
        "    pack: \"ari_vale_temporal@=1.0.0\"\n}",
        "    pack: \"ari_vale_temporal@=1.0.0\"\n    override profile.date_context.canonical_personality_write_back: true\n}",
    );
    let error = compile_with_modules(&forbidden, &options, &temporal_catalog())
        .expect_err("temporal context writeback must fail");
    assert!(error.diagnostics.iter().any(|diagnostic| {
        diagnostic.code.0 == "D140" && diagnostic.message.contains("read-only path")
    }));
}
