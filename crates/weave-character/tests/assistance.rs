use std::collections::{BTreeMap, VecDeque};

#[path = "../examples/support/assistance.rs"]
mod assistance_support;

use assistance_support::{
    offline_batch_request, offline_request, reference_assistance_template, request_with_provider,
};
use weave_character::{
    AssistanceAdvisoryIssue, AssistanceAdvisorySeverity, AssistanceCandidateSet,
    AssistanceCandidateValue, AssistanceCredentialStatus, AssistanceDecision,
    AssistanceDecisionReview, AssistanceError, AssistanceExecutionApproval,
    AssistanceGenerationOrigin, AssistanceJob, AssistanceJobState, AssistancePreview,
    AssistanceProviderFailure, AssistanceProviderFailureCode, AssistanceProviderMode,
    AssistanceProviderRef, AssistanceProviderResponse, AssistanceReceipt,
    CharacterAssistanceProvider, CharacterCollection, CharacterDiagnosticCode,
    OfflineAssistanceProvider, Secret, apply_assistance_batch_reviews, apply_assistance_review,
    approve_assistance_batch_preview, approve_assistance_preview, cancel_assistance_job,
    compare_assistance_providers, create_assistance_decision_review, execute_assistance_request,
    preview_assistance_batch, preview_assistance_request, resume_assistance_job,
    review_assistance_candidates_offline, start_assistance_job,
    validate_assistance_advisory_review, validate_assistance_comparison,
};

const COLLECTION_JSON: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/collection.character-collection.json"
);

#[derive(Debug)]
struct ScriptedProvider {
    descriptor: AssistanceProviderRef,
    credential_status: AssistanceCredentialStatus,
    actions: VecDeque<Result<String, AssistanceProviderFailure>>,
    calls: usize,
}

impl CharacterAssistanceProvider for ScriptedProvider {
    fn descriptor(&self) -> AssistanceProviderRef {
        self.descriptor.clone()
    }

    fn credential_status(&self) -> AssistanceCredentialStatus {
        self.credential_status
    }

    fn generate(
        &mut self,
        _payload: &weave_character::AssistanceProviderPayload,
    ) -> Result<String, AssistanceProviderFailure> {
        self.calls += 1;
        self.actions
            .pop_front()
            .unwrap_or(Err(AssistanceProviderFailure {
                code: AssistanceProviderFailureCode::Internal,
                retryable: false,
            }))
    }
}

fn fixture_collection() -> CharacterCollection {
    CharacterCollection::from_json(COLLECTION_JSON).expect("valid collection fixture")
}

fn external_provider() -> AssistanceProviderRef {
    AssistanceProviderRef {
        adapter_id: "org.weave.character.assistance.fake".to_owned(),
        adapter_version: "1.0.0".to_owned(),
        engine_id: "fake-engine-v1".to_owned(),
        mode: AssistanceProviderMode::External,
        supports_seed: true,
        credential_required: true,
    }
}

fn offline_raw(preview: &AssistancePreview) -> String {
    let mut provider = OfflineAssistanceProvider;
    provider
        .generate(&preview.payload)
        .expect("offline response")
}

fn complete_decisions(set: &AssistanceCandidateSet) -> BTreeMap<String, AssistanceDecision> {
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
                    rationale: "Retain the structured value as an explicit author edit.".to_owned(),
                },
                2 => AssistanceDecision::Reject {
                    rationale: "This direction does not fit the current draft.".to_owned(),
                },
                3 => AssistanceDecision::Defer {
                    rationale: "Revisit this direction after the next outline pass.".to_owned(),
                },
                _ => AssistanceDecision::Regenerate {
                    rationale: "Request another bounded option for this field.".to_owned(),
                },
            };
            (id.clone(), decision)
        })
        .collect()
}

