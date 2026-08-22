//! Editor-facing narrative-alignment review backed by the shared Character contract.

use std::collections::BTreeMap;

use weave_character::{
    AlignmentConfig, AlignmentPack, AlignmentProposal, AlignmentProposedValue, AlignmentReceipt,
    AlignmentReview, AlignmentReviewDecision, AlignmentScoreTrace, CharacterError,
    CharacterProfile, apply_reviewed_alignment, create_alignment_review, propose_alignment,
    validate_alignment_proposal_selection,
};

/// In-memory editor surface for explainable, pluggable, atomic alignment review.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentSession {
    profile: CharacterProfile,
    pack: AlignmentPack,
    config: AlignmentConfig,
    seed: u64,
    proposal: AlignmentProposal,
}

impl AlignmentSession {
    /// Select one exact pack and configuration and produce an immutable proposal.
    pub fn open(
        profile: CharacterProfile,
        pack: AlignmentPack,
        config: AlignmentConfig,
        seed: u64,
    ) -> Result<Self, CharacterError> {
        let proposal = propose_alignment(&profile, &pack, &config, seed)?;
        Ok(Self {
            profile,
            pack,
            config,
            seed,
            proposal,
        })
    }

    /// Current accepted profile state.
    #[must_use]
    pub const fn profile(&self) -> &CharacterProfile {
        &self.profile
    }

    /// Exact selected declarative pack, including methodology, labels, and limitations.
    #[must_use]
    pub const fn pack(&self) -> &AlignmentPack {
        &self.pack
    }

    /// Exact project selection and evidence-coverage policy.
    #[must_use]
    pub const fn config(&self) -> &AlignmentConfig {
        &self.config
    }

    /// Current immutable proposal. It becomes visibly stale after a committed apply.
    #[must_use]
    pub const fn proposal(&self) -> &AlignmentProposal {
        &self.proposal
    }

    /// Selected axis values in stable pack-independent identity order.
    #[must_use]
    pub const fn values(&self) -> &BTreeMap<String, AlignmentProposedValue> {
        &self.proposal.values
    }

    /// Exact fixed-point evidence, coverage, threshold, and seed trace for one axis.
    #[must_use]
    pub fn why(&self, axis_id: &str) -> Option<&AlignmentScoreTrace> {
        self.values().get(axis_id).map(|value| &value.trace)
    }

    /// Create a complete accept/reject/edit/withhold/override review.
    pub fn review(
        &self,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, AlignmentReviewDecision>,
    ) -> Result<AlignmentReview, CharacterError> {
        create_alignment_review(&self.proposal, &self.pack, reviewer, rationale, decisions)
    }

    /// Reproduce a complete review and optionally commit it as one editor transition.
    ///
    /// Dry-run returns the exact receipt while preserving profile and proposal state. A committed
    /// apply retains the consumed proposal for inspection, where it is visibly stale and cannot be
    /// reused. Only approved values enter the profile; rejected and withheld axes stay in receipt
    /// history.
    pub fn apply(
        &mut self,
        review: &AlignmentReview,
        dry_run: bool,
    ) -> Result<AlignmentReceipt, CharacterError> {
        let receipt = apply_reviewed_alignment(&self.profile, &self.pack, &self.proposal, review)?;
        if !dry_run {
            self.profile = receipt.output_profile.clone();
        }
        Ok(receipt)
    }

    /// Report whether an externally retained proposal is stale against current editor state.
    #[must_use]
    pub fn is_stale(&self, proposal: &AlignmentProposal) -> bool {
        validate_alignment_proposal_selection(
            proposal,
            &self.profile,
            &self.pack,
            &self.config,
            self.seed,
        )
        .is_err()
    }

