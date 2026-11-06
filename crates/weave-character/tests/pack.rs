//! Portable Character-pack contract tests.

use std::collections::BTreeMap;

use weave_character::{
    CHARACTER_PACK_FORMAT_VERSION, CHARACTER_PACK_MAX_RECORDS, CharacterAuthoringPack,
    CharacterCollection, CharacterDiagnosticCode, CharacterExtension, CharacterHealthDiagnostic,
    CharacterHealthDiagnosticCode, CharacterHealthDiagnosticScope, CharacterHealthSeverity,
    CharacterOverlay, CharacterPackActiveTabletop, CharacterPackAdapterCapability,
    CharacterPackAdapterCapabilityDeclaration, CharacterPackAdapterCoordinate,
    CharacterPackAdapterDefinition, CharacterPackAdapterManifest, CharacterPackInactiveTabletop,
    CharacterPackLicense, CharacterPackLicenseNotice, CharacterPackMetadata,
    CharacterPackTabletopProjection, CharacterPackTabletopState, CharacterProfile,
    CharacterRuntimePack, CharacterRuntimeProfile, CharacterTemplate, CharacterTemplateRef,
    EXPRESSION_EXTENSION_NAMESPACE, ExpressionPack, ExpressionTextAuthority, Freshness,
    HexacoTrait, RELATIONSHIP_EXTENSION_NAMESPACE, ReviewState, character_authoring_pack_schema,
    character_pack_adapter_manifest_fingerprint, character_pack_adapter_namespace_key,
    character_pack_profile_fingerprint, character_pack_value_fingerprint,
    character_runtime_pack_schema, expression_pack_ref, synthesize_character, template_fingerprint,
    validate_authoring_pack_with_installed, validate_character_pack_adapter_types,
    validate_runtime_pack_with_canonical_profile,
};
use weave_domain::{
    DomainValue, FieldDeclaration, Provenance, ProvenanceKind, ProvenanceSource, TypeExpression,
};

const COLLECTION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/collection.character-collection.json"
);
const EXPRESSION_PACK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/glasswind.expression-pack.json"
);
const TEMPLATE_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/template.character.json");
const OVERLAY_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/overlay.character.json");

const ARI_VALE: &str = "org.weave.character.ari_vale";
const SABLE_REED: &str = "org.weave.character.sable_reed";

fn mit_license() -> CharacterPackLicense {
    CharacterPackLicense {
        license: "MIT".to_owned(),
        license_url: "https://opensource.org/license/mit".to_owned(),
        license_text: "MIT License text authored for this synthetic fixture.".to_owned(),
        notices: Vec::new(),
    }
}

fn cc_by_license(notices: Vec<CharacterPackLicenseNotice>) -> CharacterPackLicense {
    CharacterPackLicense {
        license: "CC-BY-4.0".to_owned(),
        license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
        license_text: "Creative Commons Attribution 4.0 International synthetic fixture text."
            .to_owned(),
        notices,
    }
}

fn metadata(license: CharacterPackLicense) -> CharacterPackMetadata {
    CharacterPackMetadata {
        id: "org.weave.character.pack.glasswind_export".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Glasswind Portable Character Pack".to_owned(),
        description: "Original synthetic pack used to test portable Character exports.".to_owned(),
        license,
    }
}

fn expression_pack() -> ExpressionPack {
    ExpressionPack::from_json(EXPRESSION_PACK_JSON).expect("checked expression fixture")
}

fn authoring_pack() -> CharacterAuthoringPack {
    let collection =
        CharacterCollection::from_json(COLLECTION_JSON).expect("checked collection fixture");
    CharacterAuthoringPack::from_collection(
        metadata(mit_license()),
        &collection,
        vec![expression_pack()],
    )
    .expect("valid authoring pack")
}

fn diagnostic(id: &str) -> CharacterHealthDiagnostic {
    CharacterHealthDiagnostic {
        id: id.to_owned(),
        code: CharacterHealthDiagnosticCode::InvalidDocument,
        severity: CharacterHealthSeverity::Warning,
        scope: CharacterHealthDiagnosticScope::Collection,
        path: "pack".to_owned(),
        source: None,
        explanation: "Synthetic diagnostic used to exercise deterministic ordering.".to_owned(),
        remediation: "Remove the duplicate diagnostic.".to_owned(),
        suppressed_by: None,
    }
}

fn refresh_expression_references(pack: &mut CharacterAuthoringPack, expression: &ExpressionPack) {
    let reference = expression_pack_ref(expression).expect("expression pack fingerprint");
    for entry in &mut pack.characters {
        if let Some(CharacterExtension::Expression(record)) = entry
            .profile
            .extensions
            .get_mut(EXPRESSION_EXTENSION_NAMESPACE)
        {
            for existing in &mut record.value.source_pack_refs {
                if existing.id == reference.id {
                    *existing = reference.clone();
                }
            }
            for assignment in record.value.template_assignments.values_mut() {
                if assignment.pack.id == reference.id {
                    assignment.pack = reference.clone();
                }
            }
        }
    }
}

fn json_contains_key(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key(needle) || map.values().any(|value| json_contains_key(value, needle))
        }
        serde_json::Value::Array(values) => {
            values.iter().any(|value| json_contains_key(value, needle))
        }
        _ => false,
    }
}

fn profile<'a>(pack: &'a CharacterRuntimePack, id: &str) -> &'a CharacterRuntimeProfile {
    pack.characters
        .iter()
        .find(|profile| profile.id == id)
        .expect("fixture character is present")
}

#[test]
fn authoring_pack_round_trips_across_json_and_ron() {
    let pack = authoring_pack();
    let json = pack.to_json().expect("canonical JSON");
    let ron = pack.to_ron().expect("canonical RON");

    let from_json = CharacterAuthoringPack::from_json(&json).expect("JSON parse");
    let from_ron = CharacterAuthoringPack::from_ron(&ron).expect("RON parse");
    assert_eq!(from_json, pack);
    assert_eq!(from_ron, pack);
    assert_eq!(from_json.to_json().expect("re-serialized JSON"), json);
    assert_eq!(from_ron.to_ron().expect("re-serialized RON"), ron);

    assert_eq!(from_json.characters.len(), 2);
    assert_eq!(from_json.expression_packs.len(), 1);
    assert_eq!(from_json.provenance.sources.len(), 6);

    let schema: serde_json::Value =
        serde_json::from_str(&character_authoring_pack_schema().expect("schema")).expect("JSON");
    assert_eq!(schema["$id"], "urn:weave:schema:character-authoring-pack:1");
    let runtime_schema: serde_json::Value =
        serde_json::from_str(&character_runtime_pack_schema().expect("schema")).expect("JSON");
    assert_eq!(
        runtime_schema["$id"],
        "urn:weave:schema:character-runtime-pack:1"
    );
}

