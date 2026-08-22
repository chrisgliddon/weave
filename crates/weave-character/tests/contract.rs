use weave_character::{
    CharacterDiagnosticCode, CharacterExtension, CharacterOperationAction, CharacterOverlay,
    CharacterProfile, CharacterSynthesisResult, CharacterTemplate, HexacoProfile, LockState,
    ReviewState, TraitBand, TraitMeasurement, ValueState, character_diagnostic_schema,
    character_overlay_schema, character_profile_schema, character_synthesis_schema,
    character_template_schema, synthesize_character,
};

const PROFILE_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.json");
const PROFILE_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.ron");
const TEMPLATE_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/template.character.json");
const TEMPLATE_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/template.character.ron");
const OVERLAY_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/overlay.character.json");
const OVERLAY_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/overlay.character.ron");
const SYNTHESIS_JSON: &str =
    include_str!("../../../examples/domain-modules/weave-character/synthesis.character.json");
const SYNTHESIS_RON: &str =
    include_str!("../../../examples/domain-modules/weave-character/synthesis.character.ron");
const OMITTED_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/omitted-extensions.character.json"
);
const OMITTED_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/omitted-extensions.character.ron"
);
const UNKNOWN_EXTENSION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/unknown-extension-preserved.character.json"
);
const UNKNOWN_EXTENSION_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/unknown-extension-preserved.character.ron"
);
const UNKNOWN_PROFILE_VERSION: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/unknown-profile-version.character.json"
);
const CONFLICTING_OVERLAY: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/conflicting-overlay.character.json"
);
const STALE_OVERLAY: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/stale-overlay.character.json"
);
const INVALID_REFERENCE: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/invalid-reference.character.json"
);
const UNKNOWN_TYPED_EXTENSION: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/unknown-typed-extension-version.character.json"
);
const DERIVED_CANON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/invalid/derived-canonical-evidence.character.json"
);
const PROFILE_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-profile-v1.schema.json");
const TEMPLATE_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-template-v1.schema.json");
const OVERLAY_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-overlay-v1.schema.json");
const SYNTHESIS_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-synthesis-v1.schema.json");
const DIAGNOSTIC_SCHEMA: &str =
    include_str!("../../../schemas/weave-character-diagnostic-v1.schema.json");

#[test]
fn all_six_factors_and_twenty_four_facets_round_trip_exactly() {
    let json = CharacterProfile::from_json(PROFILE_JSON).expect("profile JSON");
    let ron = CharacterProfile::from_ron(PROFILE_RON).expect("profile RON");
    assert_eq!(json, ron);
    assert_complete_hexaco(&json.canon.personality);
    assert_eq!(json.derived.ocean.openness.as_ref().unwrap().score, 0.83);
    assert_eq!(
        json.derived.ocean.conscientiousness.as_ref().unwrap().score,
        0.78
    );
    assert_eq!(
        json.derived.ocean.extraversion.as_ref().unwrap().score,
        0.68
    );
    assert_eq!(
        json.derived.ocean.agreeableness.as_ref().unwrap().score,
        0.63
    );
    assert_eq!(json.derived.ocean.neuroticism.as_ref().unwrap().score, 0.57);
    assert!(json.derived.ocean.lossy);
    assert!(!json.derived.ocean.independent_evidence);
}

#[test]
fn template_and_overlay_remain_separate_and_synthesize_deterministically() {
    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("template");
    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).expect("overlay");
    let original_template = template.clone();
    let original_overlay = overlay.clone();
    let first = synthesize_character(Some(&template), &overlay).expect("first synthesis");
    let second = synthesize_character(Some(&template), &overlay).expect("second synthesis");
    assert_eq!(first, second);
    assert_eq!(template, original_template);
    assert_eq!(overlay, original_overlay);
    assert_eq!(
        first.effective_profile.canon.identity.display_name.value,
        "Ari Vale of Glasswind"
    );
    assert!(matches!(
        first
            .effective_profile
            .canon
            .personality
            .conscientiousness
            .prudence
            .as_ref()
            .unwrap()
            .value,
        TraitMeasurement::Band {
            band: TraitBand::VeryHigh
        }
    ));
    assert!(matches!(
        first
            .effective_profile
            .extensions
            .get("org.weave.character.future_palette"),
        Some(CharacterExtension::Opaque(record)) if record.header.extension_version == 99
    ));
    assert!(first.origins.contains_key("canon.identity.display_name"));
    assert!(
        first
            .origins
            .contains_key("canon.personality.conscientiousness.prudence")
    );
}

