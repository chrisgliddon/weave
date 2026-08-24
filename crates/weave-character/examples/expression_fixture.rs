use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/expression.rs"]
mod expression_support;

use weave_character::{
    CHARACTER_PROFILE_FORMAT_VERSION, CharacterProfile,
    EXPRESSION_ASSIGNMENT_REQUEST_FORMAT_VERSION, EXPRESSION_RESOLUTION_REQUEST_FORMAT_VERSION,
    EXPRESSION_REVISION_FORMAT_VERSION, ExpressionApplicability, ExpressionAssignmentRequest,
    ExpressionContextPredicate, ExpressionDiagnosticCode, ExpressionMutation,
    ExpressionRecordOrigin, ExpressionResolutionRequest, ExpressionRevision,
    ExpressionRuntimeParticipant, ExpressionTermKind, NormalizedExpressionTerm, ReviewState,
    TraitBand, TraitMeasurement, apply_expression_revision, assign_expression_pack,
    expression_assignment_receipt_schema, expression_assignment_request_schema,
    expression_coverage_schema, expression_lint_schema, expression_pack_ref,
    expression_pack_schema, expression_profile_fingerprint, expression_resolution_request_schema,
    expression_resolution_schema, expression_revision_schema, inspect_expression_coverage,
    lint_expression, normalize_expression_revision, recompute_derived, resolve_expression_dialogue,
    validate_expression_profile,
};
use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--check".to_owned());
    let write = match mode.as_str() {
        "--write" => true,
        "--check" => false,
        _ => return Err("expected --write or --check".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root")?
        .to_path_buf();
    let fixture = root.join("examples/domain-modules/weave-character/expression");
    let schemas = root.join("schemas");

    let input = read_profile(
        &root.join("examples/domain-modules/weave-character/omitted-extensions.character.json"),
    )?;
    if input.profile_format_version != CHARACTER_PROFILE_FORMAT_VERSION {
        return Err("expression fixture input profile version changed".into());
    }
    let pack = expression_support::reference_expression_pack();

    let raw_revision = authored_revision(&input)?;
    let revision = normalize_expression_revision(&raw_revision)?;
    let revised = apply_expression_revision(&input, &revision)?;

    let assignment_request = assignment_request(&revised, &pack)?;
    let assignment_receipt = assign_expression_pack(&revised, &pack, &assignment_request)?;
    let applied = &assignment_receipt.output_profile;
    validate_expression_profile(applied, std::slice::from_ref(&pack))?;
    let lint = lint_expression(applied, std::slice::from_ref(&pack))?;
    let coverage = inspect_expression_coverage(applied, std::slice::from_ref(&pack))?;

    let resolution_request = contextual_resolution_request();
    let resolution = resolve_expression_dialogue(applied, &pack, &resolution_request)?;
    if resolution.selected_variant_id != "friend_at_stormwatch" || resolution.fallback_used {
        return Err("contextual expression fixture selected an unexpected variant".into());
    }

    let mut fallback_profile = applied.clone();
    fallback_profile
        .canon
        .personality
        .openness
        .factor
        .as_mut()
        .ok_or("expression fixture requires an Openness factor")?
        .value = TraitMeasurement::Band {
        band: TraitBand::Middle,
    };
    recompute_derived(&mut fallback_profile);
    let fallback_request = fallback_resolution_request();
    let fallback = resolve_expression_dialogue(&fallback_profile, &pack, &fallback_request)?;
    if fallback.selected_variant_id != "fallback" || !fallback.fallback_used {
        return Err("expression fixture did not use its stable fallback".into());
    }

    let invalid_pack = invalid_placeholder_pack(&pack);
    let invalid_lint = lint_expression(applied, std::slice::from_ref(&invalid_pack))?;
    if !invalid_lint.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            ExpressionDiagnosticCode::UnsafePersonalization
                | ExpressionDiagnosticCode::UnresolvedToken
        ) && diagnostic
            .source
            .as_ref()
            .is_some_and(|source| source.line == 6)
    }) {
        return Err("invalid placeholder fixture omitted a source-located diagnostic".into());
    }

    write_pair(&fixture, "glasswind.expression-pack", &pack, write)?;
    write_pair(&fixture, "raw.expression-revision", &raw_revision, write)?;
    write_pair(&fixture, "normalized.expression-revision", &revision, write)?;
    write_pair(&fixture, "revised.character", &revised, write)?;
    write_pair(
        &fixture,
        "assignment.expression-request",
        &assignment_request,
        write,
    )?;
    write_pair(
        &fixture,
        "assignment.expression-receipt",
        &assignment_receipt,
        write,
    )?;
    write_pair(&fixture, "applied.character", applied, write)?;
    write_pair(&fixture, "lint.expression-lint", &lint, write)?;
    write_pair(&fixture, "coverage.expression-coverage", &coverage, write)?;
    write_pair(
        &fixture,
        "contextual.expression-resolution-request",
        &resolution_request,
        write,
    )?;
    write_pair(
        &fixture,
        "contextual.expression-resolution",
        &resolution,
        write,
    )?;
    write_pair(
        &fixture,
        "fallback-profile.character",
        &fallback_profile,
        write,
    )?;
    write_pair(
        &fixture,
        "fallback.expression-resolution-request",
        &fallback_request,
        write,
    )?;
    write_pair(&fixture, "fallback.expression-resolution", &fallback, write)?;
    write_pair(
        &fixture.join("invalid"),
        "restricted-placeholder.expression-pack",
        &invalid_pack,
        write,
    )?;
    write_pair(
        &fixture.join("invalid"),
        "restricted-placeholder.expression-lint",
        &invalid_lint,
        write,
    )?;

    for (name, contents) in [
        (
            "weave-character-expression-pack-v1.schema.json",
            expression_pack_schema()?,
        ),
        (
            "weave-character-expression-revision-v1.schema.json",
            expression_revision_schema()?,
        ),
        (
            "weave-character-expression-assignment-request-v1.schema.json",
            expression_assignment_request_schema()?,
        ),
        (
            "weave-character-expression-assignment-receipt-v1.schema.json",
            expression_assignment_receipt_schema()?,
        ),
        (
            "weave-character-expression-resolution-request-v1.schema.json",
            expression_resolution_request_schema()?,
        ),
        (
            "weave-character-expression-resolution-v1.schema.json",
            expression_resolution_schema()?,
        ),
        (
            "weave-character-expression-lint-v1.schema.json",
            expression_lint_schema()?,
        ),
        (
            "weave-character-expression-coverage-v1.schema.json",
            expression_coverage_schema()?,
        ),
    ] {
        write_or_check(&schemas.join(name), contents.as_bytes(), write)?;
    }
    Ok(())
}

