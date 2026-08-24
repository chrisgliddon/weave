use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/assistance.rs"]
mod assistance_support;

use assistance_support::{
    offline_batch_request, offline_request, reference_assistance_template, request_with_provider,
};
use weave_character::{
    AssistanceCandidateSet, AssistanceCredentialStatus, AssistanceDecision,
    AssistanceDecisionReview, AssistanceJobState, AssistanceProviderFailure,
    AssistanceProviderMode, AssistanceProviderPayload, AssistanceProviderRef,
    AssistanceProviderResponse, CharacterAssistanceProvider, CharacterCollection,
    OfflineAssistanceProvider, apply_assistance_batch_reviews, apply_assistance_review,
    approve_assistance_batch_preview, approve_assistance_preview,
    assistance_advisory_review_schema, assistance_approval_schema, assistance_batch_preview_schema,
    assistance_batch_receipt_schema, assistance_batch_request_schema,
    assistance_candidate_set_schema, assistance_comparison_schema,
    assistance_decision_review_schema, assistance_job_schema, assistance_preview_schema,
    assistance_provider_response_schema, assistance_receipt_schema, assistance_request_schema,
    assistance_template_schema, compare_assistance_providers, create_assistance_decision_review,
    execute_assistance_request, preview_assistance_batch, preview_assistance_request,
    resume_assistance_job, review_assistance_candidates_offline, start_assistance_job,
};

#[derive(Debug, Clone, Default)]
struct AlternateOfflineProvider;

impl CharacterAssistanceProvider for AlternateOfflineProvider {
    fn descriptor(&self) -> AssistanceProviderRef {
        alternate_offline_provider_ref()
    }

    fn credential_status(&self) -> AssistanceCredentialStatus {
        AssistanceCredentialStatus::NotRequired
    }