    /// Recompute from current accepted state and the exact selected pack and configuration.
    pub fn refresh(&mut self) -> Result<&AlignmentProposal, CharacterError> {
        self.proposal = propose_alignment(&self.profile, &self.pack, &self.config, self.seed)?;
        Ok(&self.proposal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_character::{
        ALIGNMENT_EXTENSION_NAMESPACE, AlignmentReviewAction, CharacterExtension,
    };

    const PROFILE: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/input.character.json"
    );
    const PACK: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json"
    );
    const CONFIG: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/selection.alignment-config.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/proposal.alignment-proposal.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/review.alignment-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/alignment/receipt.alignment-receipt.json"
    );

    fn session() -> AlignmentSession {
        AlignmentSession::open(
            CharacterProfile::from_json(PROFILE).unwrap(),
            AlignmentPack::from_json(PACK).unwrap(),
            AlignmentConfig::from_json(CONFIG).unwrap(),
            20_260_822,
        )
        .unwrap()
    }

    #[test]
    fn editor_selection_values_and_why_match_checked_source_bytes() {
        let session = session();
        assert_eq!(
            session.proposal(),
            &AlignmentProposal::from_json(PROPOSAL).unwrap()
        );
        assert_eq!(session.values().len(), 5);
        assert_eq!(session.pack().axes.len(), 5);
        assert_eq!(session.config().selected_axes.len(), 5);

        let trace = session.why("horizon").expect("selected axis has a trace");
        assert_eq!(trace.coverage_micros, 1_000_000);
        assert_eq!(trace.threshold_id.as_deref(), Some("seeking"));
        assert!(trace.ordered_inputs.iter().all(|input| {
            input.profile_path.starts_with("canon.personality.") && input.profile_micros.is_some()
        }));
        assert!(session.why("not_declared").is_none());
    }

    #[test]
    fn editor_review_dry_run_commit_and_stale_state_are_atomic() {
        let mut session = session();
        let proposal = session.proposal().clone();
        let expected_review = AlignmentReview::from_json(REVIEW).unwrap();
        assert!(
            expected_review.decisions.values().any(|decision| {
                matches!(decision.action, AlignmentReviewAction::Override { .. })
            })
        );
        let recreated = session
            .review(
                expected_review.reviewer.clone(),
                expected_review.rationale.clone(),
                expected_review.decisions.clone(),
            )
            .unwrap();
        assert_eq!(recreated, expected_review);

        let input = session.profile().clone();
        let dry_run = session.apply(&recreated, true).unwrap();
        assert_eq!(session.profile(), &input);
        assert_eq!(dry_run, AlignmentReceipt::from_json(RECEIPT).unwrap());

        let committed = session.apply(&recreated, false).unwrap();
        assert_eq!(session.profile(), &committed.output_profile);
        let CharacterExtension::AlignmentView(view) =
            &session.profile().extensions[ALIGNMENT_EXTENSION_NAMESPACE]
        else {
            panic!("reserved alignment namespace has the typed view")
        };
        assert_eq!(view.value.values.len(), 3);
        assert!(!view.value.values.contains_key("signal"));
        assert!(!view.value.values.contains_key("tempo"));
        assert!(session.is_stale(session.proposal()));
        assert!(session.is_stale(&proposal));
        assert!(session.apply(&recreated, false).is_err());
        assert_eq!(session.profile(), &committed.output_profile);
    }

    #[test]
    fn editor_marks_proposals_stale_after_configuration_or_seed_reselection() {
        let original = session().proposal().clone();
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let pack = AlignmentPack::from_json(PACK).unwrap();
        let mut changed_config = AlignmentConfig::from_json(CONFIG).unwrap();
        changed_config.minimum_coverage_micros = 500_000;
        let changed_config_session =
            AlignmentSession::open(profile.clone(), pack.clone(), changed_config, 20_260_822)
                .unwrap();
        assert!(changed_config_session.is_stale(&original));

        let changed_seed_session = AlignmentSession::open(
            profile,
            pack,
            AlignmentConfig::from_json(CONFIG).unwrap(),
            20_260_823,
        )
        .unwrap();
        assert!(changed_seed_session.is_stale(&original));
    }
}