#[test]
fn single_profile_pack_is_a_deterministic_collection_of_one() {
    let collection =
        CharacterCollection::from_json(COLLECTION_JSON).expect("checked collection fixture");
    let mut profile = collection
        .characters
        .get(ARI_VALE)
        .expect("fixture character")
        .clone();
    profile.extensions.remove(RELATIONSHIP_EXTENSION_NAMESPACE);

    let pack = CharacterAuthoringPack::from_profile(
        metadata(mit_license()),
        profile,
        vec![expression_pack()],
    )
    .expect("valid single-profile pack");
    assert_eq!(pack.characters.len(), 1);
    let json = pack.to_json().expect("canonical JSON");
    let ron = pack.to_ron().expect("canonical RON");
    assert_eq!(
        CharacterAuthoringPack::from_json(&json).expect("JSON parse"),
        pack
    );
    assert_eq!(
        CharacterAuthoringPack::from_ron(&ron).expect("RON parse"),
        pack
    );
    assert_eq!(
        pack.to_runtime_pack()
            .expect("runtime export")
            .characters
            .len(),
        1
    );
}

#[test]
fn runtime_pack_round_trips_and_omits_authoring_metadata() {
    let authoring = authoring_pack();
    let runtime = authoring.to_runtime_pack().expect("runtime export");
    let json = runtime.to_json().expect("runtime JSON");
    let ron = runtime.to_ron().expect("runtime RON");

    assert_eq!(
        CharacterRuntimePack::from_json(&json).expect("JSON parse"),
        runtime
    );
    assert_eq!(
        CharacterRuntimePack::from_ron(&ron).expect("RON parse"),
        runtime
    );
    assert_eq!(runtime.characters.len(), 2);
    assert_eq!(runtime.license, authoring.license);

    let value: serde_json::Value = serde_json::from_str(&json).expect("runtime JSON value");
    for key in [
        "suggestions",
        "provenance",
        "lineage",
        "rationale",
        "explanation",
        "review",
        "lock",
        "health_diagnostics",
        "input_paths",
        "source_ids",
        "evidence",
        "review_sha256",
        "proposal_sha256",
        "applied_sha256",
        "source_pack_refs",
        "derived",
    ] {
        assert!(
            !json_contains_key(&value, key),
            "runtime pack contains {key}"
        );
    }

    let ari = profile(&runtime, ARI_VALE);
    assert!(ari.personality.contains_key(&HexacoTrait::Sincerity));
    assert!(ari.birth_date.is_some());
    assert!(ari.presentation.is_some());
    assert!(!ari.roles.is_empty());
    assert!(!ari.alignment.is_empty());
    assert!(!ari.relationships.is_empty());
    let expression = ari.expression.as_ref().expect("approved expression data");
    assert!(expression.lexicon.contains_key("trailmark"));
    assert!(expression.dialogue.contains_key("arrival_greeting"));
    assert!(
        expression
            .template_assignments
            .contains_key("arrival_greeting")
    );
}

#[test]
fn runtime_pack_excludes_unreviewed_and_stale_authoring_values() {
    let mut authoring = authoring_pack();

    let expression_pack = &mut authoring.expression_packs[0];
    let template = expression_pack
        .templates
        .get_mut("arrival_greeting")
        .expect("fixture template");
    let mut suggested = template
        .variants
        .get("open_arrival")
        .expect("fixture variant")
        .clone();
    suggested.id = "suggested_variant".to_owned();
    suggested.authority = ExpressionTextAuthority::Suggested;
    suggested.review = ReviewState::Pending;
    template.variants.insert(suggested.id.clone(), suggested);
    let modified_expression = authoring.expression_packs[0].clone();
    refresh_expression_references(&mut authoring, &modified_expression);

    let ari = authoring
        .characters
        .iter_mut()
        .find(|entry| entry.profile.id == ARI_VALE)
        .expect("fixture character");
    if let Some(CharacterExtension::Relationships(record)) = ari
        .profile
        .extensions
        .get_mut(RELATIONSHIP_EXTENSION_NAMESPACE)
    {
        for edge in record.value.edges.values_mut() {
            edge.freshness = Freshness::Stale;
        }
    }
    authoring
        .validate()
        .expect("authoring pack permits staged values");

    let runtime = authoring.to_runtime_pack().expect("runtime export");
    let json = runtime.to_json().expect("runtime JSON");
    assert!(!json.contains("suggested_variant"));
    assert!(!json.contains("night_market_memory"));
    assert!(!json.contains("ComputedAffinity"));

    let ari = profile(&runtime, ARI_VALE);
    let expression = ari.expression.as_ref().expect("approved expression data");
    let dialogue = expression
        .dialogue
        .get("arrival_greeting")
        .expect("approved template");
    assert!(
        dialogue
            .variants
            .iter()
            .all(|variant| variant.authority != ExpressionTextAuthority::Suggested)
    );
    assert!(
        ari.relationships.is_empty(),
        "stale edges are authoring-only"
    );
}

#[test]
fn duplicate_identifiers_fail_in_both_formats() {
    let mut duplicate_characters = authoring_pack();
    duplicate_characters
        .characters
        .push(duplicate_characters.characters[0].clone());
    let error = duplicate_characters
        .validate()
        .expect_err("duplicate character");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ConflictingOverlay
    );

    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("checked template");
    let mut duplicate_templates = authoring_pack();
    duplicate_templates.templates = vec![template.clone(), template];
    let error = duplicate_templates
        .validate()
        .expect_err("duplicate template");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ConflictingOverlay
    );

    let mut duplicate_diagnostics = authoring_pack();
    duplicate_diagnostics.health_diagnostics =
        vec![diagnostic("pack.duplicate"), diagnostic("pack.duplicate")];
    let error = duplicate_diagnostics
        .validate()
        .expect_err("duplicate diagnostic");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ConflictingOverlay
    );

    let mut duplicate_runtime = authoring_pack().to_runtime_pack().expect("runtime export");
    duplicate_runtime
        .characters
        .push(duplicate_runtime.characters[0].clone());
    let error = duplicate_runtime
        .validate()
        .expect_err("duplicate character");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ConflictingOverlay
    );

    let json = authoring_pack().to_json().expect("canonical JSON");
    let needle = "\"pack_format_version\": 1,";
    assert!(json.contains(needle));
    let duplicated = json.replacen(
        needle,
        "\"pack_format_version\": 1,\n  \"pack_format_version\": 1,",
        1,
    );
    let error = CharacterAuthoringPack::from_json(&duplicated).expect_err("duplicate JSON key");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
}

