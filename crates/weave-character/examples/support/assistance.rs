use std::collections::BTreeMap;

use weave_character::{
    ASSISTANCE_BATCH_REQUEST_FORMAT_VERSION, ASSISTANCE_REQUEST_FORMAT_VERSION,
    ASSISTANCE_TEMPLATE_FORMAT_VERSION, AssistanceBatchBudget, AssistanceBatchFilter,
    AssistanceBatchRequest, AssistanceCachePolicy, AssistanceFieldKind, AssistanceInputField,
    AssistanceProviderFailureCode, AssistanceProviderRef, AssistanceRequest, AssistanceRetryPolicy,
    AssistanceSafetyContract, AssistanceSettings, AssistanceTemplate, AssistanceTemplateField,
    AssistanceTemplateRef, CharacterCollection, assistance_profile_fingerprint,
    assistance_template_fingerprint, offline_assistance_provider_ref,
};
use weave_domain::{DomainValue, Provenance, ProvenanceKind, ProvenanceSource};

pub fn reference_assistance_template() -> AssistanceTemplate {
    let fields = all_fields()
        .into_iter()
        .map(|kind| {
            (
                kind,
                AssistanceTemplateField {
                    kind,
                    instructions: instructions(kind).to_owned(),
                    required_inputs: vec![AssistanceInputField::DisplayName],
                    target_path: format!("suggestions.{}", kind.as_str()),
                    minimum_text_chars: 20,
                    maximum_text_chars: 2_048,
                    maximum_candidates: 4,
                },
            )
        })
        .collect();
    AssistanceTemplate {
        template_format_version: ASSISTANCE_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.assistance.glasswind_development".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Glasswind Character Development".to_owned(),
        description: "Original structured prompts for optional, review-gated character development suggestions in offline or adapter-backed workflows."
            .to_owned(),
        compatible_profile_versions: vec![1],
        offline_scaffold_version: 1,
        fields,
        safety: AssistanceSafetyContract {
            reject_placeholders: true,
            reject_credential_shapes: true,
            reject_control_characters: true,
            require_known_references: true,
            maximum_total_response_chars: 1_048_576,
        },
        license: "MIT".to_owned(),
        license_url: "https://opensource.org/license/mit/".to_owned(),
        provenance: original_provenance(
            "assistance_template_original",
            "template",
            "Original synthetic Weave assistance prompts and offline scaffold contract.",
        ),
    }
}

pub fn offline_request(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    profile_id: &str,
) -> AssistanceRequest {
    request_with_provider(
        collection,
        template,
        profile_id,
        offline_assistance_provider_ref(),
        Some(20_260_824),
    )
}

pub fn request_with_provider(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    profile_id: &str,
    provider: AssistanceProviderRef,
    seed: Option<u64>,
) -> AssistanceRequest {
    let profile = &collection.characters[profile_id];
    AssistanceRequest {
        request_format_version: ASSISTANCE_REQUEST_FORMAT_VERSION,
        id: format!(
            "org.weave.character.assistance.{}",
            profile_id.rsplit('.').next().unwrap_or("character")
        ),
        profile_id: profile_id.to_owned(),
        expected_profile_sha256: assistance_profile_fingerprint(profile)
            .expect("valid synthetic assistance profile"),
        template: template_ref(template),
        provider,
        fields: all_fields(),
        included_inputs: vec![
            AssistanceInputField::DisplayName,
            AssistanceInputField::Personality,
            AssistanceInputField::InnerLife,
            AssistanceInputField::Voice,
            AssistanceInputField::Relationships,
        ],
        settings: reference_settings(),
        seed,
        generation: 0,
        provenance: original_provenance(
            "assistance_request_original",
            "request",
            "Original synthetic assistance request fixture.",
        ),
    }
}

