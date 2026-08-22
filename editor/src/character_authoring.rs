//! Keyboard-operable guided Character authoring backed by `weave-character` source artifacts.

use weave_character::{
    CharacterAuthoringDraft, CharacterAuthoringDraftSummary, CharacterAuthoringFieldView,
    CharacterAuthoringPreview, CharacterAuthoringRevision, CharacterAuthoringWorkspace,
    CharacterFinalReviewDecision, CharacterOverlay, CharacterProfile, CharacterTemplate,
    apply_authoring_revision, clone_authoring_draft, create_authoring_draft,
    export_authoring_profile, inspect_authoring_fields, list_authoring_drafts,
    preview_authoring_revision, review_authoring_draft, show_authoring_draft,
    validate_authoring_workspace,
};

/// Stable focus order shared by visual and text-equivalent authoring surfaces.
pub const CHARACTER_AUTHORING_CONTROLS: [CharacterAuthoringControl; 14] = [
    CharacterAuthoringControl::Identity,
    CharacterAuthoringControl::Presentation,
    CharacterAuthoringControl::BirthDate,
    CharacterAuthoringControl::DirectFacets,
    CharacterAuthoringControl::Questionnaire,
    CharacterAuthoringControl::ConfidenceReview,
    CharacterAuthoringControl::DerivedOcean,
    CharacterAuthoringControl::AlignmentReview,
    CharacterAuthoringControl::DateContextReview,
    CharacterAuthoringControl::InnerLife,
    CharacterAuthoringControl::Voice,
    CharacterAuthoringControl::ConflictReview,
    CharacterAuthoringControl::FinalReview,
    CharacterAuthoringControl::Export,
];

/// Closed authoring panels. Their names map directly to documented source artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterAuthoringControl {
    Identity,
    Presentation,
    BirthDate,
    DirectFacets,
    Questionnaire,
    ConfidenceReview,
    DerivedOcean,
    AlignmentReview,
    DateContextReview,
    InnerLife,
    Voice,
    ConflictReview,
    FinalReview,
    Export,
}

impl CharacterAuthoringControl {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::Presentation => "Pronouns, appearance, palette, and assets",
            Self::BirthDate => "Birth date",
            Self::DirectFacets => "Direct HEXACO facets",
            Self::Questionnaire => "Narrative questionnaire",
            Self::ConfidenceReview => "Facet confidence review",
            Self::DerivedOcean => "Derived OCEAN compatibility view",
            Self::AlignmentReview => "Optional alignment review",
            Self::DateContextReview => "Historical-context candidates",
            Self::InnerLife => "Inner life",
            Self::Voice => "Voice direction",
            Self::ConflictReview => "Conflict review",
            Self::FinalReview => "Final validation and review",
            Self::Export => "Export",
        }
    }

    #[must_use]
    pub const fn source_representation(self) -> &'static str {
        match self {
            Self::Identity => "CharacterOverlay.operations.set_display_name/set_aliases",
            Self::Presentation => {
                "CharacterOverlay.operations presentation actions + PresentationReceipt"
            }
            Self::BirthDate => "CharacterOverlay.operations.set_birth_date",
            Self::DirectFacets => "CharacterOverlay.operations.set_hexaco_trait",
            Self::Questionnaire => "CharacterQuestionnaireProposal",
            Self::ConfidenceReview => "CharacterQuestionnaireReview",
            Self::DerivedOcean => "CharacterProfile.derived.ocean (read-only)",
            Self::AlignmentReview => "AlignmentReview + AlignmentReceipt",
            Self::DateContextReview => "TemporalContextReview + TemporalContextReceipt",
            Self::InnerLife => "CharacterOverlay.operations.upsert_inner_life",
            Self::Voice => "CharacterOverlay.operations.upsert_voice",
            Self::ConflictReview => "CharacterQuestionnaireReview.conflict_decisions",
            Self::FinalReview => "CharacterFinalReview",
            Self::Export => "validated CharacterProfile",
        }
    }
}

/// Logical keys accepted by the authoring surface independent of a particular window toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterAuthoringKey {
    Next,
    Previous,
    Home,
    End,
    Activate,
}