    fn generate(
        &mut self,
        payload: &AssistanceProviderPayload,
    ) -> Result<String, AssistanceProviderFailure> {
        OfflineAssistanceProvider.generate(payload)
    }
}

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
    let fixture = root.join("examples/domain-modules/weave-character/assistance");
    let schemas = root.join("schemas");
    let input = CharacterCollection::from_json(&fs::read_to_string(root.join(
        "examples/domain-modules/weave-character/operations/collection.character-collection.json",
    ))?)?;
    let template = reference_assistance_template();
    let profile_id = input.characters.keys().next().ok_or("missing profile")?;
    let mut request = offline_request(&input, &template, profile_id);
    request.settings.candidates_per_field = 1;
    let preview = preview_assistance_request(&input, &template, &request)?;
    let approval = approve_assistance_preview(
        &preview,
        "org.weave.reviewer.fixture",
        "Approve the exact visible offline scope for this original synthetic fixture.",
    )?;
    let mut provider = OfflineAssistanceProvider;
    let candidates =
        execute_assistance_request(&input, &template, &preview, &approval, &mut provider)?;
    let response = AssistanceProviderResponse::from_json(&provider.generate(&preview.payload)?)?;
    let advisory =
        review_assistance_candidates_offline(&candidates, "org.weave.reviewer.advisory")?;
    let review = create_assistance_decision_review(
        &candidates,
        "org.weave.reviewer.fixture",
        "Exercise accept, edit, reject, defer, and regenerate as explicit fixture decisions.",
        mixed_decisions(&candidates),
    )?;
    let receipt = apply_assistance_review(
        &candidates.input_profile,
        &candidates,
        &review,
        std::slice::from_ref(&advisory),
    )?;

    let batch_request = offline_batch_request(&input, &template);
    let batch_preview = preview_assistance_batch(&input, &template, &batch_request)?;
    let batch_approval = approve_assistance_batch_preview(
        &batch_preview,
        "org.weave.reviewer.fixture",
        "Approve every exact per-character disclosure in the deterministic offline fixture batch.",
    )?;
    let mut job = start_assistance_job(&input, &template, &batch_preview, &batch_approval)?;
    while job.state != AssistanceJobState::ReadyForReview {
        job = resume_assistance_job(&input, &job, &mut provider, 64)?;
    }
    let batch_reviews = job
        .candidate_sets
        .iter()
        .map(|(id, set)| Ok((id.clone(), accept_all_review(set)?)))
        .collect::<Result<BTreeMap<_, _>, weave_character::CharacterError>>()?;
    let batch_advisories = job
        .candidate_sets
        .iter()
        .map(|(id, set)| {
            Ok((
                id.clone(),
                vec![review_assistance_candidates_offline(
                    set,
                    "org.weave.reviewer.advisory",
                )?],
            ))
        })
        .collect::<Result<BTreeMap<_, _>, weave_character::CharacterError>>()?;
    let batch_receipt =
        apply_assistance_batch_reviews(&input, &job, &batch_reviews, &batch_advisories)?;

    let mut alternate_request = request_with_provider(
        &input,
        &template,
        profile_id,
        alternate_offline_provider_ref(),
        Some(20_260_824),
    );
    alternate_request.settings.candidates_per_field = 1;
    let alternate_preview = preview_assistance_request(&input, &template, &alternate_request)?;
    let alternate_approval = approve_assistance_preview(
        &alternate_preview,
        "org.weave.reviewer.fixture",
        "Approve an alternate credential-free adapter coordinate for comparison coverage.",
    )?;
    let mut alternate_provider = AlternateOfflineProvider;
    let alternate_candidates = execute_assistance_request(
        &input,
        &template,
        &alternate_preview,
        &alternate_approval,
        &mut alternate_provider,
    )?;
    let alternate_advisory = review_assistance_candidates_offline(
        &alternate_candidates,
        "org.weave.reviewer.alternate",
    )?;
    let comparison = compare_assistance_providers(
        &[candidates.clone(), alternate_candidates.clone()],
        &[advisory.clone(), alternate_advisory.clone()],
    )?;

    write_pair(write, &fixture, "input.character-collection", &input)?;
    write_pair(write, &fixture, "glasswind.assistance-template", &template)?;
    write_pair(write, &fixture, "single.assistance-request", &request)?;
    write_pair(write, &fixture, "single.assistance-preview", &preview)?;
    write_pair(write, &fixture, "single.assistance-approval", &approval)?;
    write_pair(
        write,
        &fixture,
        "single.assistance-provider-response",
        &response,
    )?;
    write_pair(
        write,
        &fixture,
        "single.assistance-candidate-set",
        &candidates,
    )?;
    write_pair(
        write,
        &fixture,
        "single.assistance-advisory-review",
        &advisory,
    )?;
    write_pair(
        write,
        &fixture,
        "single.assistance-decision-review",
        &review,
    )?;
    write_pair(
        write,
        &fixture,
        "single.assistance-decisions",
        &review.decisions,
    )?;
    write_pair(write, &fixture, "single.assistance-receipt", &receipt)?;
    write_pair(
        write,
        &fixture,
        "single.applied-character",
        &receipt.output_profile,
    )?;
    write_pair(write, &fixture, "batch.assistance-request", &batch_request)?;
    write_pair(write, &fixture, "batch.assistance-preview", &batch_preview)?;
    write_pair(
        write,
        &fixture,
        "batch.assistance-approval",
        &batch_approval,
    )?;
    write_pair(write, &fixture, "batch.assistance-job", &job)?;
    write_pair(write, &fixture, "batch.assistance-receipt", &batch_receipt)?;
    write_pair(
        write,
        &fixture,
        "batch.applied-character-collection",
        &batch_receipt.output_collection,
    )?;
    for (profile_id, review) in &batch_reviews {
        let local_id = profile_id.rsplit('.').next().unwrap_or(profile_id);
        write_pair(
            write,
            &fixture,
            &format!("batch.{local_id}.assistance-decision-review"),
            review,
        )?;
    }
    for (profile_id, advisories) in &batch_advisories {
        let local_id = profile_id.rsplit('.').next().unwrap_or(profile_id);
        for (index, advisory) in advisories.iter().enumerate() {
            write_pair(
                write,
                &fixture,
                &format!("batch.{local_id}.{index}.assistance-advisory-review"),
                advisory,
            )?;
        }
    }
    write_pair(
        write,
        &fixture,
        "alternate.assistance-candidate-set",
        &alternate_candidates,
    )?;
    write_pair(
        write,
        &fixture,
        "alternate.assistance-advisory-review",
        &alternate_advisory,
    )?;
    write_pair(
        write,
        &fixture,
        "providers.assistance-comparison",
        &comparison,
    )?;

    for (name, schema) in [
        (
            "weave-character-assistance-template-v1.schema.json",
            assistance_template_schema()?,
        ),
        (
            "weave-character-assistance-request-v1.schema.json",
            assistance_request_schema()?,
        ),
        (
            "weave-character-assistance-preview-v1.schema.json",
            assistance_preview_schema()?,
        ),
        (
            "weave-character-assistance-approval-v1.schema.json",
            assistance_approval_schema()?,
        ),
        (
            "weave-character-assistance-provider-response-v1.schema.json",
            assistance_provider_response_schema()?,
        ),
        (
            "weave-character-assistance-candidate-set-v1.schema.json",
            assistance_candidate_set_schema()?,
        ),
        (
            "weave-character-assistance-advisory-review-v1.schema.json",
            assistance_advisory_review_schema()?,
        ),
        (
            "weave-character-assistance-decision-review-v1.schema.json",
            assistance_decision_review_schema()?,
        ),
        (
            "weave-character-assistance-receipt-v1.schema.json",
            assistance_receipt_schema()?,
        ),
        (
            "weave-character-assistance-batch-request-v1.schema.json",
            assistance_batch_request_schema()?,
        ),
        (
            "weave-character-assistance-batch-preview-v1.schema.json",
            assistance_batch_preview_schema()?,
        ),
        (
            "weave-character-assistance-job-v1.schema.json",
            assistance_job_schema()?,
        ),
        (
            "weave-character-assistance-batch-receipt-v1.schema.json",
            assistance_batch_receipt_schema()?,
        ),
        (
            "weave-character-assistance-comparison-v1.schema.json",
            assistance_comparison_schema()?,
        ),
    ] {
        write_text(write, &schemas.join(name), &schema)?;
    }
    Ok(())
}