fn authored_revision(
    profile: &CharacterProfile,
) -> Result<ExpressionRevision, Box<dyn std::error::Error>> {
    Ok(ExpressionRevision {
        revision_format_version: EXPRESSION_REVISION_FORMAT_VERSION,
        id: "org.weave.character.expression.revision.ari_authored".to_owned(),
        character_id: profile.id.clone(),
        expected_profile_sha256: expression_profile_fingerprint(profile)?,
        mutations: vec![ExpressionMutation::UpsertTerm {
            value: NormalizedExpressionTerm {
                id: "lantern_turn".to_owned(),
                character_id: profile.id.clone(),
                category: "org.weave.expression.navigation".to_owned(),
                kind: ExpressionTermKind::Phrase,
                surface: "  Lantern   Turn  ".to_owned(),
                normalized: "stale-input-value".to_owned(),
                strength: 0.7,
                applicability: ExpressionApplicability {
                    scenario_ids: vec!["departure".to_owned(), "arrival".to_owned()],
                    predicates: vec![ExpressionContextPredicate::WorldContext {
                        tag: "glasswind".to_owned(),
                    }],
                },
                origin: ExpressionRecordOrigin::Authored,
                review: ReviewState::NotRequired,
                source_ids: vec!["weave_expression_authored_fixture".to_owned()],
                rationale: None,
            },
        }],
        rationale: "  Add   one original phrase through the normalized authoring path.  "
            .to_owned(),
        provenance: original_provenance(
            "weave_expression_authored_fixture",
            "mutations",
            "Original synthetic expression revision and phrase.",
        ),
    })
}

