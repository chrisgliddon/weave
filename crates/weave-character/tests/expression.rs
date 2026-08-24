use weave_character::{
    CharacterExtension, CharacterProfile, ExpressionAssignmentReceipt, ExpressionAssignmentRequest,
    ExpressionDiagnosticCode, ExpressionDiagnosticSeverity, ExpressionPack, ExpressionRecordOrigin,
    ExpressionResolution, ExpressionResolutionRequest, ExpressionRevision, ReviewState,
    assign_expression_pack, expression_pack_fingerprint, expression_pack_schema,
    expression_profile_fingerprint, inspect_expression_coverage, lint_expression,
    normalize_expression_revision, resolve_expression_dialogue, validate_expression_pack,
    validate_expression_profile,
};

const PACK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/glasswind.expression-pack.json"
);
const PACK_RON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/glasswind.expression-pack.ron"
);
const RAW_REVISION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/raw.expression-revision.json"
);
const NORMALIZED_REVISION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/normalized.expression-revision.json"
);
const REVISED_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/revised.character.json"
);
const REQUEST_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/assignment.expression-request.json"
);
const RECEIPT_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/assignment.expression-receipt.json"
);
const APPLIED_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/applied.character.json"
);
const RESOLUTION_REQUEST_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json"
);
const RESOLUTION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/contextual.expression-resolution.json"
);
const FALLBACK_PROFILE_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/fallback-profile.character.json"
);
const FALLBACK_REQUEST_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/fallback.expression-resolution-request.json"
);
const FALLBACK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/fallback.expression-resolution.json"
);
const INVALID_PACK_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/invalid/restricted-placeholder.expression-pack.json"
);

#[test]
fn pack_and_revision_round_trip_in_json_and_ron_with_canonical_hashes() {
    let json = ExpressionPack::from_json(PACK_JSON).unwrap();
    let ron = ExpressionPack::from_ron(PACK_RON).unwrap();
    assert_eq!(json, ron);
    assert_eq!(json.to_json().unwrap(), PACK_JSON);
    assert_eq!(json.to_ron().unwrap(), PACK_RON);
    assert_eq!(
        expression_pack_fingerprint(&json).unwrap(),
        expression_pack_fingerprint(&ron).unwrap()
    );

    let raw: ExpressionRevision = weave_domain::parse_strict_json(RAW_REVISION_JSON).unwrap();
    let original = raw.clone();
    let normalized = normalize_expression_revision(&raw).unwrap();
    assert_eq!(
        raw, original,
        "normalization must not rewrite its source value"
    );
    assert_eq!(
        normalized,
        ExpressionRevision::from_json(NORMALIZED_REVISION_JSON).unwrap()
    );
    assert_eq!(normalized.to_json().unwrap(), NORMALIZED_REVISION_JSON);

    let schema: serde_json::Value =
        serde_json::from_str(&expression_pack_schema().unwrap()).unwrap();
    assert_eq!(
        schema["$id"],
        "urn:weave:schema:character-expression-pack:1"
    );
}

#[test]
fn reviewed_pack_assignment_replays_exactly_and_meets_declared_coverage() {
    let profile = CharacterProfile::from_json(REVISED_JSON).unwrap();
    let pack = ExpressionPack::from_json(PACK_JSON).unwrap();
    let request = ExpressionAssignmentRequest::from_json(REQUEST_JSON).unwrap();
    let expected = ExpressionAssignmentReceipt::from_json(RECEIPT_JSON).unwrap();
    let receipt = assign_expression_pack(&profile, &pack, &request).unwrap();
    assert_eq!(receipt, expected);
    assert_eq!(receipt.output_profile.to_json().unwrap(), APPLIED_JSON);
    assert_eq!(
        receipt.output_sha256,
        expression_profile_fingerprint(&receipt.output_profile).unwrap()
    );
    validate_expression_profile(&receipt.output_profile, std::slice::from_ref(&pack)).unwrap();
    let coverage =
        inspect_expression_coverage(&receipt.output_profile, std::slice::from_ref(&pack)).unwrap();
    assert!(coverage.diagnostics.is_empty());
    assert!(coverage.scenario_coverage["arrival"].has_fallback);
    assert!(coverage.scenario_coverage["arrival"].reviewed_variant_count >= 4);
}