fn accept_all_review(set: &AssistanceCandidateSet) -> AssistanceDecisionReview {
    create_assistance_decision_review(
        set,
        "org.weave.reviewer.fixture",
        "Keep every synthetic option visible as a pending suggestion for fixture coverage.",
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
    .expect("complete accept review")
}

#[test]
fn offline_preview_review_apply_and_round_trip_remain_non_canonical() {
    let collection = fixture_collection();
    let original = collection.clone();
    let template = reference_assistance_template();
    let profile_id = collection.characters.keys().next().expect("profile");
    let request = offline_request(&collection, &template, profile_id);

    let preview = preview_assistance_request(&collection, &template, &request).expect("preview");
    assert_eq!(collection, original);
    assert!(!preview.provider_called);
    assert!(!preview.credential_value_stored);
    assert_eq!(
        AssistancePreview::from_json(&preview.to_json().expect("preview JSON"))
            .expect("parse JSON"),
        preview
    );
    assert_eq!(
        AssistancePreview::from_ron(&preview.to_ron().expect("preview RON")).expect("parse RON"),
        preview
    );
    let mut malformed_payload = preview.payload.clone();
    malformed_payload.required_evidence.clear();
    let failure = OfflineAssistanceProvider
        .generate(&malformed_payload)
        .expect_err("malformed direct offline payload");
    assert_eq!(failure.code, AssistanceProviderFailureCode::Internal);

    let approval = approve_assistance_preview(
        &preview,
        "org.weave.reviewer.fixture",
        "Execute the deterministic offline scaffold after reviewing its complete scope.",
    )
    .expect("approval");
    let mut provider = OfflineAssistanceProvider;
    let set =
        execute_assistance_request(&collection, &template, &preview, &approval, &mut provider)
            .expect("offline execution");
    assert_eq!(set.origin, AssistanceGenerationOrigin::OfflineScaffold);
    assert_eq!(set.candidates.len(), 22);
    assert_eq!(
        AssistanceCandidateSet::from_json(&set.to_json().expect("set JSON")).expect("parse JSON"),
        set
    );
    assert_eq!(
        AssistanceCandidateSet::from_ron(&set.to_ron().expect("set RON")).expect("parse RON"),
        set
    );

    let advisory = review_assistance_candidates_offline(&set, "org.weave.reviewer.advisory")
        .expect("advisory");
    let mut unsafe_advisory = advisory.clone();
    let biography_id = set
        .candidates
        .iter()
        .find_map(|(id, candidate)| {
            (candidate.field == weave_character::AssistanceFieldKind::Biography).then_some(id)
        })
        .expect("biography candidate");
    unsafe_advisory
        .assessments
        .get_mut(biography_id)
        .expect("biography assessment")
        .proposed_edit = Some(AssistanceCandidateValue::Biography {
        summary: "api_key=definitely_not_a_real_secret".to_owned(),
        formative_thread: "An original synthetic formative thread for validation.".to_owned(),
    });
    let error = validate_assistance_advisory_review(&set, &unsafe_advisory)
        .expect_err("unsafe advisory edit");
    assert!(error.to_string().contains("[REDACTED]"));
    assert!(!error.to_string().contains("definitely_not_a_real_secret"));
    let review = create_assistance_decision_review(
        &set,
        "org.weave.reviewer.fixture",
        "Exercise every explicit candidate decision without changing canonical character data.",
        complete_decisions(&set),
    )
    .expect("decision review");
    let receipt = apply_assistance_review(
        &set.input_profile,
        &set,
        &review,
        std::slice::from_ref(&advisory),
    )
    .expect("review application");
    assert_eq!(receipt.input_profile.canon, receipt.output_profile.canon);
    assert_eq!(
        receipt.input_profile.extensions,
        receipt.output_profile.extensions
    );
    assert_eq!(
        receipt.input_profile.derived,
        receipt.output_profile.derived
    );
    assert!(!receipt.accepted_suggestion_ids.is_empty());
    assert!(!receipt.rejected_candidate_ids.is_empty());
    assert!(!receipt.deferred_candidate_ids.is_empty());
    assert!(!receipt.regenerate_candidate_ids.is_empty());
    assert!(receipt.regeneration_request.is_some());
    assert_eq!(
        apply_assistance_review(&receipt.output_profile, &set, &review, &[advisory])
            .expect("idempotent replay"),
        receipt
    );
    assert_eq!(
        AssistanceReceipt::from_json(&receipt.to_json().expect("receipt JSON"))
            .expect("parse JSON"),
        receipt
    );
    assert_eq!(
        AssistanceReceipt::from_ron(&receipt.to_ron().expect("receipt RON")).expect("parse RON"),
        receipt
    );

    let mut stale = set.input_profile.clone();
    stale.canon.identity.display_name.value.push_str(" Revised");
    let error = apply_assistance_review(&stale, &set, &review, &[]).expect_err("stale profile");
    assert_eq!(error.diagnostic().code, CharacterDiagnosticCode::StaleInput);
}

#[test]
fn external_adapter_boundary_redacts_secrets_and_rejects_malformed_or_unsafe_output() {
    let collection = fixture_collection();
    let template = reference_assistance_template();
    let profile_id = collection.characters.keys().next().expect("profile");
    let request = request_with_provider(
        &collection,
        &template,
        profile_id,
        external_provider(),
        Some(41),
    );
    let preview = preview_assistance_request(&collection, &template, &request).expect("preview");
    let approval = approve_assistance_preview(
        &preview,
        "org.weave.reviewer.fixture",
        "The complete visible scope is approved for the deterministic fake adapter.",
    )
    .expect("approval");

    let credential = Secret::new("definitely_not_a_real_credential");
    assert_eq!(format!("{credential}"), "[REDACTED]");
    assert_eq!(format!("{credential:?}"), "[REDACTED]");
    let preview_json = preview.to_json().expect("preview JSON");
    assert!(!preview_json.contains("definitely_not_a_real_credential"));
    assert!(preview_json.contains("\"credential_value_stored\": false"));

    let mut missing = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Missing,
        actions: VecDeque::new(),
        calls: 0,
    };
    let error =
        execute_assistance_request(&collection, &template, &preview, &approval, &mut missing)
            .expect_err("missing credential");
    assert!(matches!(
        error,
        AssistanceError::Provider(AssistanceProviderFailure {
            code: AssistanceProviderFailureCode::Authentication,
            ..
        })
    ));
    assert_eq!(missing.calls, 0);

    let mut malformed = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Ok("{ malformed and private }".to_owned())]),
        calls: 0,
    };
    let error =
        execute_assistance_request(&collection, &template, &preview, &approval, &mut malformed)
            .expect_err("malformed output");
    let AssistanceError::Contract(error) = error else {
        panic!("malformed provider output must be a contract error");
    };
    assert_eq!(
        error.diagnostic().code,
        CharacterDiagnosticCode::InvalidEncoding
    );
    assert!(!error.to_string().contains("private"));

    let valid_raw = offline_raw(&preview);
    let mut secret_response = AssistanceProviderResponse::from_json(&valid_raw).expect("response");
    let AssistanceCandidateValue::Biography { summary, .. } =
        &mut secret_response.candidates[0].value
    else {
        panic!("first candidate must be a biography");
    };
    *summary = "api_key=definitely_not_a_real_secret".to_owned();
    let mut unsafe_provider = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Ok(
            weave_domain::to_pretty_json(&secret_response).expect("unsafe response JSON")
        )]),
        calls: 0,
    };
    let error = execute_assistance_request(
        &collection,
        &template,
        &preview,
        &approval,
        &mut unsafe_provider,
    )
    .expect_err("credential-shaped output");
    let message = error.to_string();
    assert!(message.contains("[REDACTED]"));
    assert!(!message.contains("definitely_not_a_real_secret"));

    let mut placeholder_response =
        AssistanceProviderResponse::from_json(&valid_raw).expect("response");
    let AssistanceCandidateValue::Biography { summary, .. } =
        &mut placeholder_response.candidates[0].value
    else {
        panic!("first candidate must be a biography");
    };
    *summary = "TODO".to_owned();
    let mut placeholder_provider = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Ok(
            weave_domain::to_pretty_json(&placeholder_response).expect("placeholder JSON")
        )]),
        calls: 0,
    };
    let error = execute_assistance_request(
        &collection,
        &template,
        &preview,
        &approval,
        &mut placeholder_provider,
    )
    .expect_err("placeholder output");
    assert!(error.to_string().contains("unresolved placeholder"));

    let mut valid_provider = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Ok(valid_raw)]),
        calls: 0,
    };
    let set = execute_assistance_request(
        &collection,
        &template,
        &preview,
        &approval,
        &mut valid_provider,
    )
    .expect("valid fake provider");
    assert_eq!(set.origin, AssistanceGenerationOrigin::ProviderAdapter);
    assert_eq!(valid_provider.calls, 1);
}