/// Result of one keyboard command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterAuthoringKeyboardAction {
    Focused(CharacterAuthoringControl),
    Activated(CharacterAuthoringControl),
}

/// Accessibility metadata for the currently focused authoring control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterAuthoringAccessibility {
    pub role: &'static str,
    pub label: &'static str,
    pub source_representation: &'static str,
    pub position: usize,
    pub total: usize,
    pub keyboard_hint: &'static str,
    pub read_only_derived: bool,
}

/// Editor session using the exact same create/clone/revise/review/export functions as the CLI.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterAuthoringSession {
    workspace: CharacterAuthoringWorkspace,
    selected_draft_id: Option<String>,
    focus_index: usize,
    last_preview: Option<CharacterAuthoringPreview>,
}

impl CharacterAuthoringSession {
    /// Reopen and validate one serialized workspace without changing it.
    pub fn open(
        workspace: CharacterAuthoringWorkspace,
    ) -> Result<Self, weave_character::CharacterError> {
        validate_authoring_workspace(&workspace)?;
        let selected_draft_id = workspace.drafts.keys().next().cloned();
        Ok(Self {
            workspace,
            selected_draft_id,
            focus_index: 0,
            last_preview: None,
        })
    }

    #[must_use]
    pub const fn workspace(&self) -> &CharacterAuthoringWorkspace {
        &self.workspace
    }

    #[must_use]
    pub fn selected_draft_id(&self) -> Option<&str> {
        self.selected_draft_id.as_deref()
    }

    pub fn select(&mut self, id: &str) -> Result<(), weave_character::CharacterError> {
        show_authoring_draft(&self.workspace, id)?;
        self.selected_draft_id = Some(id.to_owned());
        self.last_preview = None;
        Ok(())
    }

    pub fn list(
        &self,
    ) -> Result<Vec<CharacterAuthoringDraftSummary>, weave_character::CharacterError> {
        list_authoring_drafts(&self.workspace)
    }

    pub fn show(
        &self,
        id: &str,
    ) -> Result<&CharacterAuthoringDraft, weave_character::CharacterError> {
        show_authoring_draft(&self.workspace, id)
    }

    /// Create from a blank overlay or exact versioned template as one atomic editor transition.
    pub fn create(
        &mut self,
        template: Option<CharacterTemplate>,
        overlay: CharacterOverlay,
    ) -> Result<&CharacterAuthoringDraft, weave_character::CharacterError> {
        let id = overlay.character_id.clone();
        let candidate = create_authoring_draft(&self.workspace, template, overlay)?;
        self.workspace = candidate;
        self.selected_draft_id = Some(id.clone());
        self.last_preview = None;
        self.show(&id)
    }

    /// Clone retained template/overlay source and fail atomically on broken owned references.
    pub fn clone_draft(
        &mut self,
        source_id: &str,
        new_character_id: impl Into<String>,
        new_overlay_id: impl Into<String>,
    ) -> Result<&CharacterAuthoringDraft, weave_character::CharacterError> {
        let new_character_id = new_character_id.into();
        let candidate = clone_authoring_draft(
            &self.workspace,
            source_id,
            new_character_id.clone(),
            new_overlay_id,
        )?;
        self.workspace = candidate;
        self.selected_draft_id = Some(new_character_id.clone());
        self.last_preview = None;
        self.show(&new_character_id)
    }

    /// Preview base/effective differences and invalidation effects without changing editor state.
    pub fn preview(
        &mut self,
        revision: &CharacterAuthoringRevision,
    ) -> Result<&CharacterAuthoringPreview, weave_character::CharacterError> {
        let preview = preview_authoring_revision(&self.workspace, revision)?;
        self.last_preview = Some(preview);
        Ok(self.last_preview.as_ref().expect("preview just assigned"))
    }

    #[must_use]
    pub const fn last_preview(&self) -> Option<&CharacterAuthoringPreview> {
        self.last_preview.as_ref()
    }

    /// Apply one previously previewable revision. Errors preserve the entire prior workspace.
    pub fn revise(
        &mut self,
        revision: &CharacterAuthoringRevision,
    ) -> Result<&CharacterAuthoringDraft, weave_character::CharacterError> {
        let candidate = apply_authoring_revision(&self.workspace, revision)?;
        self.workspace = candidate;
        self.selected_draft_id = Some(revision.draft_id.clone());
        self.last_preview = None;
        self.show(&revision.draft_id)
    }