#[test]
fn authored_context_wins_and_missing_optional_context_uses_the_stable_fallback() {
    let profile = CharacterProfile::from_json(APPLIED_JSON).unwrap();
    let pack = ExpressionPack::from_json(PACK_JSON).unwrap();
    let request = ExpressionResolutionRequest::from_json(RESOLUTION_REQUEST_JSON).unwrap();
    let first = resolve_expression_dialogue(&profile, &pack, &request).unwrap();
    let second = resolve_expression_dialogue(&profile, &pack, &request).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first,
        ExpressionResolution::from_json(RESOLUTION_JSON).unwrap()
    );
    assert_eq!(first.selected_variant_id, "friend_at_stormwatch");
    assert!(!first.fallback_used);
    assert_eq!(
        first.rendered_text,
        "Ari Vale greets Tavi Quill at Stormwatch Gate and names their org.weave.relationship.friend a trailmark."
    );

    let fallback_profile = CharacterProfile::from_json(FALLBACK_PROFILE_JSON).unwrap();
    let fallback_request = ExpressionResolutionRequest::from_json(FALLBACK_REQUEST_JSON).unwrap();
    let fallback =
        resolve_expression_dialogue(&fallback_profile, &pack, &fallback_request).unwrap();
    assert_eq!(
        fallback,
        ExpressionResolution::from_json(FALLBACK_JSON).unwrap()
    );
    assert!(fallback.fallback_used);
    assert_eq!(fallback.selected_variant_id, "fallback");
    assert_eq!(fallback.rendered_text, "Ari Vale checks the route.");
}

#[test]
fn placeholder_failures_are_source_located_and_never_echo_rejected_content() {
    let profile = CharacterProfile::from_json(APPLIED_JSON).unwrap();
    let valid_pack = ExpressionPack::from_json(PACK_JSON).unwrap();
    let invalid: ExpressionPack = weave_domain::parse_strict_json(INVALID_PACK_JSON).unwrap();
    let failure = validate_expression_pack(&invalid).unwrap_err();
    assert_eq!(
        failure.diagnostic().code,
        ExpressionDiagnosticCode::UnsafePersonalization
    );
    assert_eq!(failure.diagnostic().source.as_ref().unwrap().line, 6);

    let rejected = "credential-shaped-test-value";
    let mut malformed = valid_pack.clone();
    let variant = malformed
        .templates
        .get_mut("arrival_greeting")
        .unwrap()
        .variants
        .get_mut("fallback")
        .unwrap();
    variant.content = format!("{{{{speaker_name}}}} {{{{{rejected}");
    variant.source.line = 7;
    let failure = validate_expression_pack(&malformed).unwrap_err();
    assert_eq!(
        failure.diagnostic().code,
        ExpressionDiagnosticCode::InvalidPlaceholder
    );
    assert_eq!(failure.diagnostic().source.as_ref().unwrap().line, 7);
    assert!(!failure.to_string().contains(rejected));

    let mut unknown = valid_pack.clone();
    let variant = unknown
        .templates
        .get_mut("arrival_greeting")
        .unwrap()
        .variants
        .get_mut("fallback")
        .unwrap();
    variant.content = "{{speaker_name}} checks {{unknown_field}}.".to_owned();
    variant.source.line = 8;
    let failure = validate_expression_pack(&unknown).unwrap_err();
    assert_eq!(
        failure.diagnostic().code,
        ExpressionDiagnosticCode::UnresolvedToken
    );
    assert_eq!(failure.diagnostic().source.as_ref().unwrap().line, 8);

    let mut unavailable = valid_pack.clone();
    unavailable
        .templates
        .get_mut("arrival_greeting")
        .unwrap()
        .placeholders
        .get_mut("listener_name")
        .unwrap()
        .required = true;
    let mut assigned_profile = profile;
    let expression = match assigned_profile
        .extensions
        .get_mut("org.weave.character.expression")
        .unwrap()
    {
        CharacterExtension::Expression(value) => &mut value.value,
        _ => panic!("expression extension changed kind"),
    };
    let old_ref = expression.template_assignments["arrival_greeting"]
        .pack
        .clone();
    let new_ref = weave_character::expression_pack_ref(&unavailable).unwrap();
    expression
        .source_pack_refs
        .retain(|value| value != &old_ref);
    expression.source_pack_refs.push(new_ref.clone());
    expression.source_pack_refs.sort();
    expression
        .template_assignments
        .get_mut("arrival_greeting")
        .unwrap()
        .pack = new_ref;
    let request = ExpressionResolutionRequest::from_json(FALLBACK_REQUEST_JSON).unwrap();
    let failure =
        resolve_expression_dialogue(&assigned_profile, &unavailable, &request).unwrap_err();
    assert_eq!(
        failure.diagnostic().code,
        ExpressionDiagnosticCode::UnavailablePlaceholder
    );
    assert_eq!(failure.diagnostic().source.as_ref().unwrap().line, 1);
}

