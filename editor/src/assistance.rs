//! Editor-facing preview, evidence inspection, advisory review, and explicit assistance decisions.

use std::collections::BTreeMap;

use weave_character::{
    AssistanceAdvisoryReview, AssistanceCandidate, AssistanceCandidateSet, AssistanceDecision,
    AssistanceDecisionReview, AssistanceError, AssistanceExecutionApproval, AssistancePreview,
    AssistanceReceipt, AssistanceRequest, AssistanceTemplate, CharacterAssistanceProvider,
    CharacterCollection, apply_assistance_review, approve_assistance_preview,
    create_assistance_decision_review, execute_assistance_request, preview_assistance_request,
    review_assistance_candidates_offline, validate_character_collection,
};

/// Editor-side atomic commit failure spanning profile and collection contracts.
#[derive(Debug, thiserror::Error)]
pub enum CharacterAssistanceSessionError {
    #[error(transparent)]
    Character(#[from] weave_character::CharacterError),
    #[error(transparent)]
    Collection(#[from] weave_character::CharacterCorpusError),
    #[error("Character assistance profile {profile_id} is absent from the open collection")]
    MissingProfile { profile_id: String },
    #[error("Character assistance collection revision overflowed")]
    RevisionOverflow,
}

/// In-memory editor session backed exclusively by the shared provider-neutral assistance contract.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterAssistanceSession {
    collection: CharacterCollection,
    template: AssistanceTemplate,
    request: AssistanceRequest,
    last_preview: Option<AssistancePreview>,
    last_candidates: Option<AssistanceCandidateSet>,
    last_advisory: Option<AssistanceAdvisoryReview>,
    last_receipt: Option<AssistanceReceipt>,
}

impl CharacterAssistanceSession {
    /// Open exact inputs only after reproducing their credential-free disclosure preview.
    pub fn open(
        collection: CharacterCollection,
        template: AssistanceTemplate,
        request: AssistanceRequest,
    ) -> Result<Self, weave_character::CharacterError> {
        let _ = preview_assistance_request(&collection, &template, &request)?;
        Ok(Self {
            collection,
            template,
            request,
            last_preview: None,
            last_candidates: None,
            last_advisory: None,
            last_receipt: None,
        })
    }

    #[must_use]
    pub const fn collection(&self) -> &CharacterCollection {
        &self.collection
    }

    #[must_use]
    pub const fn template(&self) -> &AssistanceTemplate {
        &self.template
    }

    #[must_use]
    pub const fn request(&self) -> &AssistanceRequest {
        &self.request
    }

    #[must_use]
    pub const fn last_preview(&self) -> Option<&AssistancePreview> {
        self.last_preview.as_ref()
    }

    #[must_use]
    pub const fn last_candidates(&self) -> Option<&AssistanceCandidateSet> {
        self.last_candidates.as_ref()
    }

    #[must_use]
    pub const fn last_advisory(&self) -> Option<&AssistanceAdvisoryReview> {
        self.last_advisory.as_ref()
    }

    #[must_use]
    pub const fn last_receipt(&self) -> Option<&AssistanceReceipt> {
        self.last_receipt.as_ref()
    }

    /// Render and retain the exact scope and expected output contract without calling a provider.
    pub fn preview(&mut self) -> Result<&AssistancePreview, weave_character::CharacterError> {
        let preview = preview_assistance_request(&self.collection, &self.template, &self.request)?;
        self.last_candidates = None;
        self.last_advisory = None;
        self.last_receipt = None;
        Ok(self.last_preview.insert(preview))
    }

    /// Record explicit approval for one retained, visible disclosure.
    pub fn approve(
        &self,
        preview: &AssistancePreview,
        author: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Result<AssistanceExecutionApproval, weave_character::CharacterError> {
        approve_assistance_preview(preview, author, rationale)
    }

    /// Invoke the exact selected adapter only after preview and approval match current inputs.
    pub fn execute<P: CharacterAssistanceProvider>(
        &mut self,
        preview: &AssistancePreview,
        approval: &AssistanceExecutionApproval,
        provider: &mut P,
    ) -> Result<&AssistanceCandidateSet, AssistanceError> {
        let candidates = execute_assistance_request(
            &self.collection,
            &self.template,
            preview,
            approval,
            provider,
        )?;
        self.last_advisory = None;
        self.last_receipt = None;
        Ok(self.last_candidates.insert(candidates))
    }

    /// Inspect one immutable typed candidate and its evidence without applying it.
    #[must_use]
    pub fn inspect<'a>(
        &self,
        candidates: &'a AssistanceCandidateSet,
        candidate_id: &str,
    ) -> Option<&'a AssistanceCandidate> {
        candidates.candidates.get(candidate_id)
    }

    /// Run and retain the deterministic advisory-only review pass.
    pub fn advise(
        &mut self,
        candidates: &AssistanceCandidateSet,
        reviewer: impl Into<String>,
    ) -> Result<&AssistanceAdvisoryReview, weave_character::CharacterError> {
        let advisory = review_assistance_candidates_offline(candidates, reviewer)?;
        Ok(self.last_advisory.insert(advisory))
    }

    /// Build a complete author review without mutating candidates or character data.
    pub fn review(
        &self,
        candidates: &AssistanceCandidateSet,
        author: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, AssistanceDecision>,
    ) -> Result<AssistanceDecisionReview, weave_character::CharacterError> {
        create_assistance_decision_review(candidates, author, rationale, decisions)
    }