pub fn offline_batch_request(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
) -> AssistanceBatchRequest {
    let mut settings = reference_settings();
    settings.candidates_per_field = 1;
    AssistanceBatchRequest {
        batch_request_format_version: ASSISTANCE_BATCH_REQUEST_FORMAT_VERSION,
        id: "org.weave.character.assistance.glasswind_batch".to_owned(),
        collection_id: collection.id.clone(),
        expected_collection_sha256: weave_character::collection_fingerprint(collection)
            .expect("valid synthetic assistance collection"),
        template: template_ref(template),
        provider: offline_assistance_provider_ref(),
        fields: vec![
            AssistanceFieldKind::Biography,
            AssistanceFieldKind::Motivation,
        ],
        included_inputs: vec![AssistanceInputField::DisplayName],
        settings,
        seed: Some(20_260_825),
        filter: AssistanceBatchFilter {
            character_ids: Vec::new(),
            id_prefixes: vec!["org.weave.character".to_owned()],
            fields_missing_suggestions: vec![AssistanceFieldKind::Biography],
        },
        budget: AssistanceBatchBudget {
            maximum_characters: 64,
            maximum_provider_calls: 128,
            maximum_candidates: 4_096,
            maximum_input_chars: 1_048_576,
        },
        retry: AssistanceRetryPolicy {
            maximum_retries_per_character: 2,
            retryable_codes: vec![
                AssistanceProviderFailureCode::RateLimited,
                AssistanceProviderFailureCode::Timeout,
            ],
        },
        rate_limit_calls_per_resume: 1,
        cache_policy: AssistanceCachePolicy::JobLocal,
        provenance: original_provenance(
            "assistance_batch_request_original",
            "batch_request",
            "Original synthetic assistance batch request fixture.",
        ),
    }
}

fn reference_settings() -> AssistanceSettings {
    AssistanceSettings {
        candidates_per_field: 2,
        language: "en-US".to_owned(),
        creativity_micros: 350_000,
        maximum_output_chars: 200_000,
        provider_parameters: BTreeMap::from([(
            "quality".to_owned(),
            DomainValue::Symbol("balanced".to_owned()),
        )]),
    }
}

fn template_ref(template: &AssistanceTemplate) -> AssistanceTemplateRef {
    AssistanceTemplateRef {
        id: template.id.clone(),
        version: template.version.clone(),
        sha256: assistance_template_fingerprint(template)
            .expect("valid synthetic assistance template"),
    }
}

fn original_provenance(id: &str, claim: &str, attribution: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://weave.dev/character/assistance".to_owned(),
            revision: "1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://opensource.org/license/mit/".to_owned(),
            attribution: attribution.to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([(claim.to_owned(), vec![id.to_owned()])]),
    }
}

fn all_fields() -> Vec<AssistanceFieldKind> {
    vec![
        AssistanceFieldKind::Biography,
        AssistanceFieldKind::Motivation,
        AssistanceFieldKind::Fear,
        AssistanceFieldKind::GuardedTruth,
        AssistanceFieldKind::Tension,
        AssistanceFieldKind::NarrativeHook,
        AssistanceFieldKind::PresentationCue,
        AssistanceFieldKind::RoleIdea,
        AssistanceFieldKind::RelationshipCue,
        AssistanceFieldKind::ContextReaction,
        AssistanceFieldKind::ExpressionExample,
    ]
}

const fn instructions(kind: AssistanceFieldKind) -> &'static str {
    match kind {
        AssistanceFieldKind::Biography => {
            "Suggest a concise biography thread and identify the formative thread it opens."
        }
        AssistanceFieldKind::Motivation => {
            "Suggest one optional objective and a distinct reason the character may pursue it."
        }
        AssistanceFieldKind::Fear => {
            "Suggest one optional concern and a concrete situation that could bring it forward."
        }
        AssistanceFieldKind::GuardedTruth => {
            "Suggest one truth the character might guard and a reason for that caution."
        }
        AssistanceFieldKind::Tension => {
            "Suggest a dramatic premise and a distinct pressure that pulls against it."
        }
        AssistanceFieldKind::NarrativeHook => {
            "Suggest a scene-ready hook and explicit stakes without asserting new canon."
        }
        AssistanceFieldKind::PresentationCue => {
            "Suggest a repeatable presentation cue and explain its limited narrative purpose."
        }
        AssistanceFieldKind::RoleIdea => {
            "Suggest an optional role label and explain how it could support a scene."
        }
        AssistanceFieldKind::RelationshipCue => {
            "Suggest a reversible relationship cue and explain what it could help explore."
        }
        AssistanceFieldKind::ContextReaction => {
            "Suggest a context and a possible reaction while keeping both non-canonical."
        }
        AssistanceFieldKind::ExpressionExample => {
            "Suggest a bounded dialogue context and an original example line for author review."
        }
    }
}