#[test]
fn missing_references_fail_in_both_formats() {
    let mut missing_expression_pack = authoring_pack();
    missing_expression_pack.expression_packs.clear();
    let error = missing_expression_pack
        .validate()
        .expect_err("required expression pack is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_template = authoring_pack();
    let ari = missing_template
        .characters
        .iter_mut()
        .find(|entry| entry.profile.id == ARI_VALE)
        .expect("fixture character");
    if let Some(CharacterExtension::Expression(record)) = ari
        .profile
        .extensions
        .get_mut(EXPRESSION_EXTENSION_NAMESPACE)
    {
        for assignment in record.value.template_assignments.values_mut() {
            assignment.template_id = "missing_greeting".to_owned();
        }
    }
    let error = missing_template
        .validate()
        .expect_err("assigned template is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_receipt = authoring_pack();
    missing_receipt.characters[0].assistance_receipt_ids =
        vec!["org.weave.character.missing_receipt".to_owned()];
    let error = missing_receipt
        .validate()
        .expect_err("assistance receipt is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_target = authoring_pack();
    missing_target
        .characters
        .retain(|entry| entry.profile.id != SABLE_REED);
    let error = missing_target
        .validate()
        .expect_err("relationship target is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_runtime_target = authoring_pack().to_runtime_pack().expect("runtime export");
    missing_runtime_target
        .characters
        .retain(|character| character.id != SABLE_REED);
    let error = missing_runtime_target
        .validate()
        .expect_err("runtime relationship target is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_dialogue = authoring_pack().to_runtime_pack().expect("runtime export");
    missing_dialogue
        .characters
        .iter_mut()
        .find(|character| character.id == ARI_VALE)
        .expect("fixture character")
        .expression
        .as_mut()
        .expect("expression data")
        .dialogue
        .clear();
    let error = missing_dialogue
        .validate()
        .expect_err("assigned dialogue template is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut missing_term = authoring_pack().to_runtime_pack().expect("runtime export");
    missing_term
        .characters
        .iter_mut()
        .find(|character| character.id == ARI_VALE)
        .expect("fixture character")
        .expression
        .as_mut()
        .expect("expression data")
        .lexicon
        .remove("trailmark");
    let error = missing_term
        .validate()
        .expect_err("dialogue placeholder term is absent");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );
}

#[test]
fn unsupported_versions_fail_with_an_actionable_error() {
    let mut authoring = authoring_pack();
    authoring.pack_format_version = CHARACTER_PACK_FORMAT_VERSION + 1;
    let error = authoring
        .validate()
        .expect_err("unsupported authoring version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );
    assert!(error.to_string().contains("version 1"));

    let json = serde_json::to_string_pretty(&authoring).expect("serialize unsupported version");
    let error = CharacterAuthoringPack::from_json(&json).expect_err("unsupported JSON version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );

    let mut runtime = authoring_pack().to_runtime_pack().expect("runtime export");
    runtime.pack_format_version = CHARACTER_PACK_FORMAT_VERSION + 6;
    let error = runtime.validate().expect_err("unsupported runtime version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );
    let json = serde_json::to_string_pretty(&runtime).expect("serialize unsupported version");
    let error = CharacterRuntimePack::from_json(&json).expect_err("unsupported JSON version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );
}

#[test]
fn migration_normalizes_order_and_keeps_the_licence() {
    let sorted = authoring_pack();
    let mut shuffled = sorted.clone();
    shuffled.characters.reverse();
    shuffled.expression_packs.reverse();
    shuffled.normalize();
    assert_eq!(shuffled, sorted);
    assert_eq!(
        shuffled.to_json().expect("canonical JSON"),
        sorted.to_json().expect("canonical JSON")
    );

    let migrated = shuffled.migrate().expect("current-version migration");
    assert_eq!(migrated.pack_format_version, CHARACTER_PACK_FORMAT_VERSION);
    assert_eq!(migrated, sorted);

    let runtime = migrated.to_runtime_pack().expect("runtime export");
    assert_eq!(runtime.pack_format_version, CHARACTER_PACK_FORMAT_VERSION);
    assert_eq!(runtime.license, sorted.license);
    assert!(
        runtime
            .to_json()
            .expect("runtime JSON")
            .contains("MIT License text")
    );

    let mut unordered_runtime = runtime.clone();
    unordered_runtime.characters.reverse();
    for character in &mut unordered_runtime.characters {
        if let Some(expression) = &mut character.expression {
            for template in expression.dialogue.values_mut() {
                template.variants.reverse();
            }
        }
    }
    unordered_runtime.normalize();
    assert_eq!(unordered_runtime, runtime);
    let runtime_migrated = unordered_runtime
        .migrate()
        .expect("current-version runtime migration");
    assert_eq!(runtime_migrated, runtime);
}

#[test]
fn separately_licensed_material_keeps_its_own_text() {
    let mut collection =
        CharacterCollection::from_json(COLLECTION_JSON).expect("checked collection fixture");
    let component = "glasswind_presentation_cc_by";
    for character in collection.characters.values_mut() {
        character.provenance.sources.push(ProvenanceSource {
            id: component.to_owned(),
            kind: ProvenanceKind::PublicSource,
            url: "https://example.org/synthetic-presentation".to_owned(),
            revision: "fixture-1".to_owned(),
            sha256: Some("a".repeat(64)),
            license: "CC-BY-4.0".to_owned(),
            license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            attribution: "Synthetic presentation fixture author.".to_owned(),
            modified: true,
        });
        character
            .provenance
            .sources
            .sort_by_key(|source| source.id.clone());
        character.provenance.claims.insert(
            "presentation.synthetic_asset".to_owned(),
            vec![component.to_owned()],
        );
    }

    let without_notice = CharacterAuthoringPack::from_collection(
        metadata(cc_by_license(Vec::new())),
        &collection,
        vec![expression_pack()],
    )
    .expect_err("separately licensed source requires a notice");
    assert_eq!(
        without_notice.diagnostic().code,
        CharacterDiagnosticCode::InvalidLineage
    );

    let notice = CharacterPackLicenseNotice {
        id: component.to_owned(),
        component: component.to_owned(),
        license: "CC-BY-4.0".to_owned(),
        license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
        text: "Creative Commons Attribution 4.0 International fixture notice text.".to_owned(),
    };
    let authoring = CharacterAuthoringPack::from_collection(
        metadata(cc_by_license(vec![notice.clone()])),
        &collection,
        vec![expression_pack()],
    )
    .expect("notices satisfy the licence boundary");
    assert_eq!(authoring.license.license, "CC-BY-4.0");
    assert_eq!(authoring.license.notices, vec![notice]);

    let runtime = authoring.to_runtime_pack().expect("runtime export");
    assert_eq!(runtime.license.license, "CC-BY-4.0");
    assert_ne!(runtime.license.license, "MIT");
    assert!(
        runtime
            .license
            .license_text
            .contains("Attribution 4.0 International")
    );
    let json = runtime.to_json().expect("runtime JSON");
    assert!(json.contains("CC-BY-4.0"));
    assert!(json.contains("fixture notice text"));
}

#[test]
fn template_and_override_lineage_is_independently_reproduced() {
    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("checked template");
    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).expect("checked overlay");
    let fingerprint = template_fingerprint(&template).expect("template fingerprint");
    let effective = synthesize_character(Some(&template), &overlay)
        .expect("checked synthesis")
        .effective_profile;

    let mut authoring = authoring_pack();
    let entry = authoring
        .characters
        .iter_mut()
        .find(|entry| entry.profile.id == ARI_VALE)
        .expect("fixture character");
    entry.profile = effective.clone();
    entry.template = Some(CharacterTemplateRef {
        id: template.id.clone(),
        version: template.version.clone(),
        sha256: fingerprint,
    });
    entry.override_id = Some(overlay.id.clone());
    authoring.templates = vec![template];
    authoring.overrides = vec![overlay];
    authoring.normalize();
    authoring.validate().expect("reproducible template lineage");

    authoring.characters[0]
        .profile
        .canon
        .identity
        .display_name
        .value = "A Different Display Name".to_owned();
    let error = authoring
        .validate()
        .expect_err("profile no longer reproduces from its immutable inputs");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );

    authoring.characters[0].profile = effective;
    authoring.characters[0]
        .template
        .as_mut()
        .expect("template reference")
        .sha256 = "0".repeat(64);
    let error = authoring
        .validate()
        .expect_err("template fingerprint is stale");
    assert_eq!(error.diagnostic().code, CharacterDiagnosticCode::StaleInput);
}

fn duplicate_first_map_entry(ron: &str, marker: &str) -> String {
    let start = ron
        .find(marker)
        .unwrap_or_else(|| panic!("map marker is present: {marker}"));
    let content_start = start + marker.len();
    let length = ron[content_start..]
        .find('\n')
        .expect("map entry line is present");
    let line_end = content_start + length + 1;
    let line = &ron[content_start..line_end];
    format!("{}{}{}", &ron[..line_end], line, &ron[line_end..])
}

fn set_inverse(
    pack: &mut CharacterAuthoringPack,
    character_id: &str,
    edge_id: &str,
    inverse: Option<&str>,
) {
    for entry in &mut pack.characters {
        if entry.profile.id != character_id {
            continue;
        }
        if let Some(CharacterExtension::Relationships(record)) = entry
            .profile
            .extensions
            .get_mut(RELATIONSHIP_EXTENSION_NAMESPACE)
            && let Some(edge) = record.value.edges.get_mut(edge_id)
        {
            edge.inverse_edge_id = inverse.map(str::to_owned);
        }
    }
}

fn install_target_reciprocal_edges(pack: &mut CharacterAuthoringPack) {
    for entry in &mut pack.characters {
        let character_id = entry.profile.id.clone();
        if let Some(CharacterExtension::Relationships(record)) = entry
            .profile
            .extensions
            .get_mut(RELATIONSHIP_EXTENSION_NAMESPACE)
        {
            match character_id.as_str() {
                ARI_VALE => {
                    if let Some(edge) = record.value.edges.get_mut("mentor_sable") {
                        edge.inverse_edge_id = Some("mentor_ari".to_owned());
                    }
                }
                SABLE_REED => {
                    let mut edge = record
                        .value
                        .edges
                        .remove("mentor_sable")
                        .expect("fixture edge");
                    edge.id = "mentor_ari".to_owned();
                    edge.inverse_edge_id = Some("mentor_sable".to_owned());
                    record.value.edges.insert("mentor_ari".to_owned(), edge);
                }
                _ => {}
            }
        }
    }
}

fn install_owner_reciprocal_edges(pack: &mut CharacterAuthoringPack) {
    for entry in &mut pack.characters {
        if entry.profile.id != ARI_VALE {
            continue;
        }
        if let Some(CharacterExtension::Relationships(record)) = entry
            .profile
            .extensions
            .get_mut(RELATIONSHIP_EXTENSION_NAMESPACE)
        {
            let mut first = record
                .value
                .edges
                .get("mentor_sable")
                .expect("fixture edge")
                .clone();
            first.id = "self_a".to_owned();
            first.target_character_id = ARI_VALE.to_owned();
            first.inverse_edge_id = Some("self_b".to_owned());
            let mut second = first.clone();
            second.id = "self_b".to_owned();
            second.inverse_edge_id = Some("self_a".to_owned());
            record.value.edges.insert("self_a".to_owned(), first);
            record.value.edges.insert("self_b".to_owned(), second);
        }
    }
}

#[test]
fn ron_duplicate_map_keys_fail_closed() {
    let authoring = authoring_pack();
    let duplicated =
        duplicate_first_map_entry(&authoring.to_ron().expect("authoring RON"), "colors: {\n");
    let error = CharacterAuthoringPack::from_ron(&duplicated).expect_err("duplicate authoring key");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(
        error.to_string().contains("duplicate map key"),
        "unexpected authoring error: {error}"
    );

    let runtime = authoring.to_runtime_pack().expect("runtime export");
    let duplicated =
        duplicate_first_map_entry(&runtime.to_ron().expect("runtime RON"), "palette: {\n");
    let error = CharacterRuntimePack::from_ron(&duplicated).expect_err("duplicate runtime key");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(error.to_string().contains("duplicate map key"));
}

#[test]
fn inverse_relationship_edges_must_be_reciprocal_and_local() {
    let mut authoring = authoring_pack();
    install_target_reciprocal_edges(&mut authoring);
    authoring
        .validate()
        .expect("target-profile reciprocal edges are accepted");
    let runtime = authoring.to_runtime_pack().expect("runtime export");
    runtime
        .validate()
        .expect("runtime target-profile reciprocal edges are accepted");

    let mut owner = authoring_pack();
    install_owner_reciprocal_edges(&mut owner);
    owner
        .validate()
        .expect("owner-profile reciprocal edges are accepted");
    owner
        .to_runtime_pack()
        .expect("runtime export")
        .validate()
        .expect("runtime owner-profile reciprocal edges are accepted");

    let mut missing = authoring_pack();
    set_inverse(
        &mut missing,
        ARI_VALE,
        "mentor_sable",
        Some("missing_inverse"),
    );
    let error = missing.validate().expect_err("missing inverse edge");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut unpaired = authoring_pack();
    set_inverse(&mut unpaired, ARI_VALE, "mentor_sable", None);
    unpaired
        .validate()
        .expect("an absent inverse id asserts no pairing");

    let mut broken = authoring_pack();
    install_target_reciprocal_edges(&mut broken);
    let mut runtime_broken = broken.to_runtime_pack().expect("runtime export");
    let sable = runtime_broken
        .characters
        .iter_mut()
        .find(|character| character.id == SABLE_REED)
        .expect("fixture character");
    let edge = sable
        .relationships
        .get_mut("mentor_ari")
        .expect("reciprocal runtime edge");
    edge.inverse_edge_id = Some("different_edge".to_owned());
    let error = runtime_broken
        .validate()
        .expect_err("non-reciprocal runtime edge");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );
}

#[test]
fn relative_paths_reject_drive_schemes_and_control_characters() {
    for invalid in ["C:/x", "C:x", "a:b", "a\u{0}b", "a\nb"] {
        let mut runtime = authoring_pack().to_runtime_pack().expect("runtime export");
        let ari = runtime
            .characters
            .iter_mut()
            .find(|character| character.id == ARI_VALE)
            .expect("fixture character");
        let asset = ari
            .presentation
            .as_mut()
            .expect("fixture presentation")
            .assets
            .first_mut()
            .expect("fixture asset");
        asset.path = invalid.to_owned();
        let error = runtime.validate().expect_err("unsafe relative path");
        assert_eq!(
            error.diagnostic().code,
            CharacterDiagnosticCode::InvalidReference,
            "{invalid:?}"
        );
    }

    authoring_pack()
        .to_runtime_pack()
        .expect("runtime export")
        .validate()
        .expect("fixture paths remain valid");
}

#[test]
fn pack_record_caps_fail_closed() {
    let mut over_health = authoring_pack();
    over_health.health_diagnostics = (0..=CHARACTER_PACK_MAX_RECORDS)
        .map(|index| diagnostic(&format!("pack_{index:05}")))
        .collect();
    let error = over_health.validate().expect_err("health diagnostic cap");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(error.to_string().contains("4096"));

    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).expect("checked overlay");
    let mut over_overrides = authoring_pack();
    over_overrides.overrides = vec![overlay; CHARACTER_PACK_MAX_RECORDS + 1];
    let error = over_overrides.validate().expect_err("override cap");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(error.to_string().contains("4096"));
}

#[test]
fn ron_duplicate_keys_compare_decoded_spellings() {
    let mixed = r#"(personality: { openness: 0, "openness": 1 })"#;
    let error = CharacterRuntimePack::from_ron(mixed).expect_err("mixed spelling duplicate");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(error.to_string().contains("duplicate map key"));

    let escaped = r#"(personality: { openness: 0, "open\u{6e}ess": 1 })"#;
    let error = CharacterRuntimePack::from_ron(escaped).expect_err("escaped duplicate");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(error.to_string().contains("duplicate map key"));

    let distinct = r#"(personality: { openness: 0, conscientiousness: 1 })"#;
    let error = CharacterRuntimePack::from_ron(distinct).expect_err("incomplete pack");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(!error.to_string().contains("duplicate map key"));
}

#[test]
fn large_ron_map_scans_in_linear_time() {
    let mut pack = authoring_pack();
    let source_id = pack
        .provenance
        .sources
        .first()
        .expect("fixture provenance source")
        .id
        .clone();
    for index in 0..20_000 {
        pack.provenance.claims.insert(
            format!("bulk.claim.{index:05}.presentation.reference"),
            vec![source_id.clone()],
        );
    }
    let ron = pack.to_ron().expect("large canonical RON");
    assert!(ron.len() >= 1_000_000, "large fixture is at least 1 MiB");

    let started = std::time::Instant::now();
    let parsed = CharacterAuthoringPack::from_ron(&ron).expect("large RON parses");
    let elapsed = started.elapsed();
    assert_eq!(parsed, pack);
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "large RON scan took {elapsed:?}"
    );
}

fn adapter_notice(component: &str, license: &str, text: &str) -> CharacterPackLicenseNotice {
    CharacterPackLicenseNotice {
        id: "plug_and_play_adapter".to_owned(),
        component: component.to_owned(),
        license: license.to_owned(),
        license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
        text: text.to_owned(),
    }
}

fn adapter_provenance() -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: "plug_and_play_adapter".to_owned(),
            kind: ProvenanceKind::PublicSource,
            url: "https://example.org/synthetic-adapter".to_owned(),
            revision: "1.0.0".to_owned(),
            sha256: Some("b".repeat(64)),
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_owned(),
            attribution: "Synthetic adapter fixture author.".to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([(
            "manifest".to_owned(),
            vec!["plug_and_play_adapter".to_owned()],
        )]),
    }
}

fn adapter_manifest() -> CharacterPackAdapterManifest {
    let mut manifest = CharacterPackAdapterManifest {
        manifest_format_version: 1,
        coordinate: CharacterPackAdapterCoordinate {
            id: "org.weave.tabletop.plug_and_play".to_owned(),
            version: "1.0.0".to_owned(),
            schema_version: 1,
            content_sha256: String::new(),
        },
        namespace: "plug_and_play".to_owned(),
        title: "Synthetic Plug-And-Play Adapter".to_owned(),
        capabilities: vec![
            CharacterPackAdapterCapabilityDeclaration {
                capability: CharacterPackAdapterCapability::CharacterCreation,
                version: 1,
            },
            CharacterPackAdapterCapabilityDeclaration {
                capability: CharacterPackAdapterCapability::DerivedValues,
                version: 1,
            },
        ],
        creation_step_ids: vec!["origin".to_owned(), "training".to_owned()],
        definition_type: TypeExpression::String {
            min_length: 1,
            max_length: 64,
        },
        state_type: TypeExpression::Number {
            integer: true,
            minimum: Some(0.0),
            maximum: Some(100.0),
        },
        types: BTreeMap::new(),
        license: CharacterPackLicense {
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_owned(),
            license_text: "CC0 1.0 synthetic adapter licence text.".to_owned(),
            notices: vec![adapter_notice(
                "plug_and_play_adapter",
                "CC0-1.0",
                "CC0 1.0 Universal synthetic adapter notice text.",
            )],
        },
    };
    manifest.coordinate.content_sha256 =
        character_pack_adapter_manifest_fingerprint(&manifest).expect("manifest fingerprint");
    manifest
}

fn tabletop_projection(profile: &CharacterProfile) -> CharacterPackTabletopProjection {
    let manifest = adapter_manifest();
    let definition = DomainValue::String("Wayfinder".to_owned());
    let definition_sha256 =
        character_pack_value_fingerprint(&definition).expect("definition fingerprint");
    let active = CharacterPackAdapterDefinition {
        adapter: manifest.coordinate.clone(),
        definition,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec!["origin".to_owned()],
    };
    let state = CharacterPackTabletopState {
        state_format_version: 1,
        adapter: manifest.coordinate.clone(),
        owner_id: profile.id.clone(),
        definition_sha256,
        revision: 3,
        entropy_algorithm: "sha256_counter_v1".to_owned(),
        entropy_seed: 42,
        entropy_cursor: 5,
        value: DomainValue::Number(12.0),
    };
    CharacterPackTabletopProjection {
        projection_format_version: 1,
        character_id: profile.id.clone(),
        canonical_profile_sha256: character_pack_profile_fingerprint(profile)
            .expect("profile fingerprint"),
        canonical_character_write_back: false,
        manifest,
        provenance: adapter_provenance(),
        active,
        state: Some(state),
        inactive: BTreeMap::new(),
    }
}

fn authoring_pack_with_tabletop() -> CharacterAuthoringPack {
    let mut pack = authoring_pack();
    let entry = pack
        .characters
        .iter_mut()
        .find(|entry| entry.profile.id == ARI_VALE)
        .expect("fixture character");
    entry.tabletop = Some(tabletop_projection(&entry.profile));
    pack.normalize();
    pack.validate().expect("valid tabletop projection");
    pack
}

fn ari_tabletop_mut(pack: &mut CharacterAuthoringPack) -> &mut CharacterPackTabletopProjection {
    pack.characters
        .iter_mut()
        .find(|entry| entry.profile.id == ARI_VALE)
        .and_then(|entry| entry.tabletop.as_mut())
        .expect("fixture tabletop projection")
}

fn repin_adapter_projection(projection: &mut CharacterPackTabletopProjection) {
    let fingerprint =
        character_pack_adapter_manifest_fingerprint(&projection.manifest).expect("fingerprint");
    projection.manifest.coordinate.content_sha256 = fingerprint;
    let coordinate = projection.manifest.coordinate.clone();
    projection.active.adapter = coordinate.clone();
    if let Some(state) = &mut projection.state {
        state.adapter = coordinate;
    }
}

fn ari_runtime_tabletop_mut(pack: &mut CharacterRuntimePack) -> &mut CharacterPackActiveTabletop {
    pack.characters
        .iter_mut()
        .find(|character| character.id == ARI_VALE)
        .and_then(|character| character.tabletop.as_mut())
        .expect("fixture runtime tabletop projection")
}

#[test]
fn tabletop_projection_round_trips_in_json_and_ron() {
    let pack = authoring_pack_with_tabletop();
    let json = pack.to_json().expect("authoring JSON");
    let ron = pack.to_ron().expect("authoring RON");
    assert_eq!(
        CharacterAuthoringPack::from_json(&json).expect("JSON"),
        pack
    );
    assert_eq!(CharacterAuthoringPack::from_ron(&ron).expect("RON"), pack);

    let runtime = pack.to_runtime_pack().expect("runtime export");
    let runtime_json = runtime.to_json().expect("runtime JSON");
    let runtime_ron = runtime.to_ron().expect("runtime RON");
    assert_eq!(
        CharacterRuntimePack::from_json(&runtime_json).expect("JSON"),
        runtime
    );
    assert_eq!(
        CharacterRuntimePack::from_ron(&runtime_ron).expect("RON"),
        runtime
    );

    let ari = profile(&runtime, ARI_VALE);
    let tabletop = ari.tabletop.as_ref().expect("runtime tabletop");
    assert_eq!(tabletop.active.adapter, tabletop.manifest.coordinate);
    assert_eq!(
        tabletop
            .state
            .as_ref()
            .expect("runtime state")
            .definition_sha256,
        tabletop.active.definition_sha256
    );
    assert_eq!(tabletop.manifest.license.license, "CC0-1.0");
    assert!(runtime_json.contains("CC0 1.0 synthetic adapter licence text"));
    let value: serde_json::Value = serde_json::from_str(&runtime_json).expect("JSON value");
    assert!(!json_contains_key(&value, "provenance"));
    assert!(!json_contains_key(&value, "inactive"));
}

fn inactive_tabletop(
    definition_type: TypeExpression,
    definition: DomainValue,
) -> (String, CharacterPackInactiveTabletop) {
    let mut manifest = adapter_manifest();
    manifest.coordinate.version = "0.9.0".to_owned();
    manifest.definition_type = definition_type;
    manifest.coordinate.content_sha256 =
        character_pack_adapter_manifest_fingerprint(&manifest).expect("manifest fingerprint");
    let key = character_pack_adapter_namespace_key(&manifest.coordinate);
    let definition_sha256 =
        character_pack_value_fingerprint(&definition).expect("definition fingerprint");
    (
        key,
        CharacterPackInactiveTabletop {
            manifest,
            provenance: adapter_provenance(),
            definition,
            definition_sha256,
            completed_creation_steps: vec!["origin".to_owned()],
            reason: "Superseded synthetic archive.".to_owned(),
        },
    )
}

#[test]
fn inactive_tabletop_projections_are_authoring_only() {
    let mut pack = authoring_pack_with_tabletop();
    let (key, inactive) = inactive_tabletop(
        TypeExpression::String {
            min_length: 1,
            max_length: 64,
        },
        DomainValue::String("Prior Wayfinder".to_owned()),
    );
    ari_tabletop_mut(&mut pack)
        .inactive
        .insert(key.clone(), inactive);
    pack.normalize();
    pack.validate().expect("authoring archive is valid");
    assert!(pack.to_json().expect("authoring JSON").contains("0.9.0"));

    let runtime = pack.to_runtime_pack().expect("runtime export");
    let runtime_json = runtime.to_json().expect("runtime JSON");
    assert!(profile(&runtime, ARI_VALE).tabletop.is_some());
    assert!(!runtime_json.contains("0.9.0"));
    assert!(!runtime_json.contains(&key));
    let value: serde_json::Value = serde_json::from_str(&runtime_json).expect("JSON value");
    assert!(!json_contains_key(&value, "inactive"));
}

#[test]
fn tabletop_coordinate_mismatch_is_rejected() {
    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).active.adapter.version = "9.9.9".to_owned();
    let error = pack.validate().expect_err("active coordinate mismatch");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );

    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack)
        .manifest
        .coordinate
        .content_sha256 = "0".repeat(64);
    let error = pack.validate().expect_err("content hash mismatch");
    assert_eq!(error.diagnostic().code, CharacterDiagnosticCode::StaleInput);

    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).character_id = SABLE_REED.to_owned();
    let error = pack.validate().expect_err("character mismatch");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );
}

