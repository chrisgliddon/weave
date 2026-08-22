use semver::Version;
use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{DomainCatalog, DomainPack, ModuleManifest};
use weave_world::{
    CompactWorldExport, FullWorldExport, WorldCompositionError, WorldCompositionPlan,
    WorldConflictPolicy, WorldLayerRole, WorldLayerSelection, WorldLayerSpec, WorldPackSelector,
    compact_world_schema, compose_world_pack, export_world, full_world_schema,
    world_composition_schema,
};

const MANIFEST: &str =
    include_str!("../../../examples/domain-modules/weave-world/module.weave-module.json");
const BROAD: &str =
    include_str!("../../../examples/domain-modules/weave-world/pack.weave-domain.json");
const REGIONAL: &str = include_str!(
    "../../../examples/domain-modules/weave-world/packs/hokkaido_japan.weave-domain.json"
);
const ECOSYSTEM: &str = include_str!(
    "../../../examples/domain-modules/weave-world/packs/british_columbia_temperate_forest.weave-domain.json"
);
const AUTHORED_SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-world/authored-setting.weave");
const PLAN: &str =
    include_str!("../../../examples/domain-modules/weave-world/composition.weave-world.json");
const COMPOSED_SOURCE: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-setting.weave");
const CHECKED_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-world/packs/glasswind_composed.weave-domain.json"
);
const CHECKED_RECEIPT_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composition.receipt.json");
const CHECKED_RECEIPT_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composition.receipt.ron");
const CHECKED_STORY_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-setting.story.json");
const CHECKED_STORY_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-setting.story.ron");
const CHECKED_FULL_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-world.full.json");
const CHECKED_FULL_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-world.full.ron");
const CHECKED_COMPACT_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-world.compact.json");
const CHECKED_COMPACT_RON: &str =
    include_str!("../../../examples/domain-modules/weave-world/composed-world.compact.ron");
const CHECKED_COMPOSITION_SCHEMA: &str =
    include_str!("../../../schemas/weave-world-composition-v1.schema.json");
const CHECKED_FULL_SCHEMA: &str = include_str!("../../../schemas/weave-world-full-v1.schema.json");
const CHECKED_COMPACT_SCHEMA: &str =
    include_str!("../../../schemas/weave-world-compact-v1.schema.json");

#[test]
fn composes_broad_regional_and_ecosystem_layers_deterministically() {
    let manifest = manifest();
    let inputs = packs();
    let originals = inputs.clone();
    let plan = exact_plan(WorldConflictPolicy::Replace);

    let first = compose_world_pack(&plan, &manifest, inputs.clone(), &weave_version())
        .expect("first composition");
    let second =
        compose_world_pack(&plan, &manifest, inputs, &weave_version()).expect("second composition");

    assert_eq!(first, second);
    assert_eq!(
        originals,
        packs(),
        "composition must not mutate input packs"
    );
    assert_eq!(
        first
            .pack
            .dependencies
            .iter()
            .map(|dependency| (dependency.pack_id.as_str(), dependency.version.as_str()))
            .collect::<Vec<_>>(),
        [
            ("aotearoa_new_zealand", "=1.1.0"),
            ("british_columbia_temperate_forest", "=1.1.0"),
            ("hokkaido_japan", "=1.1.0"),
        ]
    );
    assert!(first.receipt.differences.iter().any(|difference| {
        difference.path == "seed.climate.band" && difference.incoming_layer_id == "regional_climate"
    }));
    assert!(first.receipt.differences.iter().any(|difference| {
        difference.path == "seed.primary_biome"
            && difference.incoming_layer_id == "ecosystem_surface"
    }));
    assert!(
        first
            .pack
            .provenance
            .sources
            .iter()
            .any(|source| { source.id == "composition_plan" })
    );
    assert!(
        first
            .pack
            .provenance
            .transformations
            .iter()
            .any(|transformation| transformation.id == "world_composition")
    );
}