#[test]
fn lint_reports_conflicts_unreviewed_text_and_duplicates_without_mutation() {
    let mut profile = CharacterProfile::from_json(APPLIED_JSON).unwrap();
    let mut pack = ExpressionPack::from_json(PACK_JSON).unwrap();
    let expression = match profile
        .extensions
        .get_mut("org.weave.character.expression")
        .unwrap()
    {
        CharacterExtension::Expression(value) => &mut value.value,
        _ => panic!("expression extension changed kind"),
    };
    let mut conflict = expression.voice_constraints["avoid_crowded_metaphors"].clone();
    conflict.id = "prefer_crowded_metaphors".to_owned();
    conflict.effect = weave_character::ExpressionConstraintEffect::Prefer;
    expression
        .voice_constraints
        .insert(conflict.id.clone(), conflict);
    let mut suggestion = expression.lexicon["trailmark"].clone();
    suggestion.id = "suggested_trailmark".to_owned();
    suggestion.origin = ExpressionRecordOrigin::Suggested;
    suggestion.review = ReviewState::Pending;
    suggestion.rationale = Some("Pending synthetic editorial suggestion.".to_owned());
    expression.lexicon.insert(suggestion.id.clone(), suggestion);

    let template = pack.templates.get_mut("arrival_greeting").unwrap();
    let mut duplicate = template.variants["stormwatch_arrival"].clone();
    duplicate.id = "stormwatch_duplicate".to_owned();
    template.variants.insert(duplicate.id.clone(), duplicate);
    let mut near_duplicate = template.variants["friend_at_stormwatch"].clone();
    near_duplicate.id = "friend_at_stormwatch_near_duplicate".to_owned();
    near_duplicate.content.push_str(" Today.");
    template
        .variants
        .insert(near_duplicate.id.clone(), near_duplicate);

    let profile_before = profile.clone();
    let pack_before = pack.clone();
    let report = lint_expression(&profile, std::slice::from_ref(&pack)).unwrap();
    assert_eq!(profile, profile_before);
    assert_eq!(pack, pack_before);
    for code in [
        ExpressionDiagnosticCode::ConflictingConstraint,
        ExpressionDiagnosticCode::UnreviewedSuggestion,
        ExpressionDiagnosticCode::DuplicateVariant,
        ExpressionDiagnosticCode::NearDuplicateVariant,
    ] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code)
        );
    }
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == ExpressionDiagnosticCode::UnreviewedSuggestion
            && diagnostic.severity == ExpressionDiagnosticSeverity::Warning
    }));
}