#[test]
fn tabletop_unsupported_schema_version_is_rejected() {
    let mut pack = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut pack);
        projection.manifest.coordinate.schema_version = 2;
        projection.active.adapter.schema_version = 2;
        repin_adapter_projection(projection);
    }
    let error = pack.validate().expect_err("unsupported schema version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );
    assert!(error.to_string().contains("schema version 1"));

    let mut pack = authoring_pack_with_tabletop();
    if let Some(state) = ari_tabletop_mut(&mut pack).state.as_mut() {
        state.state_format_version = 2;
    }
    let error = pack.validate().expect_err("unsupported state version");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::UnsupportedVersion
    );
}

#[test]
fn tabletop_canonical_write_back_is_rejected() {
    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).canonical_character_write_back = true;
    let error = pack.validate().expect_err("authoring write-back");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ForbiddenWriteBack
    );

    let mut runtime = authoring_pack_with_tabletop()
        .to_runtime_pack()
        .expect("runtime export");
    ari_runtime_tabletop_mut(&mut runtime).canonical_character_write_back = true;
    let error = runtime.validate().expect_err("runtime write-back");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::ForbiddenWriteBack
    );
}

#[test]
fn separately_licensed_adapter_requires_its_own_notice() {
    let mut pack = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut pack);
        projection.provenance.sources[0].license = "CC-BY-4.0".to_owned();
        projection.provenance.sources[0].license_url =
            "https://creativecommons.org/licenses/by/4.0/".to_owned();
        projection.manifest.license = CharacterPackLicense {
            license: "CC-BY-4.0".to_owned(),
            license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            license_text: "Creative Commons Attribution 4.0 adapter licence text.".to_owned(),
            notices: Vec::new(),
        };
        repin_adapter_projection(projection);
    }
    let error = pack.validate().expect_err("missing adapter notice");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidLineage
    );

    let mut pack = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut pack);
        projection.provenance.sources[0].license = "CC-BY-4.0".to_owned();
        projection.provenance.sources[0].license_url =
            "https://creativecommons.org/licenses/by/4.0/".to_owned();
        projection.manifest.license = CharacterPackLicense {
            license: "CC-BY-4.0".to_owned(),
            license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            license_text: "Creative Commons Attribution 4.0 adapter licence text.".to_owned(),
            notices: vec![adapter_notice(
                "plug_and_play_adapter",
                "CC-BY-4.0",
                "Creative Commons Attribution 4.0 adapter notice text.",
            )],
        };
        repin_adapter_projection(projection);
    }
    pack.validate()
        .expect("separately licensed adapter is valid");
    let runtime = pack.to_runtime_pack().expect("runtime export");
    let ari = profile(&runtime, ARI_VALE);
    let tabletop = ari.tabletop.as_ref().expect("runtime tabletop");
    assert_eq!(tabletop.manifest.license.license, "CC-BY-4.0");
    assert!(
        runtime
            .to_json()
            .expect("runtime JSON")
            .contains("Attribution 4.0 adapter notice text")
    );
}