fn alternate_offline_provider_ref() -> AssistanceProviderRef {
    AssistanceProviderRef {
        adapter_id: "org.weave.character.assistance.alternate_offline".to_owned(),
        adapter_version: "1.0.0".to_owned(),
        engine_id: "glasswind_scaffold_alternate_v1".to_owned(),
        mode: AssistanceProviderMode::Offline,
        supports_seed: true,
        credential_required: false,
    }
}

fn mixed_decisions(set: &AssistanceCandidateSet) -> BTreeMap<String, AssistanceDecision> {
    set.candidates
        .iter()
        .enumerate()
        .map(|(index, (id, candidate))| {
            let decision = match index % 5 {
                0 => AssistanceDecision::Accept {
                    rationale: "Keep this candidate in the non-canonical review queue.".to_owned(),
                },
                1 => AssistanceDecision::Edit {
                    value: candidate.value.clone(),
                    rationale: "Retain this structured value as an explicit author edit."
                        .to_owned(),
                },
                2 => AssistanceDecision::Reject {
                    rationale: "This direction does not fit the current fixture draft.".to_owned(),
                },
                3 => AssistanceDecision::Defer {
                    rationale: "Revisit this direction after another outline pass.".to_owned(),
                },
                _ => AssistanceDecision::Regenerate {
                    rationale: "Request another bounded option for this field.".to_owned(),
                },
            };
            (id.clone(), decision)
        })
        .collect()
}

fn accept_all_review(
    set: &AssistanceCandidateSet,
) -> Result<AssistanceDecisionReview, weave_character::CharacterError> {
    create_assistance_decision_review(
        set,
        "org.weave.reviewer.fixture",
        "Keep every synthetic batch option visible as a pending suggestion.",
        set.candidates
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    AssistanceDecision::Accept {
                        rationale: "Retain this synthetic candidate for explicit later review."
                            .to_owned(),
                    },
                )
            })
            .collect(),
    )
}

fn write_pair<T>(
    write: bool,
    directory: &Path,
    stem: &str,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::Serialize,
{
    let json = weave_domain::to_pretty_json(value)?;
    let ron = weave_domain::to_pretty_ron(value)?;
    write_text(write, &directory.join(format!("{stem}.json")), &json)?;
    write_text(write, &directory.join(format!("{stem}.ron")), &ron)
}

fn write_text(write: bool, path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        return Ok(());
    }
    let existing = fs::read_to_string(path)?;
    if existing != content {
        return Err(format!("checked fixture is stale: {}", path.display()).into());
    }
    Ok(())
}
