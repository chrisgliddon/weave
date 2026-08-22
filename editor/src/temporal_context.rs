//! Editor-facing temporal-context review backed entirely by the shared Character contract.

use std::collections::BTreeMap;

use weave_character::{
    CharacterError, CharacterProfile, TemporalContextCandidate, TemporalContextConfig,
    TemporalContextPack, TemporalContextProposal, TemporalContextReceipt, TemporalContextReview,
    TemporalCoverageEntry, TemporalRankingTrace, TemporalReviewDecision,
    apply_reviewed_temporal_context, create_temporal_context_review, propose_temporal_context,
    validate_temporal_context_proposal,
};

/// In-memory editor surface for explainable, atomic temporal enrichment.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalContextSession {
    profile: CharacterProfile,
    packs: Vec<TemporalContextPack>,
    config: TemporalContextConfig,
    seed: u64,
    proposal: TemporalContextProposal,
}

impl TemporalContextSession {
    /// Open a session and produce its exact immutable proposal without changing the profile.
    pub fn open(
        profile: CharacterProfile,
        packs: Vec<TemporalContextPack>,
        config: TemporalContextConfig,
        seed: u64,
    ) -> Result<Self, CharacterError> {
        let proposal = propose_temporal_context(&profile, &packs, &config, seed)?;
        Ok(Self {
            profile,
            packs,
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

    /// Current immutable review proposal. It becomes visibly stale after a committed apply.
    #[must_use]
    pub const fn proposal(&self) -> &TemporalContextProposal {
        &self.proposal
    }

    /// Ranked candidates carrying content, separate fact/cue lineage, and exact score traces.
    #[must_use]
    pub fn candidates(&self) -> &[TemporalContextCandidate] {
        self.proposal.candidates.as_slice()
    }

    /// Selected, downgraded, and skipped coverage entries for the complete pack scan.
    #[must_use]
    pub fn coverage(&self) -> &[TemporalCoverageEntry] {
        self.proposal.coverage.as_slice()
    }

    /// Exact, human-readable "why" trace for one ranked cue.
    #[must_use]
    pub fn why(&self, candidate_id: &str) -> Option<&TemporalRankingTrace> {
        self.candidates()
            .iter()
            .find(|candidate| candidate.id == candidate_id)
            .map(|candidate| &candidate.trace)
    }

    /// Create a complete accept/reject/edit/withhold/override review through shared validation.
    pub fn review(
        &self,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decisions: BTreeMap<String, TemporalReviewDecision>,
    ) -> Result<TemporalContextReview, CharacterError> {
        create_temporal_context_review(&self.proposal, reviewer, rationale, decisions)
    }

    /// Reproduce a complete review and optionally commit it as one editor state transition.
    ///
    /// Dry-run returns the exact receipt and preserves profile/proposal state. A committed apply
    /// leaves the consumed proposal inspectable but stale so it cannot be silently reused.
    pub fn apply(
        &mut self,
        review: &TemporalContextReview,
        dry_run: bool,
    ) -> Result<TemporalContextReceipt, CharacterError> {
        let receipt =
            apply_reviewed_temporal_context(&self.profile, &self.packs, &self.proposal, review)?;
        if !dry_run {
            self.profile = receipt.output_profile.clone();
        }
        Ok(receipt)
    }

    /// Report whether an externally retained proposal is stale against current editor state.
    #[must_use]
    pub fn is_stale(&self, proposal: &TemporalContextProposal) -> bool {
        validate_temporal_context_proposal(proposal, &self.profile, &self.packs).is_err()
    }

    /// Re-rank from current accepted state and exact configured packs.
    pub fn refresh(&mut self) -> Result<&TemporalContextProposal, CharacterError> {
        let proposal =
            propose_temporal_context(&self.profile, &self.packs, &self.config, self.seed)?;
        self.proposal = proposal;
        Ok(&self.proposal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str =
        include_str!("../../examples/domain-modules/weave-character/context/input.character.json");
    const APOLLO: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json"
    );
    const CALENDAR: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/calendar.temporal-pack.json"
    );
    const WORLD: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/world.temporal-pack.json"
    );
    const CONFIG: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/ranking.temporal-config.json"
    );
    const PROPOSAL: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/proposal.temporal-proposal.json"
    );
    const REVIEW: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/review.temporal-review.json"
    );
    const RECEIPT: &str = include_str!(
        "../../examples/domain-modules/weave-character/context/receipt.temporal-receipt.json"
    );

    fn session() -> TemporalContextSession {
        TemporalContextSession::open(
            CharacterProfile::from_json(PROFILE).unwrap(),
            vec![
                TemporalContextPack::from_json(APOLLO).unwrap(),
                TemporalContextPack::from_json(CALENDAR).unwrap(),
                TemporalContextPack::from_json(WORLD).unwrap(),
            ],
            TemporalContextConfig::from_json(CONFIG).unwrap(),
            19_690_720,
        )
        .unwrap()
    }

    #[test]
    fn editor_why_coverage_and_proposal_match_checked_source_bytes() {
        let session = session();
        let expected = TemporalContextProposal::from_json(PROPOSAL).unwrap();
        assert_eq!(session.proposal(), &expected);
        assert!(session.coverage().iter().any(|entry| {
            entry.disposition == weave_character::TemporalCoverageDisposition::Downgraded
        }));
        let candidate = session.candidates().first().unwrap();
        assert_eq!(session.why(&candidate.id), Some(&candidate.trace));
        assert!(!candidate.fact_source_ids.is_empty());
        assert!(!candidate.cue_source_ids.is_empty());
    }

    #[test]
    fn editor_review_dry_run_commit_and_stale_state_are_atomic() {
        let mut session = session();
        let proposal = session.proposal().clone();
        let expected_review = TemporalContextReview::from_json(REVIEW).unwrap();
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
        assert_eq!(dry_run, TemporalContextReceipt::from_json(RECEIPT).unwrap());

        let committed = session.apply(&recreated, false).unwrap();
        assert_eq!(session.profile(), &committed.output_profile);
        assert!(session.is_stale(session.proposal()));
        assert!(session.is_stale(&proposal));
        assert!(session.apply(&recreated, false).is_err());
        assert_eq!(session.profile(), &committed.output_profile);
    }
}
