//! Generic editor discovery for declarative tabletop adapter contracts.

use std::collections::BTreeMap;

use weave_tabletop::{
    AdapterManifest, AdapterSelection, CreationStep, ResolvedAdapter, TabletopCapability,
    TabletopError, has_capability, resolved_adapter, validate_adapter_manifest,
    validate_adapter_selection,
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
}
