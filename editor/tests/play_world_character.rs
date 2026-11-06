//! Resolution tests for World- and Character-driven Play presentation.
//!
//! Every fixture is synthetic. The tests build compiled-story data in memory
//! and supply explicit host selectors, so they exercise the generic module
//! contract without depending on a World or Character schema.

use std::collections::BTreeMap;

use weave_core::ir::{
    AnchorIr, DomainExportIr, DomainExportSourceIr, DomainModuleIr, DomainValueIr, Fit, SceneIr,
    Slot, StageIr, StoryIr,
};
use weave_editor::play_view::{
    CharacterPresentation, CharacterProjection, PresentationDiagnosticKind,
    RESOLVED_PRESENTATION_FORMAT_VERSION, ResolvedPresentation, ValueRef, WorldPresentation,
    resolve_presentation,
};
use weave_runtime::Value;

fn text(value: &str) -> DomainValueIr {
    DomainValueIr::String(value.to_owned())
}

fn symbol(value: &str) -> DomainValueIr {
    DomainValueIr::Symbol(value.to_owned())
}

fn object(fields: [(&str, DomainValueIr); 2]) -> DomainValueIr {
    DomainValueIr::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn export(value: DomainValueIr) -> DomainExportIr {
    DomainExportIr {
        source: DomainExportSourceIr::Pack,
        value,
    }
}

fn story() -> StoryIr {
    let mut story = StoryIr::new("start");
    story.presentation.scenes.insert(
        "harbour".to_owned(),
        SceneIr {
            background: Some("art/harbour.png".to_owned()),
            fit: Fit::Cover,
            anchor: AnchorIr::default(),
        },
    );
    story.presentation.scenes.insert(
        "cabin".to_owned(),
        SceneIr {
            background: Some("art/cabin.png".to_owned()),
            fit: Fit::Contain,
            anchor: AnchorIr::default(),
        },
    );
    story.presentation.stages.insert(
        "mara".to_owned(),
        StageIr {
            image: Some("art/mara_neutral.png".to_owned()),
            name: Some("Mara".to_owned()),
            slot: Slot::Left,
            layer: 1,
            variants: BTreeMap::from([
                ("surprised".to_owned(), "art/mara_surprised.png".to_owned()),
                ("angry".to_owned(), "art/mara_angry.png".to_owned()),
            ]),
        },
    );
    story.presentation.stages.insert(
        "keeper".to_owned(),
        StageIr {
            image: Some("art/keeper_neutral.png".to_owned()),
            name: Some("The Keeper".to_owned()),
            slot: Slot::Right,
            layer: 2,
            variants: BTreeMap::new(),
        },
    );
    story.modules.insert(
        "world".to_owned(),
        DomainModuleIr {
            id: "org.weave.world".to_owned(),
            version: "1.0.0".to_owned(),
            pack_id: "example.world".to_owned(),
            pack_version: "1.0.0".to_owned(),
            exports: BTreeMap::from([
                (
                    "seed".to_owned(),
                    export(object([
                        ("primary_biome", symbol("coastal")),
                        ("terrain", symbol("harbour")),
                    ])),
                ),
                (
                    "metrics".to_owned(),
                    export(object([
                        ("temperature", DomainValueIr::Number(12.5)),
                        ("coastal", DomainValueIr::Bool(true)),
                    ])),
                ),
            ]),
            authored_overrides: Vec::new(),
        },
    );
    story.modules.insert(
        "character".to_owned(),
        DomainModuleIr {
            id: "org.weave.character".to_owned(),
            version: "1.0.0".to_owned(),
            pack_id: "example.character".to_owned(),
            pack_version: "1.0.0".to_owned(),
            exports: BTreeMap::from([(
                "profile".to_owned(),
                export(object([
                    ("id", text("mara")),
                    ("expression", symbol("surprised")),
                ])),
            )]),
            authored_overrides: Vec::new(),
        },
    );
    story
}

fn snapshot<const N: usize>(entries: [(&str, Value); N]) -> BTreeMap<String, Value> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

fn world_from_variable() -> WorldPresentation {
    WorldPresentation {
        selector: ValueRef::Variable {
            name: "place".to_owned(),
        },
        scenes: BTreeMap::from([
            ("harbour".to_owned(), "harbour".to_owned()),
            ("cabin".to_owned(), "cabin".to_owned()),
        ]),
        fallback: Some("cabin".to_owned()),
    }
}

fn character_from_variable() -> CharacterPresentation {
    CharacterPresentation {
        selector: ValueRef::Variable {
            name: "speaker".to_owned(),
        },
        projections: BTreeMap::from([(
            "mara".to_owned(),
            CharacterProjection {
                display_name: Some("Mara Vale".to_owned()),
                stage: Some("mara".to_owned()),
                expression: Some("surprised".to_owned()),
                portrait: Some("art/mara_portrait.png".to_owned()),
            },
        )]),
        stage_ids: BTreeMap::from([("mara".to_owned(), "mara".to_owned())]),
        expressions: BTreeMap::from([("surprised".to_owned(), "surprised".to_owned())]),
        fallback_stage: Some("keeper".to_owned()),
    }
}

fn kinds(plan: &ResolvedPresentation) -> Vec<PresentationDiagnosticKind> {
    plan.diagnostics
        .iter()
        .map(|diagnostic| diagnostic.kind)
        .collect()
}

#[test]
fn a_typed_world_variable_selects_a_declared_scene() {
    let plan = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        None,
        Some(&snapshot([("place", Value::String("harbour".to_owned()))])),
    );

    let scene = plan.scene.as_ref().expect("a scene is selected");
    assert_eq!(scene.id, "harbour");
    assert!(!scene.fallback);
    assert_eq!(scene.source, world_from_variable().selector);
    assert!(plan.stage.is_none());
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn a_world_environment_export_value_selects_a_declared_scene() {
    let world = WorldPresentation {
        selector: ValueRef::Module {
            alias: "world".to_owned(),
            path: vec!["seed".to_owned(), "primary_biome".to_owned()],
        },
        scenes: BTreeMap::from([("coastal".to_owned(), "harbour".to_owned())]),
        fallback: Some("cabin".to_owned()),
    };

    let plan = resolve_presentation(&story(), Some(&world), None, None);
    assert_eq!(
        plan.scene.as_ref().map(|scene| scene.id.as_str()),
        Some("harbour")
    );
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn a_character_projection_supplies_name_stage_and_expression_variant() {
    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character_from_variable()),
        Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
    );

    let stage = plan.stage.as_ref().expect("a stage is selected");
    assert_eq!(stage.id, "mara");
    assert_eq!(stage.name, "Mara Vale");
    assert_eq!(stage.image.as_deref(), Some("art/mara_surprised.png"));
    assert_eq!(stage.variant.as_deref(), Some("surprised"));
    assert!(!stage.fallback);
    assert_eq!(stage.source, character_from_variable().selector);
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn a_character_selector_reads_a_namespaced_module_export() {
    let character = CharacterPresentation {
        selector: ValueRef::Module {
            alias: "character".to_owned(),
            path: vec!["profile".to_owned(), "id".to_owned()],
        },
        projections: BTreeMap::from([(
            "mara".to_owned(),
            CharacterProjection {
                display_name: Some("Mara Vale".to_owned()),
                stage: Some("mara".to_owned()),
                expression: None,
                portrait: Some("art/mara_portrait.png".to_owned()),
            },
        )]),
        stage_ids: BTreeMap::from([("mara".to_owned(), "mara".to_owned())]),
        expressions: BTreeMap::new(),
        fallback_stage: Some("keeper".to_owned()),
    };

    let plan = resolve_presentation(&story(), None, Some(&character), None);
    let stage = plan.stage.as_ref().expect("a stage is selected");
    assert_eq!(stage.id, "mara");
    assert_eq!(stage.name, "Mara Vale");
    assert_eq!(stage.image.as_deref(), Some("art/mara_portrait.png"));
    assert_eq!(stage.variant, None);
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn missing_optional_modules_produce_an_empty_plan() {
    let plan = resolve_presentation(&story(), None, None, None);
    assert_eq!(plan, ResolvedPresentation::empty());
    assert_eq!(plan.format_version, RESOLVED_PRESENTATION_FORMAT_VERSION);
}

#[test]
fn a_missing_module_alias_reports_unknown_module_and_uses_the_scene_fallback() {
    let world = WorldPresentation {
        selector: ValueRef::Module {
            alias: "absent".to_owned(),
            path: vec!["seed".to_owned()],
        },
        scenes: BTreeMap::from([("coastal".to_owned(), "harbour".to_owned())]),
        fallback: Some("cabin".to_owned()),
    };

    let plan = resolve_presentation(&story(), Some(&world), None, None);
    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnknownModule,
            PresentationDiagnosticKind::Fallback
        ]
    );
    let scene = plan.scene.as_ref().expect("the fallback scene is used");
    assert_eq!(scene.id, "cabin");
    assert!(scene.fallback);
}