#[test]
fn adapter_manifest_fingerprint_is_order_invariant() {
    let manifest = adapter_manifest();
    let mut reversed = manifest.clone();
    reversed.capabilities.reverse();
    reversed.creation_step_ids.reverse();
    assert_eq!(
        character_pack_adapter_manifest_fingerprint(&manifest).expect("fingerprint"),
        character_pack_adapter_manifest_fingerprint(&reversed).expect("fingerprint")
    );

    let mut authoring = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut authoring);
        projection.manifest.capabilities.reverse();
        projection.manifest.creation_step_ids.reverse();
    }
    authoring.normalize();
    authoring
        .validate()
        .expect("normalizing an unordered manifest preserves the pinned hash");
}

#[test]
fn tabletop_type_contract_is_validated() {
    let mut pack = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut pack);
        projection.manifest.types.insert(
            "node".to_owned(),
            TypeExpression::Named {
                name: "node".to_owned(),
            },
        );
        projection.manifest.definition_type = TypeExpression::Named {
            name: "node".to_owned(),
        };
        repin_adapter_projection(projection);
    }
    let error = pack.validate().expect_err("recursive adapter type");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(error.to_string().contains("recursive"));

    let mut pack = authoring_pack_with_tabletop();
    {
        let projection = ari_tabletop_mut(&mut pack);
        projection.manifest.definition_type = TypeExpression::String {
            min_length: 0,
            max_length: 70_000,
        };
        repin_adapter_projection(projection);
    }
    let error = pack.validate().expect_err("unbounded adapter type");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(error.to_string().contains("bounds"));
}