    /// Save the complete final summary and unresolved diagnostics.
    pub fn review(
        &mut self,
        id: &str,
        reviewer: impl Into<String>,
        rationale: impl Into<String>,
        decision: CharacterFinalReviewDecision,
    ) -> Result<&CharacterAuthoringDraft, weave_character::CharacterError> {
        let candidate = review_authoring_draft(&self.workspace, id, reviewer, rationale, decision)?;
        self.workspace = candidate;
        self.selected_draft_id = Some(id.to_owned());
        self.last_preview = None;
        self.show(id)
    }

    pub fn export(
        &self,
        id: &str,
        require_final_review: bool,
    ) -> Result<CharacterProfile, weave_character::CharacterError> {
        export_authoring_profile(&self.workspace, id, require_final_review)
    }

    pub fn fields(
        &self,
        id: &str,
    ) -> Result<Vec<CharacterAuthoringFieldView>, weave_character::CharacterError> {
        inspect_authoring_fields(self.show(id)?)
    }

    #[must_use]
    pub const fn focused_control(&self) -> CharacterAuthoringControl {
        CHARACTER_AUTHORING_CONTROLS[self.focus_index]
    }

    #[must_use]
    pub fn accessibility(&self) -> CharacterAuthoringAccessibility {
        let control = self.focused_control();
        CharacterAuthoringAccessibility {
            role: "tab",
            label: control.label(),
            source_representation: control.source_representation(),
            position: self.focus_index + 1,
            total: CHARACTER_AUTHORING_CONTROLS.len(),
            keyboard_hint: "Tab/Shift+Tab move; Home/End jump; Enter or Space activates",
            read_only_derived: control == CharacterAuthoringControl::DerivedOcean,
        }
    }

