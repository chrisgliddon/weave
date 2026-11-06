//! Portable Character-pack contract tests.
//!
//! The sibling `src/pack.rs` module is included here until the crate root exports it. The
//! module aliases the crate root as `weave_character`, so this inclusion compiles it exactly as
//! a library module while exercising the public contract from an integration test.

use ::weave_character::*;

#[allow(dead_code)]
#[path = "../src/pack.rs"]
mod pack;

use pack::{
    CHARACTER_PACK_FORMAT_VERSION, CharacterAuthoringPack, CharacterPackLicense,
    CharacterPackLicenseNotice, CharacterPackMetadata, CharacterRuntimePack,
    character_authoring_pack_schema, character_runtime_pack_schema,
};
use weave_character::{
    CharacterCollection, CharacterDiagnosticCode, CharacterExtension, CharacterHealthDiagnostic,
    CharacterHealthDiagnosticCode, CharacterHealthDiagnosticScope, CharacterHealthSeverity,
    CharacterOverlay, CharacterTemplate, CharacterTemplateRef, EXPRESSION_EXTENSION_NAMESPACE,
    ExpressionPack, ExpressionTextAuthority, Freshness, HexacoTrait,
    RELATIONSHIP_EXTENSION_NAMESPACE, ReviewState, expression_pack_ref, synthesize_character,
    template_fingerprint,
};
use weave_domain::{ProvenanceKind, ProvenanceSource};

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

fn profile<'a>(pack: &'a CharacterRuntimePack, id: &str) -> &'a pack::CharacterRuntimeProfile {
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