#[test]
fn runtime_tabletop_requires_the_exact_canonical_profile() {
    let authoring = authoring_pack_with_tabletop();
    let canonical = authoring
        .characters
        .iter()
        .find(|entry| entry.profile.id == ARI_VALE)
        .expect("fixture character")
        .profile
        .clone();
    let runtime = authoring.to_runtime_pack().expect("runtime export");
    validate_runtime_pack_with_canonical_profile(&runtime, &canonical)
        .expect("exact canonical profile");

    let mut changed = canonical.clone();
    changed.canon.identity.display_name.value = "Changed Display Name".to_owned();
    let error = validate_runtime_pack_with_canonical_profile(&runtime, &changed)
        .expect_err("stale canonical profile");
    assert_eq!(error.diagnostic().code, CharacterDiagnosticCode::StaleInput);
}

#[test]
fn inactive_archives_use_their_own_manifest_contract() {
    let archived_type = TypeExpression::Number {
        integer: true,
        minimum: Some(0.0),
        maximum: Some(100.0),
    };
    let (key, inactive) = inactive_tabletop(archived_type.clone(), DomainValue::Number(7.0));
    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).inactive.insert(key, inactive);
    pack.normalize();
    pack.validate()
        .expect("archived Number contract validates against its own manifest");

    let (key, mut forged) = inactive_tabletop(archived_type, DomainValue::Number(7.0));
    forged.definition = DomainValue::String("Wayfinder".to_owned());
    forged.definition_sha256 =
        character_pack_value_fingerprint(&forged.definition).expect("definition fingerprint");
    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).inactive.insert(key, forged);
    pack.normalize();
    let error = pack.validate().expect_err("forged archived definition");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
}