fn assignment_request(
    profile: &CharacterProfile,
    pack: &weave_character::ExpressionPack,
) -> Result<ExpressionAssignmentRequest, Box<dyn std::error::Error>> {
    Ok(ExpressionAssignmentRequest {
        request_format_version: EXPRESSION_ASSIGNMENT_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.expression.assignment.ari_glasswind".to_owned(),
        character_id: profile.id.clone(),
        expected_profile_sha256: expression_profile_fingerprint(profile)?,
        pack: expression_pack_ref(pack)?,
        entry_ids: pack.entries.keys().cloned().collect(),
        vocabulary_pool_ids: pack.vocabulary_pools.keys().cloned().collect(),
        template_ids: pack.templates.keys().cloned().collect(),
        reviewer: "org.weave.reviewer.expression_fixture".to_owned(),
        rationale: "Accept every independently authored Glasswind expression record for the public offline fixture."
            .to_owned(),
        seed: 20_260_824,
        provenance: original_provenance(
            "weave_expression_assignment_fixture",
            "assignment",
            "Original synthetic reviewed expression-pack assignment.",
        ),
    })
}

fn contextual_resolution_request() -> ExpressionResolutionRequest {
    ExpressionResolutionRequest {
        request_format_version: EXPRESSION_RESOLUTION_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.expression.resolution.ari_arrival".to_owned(),
        assignment_id: "arrival_greeting".to_owned(),
        scenario_id: "arrival".to_owned(),
        speaker_character_id: "org.weave.character.ari_vale".to_owned(),
        listener: Some(ExpressionRuntimeParticipant {
            character_id: "org.weave.character.tavi_quill".to_owned(),
            display_name: "Tavi Quill".to_owned(),
        }),
        relationship_kind_ids: vec!["org.weave.relationship.friend".to_owned()],
        date_label: Some("Harbor Festival".to_owned()),
        date_context_ids: vec!["harbor_festival".to_owned()],
        world_place_name: Some("Stormwatch Gate".to_owned()),
        world_context_tags: vec!["stormwatch".to_owned()],
        seed: 42,
    }
}

fn fallback_resolution_request() -> ExpressionResolutionRequest {
    ExpressionResolutionRequest {
        request_format_version: EXPRESSION_RESOLUTION_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.expression.resolution.ari_fallback".to_owned(),
        assignment_id: "arrival_greeting".to_owned(),
        scenario_id: "arrival".to_owned(),
        speaker_character_id: "org.weave.character.ari_vale".to_owned(),
        listener: None,
        relationship_kind_ids: Vec::new(),
        date_label: None,
        date_context_ids: Vec::new(),
        world_place_name: None,
        world_context_tags: Vec::new(),
        seed: 42,
    }
}

fn invalid_placeholder_pack(
    pack: &weave_character::ExpressionPack,
) -> weave_character::ExpressionPack {
    let mut invalid = pack.clone();
    let template = invalid
        .templates
        .get_mut("arrival_greeting")
        .expect("reference template exists");
    template.placeholders.insert(
        "secret_token".to_owned(),
        weave_character::ExpressionPlaceholderDeclaration {
            id: "secret_token".to_owned(),
            value: weave_character::ExpressionPlaceholderValue::SpeakerDisplayName,
            required: false,
        },
    );
    let fallback = template
        .variants
        .get_mut("fallback")
        .expect("reference fallback exists");
    fallback.content = "{{speaker_name}} reveals {{secret_token}}.".to_owned();
    fallback.source.line = 6;
    invalid
}

fn original_provenance(source_id: &str, claim: &str, attribution: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: source_id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "expression-dialogue-v1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            attribution: attribution.to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([(claim.to_owned(), vec![source_id.to_owned()])]),
    }
}

fn read_profile(path: &Path) -> Result<CharacterProfile, Box<dyn std::error::Error>> {
    Ok(CharacterProfile::from_json(&fs::read_to_string(path)?)?)
}

fn write_pair<T: serde::Serialize>(
    directory: &Path,
    stem: &str,
    value: &T,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = weave_domain::to_pretty_json(value)?;
    let ron = weave_domain::to_pretty_ron(value)?;
    write_or_check(
        &directory.join(format!("{stem}.json")),
        json.as_bytes(),
        write,
    )?;
    write_or_check(
        &directory.join(format!("{stem}.ron")),
        ron.as_bytes(),
        write,
    )
}

fn write_or_check(
    path: &Path,
    expected: &[u8],
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, expected)?;
        return Ok(());
    }
    let actual = fs::read(path)?;
    if actual != expected {
        return Err(format!("{} is stale", path.display()).into());
    }
    Ok(())
}
