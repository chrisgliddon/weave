use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_core::ir::{DomainValueIr, StoryIr};
use weave_domain::{DomainCatalog, DomainError, DomainPack, ModuleManifest};

const SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.weave");
const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/weave-world/module.weave-module.json");
const PACK: &str =
    include_str!("../../../examples/domain-modules/weave-world/pack.weave-domain.json");
const CHECKED_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.story.ron");
const CHECKED_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.story.json");

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST).expect("canonical world manifest")
}

fn pack() -> DomainPack {
    DomainPack::from_json(PACK).expect("canonical world pack")
}

fn catalog_with(pack: DomainPack) -> DomainCatalog {
    DomainCatalog::from_artifacts([manifest()], [pack]).expect("world catalog")
}

fn options() -> CompileOptions {
    CompileOptions {
        source_name: Some("examples/domain-modules/weave-world/reference-place.weave".to_owned()),
    }
}

#[test]
fn reference_place_shorthand_is_deterministic_and_portable() {
    let catalog = catalog_with(pack());
    let first = compile_with_modules(SOURCE, &options(), &catalog).expect("world source compiles");
    let second =
        compile_with_modules(SOURCE, &options(), &catalog).expect("world source recompiles");
    let ron = to_ron(&first.story).expect("serialize world RON");
    let json = to_json(&first.story).expect("serialize world JSON");

    assert_eq!(ron, to_ron(&second.story).expect("repeat world RON"));
    assert_eq!(json, to_json(&second.story).expect("repeat world JSON"));
    assert_eq!(ron, CHECKED_RON);
    assert_eq!(json, CHECKED_JSON);
    let from_ron: StoryIr = ron::from_str(&ron).expect("decode world RON");
    let from_json: StoryIr = serde_json::from_str(&json).expect("decode world JSON");
    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron, first.story);
    assert_eq!(
        from_ron.modules["world"].value(&["seed", "identity", "preset"]),
        Some(&DomainValueIr::String("aotearoa_new_zealand".to_owned()))
    );
    assert_eq!(
        from_ron.modules["world"].value(&["seed", "seasonality", "coolest_month"]),
        Some(&DomainValueIr::Symbol("july".to_owned()))
    );
    assert_eq!(
        first.domain_modules["world"].pack.provenance.sources.len(),
        6
    );
}

#[test]
fn preset_resolution_failures_are_distinct_and_actionable() {
    let catalog = catalog_with(pack());

    let unknown = SOURCE.replace("aotearoa_new_zealand", "unmapped_reference");
    let error =
        compile_with_modules(&unknown, &options(), &catalog).expect_err("unknown preset must fail");
    assert_eq!(error.diagnostics[0].code.0, "D151");
    assert!(
        error.diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("install the selected preset pack"))
    );

    let unavailable = SOURCE.replace("aotearoa_new_zealand@=1.0.0", "aotearoa_new_zealand@=9.0.0");
    let error = compile_with_modules(&unavailable, &options(), &catalog)
        .expect_err("unavailable preset release must fail");
    assert_eq!(error.diagnostics[0].code.0, "D104");
    assert!(
        error.diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("install a matching preset release"))
    );

    let mut incompatible_pack = pack();
    incompatible_pack.module.version = "=9.0.0".to_owned();
    let error = compile_with_modules(SOURCE, &options(), &catalog_with(incompatible_pack))
        .expect_err("incompatible preset must fail");
    assert_eq!(error.diagnostics[0].code.0, "D104");
    assert!(
        error.diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("whose module range includes"))
    );

    let duplicate = pack();
    let error = DomainCatalog::from_artifacts([manifest()], [duplicate.clone(), duplicate])
        .expect_err("duplicate preset coordinate must be ambiguous");
    assert!(matches!(error, DomainError::DuplicatePackArtifact));
    assert!(error.to_string().contains("ambiguous"));
}