#[test]
fn provider_failures_are_safe_and_retry_policy_is_resumable() {
    let collection = fixture_collection();
    let template = reference_assistance_template();
    let mut batch_request = offline_batch_request(&collection, &template);
    batch_request.provider = external_provider();
    batch_request.seed = Some(99);
    batch_request.rate_limit_calls_per_resume = 2;
    let preview = preview_assistance_batch(&collection, &template, &batch_request)
        .expect("external batch preview");
    let approval = approve_assistance_batch_preview(
        &preview,
        "org.weave.reviewer.fixture",
        "Approve the complete synthetic batch disclosure for retry testing.",
    )
    .expect("batch approval");
    let job = start_assistance_job(&collection, &template, &preview, &approval).expect("job");
    let first_preview = &preview.previews[&preview.ordered_target_ids[0]];
    let mut provider = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([
            Err(AssistanceProviderFailure {
                code: AssistanceProviderFailureCode::Timeout,
                retryable: true,
            }),
            Ok(offline_raw(first_preview)),
        ]),
        calls: 0,
    };
    let retried =
        resume_assistance_job(&collection, &job, &mut provider, 2).expect("retry then success");
    assert_eq!(retried.attempts[&preview.ordered_target_ids[0]], 2);
    assert_eq!(retried.next_index, 1);
    assert!(retried.failures.is_empty());
    assert_eq!(provider.calls, 2);

    let mut failed = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Err(AssistanceProviderFailure {
            code: AssistanceProviderFailureCode::Unavailable,
            retryable: false,
        })]),
        calls: 0,
    };
    let terminal =
        resume_assistance_job(&collection, &job, &mut failed, 1).expect("safe terminal failure");
    assert_eq!(terminal.next_index, 1);
    assert_eq!(
        terminal.failures[&preview.ordered_target_ids[0]].code,
        AssistanceProviderFailureCode::Unavailable
    );
    assert!(
        !terminal
            .to_json()
            .expect("job JSON")
            .contains("definitely_not_a_real_credential")
    );
}