    pub fn handle_key(&mut self, key: CharacterAuthoringKey) -> CharacterAuthoringKeyboardAction {
        match key {
            CharacterAuthoringKey::Next => {
                self.focus_index = (self.focus_index + 1) % CHARACTER_AUTHORING_CONTROLS.len();
                CharacterAuthoringKeyboardAction::Focused(self.focused_control())
            }
            CharacterAuthoringKey::Previous => {
                self.focus_index = self
                    .focus_index
                    .checked_sub(1)
                    .unwrap_or(CHARACTER_AUTHORING_CONTROLS.len() - 1);
                CharacterAuthoringKeyboardAction::Focused(self.focused_control())
            }
            CharacterAuthoringKey::Home => {
                self.focus_index = 0;
                CharacterAuthoringKeyboardAction::Focused(self.focused_control())
            }
            CharacterAuthoringKey::End => {
                self.focus_index = CHARACTER_AUTHORING_CONTROLS.len() - 1;
                CharacterAuthoringKeyboardAction::Focused(self.focused_control())
            }
            CharacterAuthoringKey::Activate => {
                CharacterAuthoringKeyboardAction::Activated(self.focused_control())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use weave_domain::{Provenance, ProvenanceKind, ProvenanceSource};

    fn provenance() -> Provenance {
        Provenance {
            sources: vec![ProvenanceSource {
                id: "editor_original".to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "guided-authoring-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
                attribution: "Original synthetic editor authoring test.".to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([("workspace".to_owned(), vec!["editor_original".to_owned()])]),
        }
    }

    const EMPTY: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/workspace.empty.authoring-workspace.json"
    );
    const OVERLAY: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/blank.authoring-overlay.json"
    );
    const CREATED: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/workspace.created.authoring-workspace.json"
    );
    const REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/questionnaire.authoring-revision.json"
    );
    const REVISED: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/workspace.revised.authoring-workspace.json"
    );
    const REVIEWED: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/workspace.reviewed.authoring-workspace.json"
    );
    const EXPORTED: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/exported.character.json"
    );
    const INVALIDATION_WORKSPACE: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/invalidation.enriched.authoring-workspace.json"
    );
    const INVALIDATION_REVISION: &str = include_str!(
        "../../examples/domain-modules/weave-character/authoring/invalidation.authoring-revision.json"
    );

    #[test]
    fn every_authoring_control_is_keyboard_reachable_and_source_named() {
        let workspace = weave_character::new_authoring_workspace(
            "org.weave.character.authoring.editor",
            provenance(),
        )
        .unwrap();
        let mut session = CharacterAuthoringSession::open(workspace).unwrap();
        let mut reached = Vec::new();
        for _ in 0..CHARACTER_AUTHORING_CONTROLS.len() {
            let accessibility = session.accessibility();
            assert!(!accessibility.label.is_empty());
            assert!(!accessibility.source_representation.is_empty());
            reached.push(session.focused_control());
            session.handle_key(CharacterAuthoringKey::Next);
        }
        assert_eq!(reached, CHARACTER_AUTHORING_CONTROLS);
        assert_eq!(
            session.handle_key(CharacterAuthoringKey::End),
            CharacterAuthoringKeyboardAction::Focused(CharacterAuthoringControl::Export)
        );
        assert_eq!(
            session.handle_key(CharacterAuthoringKey::Activate),
            CharacterAuthoringKeyboardAction::Activated(CharacterAuthoringControl::Export)
        );
        session.handle_key(CharacterAuthoringKey::Home);
        session.handle_key(CharacterAuthoringKey::Previous);
        assert_eq!(session.focused_control(), CharacterAuthoringControl::Export);
    }

    #[test]
    fn editor_create_revise_invalidate_review_export_and_reopen_match_text_bytes() {
        let mut session =
            CharacterAuthoringSession::open(CharacterAuthoringWorkspace::from_json(EMPTY).unwrap())
                .unwrap();
        session
            .create(None, CharacterOverlay::from_json(OVERLAY).unwrap())
            .unwrap();
        assert_eq!(
            session.workspace(),
            &CharacterAuthoringWorkspace::from_json(CREATED).unwrap()
        );
        let revision = CharacterAuthoringRevision::from_json(REVISION).unwrap();
        let preview = session.preview(&revision).unwrap();
        assert!(preview.derived_ocean.recomputed_in_candidate);
        assert_eq!(session.workspace().to_json().unwrap(), CREATED);
        session.revise(&revision).unwrap();
        assert_eq!(session.workspace().to_json().unwrap(), REVISED);
        session
            .review(
                "org.weave.character.lumen_reed",
                "org.weave.reviewer.final",
                "Canonical fields, derived views, context, provenance, and diagnostics are ready.",
                CharacterFinalReviewDecision::Accepted,
            )
            .unwrap();
        assert_eq!(session.workspace().to_json().unwrap(), REVIEWED);
        assert_eq!(
            session
                .export("org.weave.character.lumen_reed", true)
                .unwrap()
                .to_json()
                .unwrap(),
            EXPORTED
        );
        let reopened = CharacterAuthoringSession::open(
            CharacterAuthoringWorkspace::from_json(&session.workspace().to_json().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(reopened.workspace(), session.workspace());

        let mut invalidation_session = CharacterAuthoringSession::open(
            CharacterAuthoringWorkspace::from_json(INVALIDATION_WORKSPACE).unwrap(),
        )
        .unwrap();
        let invalidation_revision =
            CharacterAuthoringRevision::from_json(INVALIDATION_REVISION).unwrap();
        assert!(
            invalidation_session
                .preview(&invalidation_revision)
                .unwrap()
                .invalidations
                .iter()
                .any(|item| item.id == "alignment_review" && item.blocking_final_review)
        );
        invalidation_session.revise(&invalidation_revision).unwrap();
        assert!(
            invalidation_session
                .review(
                    "org.weave.character.ari_vale",
                    "org.weave.reviewer.final",
                    "Alignment must be refreshed before this draft is ready to export.",
                    CharacterFinalReviewDecision::NeedsChanges,
                )
                .unwrap()
                .final_review
                .as_ref()
                .is_some_and(|review| !review.summary.unresolved_diagnostics.is_empty())
        );
        assert!(
            invalidation_session
                .export("org.weave.character.ari_vale", true)
                .is_err()
        );
    }
}