#[test]
fn a_missing_module_export_or_path_reports_the_exact_failure() {
    let missing_export = WorldPresentation {
        selector: ValueRef::Module {
            alias: "world".to_owned(),
            path: vec!["absent".to_owned()],
        },
        scenes: BTreeMap::new(),
        fallback: None,
    };
    let plan = resolve_presentation(&story(), Some(&missing_export), None, None);
    assert_eq!(kinds(&plan), [PresentationDiagnosticKind::UnknownExport]);
    assert_eq!(plan.scene, None);

    let missing_path = WorldPresentation {
        selector: ValueRef::Module {
            alias: "world".to_owned(),
            path: vec!["seed".to_owned(), "absent".to_owned()],
        },
        scenes: BTreeMap::new(),
        fallback: None,
    };
    let plan = resolve_presentation(&story(), Some(&missing_path), None, None);
    assert_eq!(kinds(&plan), [PresentationDiagnosticKind::UnknownPath]);
    assert_eq!(plan.scene, None);
}

#[test]
fn an_unknown_selected_value_reports_unmapped_value_and_uses_the_fallback() {
    let plan = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        None,
        Some(&snapshot([(
            "place",
            Value::String("elsewhere".to_owned()),
        )])),
    );

    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnmappedValue,
            PresentationDiagnosticKind::Fallback
        ]
    );
    assert_eq!(
        plan.scene.as_ref().map(|scene| scene.id.as_str()),
        Some("cabin")
    );
}