fn shared_type_manifest() -> CharacterPackAdapterManifest {
    let mut manifest = adapter_manifest();
    let field = |value_type: TypeExpression| FieldDeclaration {
        value_type,
        required: true,
        description: "Synthetic shared field.".to_owned(),
    };
    let mut types = BTreeMap::from([(
        "t0".to_owned(),
        TypeExpression::String {
            min_length: 0,
            max_length: 64,
        },
    )]);
    for index in 1..=12 {
        let previous = TypeExpression::Named {
            name: format!("t{}", index - 1),
        };
        types.insert(
            format!("t{index}"),
            TypeExpression::Object {
                fields: BTreeMap::from([
                    ("left".to_owned(), field(previous.clone())),
                    ("right".to_owned(), field(previous)),
                ]),
            },
        );
    }
    manifest.definition_type = TypeExpression::Named {
        name: "t12".to_owned(),
    };
    manifest.state_type = TypeExpression::String {
        min_length: 0,
        max_length: 64,
    };
    manifest.types = types;
    manifest
}

#[test]
fn adapter_type_graph_memoizes_shared_types_and_bounds_nodes() {
    let manifest = shared_type_manifest();
    validate_character_pack_adapter_types(&manifest, 100)
        .expect("shared type graph fits a small budget");
    let error = validate_character_pack_adapter_types(&manifest, 10)
        .expect_err("shared type graph exceeds the small budget");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidValue
    );
    assert!(error.to_string().contains("budget"));
}