    /// Reproduce one suggestion-only receipt. Dry-run retains the collection; commit swaps one
    /// profile atomically and increments the collection revision once when it changed.
    pub fn apply(
        &mut self,
        candidates: &AssistanceCandidateSet,
        review: &AssistanceDecisionReview,
        advisories: &[AssistanceAdvisoryReview],
        dry_run: bool,
    ) -> Result<&AssistanceReceipt, CharacterAssistanceSessionError> {
        let current_profile = self
            .collection
            .characters
            .get(&candidates.input_profile.id)
            .ok_or_else(|| CharacterAssistanceSessionError::MissingProfile {
                profile_id: candidates.input_profile.id.clone(),
            })?;
        let receipt = apply_assistance_review(current_profile, candidates, review, advisories)?;
        if !dry_run && current_profile != &receipt.output_profile {
            let mut collection = self.collection.clone();
            collection.characters.insert(
                candidates.input_profile.id.clone(),
                receipt.output_profile.clone(),
            );
            collection.revision = collection
                .revision
                .checked_add(1)
                .ok_or(CharacterAssistanceSessionError::RevisionOverflow)?;
            validate_character_collection(&collection)?;
            self.collection = collection;
            self.last_preview = None;
            self.last_candidates = None;
            self.last_advisory = None;
        }
        Ok(self.last_receipt.insert(receipt))
    }

    /// Replace exact request inputs after an author choice, clearing every stale retained artifact.
    pub fn set_request(
        &mut self,
        request: AssistanceRequest,
    ) -> Result<(), weave_character::CharacterError> {
        let _ = preview_assistance_request(&self.collection, &self.template, &request)?;
        self.request = request;
        self.last_preview = None;
        self.last_candidates = None;
        self.last_advisory = None;
        self.last_receipt = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_character::{
        AssistanceAdvisoryReview, AssistanceCandidateSet, AssistanceDecisionReview,
        AssistanceReceipt, OfflineAssistanceProvider,
    };

    const COLLECTION: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/input.character-collection.json"
    );
    const TEMPLATE: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/glasswind.assistance-template.json"
    );
    const REQUEST: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-request.json"
    );
    const PREVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-preview.json"
    );
    const APPROVAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-approval.json"
    );
    const CANDIDATES: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-candidate-set.json"
    );
    const ADVISORY: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-advisory-review.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-decision-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/assistance/single.assistance-receipt.json"
    );

    #[test]
    fn editor_preview_inspect_advice_review_and_atomic_commit_match_shared_artifacts() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let template = AssistanceTemplate::from_json(TEMPLATE).unwrap();
        let request = AssistanceRequest::from_json(REQUEST).unwrap();
        let expected_preview = AssistancePreview::from_json(PREVIEW).unwrap();
        let expected_approval = AssistanceExecutionApproval::from_json(APPROVAL).unwrap();
        let expected_candidates = AssistanceCandidateSet::from_json(CANDIDATES).unwrap();
        let expected_advisory = AssistanceAdvisoryReview::from_json(ADVISORY).unwrap();
        let expected_review = AssistanceDecisionReview::from_json(REVIEW).unwrap();
        let expected_receipt = AssistanceReceipt::from_json(RECEIPT).unwrap();
        let original_revision = collection.revision;
        let mut session =
            CharacterAssistanceSession::open(collection.clone(), template, request).unwrap();

        let preview = session.preview().unwrap().clone();
        assert_eq!(preview, expected_preview);
        assert_eq!(
            session
                .approve(
                    &preview,
                    "org.weave.reviewer.fixture",
                    "Approve the exact visible offline scope for this original synthetic fixture.",
                )
                .unwrap(),
            expected_approval
        );
        let mut provider = OfflineAssistanceProvider;
        let candidates = session
            .execute(&preview, &expected_approval, &mut provider)
            .unwrap()
            .clone();
        assert_eq!(candidates, expected_candidates);
        let inspected = session
            .inspect(&candidates, candidates.candidates.keys().next().unwrap())
            .unwrap();
        assert!(!inspected.evidence.is_empty());
        assert!(inspected.validation.safe);

        let advisory = session
            .advise(&candidates, "org.weave.reviewer.advisory")
            .unwrap()
            .clone();
        assert_eq!(advisory, expected_advisory);
        assert_eq!(
            session
                .review(
                    &candidates,
                    "org.weave.reviewer.fixture",
                    "Exercise accept, edit, reject, defer, and regenerate as explicit fixture decisions.",
                    expected_review.decisions.clone(),
                )
                .unwrap(),
            expected_review
        );

        assert_eq!(
            session
                .apply(
                    &candidates,
                    &expected_review,
                    std::slice::from_ref(&advisory),
                    true,
                )
                .unwrap(),
            &expected_receipt
        );
        assert_eq!(session.collection(), &collection);
        assert_eq!(
            session
                .apply(
                    &candidates,
                    &expected_review,
                    std::slice::from_ref(&advisory),
                    false,
                )
                .unwrap(),
            &expected_receipt
        );
        assert_eq!(session.collection().revision, original_revision + 1);
        assert_eq!(
            session.collection().characters[&candidates.input_profile.id],
            expected_receipt.output_profile
        );
    }
}