#[test]
fn an_unknown_mapped_scene_reports_unknown_scene_and_uses_the_fallback() {
    let world = WorldPresentation {
        selector: ValueRef::Variable {
            name: "place".to_owned(),
        },
        scenes: BTreeMap::from([("harbour".to_owned(), "not_declared".to_owned())]),
        fallback: Some("cabin".to_owned()),
    };
    let plan = resolve_presentation(
        &story(),
        Some(&world),
        None,
        Some(&snapshot([("place", Value::String("harbour".to_owned()))])),
    );

    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnknownScene,
            PresentationDiagnosticKind::Fallback
        ]
    );
    assert_eq!(
        plan.scene.as_ref().map(|scene| scene.id.as_str()),
        Some("cabin")
    );
}

#[test]
fn an_incompatible_variable_type_reports_incompatible_type() {
    let plan = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        None,
        Some(&snapshot([("place", Value::Bool(true))])),
    );

    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::IncompatibleType,
            PresentationDiagnosticKind::Fallback
        ]
    );
    assert_eq!(
        plan.scene.as_ref().map(|scene| scene.id.as_str()),
        Some("cabin")
    );
}

#[test]
fn an_incompatible_module_value_type_reports_incompatible_type() {
    let world = WorldPresentation {
        selector: ValueRef::Module {
            alias: "world".to_owned(),
            path: vec!["metrics".to_owned(), "temperature".to_owned()],
        },
        scenes: BTreeMap::new(),
        fallback: None,
    };
    let plan = resolve_presentation(&story(), Some(&world), None, None);

    assert_eq!(kinds(&plan), [PresentationDiagnosticKind::IncompatibleType]);
    assert!(plan.scene.is_none());
    assert!(plan.diagnostics[0].detail.contains("string or symbol"));
}