#[test]
fn batch_rate_limits_cancel_resume_apply_idempotently_and_compare_without_selection() {
    let collection = fixture_collection();
    let template = reference_assistance_template();
    let request = offline_batch_request(&collection, &template);
    let preview =
        preview_assistance_batch(&collection, &template, &request).expect("batch preview");
    assert_eq!(
        preview.ordered_target_ids.len(),
        collection.characters.len()
    );
    let approval = approve_assistance_batch_preview(
        &preview,
        "org.weave.reviewer.fixture",
        "Approve the complete deterministic offline batch disclosure.",
    )
    .expect("approval");
    assert_eq!(
        AssistanceExecutionApproval::from_json(&approval.to_json().expect("approval JSON"))
            .expect("parse approval"),
        approval
    );
    let job = start_assistance_job(&collection, &template, &preview, &approval).expect("job");
    let cancelled = cancel_assistance_job(&job, "Stop this exact batch before its first call.")
        .expect("cancellation");
    assert_eq!(cancelled.state, AssistanceJobState::Cancelled);
    let mut provider = OfflineAssistanceProvider;
    assert_eq!(
        resume_assistance_job(&collection, &cancelled, &mut provider, 10).expect("cancelled no-op"),
        cancelled
    );

    let first = resume_assistance_job(&collection, &job, &mut provider, 32)
        .expect("first rate-limited window");
    assert_eq!(first.state, AssistanceJobState::RateLimited);
    assert_eq!(first.next_index, 1);
    let serialized = AssistanceJob::from_ron(&first.to_ron().expect("job RON")).expect("parse job");
    let complete = resume_assistance_job(&collection, &serialized, &mut provider, 32)
        .expect("resumed completion");
    assert_eq!(complete.state, AssistanceJobState::ReadyForReview);
    assert_eq!(complete.next_index, complete.ordered_target_ids.len());

    let reviews = complete
        .candidate_sets
        .iter()
        .map(|(id, set)| (id.clone(), accept_all_review(set)))
        .collect::<BTreeMap<_, _>>();
    let advisories = complete
        .candidate_sets
        .iter()
        .map(|(id, set)| {
            (
                id.clone(),
                vec![
                    review_assistance_candidates_offline(set, "org.weave.reviewer.advisory")
                        .expect("advisory"),
                ],
            )
        })
        .collect::<BTreeMap<_, _>>();
    let receipt = apply_assistance_batch_reviews(&collection, &complete, &reviews, &advisories)
        .expect("batch application");
    let replay = apply_assistance_batch_reviews(
        &receipt.output_collection,
        &complete,
        &reviews,
        &advisories,
    )
    .expect("idempotent batch replay");
    assert_eq!(replay, receipt);
    assert_eq!(
        weave_character::AssistanceBatchReceipt::from_json(
            &receipt.to_json().expect("batch receipt JSON")
        )
        .expect("parse receipt"),
        receipt
    );

    let first_set = complete
        .candidate_sets
        .values()
        .next()
        .expect("offline candidate set")
        .clone();
    let external_request = request_with_provider(
        &collection,
        &template,
        &first_set.input_profile.id,
        external_provider(),
        Some(20_260_824),
    );
    let external_preview =
        preview_assistance_request(&collection, &template, &external_request).expect("preview");
    let external_approval = approve_assistance_preview(
        &external_preview,
        "org.weave.reviewer.fixture",
        "Approve the synthetic comparison adapter after inspecting its scope.",
    )
    .expect("approval");
    let mut external = ScriptedProvider {
        descriptor: external_provider(),
        credential_status: AssistanceCredentialStatus::Configured,
        actions: VecDeque::from([Ok(offline_raw(&external_preview))]),
        calls: 0,
    };
    let external_set = execute_assistance_request(
        &collection,
        &template,
        &external_preview,
        &external_approval,
        &mut external,
    )
    .expect("external set");
    let offline_advisory =
        review_assistance_candidates_offline(&first_set, "org.weave.reviewer.offline")
            .expect("offline advisory");
    let external_advisory =
        review_assistance_candidates_offline(&external_set, "org.weave.reviewer.external")
            .expect("external advisory");
    let comparison = compare_assistance_providers(
        &[first_set, external_set],
        &[offline_advisory, external_advisory],
    )
    .expect("comparison");
    assert!(comparison.advisory_only);
    assert!(!comparison.canonical_write_back);
    assert!(comparison.selected_candidate_id.is_none());
    let mut unsafe_comparison = comparison.clone();
    unsafe_comparison.entries[0]
        .issues
        .push(AssistanceAdvisoryIssue {
            code: "credential_shape".to_owned(),
            severity: AssistanceAdvisorySeverity::Caution,
            message: "api_key=definitely_not_a_real_secret".to_owned(),
        });
    let error =
        validate_assistance_comparison(&unsafe_comparison).expect_err("unsafe comparison issue");
    assert!(error.to_string().contains("[REDACTED]"));
    assert!(!error.to_string().contains("definitely_not_a_real_secret"));
}
