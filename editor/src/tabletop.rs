//! Generic editor discovery for declarative tabletop adapter contracts.

use std::collections::BTreeMap;

use weave_tabletop::{
    AdapterManifest, AdapterSelection, CreationStep, PlugAndPlayCreationPreview,
    PlugAndPlayCreationRequest, PlugAndPlayStatSource, ResolvedAdapter, TabletopCapability,
    TabletopError, create_plug_and_play_character, has_capability, resolved_adapter,
    validate_adapter_manifest, validate_adapter_selection, validate_plug_and_play_creation_preview,
};

/// One manifest-driven panel; no adapter id is matched in editor code.
#[derive(Debug, Clone, PartialEq)]
pub struct TabletopPanelInspection {
    pub adapter: ResolvedAdapter,
    pub capability: TabletopCapability,
    pub capability_version: u32,
    pub creation_steps: Vec<CreationStep>,
    pub operations: Vec<String>,
}

/// Installed manifests, exact primary selection, and capability-driven panels.
#[derive(Debug, Clone)]
pub struct TabletopEditorCatalog {
    manifests: BTreeMap<String, AdapterManifest>,
    selection: AdapterSelection,
}

/// One explicit reroll edge retained for editor audit and undo presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlugAndPlaySeedLineage {
    pub previous_seed: u64,
    pub next_seed: u64,
    pub previous_request_sha256: String,
    pub next_request_sha256: String,
}

/// Concrete authoring session layered over the portable creation request and preview artifacts.
///
/// A preview is never silently accepted. Editing or rerolling clears acceptance, while every
/// reroll records both seeds and both canonical request fingerprints.
#[derive(Debug, Clone)]
pub struct PlugAndPlayCreationSession {
    request: PlugAndPlayCreationRequest,
    preview: PlugAndPlayCreationPreview,
    accepted: bool,
    seed_lineage: Vec<PlugAndPlaySeedLineage>,
}

impl PlugAndPlayCreationSession {
    pub fn new(request: PlugAndPlayCreationRequest) -> Result<Self, TabletopError> {
        let preview = create_plug_and_play_character(&request)?;
        Ok(Self {
            request,
            preview,
            accepted: false,
            seed_lineage: Vec::new(),
        })
    }

    #[must_use]
    pub fn request(&self) -> &PlugAndPlayCreationRequest {
        &self.request
    }

    #[must_use]
    pub fn preview(&self) -> &PlugAndPlayCreationPreview {
        &self.preview
    }

    #[must_use]
    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    #[must_use]
    pub fn seed_lineage(&self) -> &[PlugAndPlaySeedLineage] {
        &self.seed_lineage
    }

    /// Replace the full form value and recompute an explainable preview atomically.
    pub fn update(&mut self, request: PlugAndPlayCreationRequest) -> Result<(), TabletopError> {
        let preview = create_plug_and_play_character(&request)?;
        self.request = request;
        self.preview = preview;
        self.accepted = false;
        Ok(())
    }

    /// Recompute rolled candidates from a deliberately supplied, different seed.
    pub fn reroll(&mut self, next_seed: u64) -> Result<(), TabletopError> {
        if !matches!(
            self.request.stat_source,
            PlugAndPlayStatSource::Rolled { .. }
        ) {
            return Err(TabletopError::InvalidField {
                path: "editor.stat_source".to_owned(),
                reason: "reroll is available only for rolled creation",
            });
        }
        if next_seed == self.request.seed {
            return Err(TabletopError::InvalidField {
                path: "editor.seed".to_owned(),
                reason: "reroll requires a different explicit seed",
            });
        }
        let previous_seed = self.request.seed;
        let previous_request_sha256 = self.preview.request_sha256.clone();
        let mut request = self.request.clone();
        request.seed = next_seed;
        let preview = create_plug_and_play_character(&request)?;
        self.seed_lineage.push(PlugAndPlaySeedLineage {
            previous_seed,
            next_seed,
            previous_request_sha256,
            next_request_sha256: preview.request_sha256.clone(),
        });
        self.request = request;
        self.preview = preview;
        self.accepted = false;
        Ok(())
    }