#[test]
fn an_intermediate_non_object_reports_incompatible_type() {
    let world = WorldPresentation {
        selector: ValueRef::Module {
            alias: "world".to_owned(),
            path: vec![
                "seed".to_owned(),
                "primary_biome".to_owned(),
                "nested".to_owned(),
            ],
        },
        scenes: BTreeMap::new(),
        fallback: None,
    };
    let plan = resolve_presentation(&story(), Some(&world), None, None);

    assert_eq!(kinds(&plan), [PresentationDiagnosticKind::IncompatibleType]);
    assert!(plan.scene.is_none());
}

#[test]
fn an_unavailable_variable_reports_unavailable_value() {
    let without_snapshot = resolve_presentation(&story(), Some(&world_from_variable()), None, None);
    assert_eq!(
        kinds(&without_snapshot),
        [
            PresentationDiagnosticKind::UnavailableValue,
            PresentationDiagnosticKind::Fallback
        ]
    );

    let absent_name = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        None,
        Some(&snapshot([("other", Value::String("harbour".to_owned()))])),
    );
    assert_eq!(
        kinds(&absent_name),
        [
            PresentationDiagnosticKind::UnavailableValue,
            PresentationDiagnosticKind::Fallback
        ]
    );
}

#[test]
fn a_missing_character_projection_uses_the_stage_fallback() {
    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character_from_variable()),
        Some(&snapshot([("speaker", Value::String("nobody".to_owned()))])),
    );

    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnavailableProjection,
            PresentationDiagnosticKind::Fallback
        ]
    );
    let stage = plan.stage.as_ref().expect("the fallback stage is used");
    assert_eq!(stage.id, "keeper");
    assert_eq!(stage.name, "The Keeper");
    assert!(stage.fallback);
}

#[test]
fn an_unmapped_stage_identity_uses_the_stage_fallback() {
    let mut character = character_from_variable();
    character.projections.insert(
        "mara".to_owned(),
        CharacterProjection {
            display_name: Some("Mara Vale".to_owned()),
            stage: Some("unmapped_identity".to_owned()),
            expression: None,
            portrait: None,
        },
    );

    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character),
        Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
    );
    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnmappedStage,
            PresentationDiagnosticKind::Fallback
        ]
    );
    let stage = plan.stage.as_ref().expect("the fallback stage is used");
    assert_eq!(stage.id, "keeper");
    assert_eq!(stage.name, "Mara Vale");
}

#[test]
fn an_unknown_stage_identity_uses_the_stage_fallback() {
    let mut character = character_from_variable();
    character.projections.insert(
        "mara".to_owned(),
        CharacterProjection {
            display_name: Some("Mara Vale".to_owned()),
            stage: Some("mara".to_owned()),
            expression: None,
            portrait: None,
        },
    );
    character
        .stage_ids
        .insert("mara".to_owned(), "not_declared".to_owned());

    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character),
        Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
    );
    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnknownStage,
            PresentationDiagnosticKind::Fallback
        ]
    );
    assert_eq!(
        plan.stage.as_ref().map(|stage| stage.id.as_str()),
        Some("keeper")
    );
}

#[test]
fn an_unknown_variant_keeps_the_portrait_and_reports_the_failure() {
    let mut character = character_from_variable();
    character.projections.insert(
        "mara".to_owned(),
        CharacterProjection {
            display_name: Some("Mara Vale".to_owned()),
            stage: Some("mara".to_owned()),
            expression: Some("mystery".to_owned()),
            portrait: Some("art/mara_portrait.png".to_owned()),
        },
    );
    character
        .expressions
        .insert("mystery".to_owned(), "ghost".to_owned());

    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character),
        Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
    );
    assert_eq!(kinds(&plan), [PresentationDiagnosticKind::UnknownVariant]);
    let stage = plan.stage.as_ref().expect("a stage is selected");
    assert_eq!(stage.variant, None);
    assert_eq!(stage.image.as_deref(), Some("art/mara_portrait.png"));
}

