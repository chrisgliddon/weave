use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_core::ir::{DomainValueIr, StoryIr};
use weave_domain::{DomainCatalog, DomainError, DomainPack, ModuleManifest};

const SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.weave");
const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/weave-world/module.weave-module.json");
const PACK: &str =
    include_str!("../../../examples/domain-modules/weave-world/pack.weave-domain.json");
const BRITISH_COLUMBIA_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-world/packs/british_columbia_temperate_forest.weave-domain.json"
);
const HOKKAIDO_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-world/packs/hokkaido_japan.weave-domain.json"
);
const MALDIVES_PACK: &str =
    include_str!("../../../examples/domain-modules/weave-world/packs/maldives.weave-domain.json");
const CHECKED_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.story.ron");
const CHECKED_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/reference-place.story.json");
const BRITISH_COLUMBIA_SOURCE: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.weave"
);
const BRITISH_COLUMBIA_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.ron"
);
const BRITISH_COLUMBIA_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.json"
);
const HOKKAIDO_SOURCE: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/hokkaido-japan.weave"
);
const HOKKAIDO_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/hokkaido-japan.story.ron"
);
const HOKKAIDO_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-world/corpus/stories/hokkaido-japan.story.json"
);
const MALDIVES_SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-world/corpus/stories/maldives.weave");
const MALDIVES_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/corpus/stories/maldives.story.ron");
const MALDIVES_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/corpus/stories/maldives.story.json");

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST).expect("canonical world manifest")
}

fn pack() -> DomainPack {
    DomainPack::from_json(PACK).expect("canonical world pack")
}

fn catalog_with(pack: DomainPack) -> DomainCatalog {
    DomainCatalog::from_artifacts([manifest()], [pack]).expect("world catalog")
}

fn corpus_catalog() -> DomainCatalog {
    DomainCatalog::from_artifacts(
        [manifest()],
        [PACK, BRITISH_COLUMBIA_PACK, HOKKAIDO_PACK, MALDIVES_PACK]
            .map(|source| DomainPack::from_json(source).expect("canonical world pack")),
    )
    .expect("world corpus catalog")
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
fn four_scale_distinct_presets_compile_to_exact_ron_and_json() {
    let catalog = corpus_catalog();
    let fixtures = [
        (
            BRITISH_COLUMBIA_SOURCE,
            "examples/domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.weave",
            BRITISH_COLUMBIA_RON,
            BRITISH_COLUMBIA_JSON,
            "british_columbia_temperate_forest",
            "temperate_conifer_forest",
        ),
        (
            HOKKAIDO_SOURCE,
            "examples/domain-modules/weave-world/corpus/stories/hokkaido-japan.weave",
            HOKKAIDO_RON,
            HOKKAIDO_JSON,
            "hokkaido_japan",
            "temperate_broadleaf_and_mixed_forest",
        ),
        (
            MALDIVES_SOURCE,
            "examples/domain-modules/weave-world/corpus/stories/maldives.weave",
            MALDIVES_RON,
            MALDIVES_JSON,
            "maldives",
            "tropical_and_subtropical_moist_broadleaf_forest",
        ),
        (
            SOURCE,
            "examples/domain-modules/weave-world/reference-place.weave",
            CHECKED_RON,
            CHECKED_JSON,
            "aotearoa_new_zealand",
            "temperate_broadleaf_and_mixed_forest",
        ),
    ];
    let mut distinct = std::collections::BTreeSet::new();
    for (source, source_name, checked_ron, checked_json, preset, biome) in fixtures {
        let compiled = compile_with_modules(
            source,
            &CompileOptions {
                source_name: Some(source_name.to_owned()),
            },
            &catalog,
        )
        .expect("corpus story compiles");
        assert_eq!(compiled.domain_modules.len(), 1);
        let ron = to_ron(&compiled.story).expect("serialize corpus RON");
        let json = to_json(&compiled.story).expect("serialize corpus JSON");
        assert_eq!(ron, checked_ron);
        assert_eq!(json, checked_json);
        assert_eq!(
            compiled.story.modules["world"].value(&["seed", "identity", "preset"]),
            Some(&DomainValueIr::String(preset.to_owned()))
        );
        assert_eq!(
            compiled.story.modules["world"].value(&["seed", "primary_biome"]),
            Some(&DomainValueIr::Symbol(biome.to_owned()))
        );
        assert_eq!(
            compiled.story.modules["world"].value(&["seed", "identity", "culture_included"]),
            Some(&DomainValueIr::Bool(false))
        );
        distinct.insert((
            format!(
                "{:?}",
                compiled.story.modules["world"].value(&["seed", "climate", "band"])
            ),
            format!(
                "{:?}",
                compiled.story.modules["world"].value(&["seed", "hazards", "tendencies"])
            ),
            format!(
                "{:?}",
                compiled.story.modules["world"].value(&[
                    "seed",
                    "context",
                    "geographic_resolution"
                ])
            ),
        ));
        let from_ron: StoryIr = ron::from_str(&ron).expect("decode corpus RON");
        let from_json: StoryIr = serde_json::from_str(&json).expect("decode corpus JSON");
        assert_eq!(from_ron, from_json);
        assert_eq!(from_ron, compiled.story);
    }
    assert_eq!(distinct.len(), 4);
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