#[test]
fn installed_adapter_manifests_bind_active_and_archived_entries() {
    let (key, inactive) = inactive_tabletop(
        TypeExpression::String {
            min_length: 1,
            max_length: 64,
        },
        DomainValue::String("Prior Wayfinder".to_owned()),
    );
    let mut pack = authoring_pack_with_tabletop();
    let active_manifest = ari_tabletop_mut(&mut pack).manifest.clone();
    ari_tabletop_mut(&mut pack)
        .inactive
        .insert(key, inactive.clone());
    pack.normalize();
    pack.validate().expect("self-consistent pack");

    validate_authoring_pack_with_installed(
        &pack,
        &[active_manifest.clone(), inactive.manifest.clone()],
    )
    .expect("trusted active and archived manifests");

    let error =
        validate_authoring_pack_with_installed(&pack, std::slice::from_ref(&active_manifest))
            .expect_err("archived manifest is not supplied by the host");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );
    assert!(error.to_string().contains("not installed"));

    let mut untrusted = inactive.manifest.clone();
    untrusted.title = "Different Archived Adapter".to_owned();
    untrusted.coordinate.content_sha256 =
        character_pack_adapter_manifest_fingerprint(&untrusted).expect("fingerprint");
    let error = validate_authoring_pack_with_installed(&pack, &[active_manifest, untrusted])
        .expect_err("archived manifest content hash differs from the trusted entry");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidReference
    );
}

#[test]
fn archived_adapters_require_their_own_licence_notice() {
    let (key, mut inactive) = inactive_tabletop(
        TypeExpression::String {
            min_length: 1,
            max_length: 64,
        },
        DomainValue::String("Prior Wayfinder".to_owned()),
    );
    inactive.manifest.license = CharacterPackLicense {
        license: "CC-BY-4.0".to_owned(),
        license_url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
        license_text: "Creative Commons Attribution 4.0 archived adapter text.".to_owned(),
        notices: Vec::new(),
    };
    inactive.provenance.sources[0].license = "CC-BY-4.0".to_owned();
    inactive.provenance.sources[0].license_url =
        "https://creativecommons.org/licenses/by/4.0/".to_owned();
    inactive.manifest.coordinate.content_sha256 =
        character_pack_adapter_manifest_fingerprint(&inactive.manifest).expect("fingerprint");

    let mut pack = authoring_pack_with_tabletop();
    ari_tabletop_mut(&mut pack).inactive.insert(key, inactive);
    pack.normalize();
    let error = pack
        .validate()
        .expect_err("archived non-MIT adapter requires a notice");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidLineage
    );
}