#[test]
fn an_unmapped_expression_keeps_the_base_stage_image() {
    let mut character = character_from_variable();
    character.projections.insert(
        "mara".to_owned(),
        CharacterProjection {
            display_name: None,
            stage: Some("mara".to_owned()),
            expression: Some("mystery".to_owned()),
            portrait: None,
        },
    );

    let plan = resolve_presentation(
        &story(),
        None,
        Some(&character),
        Some(&snapshot([("speaker", Value::String("mara".to_owned()))])),
    );
    assert_eq!(
        kinds(&plan),
        [PresentationDiagnosticKind::UnmappedExpression]
    );
    let stage = plan.stage.as_ref().expect("a stage is selected");
    assert_eq!(stage.name, "Mara");
    assert_eq!(stage.image.as_deref(), Some("art/mara_neutral.png"));
    assert_eq!(stage.variant, None);
}

#[test]
fn a_declared_scene_fallback_is_used_when_the_selected_value_is_unavailable() {
    let mut variable = world_from_variable();
    variable.fallback = Some("not_declared".to_owned());

    let plan = resolve_presentation(&story(), Some(&variable), None, None);
    assert_eq!(
        kinds(&plan),
        [
            PresentationDiagnosticKind::UnavailableValue,
            PresentationDiagnosticKind::UnknownScene
        ]
    );
    assert!(plan.scene.is_none());
}

#[test]
fn resolution_is_deterministic_for_the_same_inputs() {
    let story = story();
    let variables = snapshot([("place", Value::Symbol("harbour".to_owned()))]);
    let world = world_from_variable();
    let character = character_from_variable();

    let first = resolve_presentation(&story, Some(&world), Some(&character), Some(&variables));
    let second = resolve_presentation(&story, Some(&world), Some(&character), Some(&variables));
    assert_eq!(first, second);
    assert_eq!(
        first.scene.as_ref().map(|scene| scene.id.as_str()),
        Some("harbour")
    );
}

#[test]
fn diagnostics_never_echo_selected_values() {
    let secret = "never-print-this-token";
    let plan = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        Some(&character_from_variable()),
        Some(&snapshot([
            ("place", Value::String(secret.to_owned())),
            ("speaker", Value::String(secret.to_owned())),
        ])),
    );

    assert!(!plan.diagnostics.is_empty());
    for diagnostic in &plan.diagnostics {
        assert!(!diagnostic.source.contains(secret));
        assert!(!diagnostic.detail.contains(secret));
    }
    assert!(!format!("{plan:?}").contains(secret));
    assert!(!plan.to_json().expect("JSON").contains(secret));
    assert!(!plan.to_ron().expect("RON").contains(secret));
}

#[test]
fn ron_and_json_output_are_equivalent() {
    let plan = resolve_presentation(
        &story(),
        Some(&world_from_variable()),
        Some(&character_from_variable()),
        Some(&snapshot([("place", Value::String("harbour".to_owned()))])),
    );

    let json = plan.to_json().expect("encode JSON");
    let ron = plan.to_ron().expect("encode RON");
    let from_json = ResolvedPresentation::from_json(&json).expect("decode JSON");
    let from_ron = ResolvedPresentation::from_ron(&ron).expect("decode RON");

    assert_eq!(from_json, plan);
    assert_eq!(from_ron, plan);
    assert_eq!(from_json, from_ron);
    assert_eq!(from_json.to_json().expect("re-encode JSON"), json);
    assert_eq!(from_ron.to_ron().expect("re-encode RON"), ron);
    assert!(json.contains("\"format_version\": 1"));
    assert!(ron.contains("format_version: 1"));
}

#[test]
fn an_unsupported_plan_version_is_rejected() {
    let plan = resolve_presentation(&story(), None, None, None);
    let json = plan
        .to_json()
        .expect("JSON")
        .replace("\"format_version\": 1", "\"format_version\": 99");
    assert!(matches!(
        ResolvedPresentation::from_json(&json),
        Err(
            weave_editor::play_view::PresentationFormatError::UnsupportedVersion {
                found: 99,
                expected: RESOLVED_PRESENTATION_FORMAT_VERSION
            }
        )
    ));
}