    /// Accept the exact visible preview; later edits invalidate this decision.
    pub fn accept(&mut self) -> Result<(), TabletopError> {
        validate_plug_and_play_creation_preview(&self.preview)?;
        self.accepted = true;
        Ok(())
    }

    /// Export accepted JSON for source/runtime handoff.
    pub fn export_json(&self) -> Result<String, TabletopError> {
        self.accepted_preview()?.to_json()
    }

    /// Export accepted RON for source/runtime handoff.
    pub fn export_ron(&self) -> Result<String, TabletopError> {
        self.accepted_preview()?.to_ron()
    }

    fn accepted_preview(&self) -> Result<&PlugAndPlayCreationPreview, TabletopError> {
        if !self.accepted {
            return Err(TabletopError::InvalidField {
                path: "editor.acceptance".to_owned(),
                reason: "the current creation preview has not been accepted",
            });
        }
        Ok(&self.preview)
    }
}

impl Default for TabletopEditorCatalog {
    fn default() -> Self {
        Self {
            manifests: BTreeMap::new(),
            selection: AdapterSelection {
                selection_format_version: weave_tabletop::ADAPTER_SELECTION_FORMAT_VERSION,
                installed: Vec::new(),
                primary: Vec::new(),
            },
        }
    }
}

impl TabletopEditorCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install one exact declarative manifest for inspection.
    pub fn install(&mut self, manifest: AdapterManifest) -> Result<(), TabletopError> {
        validate_adapter_manifest(&manifest)?;
        let coordinate = resolved_adapter(&manifest)?;
        if self.manifests.contains_key(&manifest.id) {
            return Err(TabletopError::InvalidField {
                path: "editor.manifests".to_owned(),
                reason: "adapter identity is already installed",
            });
        }
        self.selection.installed.push(coordinate);
        self.selection
            .installed
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.manifests.insert(manifest.id.clone(), manifest);
        validate_adapter_selection(&self.selection, &self.manifests())
    }

    /// Select none or one exact primary adapter; failed selection leaves the catalog unchanged.
    pub fn select_primary(&mut self, id: Option<&str>) -> Result<(), TabletopError> {
        let primary = match id {
            Some(id) => vec![
                self.selection
                    .installed
                    .iter()
                    .find(|coordinate| coordinate.id == id)
                    .cloned()
                    .ok_or(TabletopError::AdapterNotInstalled)?,
            ],
            None => Vec::new(),
        };
        let candidate = AdapterSelection {
            selection_format_version: self.selection.selection_format_version,
            installed: self.selection.installed.clone(),
            primary,
        };
        validate_adapter_selection(&candidate, &self.manifests())?;
        self.selection = candidate;
        Ok(())
    }

    #[must_use]
    pub fn selection(&self) -> &AdapterSelection {
        &self.selection
    }

    /// Discover one generic panel from the active manifest's declared capability.
    pub fn panel(
        &self,
        capability: TabletopCapability,
    ) -> Result<TabletopPanelInspection, TabletopError> {
        let coordinate = self
            .selection
            .primary
            .first()
            .ok_or(TabletopError::AdapterNotInstalled)?;
        let manifest = &self.manifests[&coordinate.id];
        if !has_capability(manifest, capability) {
            return Err(TabletopError::UndeclaredCapability { capability });
        }
        Ok(TabletopPanelInspection {
            adapter: coordinate.clone(),
            capability,
            capability_version: 1,
            creation_steps: manifest
                .creation_steps
                .iter()
                .filter(|step| step.required_capability == capability)
                .cloned()
                .collect(),
            operations: manifest
                .operations
                .values()
                .filter(|operation| operation.required_capability == capability)
                .map(|operation| operation.id.clone())
                .collect(),
        })
    }

    fn manifests(&self) -> Vec<AdapterManifest> {
        self.manifests.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_tabletop::{
        PLUG_AND_PLAY_CREATION_FORMAT_VERSION, PlugAndPlayAttributes, PlugAndPlayMentalAttribute,
        PlugAndPlayModifier, PlugAndPlayPhysicalAttribute, PlugAndPlayRollAssignment,
    };

    fn manifest() -> AdapterManifest {
        AdapterManifest::from_json(include_str!(
            "../../examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json"
        ))
        .unwrap()
    }

    #[test]
    fn capability_discovery_drives_creation_and_resolver_panels() {
        let mut catalog = TabletopEditorCatalog::new();
        catalog.install(manifest()).unwrap();
        catalog
            .select_primary(Some("org.weave.tabletop.lantern_trail"))
            .unwrap();
        let creation = catalog
            .panel(TabletopCapability::CharacterCreation)
            .unwrap();
        assert_eq!(
            creation
                .creation_steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            ["identity", "ratings"]
        );
        let checks = catalog
            .panel(TabletopCapability::ChecksAndConflicts)
            .unwrap();
        assert_eq!(checks.operations, ["attempt"]);
        assert_eq!(checks.adapter.content_sha256.len(), 64);
    }

    #[test]
    fn absent_capability_and_unknown_selection_fail_without_partial_fallback() {
        let mut catalog = TabletopEditorCatalog::new();
        catalog.install(manifest()).unwrap();
        catalog
            .select_primary(Some("org.weave.tabletop.lantern_trail"))
            .unwrap();
        assert_eq!(
            catalog.panel(TabletopCapability::Advancement),
            Err(TabletopError::UndeclaredCapability {
                capability: TabletopCapability::Advancement
            })
        );
        let before = catalog.selection().clone();
        assert!(
            catalog
                .select_primary(Some("org.weave.tabletop.missing"))
                .is_err()
        );
        assert_eq!(catalog.selection(), &before);
        catalog.select_primary(None).unwrap();
        assert!(catalog.selection().primary.is_empty());
    }

    #[test]
    fn plug_and_play_preview_reroll_accept_and_export_are_explicit() {
        let request = PlugAndPlayCreationRequest {
            creation_format_version: PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
            character_id: "org.weave.character.editor_fixture".to_owned(),
            name: "Mica Rowan".to_owned(),
            age: 27,
            seed: 101,
            stat_source: PlugAndPlayStatSource::Rolled {
                selected_set: 0,
                assignment: PlugAndPlayRollAssignment {
                    agility: 0,
                    brains: 1,
                    brawn: 2,
                    wits: 3,
                },
            },
            survivability_body: PlugAndPlayPhysicalAttribute::Agility,
            survivability_mind: PlugAndPlayMentalAttribute::Wits,
            modifiers: vec![
                PlugAndPlayModifier {
                    id: "observant".to_owned(),
                    label: "Observant".to_owned(),
                    attributes: PlugAndPlayAttributes {
                        agility: 0,
                        brains: 1,
                        brawn: 0,
                        wits: -1,
                    },
                    fortune: 0,
                    explanation: "Study offsets slower improvisation.".to_owned(),
                },
                PlugAndPlayModifier {
                    id: "swift".to_owned(),
                    label: "Swift".to_owned(),
                    attributes: PlugAndPlayAttributes {
                        agility: 1,
                        brains: 0,
                        brawn: -1,
                        wits: 0,
                    },
                    fortune: 0,
                    explanation: "Speed offsets a lighter frame.".to_owned(),
                },
            ],
            inventory: vec!["folding lantern".to_owned()],
        };
        let mut session = PlugAndPlayCreationSession::new(request).unwrap();
        assert!(!session.is_accepted());
        assert!(session.export_json().is_err());
        let first_hash = session.preview().request_sha256.clone();
        session.reroll(202).unwrap();
        assert_eq!(session.seed_lineage().len(), 1);
        assert_eq!(session.seed_lineage()[0].previous_seed, 101);
        assert_eq!(session.seed_lineage()[0].next_seed, 202);
        assert_ne!(first_hash, session.preview().request_sha256);
        session.accept().unwrap();
        assert!(session.is_accepted());
        assert_eq!(
            PlugAndPlayCreationPreview::from_json(&session.export_json().unwrap()).unwrap(),
            *session.preview()
        );
        assert_eq!(
            PlugAndPlayCreationPreview::from_ron(&session.export_ron().unwrap()).unwrap(),
            *session.preview()
        );
    }
}
