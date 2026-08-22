use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_core::ir::{DomainValueIr, StoryIr};
use weave_domain::{DomainCatalog, DomainPack, DomainValue, ModuleManifest};

const SOURCE: &str = include_str!("../../../examples/domain-modules/contract/tracer.weave");
const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/contract/module.weave-module.json");
const PACK: &str = include_str!("../../../examples/domain-modules/contract/pack.weave-domain.json");

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST).expect("canonical manifest")
}

fn pack() -> DomainPack {
    DomainPack::from_json(PACK).expect("canonical pack")
}

fn catalog_with(pack: DomainPack) -> DomainCatalog {
    DomainCatalog::from_artifacts([manifest()], [pack]).expect("catalog canonical artifacts")
}

#[test]
fn synthetic_module_lowers_to_deterministic_equivalent_ron_and_json() {
    let catalog = catalog_with(pack());
    let first = compile_with_modules(SOURCE, &CompileOptions::default(), &catalog)
        .expect("tracer compiles");
    let second = compile_with_modules(SOURCE, &CompileOptions::default(), &catalog)
        .expect("repeated tracer compile succeeds");
    let first_ron = to_ron(&first.story).expect("serialize RON");
    let second_ron = to_ron(&second.story).expect("serialize repeated RON");
    let first_json = to_json(&first.story).expect("serialize JSON");
    let second_json = to_json(&second.story).expect("serialize repeated JSON");

    assert_eq!(first_ron, second_ron);
    assert_eq!(first_json, second_json);
    let from_ron: StoryIr = ron::from_str(&first_ron).expect("decode RON");
    let from_json: StoryIr = serde_json::from_str(&first_json).expect("decode JSON");
    assert_eq!(from_ron, from_json);
    assert_eq!(from_ron, first.story);
    assert_eq!(first.domain_modules.len(), 1);
    assert_eq!(
        from_ron.modules["constellation"].value(&["phase"]),
        Some(&DomainValueIr::Symbol("twilight".to_owned()))
    );
}

#[test]
fn missing_pack_and_unsupported_module_versions_have_stable_diagnostics() {
    let missing_pack =
        DomainCatalog::from_artifacts([manifest()], []).expect("manifest-only catalog is valid");
    let error = compile_with_modules(SOURCE, &CompileOptions::default(), &missing_pack)
        .expect_err("activation requires its selected pack");
    assert_eq!(error.diagnostics[0].code.0, "D151");

    let source = SOURCE.replace("=1.0.0\"\n    pack", "^2.0\"\n    pack");
    let error = compile_with_modules(&source, &CompileOptions::default(), &catalog_with(pack()))
        .expect_err("unsupported module version must fail");
    assert_eq!(error.diagnostics[0].code.0, "D103");
}

#[test]
fn invalid_pack_values_and_typed_source_fields_fail_closed() {
    let mut invalid_pack = pack();
    invalid_pack.values.insert(
        "phase".to_owned(),
        DomainValue::Symbol("unlisted_phase".to_owned()),
    );
    let error = compile_with_modules(
        SOURCE,
        &CompileOptions::default(),
        &catalog_with(invalid_pack),
    )
    .expect_err("invalid selected value must fail");
    assert_eq!(error.diagnostics[0].code.0, "D140");

    let source = SOURCE.replace(
        "constellation.observation.label",
        "constellation.observation.unknown_field",
    );
    let error = compile_with_modules(&source, &CompileOptions::default(), &catalog_with(pack()))
        .expect_err("unknown typed module field must fail");
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "D143")
    );
}

#[test]
fn provenance_failures_use_the_stable_provenance_diagnostic_family() {
    let mut invalid_pack = pack();
    invalid_pack.provenance.sources[0].license = "invalid SPDX expression".to_owned();
    let error = compile_with_modules(
        SOURCE,
        &CompileOptions::default(),
        &catalog_with(invalid_pack),
    )
    .expect_err("invalid provenance must fail");
    assert_eq!(error.diagnostics[0].code.0, "D130");
}

#[test]
fn module_aliases_cannot_collide_with_earlier_variables() {
    let source = SOURCE.replacen(
        "module constellation {",
        "VAR constellation = 0\n\nmodule constellation {",
        1,
    );
    let error = compile_with_modules(&source, &CompileOptions::default(), &catalog_with(pack()))
        .expect_err("module alias and variable must not share a name");
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "D111")
    );
}