#[test]
fn reject_and_keep_conflict_policies_are_explicit() {
    let manifest = manifest();
    let mut reject = exact_plan(WorldConflictPolicy::Replace);
    reject.layers[1].conflict = WorldConflictPolicy::Reject;
    assert!(matches!(
        compose_world_pack(&reject, &manifest, packs(), &weave_version()),
        Err(WorldCompositionError::Conflict { path }) if path == "seed.climate.annual_mean_temperature_c.maximum"
    ));

    let mut keep = exact_plan(WorldConflictPolicy::Replace);
    keep.layers[1].conflict = WorldConflictPolicy::Keep;
    let composed =
        compose_world_pack(&keep, &manifest, packs(), &weave_version()).expect("keep composition");
    assert!(composed.receipt.differences.iter().any(|difference| {
        difference.path == "seed.climate.band"
            && difference.resolution == weave_world::WorldLayerResolution::Kept
    }));
    assert_eq!(
        domain_at(&composed.pack, &["seed", "climate", "band"]),
        domain_at(&packs()[0], &["seed", "climate", "band"])
    );
}

#[test]
fn seeded_candidate_selection_is_stable_and_seed_sensitive() {
    let manifest = manifest();
    let mut plan = exact_plan(WorldConflictPolicy::Replace);
    plan.layers.truncate(1);
    plan.layers[0].selection = WorldLayerSelection::Seeded;
    plan.layers[0].candidates = vec![selector("aotearoa_new_zealand"), selector("hokkaido_japan")];

    let selected = (0..512)
        .map(|seed| {
            plan.random_seed = seed;
            let first = compose_world_pack(&plan, &manifest, packs(), &weave_version())
                .expect("seeded composition");
            let second = compose_world_pack(&plan, &manifest, packs(), &weave_version())
                .expect("repeat seeded composition");
            assert_eq!(first, second);
            first.receipt.layers[0].selected_candidate
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(selected, [0, 1].into_iter().collect());

    plan.random_seed = 73;
    let composed = compose_world_pack(&plan, &manifest, packs(), &weave_version())
        .expect("reproducible seeded receipt");
    let mut tampered = composed.receipt;
    tampered.layers[0].selected_candidate = 1 - tampered.layers[0].selected_candidate;
    assert!(tampered.to_json().is_err());
}

#[test]
fn one_pack_identity_cannot_collapse_two_selected_versions() {
    let manifest = manifest();
    let broad = DomainPack::from_json(BROAD).expect("broad pack");
    let mut newer = broad.clone();
    newer.version = "1.2.0".to_owned();
    let mut plan = exact_plan(WorldConflictPolicy::Replace);
    plan.layers = vec![
        WorldLayerSpec {
            id: "broad_identity".to_owned(),
            role: WorldLayerRole::Broad,
            candidates: vec![selector("aotearoa_new_zealand")],
            selection: WorldLayerSelection::Exact,
            include: vec!["seed.identity".to_owned()],
            conflict: WorldConflictPolicy::Reject,
        },
        WorldLayerSpec {
            id: "regional_climate".to_owned(),
            role: WorldLayerRole::Regional,
            candidates: vec![WorldPackSelector {
                pack_id: "aotearoa_new_zealand".to_owned(),
                version: "1.2.0".to_owned(),
            }],
            selection: WorldLayerSelection::Exact,
            include: vec!["seed.climate".to_owned()],
            conflict: WorldConflictPolicy::Replace,
        },
    ];
    assert!(matches!(
        compose_world_pack(&plan, &manifest, [broad, newer], &weave_version()),
        Err(WorldCompositionError::InvalidPlan { path, .. }) if path == "layers[1].candidates"
    ));
}

#[test]
fn full_and_compact_exports_round_trip_without_crossing_the_lineage_boundary() {
    let manifest = manifest();
    let composed = compose_world_pack(
        &exact_plan(WorldConflictPolicy::Replace),
        &manifest,
        packs(),
        &weave_version(),
    )
    .expect("composition");
    let catalog = DomainCatalog::from_artifacts(
        [manifest],
        packs().into_iter().chain([composed.pack.clone()]),
    )
    .expect("composed catalog");
    let source =
        AUTHORED_SOURCE.replace("aotearoa_new_zealand@=1.1.0", "glasswind_composed@=1.0.0");
    let compiled = compile_with_modules(&source, &CompileOptions::default(), &catalog)
        .expect("composed source");
    let module = &compiled.domain_modules["world"];
    let (full, compact) = export_world(module, &composed.receipt).expect("portable exports");

    let full_json = full.to_json().expect("full JSON");
    let full_ron = full.to_ron().expect("full RON");
    assert_eq!(
        FullWorldExport::from_json(&full_json).expect("full JSON round trip"),
        full
    );
    assert_eq!(
        FullWorldExport::from_ron(&full_ron).expect("full RON round trip"),
        full
    );

    let compact_json = compact.to_json().expect("compact JSON");
    let compact_ron = compact.to_ron().expect("compact RON");
    assert_eq!(
        CompactWorldExport::from_json(&compact_json).expect("compact JSON round trip"),
        compact
    );
    assert_eq!(
        CompactWorldExport::from_ron(&compact_ron).expect("compact RON round trip"),
        compact
    );
    for forbidden in [
        "authored_override_paths",
        "composition",
        "origins",
        "source_path",
    ] {
        assert!(!compact_json.contains(forbidden));
        assert!(!compact_ron.contains(forbidden));
    }
}

#[test]
fn malformed_compact_exports_fail_without_panicking() {
    let malformed = r#"{
      "format_version": 1,
      "module_id": "org.weave.world",
      "module_version": "1.1.0",
      "pack_id": "glasswind_composed",
      "pack_version": "1.0.0",
      "roots": ["missing"],
      "places": {}
    }"#;
    assert!(CompactWorldExport::from_json(malformed).is_err());
}

#[test]
fn checked_pack_story_exports_receipts_and_schemas_are_byte_exact() {
    let manifest = manifest();
    let plan = WorldCompositionPlan::from_json(PLAN).expect("checked plan");
    let composed = compose_world_pack(&plan, &manifest, packs(), &weave_version())
        .expect("checked composition");
    assert_eq!(composed.pack.to_json().expect("pack JSON"), CHECKED_PACK);
    assert_eq!(
        composed.receipt.to_json().expect("receipt JSON"),
        CHECKED_RECEIPT_JSON
    );
    assert_eq!(
        composed.receipt.to_ron().expect("receipt RON"),
        CHECKED_RECEIPT_RON
    );

    let catalog = DomainCatalog::from_artifacts(
        [manifest],
        packs().into_iter().chain([composed.pack.clone()]),
    )
    .expect("checked catalog");
    let compiled = compile_with_modules(
        COMPOSED_SOURCE,
        &CompileOptions {
            source_name: Some(
                "examples/domain-modules/weave-world/composed-setting.weave".to_owned(),
            ),
        },
        &catalog,
    )
    .expect("checked source");
    assert_eq!(
        to_json(&compiled.story).expect("story JSON"),
        CHECKED_STORY_JSON
    );
    assert_eq!(
        to_ron(&compiled.story).expect("story RON"),
        CHECKED_STORY_RON
    );

    let (full, compact) =
        export_world(&compiled.domain_modules["world"], &composed.receipt).expect("World exports");
    assert_eq!(full.to_json().expect("full JSON"), CHECKED_FULL_JSON);
    assert_eq!(full.to_ron().expect("full RON"), CHECKED_FULL_RON);
    assert_eq!(
        compact.to_json().expect("compact JSON"),
        CHECKED_COMPACT_JSON
    );
    assert_eq!(compact.to_ron().expect("compact RON"), CHECKED_COMPACT_RON);
    assert_eq!(
        world_composition_schema().expect("composition schema"),
        CHECKED_COMPOSITION_SCHEMA
    );
    assert_eq!(
        full_world_schema().expect("full schema"),
        CHECKED_FULL_SCHEMA
    );
    assert_eq!(
        compact_world_schema().expect("compact schema"),
        CHECKED_COMPACT_SCHEMA
    );
}

fn exact_plan(regional_conflict: WorldConflictPolicy) -> WorldCompositionPlan {
    let mut plan = WorldCompositionPlan::from_json(PLAN).expect("checked composition plan");
    plan.layers[1].conflict = regional_conflict;
    plan
}

fn selector(pack_id: &str) -> WorldPackSelector {
    WorldPackSelector {
        pack_id: pack_id.to_owned(),
        version: "1.1.0".to_owned(),
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(MANIFEST).expect("world manifest")
}

fn packs() -> Vec<DomainPack> {
    [BROAD, ECOSYSTEM, REGIONAL]
        .into_iter()
        .map(|source| DomainPack::from_json(source).expect("world pack"))
        .collect()
}

fn weave_version() -> Version {
    Version::new(0, 1, 0)
}

fn domain_at<'a>(pack: &'a DomainPack, path: &[&str]) -> &'a weave_domain::DomainValue {
    let (root, remaining) = path.split_first().expect("non-empty domain path");
    let mut value = &pack.values[*root];
    for field in remaining {
        let weave_domain::DomainValue::Object(object) = value else {
            panic!("domain path traverses a scalar");
        };
        value = &object[*field];
    }
    value
}