#[test]
fn locks_precedence_fingerprints_and_blank_creation_fail_closed() {
    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("template");
    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).expect("overlay");

    let mut locked = overlay.clone();
    let CharacterOperationAction::SetHexacoTrait { value, .. } = &mut locked.operations[1].action
    else {
        panic!("fixture operation changed")
    };
    value.state = ValueState::Authored;
    value.review = ReviewState::Accepted;
    assert_eq!(
        synthesize_character(Some(&template), &locked)
            .expect_err("locked field")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::LockedField
    );

    let mut lower_precedence = overlay.clone();
    let CharacterOperationAction::SetDisplayName { value } =
        &mut lower_precedence.operations[0].action
    else {
        panic!("fixture operation changed")
    };
    value.state = ValueState::Imported;
    value.review = ReviewState::Accepted;
    assert_eq!(
        synthesize_character(Some(&template), &lower_precedence)
            .expect_err("lower precedence")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::ConflictingOverlay
    );

    let mut blank = overlay;
    blank.id = "org.weave.character.overlay.blank_ari".to_owned();
    blank.template = None;
    blank.operations.truncate(1);
    blank.operations[0].expected_prior_sha256 = None;
    let CharacterOperationAction::SetDisplayName { value } = &mut blank.operations[0].action else {
        panic!("fixture operation changed")
    };
    value.state = ValueState::Authored;
    value.review = ReviewState::NotRequired;
    value.lock = LockState::Unlocked;
    value.rationale = None;
    let blank_result = synthesize_character(None, &blank).expect("blank synthesis");
    assert!(blank_result.template.is_none());
    assert_eq!(
        blank_result
            .effective_profile
            .canon
            .identity
            .display_name
            .value,
        "Ari Vale of Glasswind"
    );
}

#[test]
fn invalid_and_forward_compatible_fixtures_enforce_the_boundary() {
    assert_code_profile(
        UNKNOWN_PROFILE_VERSION,
        CharacterDiagnosticCode::UnsupportedVersion,
    );
    assert_eq!(
        CharacterOverlay::from_json(CONFLICTING_OVERLAY)
            .expect_err("conflicting overlay")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::ConflictingOverlay
    );
    assert_code_profile(INVALID_REFERENCE, CharacterDiagnosticCode::InvalidReference);
    assert_code_profile(
        UNKNOWN_TYPED_EXTENSION,
        CharacterDiagnosticCode::UnsupportedVersion,
    );
    assert_code_profile(DERIVED_CANON, CharacterDiagnosticCode::ForbiddenWriteBack);

    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("template");
    let stale = CharacterOverlay::from_json(STALE_OVERLAY).expect("structurally valid overlay");
    assert_eq!(
        synthesize_character(Some(&template), &stale)
            .expect_err("stale template")
            .diagnostic()
            .code,
        CharacterDiagnosticCode::StaleInput
    );

    let omitted_json = CharacterProfile::from_json(OMITTED_JSON).expect("omitted JSON");
    let omitted_ron = CharacterProfile::from_ron(OMITTED_RON).expect("omitted RON");
    assert_eq!(omitted_json, omitted_ron);
    assert!(omitted_json.extensions.is_empty());
    assert!(omitted_json.suggestions.is_empty());

    let opaque_json =
        CharacterProfile::from_json(UNKNOWN_EXTENSION_JSON).expect("preserved opaque JSON");
    let opaque_ron =
        CharacterProfile::from_ron(UNKNOWN_EXTENSION_RON).expect("preserved opaque RON");
    assert_eq!(opaque_json, opaque_ron);
    assert!(matches!(
        opaque_json
            .extensions
            .get("org.weave.character.future_palette"),
        Some(CharacterExtension::Opaque(record)) if record.header.extension_version == 99
    ));
}

#[test]
fn strict_json_rejects_duplicates_without_disclosing_values() {
    let duplicate = PROFILE_JSON.replacen('{', "{\n  \"profile_format_version\": 1,", 1);
    let error = CharacterProfile::from_json(&duplicate).expect_err("duplicate key");
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(!error.to_string().contains("Ari Vale"));
}

#[test]
fn checked_contract_artifacts_are_byte_exact() {
    let profile = CharacterProfile::from_json(PROFILE_JSON).expect("profile");
    assert_eq!(profile.to_json().unwrap(), PROFILE_JSON);
    assert_eq!(profile.to_ron().unwrap(), PROFILE_RON);

    let template = CharacterTemplate::from_json(TEMPLATE_JSON).expect("template");
    assert_eq!(template.to_json().unwrap(), TEMPLATE_JSON);
    assert_eq!(template.to_ron().unwrap(), TEMPLATE_RON);

    let overlay = CharacterOverlay::from_json(OVERLAY_JSON).expect("overlay");
    assert_eq!(overlay.to_json().unwrap(), OVERLAY_JSON);
    assert_eq!(overlay.to_ron().unwrap(), OVERLAY_RON);

    let synthesis = CharacterSynthesisResult::from_json(SYNTHESIS_JSON).expect("synthesis JSON");
    assert_eq!(
        CharacterSynthesisResult::from_ron(SYNTHESIS_RON).expect("synthesis RON"),
        synthesis
    );
    assert_eq!(synthesis.to_json().unwrap(), SYNTHESIS_JSON);
    assert_eq!(synthesis.to_ron().unwrap(), SYNTHESIS_RON);

    assert_eq!(character_profile_schema().unwrap(), PROFILE_SCHEMA);
    assert_eq!(character_template_schema().unwrap(), TEMPLATE_SCHEMA);
    assert_eq!(character_overlay_schema().unwrap(), OVERLAY_SCHEMA);
    assert_eq!(character_synthesis_schema().unwrap(), SYNTHESIS_SCHEMA);
    assert_eq!(character_diagnostic_schema().unwrap(), DIAGNOSTIC_SCHEMA);
    assert!(PROFILE_SCHEMA.contains("aesthetic_appreciation"));
    assert!(PROFILE_SCHEMA.contains("unconventionality"));
    assert!(PROFILE_SCHEMA.contains("canonical_personality_write_back"));
}

fn assert_code_profile(source: &str, code: CharacterDiagnosticCode) {
    assert_eq!(
        CharacterProfile::from_json(source)
            .expect_err("invalid profile")
            .diagnostic()
            .code,
        code
    );
}

fn assert_complete_hexaco(profile: &HexacoProfile) {
    let honesty_humility = &profile.honesty_humility;
    assert!(honesty_humility.factor.is_some());
    assert!(honesty_humility.sincerity.is_some());
    assert!(honesty_humility.fairness.is_some());
    assert!(honesty_humility.greed_avoidance.is_some());
    assert!(honesty_humility.modesty.is_some());
    let emotionality = &profile.emotionality;
    assert!(emotionality.factor.is_some());
    assert!(emotionality.fearfulness.is_some());
    assert!(emotionality.anxiety.is_some());
    assert!(emotionality.dependence.is_some());
    assert!(emotionality.sentimentality.is_some());
    let extraversion = &profile.extraversion;
    assert!(extraversion.factor.is_some());
    assert!(extraversion.social_self_esteem.is_some());
    assert!(extraversion.social_boldness.is_some());
    assert!(extraversion.sociability.is_some());
    assert!(extraversion.liveliness.is_some());
    let agreeableness = &profile.agreeableness;
    assert!(agreeableness.factor.is_some());
    assert!(agreeableness.forgivingness.is_some());
    assert!(agreeableness.gentleness.is_some());
    assert!(agreeableness.flexibility.is_some());
    assert!(agreeableness.patience.is_some());
    let conscientiousness = &profile.conscientiousness;
    assert!(conscientiousness.factor.is_some());
    assert!(conscientiousness.organization.is_some());
    assert!(conscientiousness.diligence.is_some());
    assert!(conscientiousness.perfectionism.is_some());
    assert!(conscientiousness.prudence.is_some());
    let openness = &profile.openness;
    assert!(openness.factor.is_some());
    assert!(openness.aesthetic_appreciation.is_some());
    assert!(openness.inquisitiveness.is_some());
    assert!(openness.creativity.is_some());
    assert!(openness.unconventionality.is_some());
}
