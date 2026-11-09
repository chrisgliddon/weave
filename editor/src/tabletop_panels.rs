//! Manifest-driven authoring panels for the tabletop adapters.
//!
//! Shared panel code never matches an adapter id. Each adapter supplies a small descriptor module
//! that declares its creation fields, its play/progression panels, and the functions that build
//! typed contract requests. Capability gating is taken from the installed manifest, so a panel for
//! an undeclared capability fails with `TT106` before any field is shown.

use std::collections::BTreeMap;
use std::sync::Arc;

use weave_domain::DomainValue;
use weave_tabletop::{
    AdapterCharacterDefinition, AdapterManifest, CreationStep, RESOLVER_FORMAT_VERSION,
    ResolutionReceipt, ResolutionRequest, ResolverRegistry, TabletopCapability, TabletopError,
    TabletopEvent, TabletopState, canonical_fingerprint, has_capability, validate_adapter_manifest,
};

/// Broad purpose of one panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// Character creation from the manifest creation steps.
    Creation,
    /// Primary play operations such as checks, conflicts, and scenes.
    Play,
    /// Advancement, campaign, and other long-running progression.
    Progression,
}

/// Accepted shape of one panel field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFieldKind {
    Text,
    Unsigned,
    Signed,
    Boolean,
    Choice(&'static [&'static str]),
}

/// One declared field path and the value shape the panel accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelField {
    pub path: &'static str,
    pub label: &'static str,
    pub kind: PanelFieldKind,
    pub required: bool,
}

/// One typed editor value. The variant must match [`PanelFieldKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Choice(String),
}

/// Field values for one panel submission.
///
/// Values are keyed by the exact adapter or contract path, including indexed collection paths such
/// as `beliefs[0].id`. Validation runs against the declared fields before any request is built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelForm {
    values: BTreeMap<String, PanelValue>,
}

impl PanelForm {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_text(mut self, path: &str, value: impl Into<String>) -> Self {
        self.values
            .insert(path.to_owned(), PanelValue::Text(value.into()));
        self
    }

    #[must_use]
    pub fn with_integer(mut self, path: &str, value: i64) -> Self {
        self.values
            .insert(path.to_owned(), PanelValue::Integer(value));
        self
    }

    #[must_use]
    pub fn with_boolean(mut self, path: &str, value: bool) -> Self {
        self.values
            .insert(path.to_owned(), PanelValue::Boolean(value));
        self
    }

    #[must_use]
    pub fn with_choice(mut self, path: &str, value: impl Into<String>) -> Self {
        self.values
            .insert(path.to_owned(), PanelValue::Choice(value.into()));
        self
    }

    #[must_use]
    pub fn get(&self, path: &str) -> Option<&PanelValue> {
        self.values.get(path)
    }

    /// Reject missing, mistyped, or unknown fields before a request is built.
    pub fn validate(&self, fields: &[PanelField]) -> Result<(), TabletopError> {
        for field in fields {
            let Some(value) = self.values.get(field.path) else {
                if field.required {
                    return Err(invalid(field.path, "required field is missing"));
                }
                continue;
            };
            match (field.kind, value) {
                (PanelFieldKind::Text, PanelValue::Text(_))
                | (PanelFieldKind::Boolean, PanelValue::Boolean(_)) => {}
                (PanelFieldKind::Unsigned, PanelValue::Integer(value)) if *value >= 0 => {}
                (PanelFieldKind::Signed, PanelValue::Integer(_)) => {}
                (PanelFieldKind::Choice(options), PanelValue::Choice(value)) => {
                    if !options.contains(&value.as_str()) {
                        return Err(invalid(field.path, "selection is not a declared choice"));
                    }
                }
                (PanelFieldKind::Text, _) => {
                    return Err(invalid(field.path, "expected text"));
                }
                (PanelFieldKind::Unsigned, _) | (PanelFieldKind::Signed, _) => {
                    return Err(invalid(field.path, "expected an integer"));
                }
                (PanelFieldKind::Boolean, _) => {
                    return Err(invalid(field.path, "expected a boolean"));
                }
                (PanelFieldKind::Choice(_), _) => {
                    return Err(invalid(field.path, "expected a declared choice"));
                }
            }
        }
        for path in self.values.keys() {
            if !fields.iter().any(|field| field.path == path) {
                return Err(invalid(path, "unknown panel field"));
            }
        }
        Ok(())
    }

    pub fn text(&self, path: &str) -> Result<&str, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Text(value)) => Ok(value),
            Some(_) => Err(invalid(path, "expected text")),
            None => Err(invalid(path, "required field is missing")),
        }
    }

    pub fn integer(&self, path: &str) -> Result<i64, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Integer(value)) => Ok(*value),
            Some(_) => Err(invalid(path, "expected an integer")),
            None => Err(invalid(path, "required field is missing")),
        }
    }

    pub fn boolean(&self, path: &str) -> Result<bool, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Boolean(value)) => Ok(*value),
            Some(_) => Err(invalid(path, "expected a boolean")),
            None => Err(invalid(path, "required field is missing")),
        }
    }

    pub fn choice(&self, path: &str) -> Result<&str, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Choice(value)) => Ok(value),
            Some(_) => Err(invalid(path, "expected a declared choice")),
            None => Err(invalid(path, "required field is missing")),
        }
    }

    pub fn optional_text(&self, path: &str) -> Result<Option<&str>, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Text(value)) => Ok(Some(value)),
            Some(_) => Err(invalid(path, "expected text")),
            None => Ok(None),
        }
    }

    pub fn optional_integer(&self, path: &str) -> Result<Option<i64>, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Integer(value)) => Ok(Some(*value)),
            Some(_) => Err(invalid(path, "expected an integer")),
            None => Ok(None),
        }
    }

    pub fn optional_choice(&self, path: &str) -> Result<Option<&str>, TabletopError> {
        match self.values.get(path) {
            Some(PanelValue::Choice(value)) => Ok(Some(value)),
            Some(_) => Err(invalid(path, "expected a declared choice")),
            None => Ok(None),
        }
    }
}

/// Explainable creation result plus canonical JSON and RON artifacts.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelCreationPreview {
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
    pub diagnostics: Vec<PanelDiagnostic>,
    pub canonical_json: String,
    pub canonical_ron: String,
}

/// One adapter-provided character creation form.
#[derive(Debug, Clone)]
pub struct CreationPanel {
    pub title: &'static str,
    pub description: &'static str,
    pub fields: Vec<PanelField>,
    pub build: fn(&PanelForm) -> Result<PanelCreationPreview, TabletopError>,
}

/// One typed resolver action exposed by a panel.
#[derive(Debug, Clone)]
pub struct PanelOperation {
    pub id: &'static str,
    pub title: &'static str,
    pub capability: TabletopCapability,
    pub fields: Vec<PanelField>,
    pub build_request: fn(
        &PanelForm,
        &AdapterCharacterDefinition,
        &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError>,
}

/// One play or progression panel declared by an adapter.
#[derive(Debug, Clone)]
pub struct PanelDescriptor {
    pub kind: PanelKind,
    pub capability: TabletopCapability,
    pub title: &'static str,
    pub description: &'static str,
    pub operations: Vec<PanelOperation>,
}

/// One optional module offered or refused by an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePanelEntry {
    pub id: String,
    pub version: String,
    pub title: String,
    pub verified: bool,
    pub provides: Vec<TabletopCapability>,
    pub reason: String,
}

/// One activation diagnostic with a stable adapter code and exact path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// Result of reviewing an optional-module activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePanelOutcome {
    pub diagnostics: Vec<PanelDiagnostic>,
    pub effective_capabilities: Vec<TabletopCapability>,
    pub enabled_modules: Vec<String>,
    pub inactive_modules: Vec<ModulePanelEntry>,
}

/// Adapter-provided optional-module review panel.
#[derive(Debug, Clone)]
pub struct ModulePanel {
    pub capability: TabletopCapability,
    pub title: &'static str,
    pub fields: Vec<PanelField>,
    pub modules: Vec<ModulePanelEntry>,
    pub apply: fn(&PanelForm) -> Result<ModulePanelOutcome, TabletopError>,
}

/// Per-adapter descriptor surface used by the shared panel session.
pub trait TabletopPanelAdapter: Send + Sync {
    fn adapter_id(&self) -> &'static str;
    fn adapter_version(&self) -> &'static str;
    fn manifest(&self) -> AdapterManifest;
    fn creation_panel(&self) -> CreationPanel;
    fn panels(&self) -> Vec<PanelDescriptor>;

    fn module_panel(&self) -> Option<ModulePanel> {
        None
    }
}

/// Visible creation panel with manifest steps and declared fields.
#[derive(Debug, Clone)]
pub struct CreationPanelView {
    pub capability: TabletopCapability,
    pub title: &'static str,
    pub description: &'static str,
    pub steps: Vec<CreationStep>,
    pub fields: Vec<PanelField>,
}

/// Visible play/progression panel summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSummary {
    pub kind: PanelKind,
    pub capability: TabletopCapability,
    pub title: String,
    pub description: String,
    pub operations: Vec<String>,
}

/// One executed operation with its typed receipt and events.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelOperationOutcome {
    pub receipt: ResolutionReceipt,
    pub events: Vec<TabletopEvent>,
}

/// Retained transition used for deterministic replay from the editor.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelOperationRecord {
    pub request: ResolutionRequest,
    pub before: TabletopState,
    pub receipt: ResolutionReceipt,
}

/// Visible optional-module review panel.
#[derive(Debug, Clone)]
pub struct ModulePanelView {
    pub capability: TabletopCapability,
    pub title: &'static str,
    pub fields: Vec<PanelField>,
    pub modules: Vec<ModulePanelEntry>,
}

/// Manifest-gated authoring and play session for one adapter.
pub struct TabletopPanelSession {
    adapter: Arc<dyn TabletopPanelAdapter>,
    manifest: AdapterManifest,
    registry: Arc<ResolverRegistry>,
    creation_form: PanelForm,
    creation_preview: Option<PanelCreationPreview>,
    creation_accepted: bool,
    definition: Option<AdapterCharacterDefinition>,
    state: Option<TabletopState>,
    records: Vec<PanelOperationRecord>,
}

impl TabletopPanelSession {
    pub fn new(
        adapter: Arc<dyn TabletopPanelAdapter>,
        registry: Arc<ResolverRegistry>,
    ) -> Result<Self, TabletopError> {
        let manifest = adapter.manifest();
        validate_adapter_manifest(&manifest)?;
        if adapter.adapter_id() != manifest.id || adapter.adapter_version() != manifest.version {
            return Err(TabletopError::ResolverMismatch);
        }
        Ok(Self {
            adapter,
            manifest,
            registry,
            creation_form: PanelForm::new(),
            creation_preview: None,
            creation_accepted: false,
            definition: None,
            state: None,
            records: Vec::new(),
        })
    }

    #[must_use]
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    #[must_use]
    pub fn creation_form(&self) -> &PanelForm {
        &self.creation_form
    }

    #[must_use]
    pub fn creation_preview(&self) -> Option<&PanelCreationPreview> {
        self.creation_preview.as_ref()
    }

    #[must_use]
    pub fn is_creation_accepted(&self) -> bool {
        self.creation_accepted
    }

    #[must_use]
    pub fn definition(&self) -> Option<&AdapterCharacterDefinition> {
        self.definition.as_ref()
    }

    #[must_use]
    pub fn state(&self) -> Option<&TabletopState> {
        self.state.as_ref()
    }

    #[must_use]
    pub fn records(&self) -> &[PanelOperationRecord] {
        &self.records
    }

    /// Every typed event produced through this session, in execution order.
    #[must_use]
    pub fn events(&self) -> Vec<TabletopEvent> {
        self.records
            .iter()
            .flat_map(|record| record.receipt.events.iter().cloned())
            .collect()
    }

    /// Declared creation panel, or `TT106` when the manifest lacks creation.
    pub fn creation_panel(&self) -> Result<CreationPanelView, TabletopError> {
        if !has_capability(&self.manifest, TabletopCapability::CharacterCreation) {
            return Err(TabletopError::UndeclaredCapability {
                capability: TabletopCapability::CharacterCreation,
            });
        }
        let panel = self.adapter.creation_panel();
        Ok(CreationPanelView {
            capability: TabletopCapability::CharacterCreation,
            title: panel.title,
            description: panel.description,
            steps: self
                .manifest
                .creation_steps
                .iter()
                .filter(|step| step.required_capability == TabletopCapability::CharacterCreation)
                .cloned()
                .collect(),
            fields: panel.fields,
        })
    }

    /// Every play and progression panel whose capability the manifest declares.
    #[must_use]
    pub fn panels(&self) -> Vec<PanelSummary> {
        self.adapter
            .panels()
            .into_iter()
            .filter(|panel| has_capability(&self.manifest, panel.capability))
            .map(|panel| PanelSummary {
                kind: panel.kind,
                capability: panel.capability,
                title: panel.title.to_owned(),
                description: panel.description.to_owned(),
                operations: panel
                    .operations
                    .into_iter()
                    .map(|operation| operation.id.to_owned())
                    .collect(),
            })
            .collect()
    }

    /// One declared panel, or `TT106` when the capability is absent.
    pub fn panel(&self, capability: TabletopCapability) -> Result<PanelSummary, TabletopError> {
        if !has_capability(&self.manifest, capability) {
            return Err(TabletopError::UndeclaredCapability { capability });
        }
        self.panels()
            .into_iter()
            .find(|panel| panel.capability == capability)
            .ok_or(TabletopError::UndeclaredCapability { capability })
    }

    /// Replace the creation form after field validation; clears any preview and acceptance.
    pub fn update_creation_form(&mut self, form: PanelForm) -> Result<(), TabletopError> {
        let panel = self.creation_panel()?;
        form.validate(&panel.fields)?;
        self.creation_form = form;
        self.creation_preview = None;
        self.creation_accepted = false;
        self.definition = None;
        self.state = None;
        self.records.clear();
        Ok(())
    }

    /// Build and validate the typed creation request without accepting it.
    pub fn preview_creation(&mut self) -> Result<&PanelCreationPreview, TabletopError> {
        self.creation_panel()?;
        let panel = self.adapter.creation_panel();
        self.creation_form.validate(&panel.fields)?;
        let preview = (panel.build)(&self.creation_form)?;
        self.creation_preview = Some(preview);
        self.creation_accepted = false;
        self.definition = None;
        self.state = None;
        self.records.clear();
        self.creation_preview
            .as_ref()
            .ok_or_else(|| invalid("editor.preview", "creation preview was not stored"))
    }

    /// Accept the exact visible preview and open the play state.
    pub fn accept_creation(&mut self) -> Result<(), TabletopError> {
        let Some(preview) = self.creation_preview.clone() else {
            return Err(invalid(
                "editor.acceptance",
                "the current creation preview has not been accepted",
            ));
        };
        self.definition = Some(preview.definition);
        self.state = Some(preview.initial_state);
        self.creation_accepted = true;
        self.records.clear();
        Ok(())
    }

    /// Execute one declared operation and retain its typed receipt for replay.
    pub fn execute_operation(
        &mut self,
        operation_id: &str,
        form: &PanelForm,
    ) -> Result<PanelOperationOutcome, TabletopError> {
        let operation = self.operation(operation_id)?;
        form.validate(&operation.fields)?;
        if !self.creation_accepted {
            return Err(invalid(
                "editor.acceptance",
                "creation must be accepted before play",
            ));
        }
        let Some(definition) = self.definition.as_ref() else {
            return Err(invalid(
                "editor.definition",
                "creation must be accepted before play",
            ));
        };
        let Some(state) = self.state.clone() else {
            return Err(invalid(
                "editor.state",
                "creation must be accepted before play",
            ));
        };
        let mut request = (operation.build_request)(form, definition, &state)?;
        request.request_id = format!("panel_{}_{}", operation.id, self.records.len());
        request.operation = operation.id.to_owned();
        request.capability = operation.capability;
        let receipt = self.registry.execute(&self.manifest, &request, &state)?;
        self.state = Some(receipt.after_state.clone());
        let outcome = PanelOperationOutcome {
            receipt: receipt.clone(),
            events: receipt.events.clone(),
        };
        self.records.push(PanelOperationRecord {
            request,
            before: state,
            receipt,
        });
        Ok(outcome)
    }

    /// Re-execute one retained transition and require an identical receipt.
    pub fn replay_record(&self, index: usize) -> Result<(), TabletopError> {
        let record = self
            .records
            .get(index)
            .ok_or_else(|| invalid("editor.records", "retained operation index is out of range"))?;
        self.registry.replay(
            &self.manifest,
            &record.request,
            &record.before,
            &record.receipt,
        )
    }

    /// Optional-module review panel, or `TT106` when the adapter has none or lacks its capability.
    pub fn module_panel(&self) -> Result<ModulePanelView, TabletopError> {
        let panel = self
            .adapter
            .module_panel()
            .ok_or(TabletopError::UndeclaredCapability {
                capability: TabletopCapability::DerivedValues,
            })?;
        if !has_capability(&self.manifest, panel.capability) {
            return Err(TabletopError::UndeclaredCapability {
                capability: panel.capability,
            });
        }
        Ok(ModulePanelView {
            capability: panel.capability,
            title: panel.title,
            fields: panel.fields,
            modules: panel.modules,
        })
    }

    /// Review one optional-module activation; diagnostics keep unverified modules inactive.
    pub fn apply_modules(&self, form: &PanelForm) -> Result<ModulePanelOutcome, TabletopError> {
        let panel = self
            .adapter
            .module_panel()
            .ok_or(TabletopError::UndeclaredCapability {
                capability: TabletopCapability::DerivedValues,
            })?;
        if !has_capability(&self.manifest, panel.capability) {
            return Err(TabletopError::UndeclaredCapability {
                capability: panel.capability,
            });
        }
        form.validate(&panel.fields)?;
        (panel.apply)(form)
    }

    fn operation(&self, operation_id: &str) -> Result<PanelOperation, TabletopError> {
        for panel in self.adapter.panels() {
            if let Some(operation) = panel
                .operations
                .iter()
                .find(|operation| operation.id == operation_id)
            {
                if !has_capability(&self.manifest, panel.capability) {
                    return Err(TabletopError::UndeclaredCapability {
                        capability: panel.capability,
                    });
                }
                return Ok(operation.clone());
            }
        }
        Err(invalid("operation", "unknown panel operation"))
    }
}

/// All built-in adapters, in stable order.
#[must_use]
pub fn builtin_panel_adapters() -> Vec<Arc<dyn TabletopPanelAdapter>> {
    vec![
        Arc::new(spark::SparkPanels),
        Arc::new(wicked_ones::WickedOnesPanels),
        Arc::new(open_tale::OpenTalePanels),
        Arc::new(open_adventure::OpenAdventurePanels),
    ]
}

/// True when an optional group has its identity, false when it is entirely absent.
///
/// Supplying any companion value without the identity is rejected with the identity path, so a
/// half-filled optional row never disappears silently.
fn optional_group(
    form: &PanelForm,
    id_path: &str,
    companions: &[&str],
) -> Result<bool, TabletopError> {
    match form.get(id_path) {
        Some(PanelValue::Text(id)) if !id.is_empty() => Ok(true),
        Some(PanelValue::Text(_)) => {
            if companions.iter().any(|path| form.get(path).is_some()) {
                Err(invalid(id_path, "required field is missing"))
            } else {
                Ok(false)
            }
        }
        Some(_) => Err(invalid(id_path, "expected text")),
        None => {
            if companions.iter().any(|path| form.get(path).is_some()) {
                Err(invalid(id_path, "required field is missing"))
            } else {
                Ok(false)
            }
        }
    }
}

fn invalid(path: impl Into<String>, reason: &'static str) -> TabletopError {
    TabletopError::InvalidField {
        path: path.into(),
        reason,
    }
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn symbol(value: &str) -> DomainValue {
    DomainValue::Symbol(value.to_owned())
}

fn number(value: i64) -> DomainValue {
    DomainValue::Number(value as f64)
}

fn text(value: &str) -> DomainValue {
    DomainValue::String(value.to_owned())
}

fn text_list(value: &str) -> DomainValue {
    DomainValue::List(
        value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(text)
            .collect(),
    )
}

/// Build the exact typed resolution request shared by every panel operation.
pub fn resolution_request(
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
    definition: &AdapterCharacterDefinition,
    state: &TabletopState,
) -> Result<ResolutionRequest, TabletopError> {
    Ok(ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "panel_request".to_owned(),
        adapter: state.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: definition.definition_sha256.clone(),
        definition: definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state)?,
        input,
    })
}

/// Spark creation, conflict, scene, and advancement panels.
pub mod spark {
    use super::*;
    use weave_tabletop::{
        SPARK_CREATION_FORMAT_VERSION, SparkAttribute, SparkAttributeSeed, SparkBeliefSeed,
        SparkCreationPreview, SparkCreationRequest, SparkRelationshipSeed, SparkStep,
        SparkTalentSeed, SparkTalentTier, create_spark_character, spark_manifest,
        validate_spark_creation_preview,
    };

    const STEPS: &[&str] = &["d4", "d6", "d8", "d10", "d12"];
    const TIERS: &[&str] = &["broad", "common", "deep"];
    const ATTRIBUTES: &[&str] = &["body", "heart", "mind", "spark"];
    const BOOST_KINDS: &[&str] = &["harm", "influence"];
    const INFLUENCE_MODES: &[&str] = &["award", "refresh"];
    const VERDICTS: &[&str] = &["confirmed", "refuted"];

    pub struct SparkPanels;

    impl TabletopPanelAdapter for SparkPanels {
        fn adapter_id(&self) -> &'static str {
            weave_tabletop::SPARK_ADAPTER_ID
        }

        fn adapter_version(&self) -> &'static str {
            weave_tabletop::SPARK_ADAPTER_VERSION
        }

        fn manifest(&self) -> AdapterManifest {
            spark_manifest()
        }

        fn creation_panel(&self) -> CreationPanel {
            CreationPanel {
                title: "Spark character",
                description: "Identity, three Beliefs, relationships, step-die attributes, and tiered Talents.",
                fields: creation_fields(),
                build: build_creation,
            }
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            vec![
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ChecksAndConflicts,
                    title: "Conflicts",
                    description: "Open a conflict, declare actions, add supporters, roll, boost, and resolve.",
                    operations: vec![
                        operation(
                            "open_conflict",
                            "Open conflict",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_text("conflict_id", "Conflict identity"),
                                required_text("stakes", "Stakes"),
                                required_unsigned("max_escalation", "Maximum escalation"),
                                required_choice(
                                    "victory_cost_kind",
                                    "Victory cost kind",
                                    BOOST_KINDS,
                                ),
                                required_unsigned("victory_cost_amount", "Victory cost amount"),
                                optional_text_field(
                                    "victory_cost_attribute",
                                    "Harm attribute, or empty",
                                ),
                            ],
                            build_open_conflict,
                        ),
                        operation(
                            "declare_action",
                            "Declare action",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_text("conflict_id", "Conflict identity"),
                                required_text("declaration_id", "Declaration identity"),
                                required_text("actor_id", "Actor identity"),
                                required_choice("attribute", "Attribute", ATTRIBUTES),
                                required_text("description", "Declared intent"),
                                optional_text_field("talent_id", "Talent, or empty"),
                            ],
                            build_declare_action,
                        ),
                        operation(
                            "add_supporter",
                            "Add supporter",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_text("conflict_id", "Conflict identity"),
                                required_text("declaration_id", "Declaration identity"),
                                required_text("relationship_id", "Relationship identity"),
                            ],
                            build_add_supporter,
                        ),
                        operation(
                            "roll_conflict",
                            "Roll conflict",
                            TabletopCapability::ChecksAndConflicts,
                            vec![required_text("conflict_id", "Conflict identity")],
                            build_roll_conflict,
                        ),
                        operation(
                            "resolve_conflict",
                            "Resolve conflict",
                            TabletopCapability::ChecksAndConflicts,
                            vec![required_text("conflict_id", "Conflict identity")],
                            build_resolve_conflict,
                        ),
                        operation(
                            "boost_roll",
                            "Boost a rolled declaration",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_text("conflict_id", "Conflict identity"),
                                required_text("declaration_id", "Declaration identity"),
                                required_choice("kind", "Boost kind", BOOST_KINDS),
                                required_unsigned("amount", "Boost amount"),
                                optional_text_field("attribute", "Harm attribute, or empty"),
                                required_text("rationale", "Boost rationale"),
                            ],
                            build_boost_roll,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::Scenes,
                    title: "Scene closing",
                    description: "Review Beliefs, Influence, and Harm recovery at scene close.",
                    operations: vec![operation(
                        "close_scene",
                        "Close scene",
                        TabletopCapability::Scenes,
                        close_scene_fields(),
                        build_close_scene,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::Advancement,
                    title: "Evolution",
                    description: "Replace a Belief through Inspiration and record Growth.",
                    operations: vec![operation(
                        "replace_belief",
                        "Replace Belief",
                        TabletopCapability::Advancement,
                        vec![
                            required_text("belief_id", "Belief identity"),
                            required_text("confirmed_by", "Confirming actors, comma separated"),
                            required_text("inspiration", "Inspiration"),
                            required_text("new_statement", "New Belief statement"),
                            required_text("rationale", "Rationale"),
                            required_text("replacement_id", "Replacement identity"),
                        ],
                        build_replace_belief,
                    )],
                },
            ]
        }
    }

    fn creation_fields() -> Vec<PanelField> {
        vec![
            required_text("character_id", "Character identity"),
            required_text("name", "Name"),
            required_text("concept", "Concept"),
            required_unsigned("starting_influence", "Starting Influence"),
            required_text("beliefs[0].id", "Belief 1 identity"),
            required_text("beliefs[0].statement", "Belief 1 statement"),
            required_text("beliefs[1].id", "Belief 2 identity"),
            required_text("beliefs[1].statement", "Belief 2 statement"),
            required_text("beliefs[2].id", "Belief 3 identity"),
            required_text("beliefs[2].statement", "Belief 3 statement"),
            optional_text_field("relationship[0].id", "Relationship identity"),
            optional_text_field("relationship[0].name", "Relationship name"),
            optional_text_field("relationship[0].description", "Relationship description"),
            required_choice("attributes.body", "Body step", STEPS),
            required_choice("attributes.heart", "Heart step", STEPS),
            required_choice("attributes.mind", "Mind step", STEPS),
            required_choice("attributes.spark", "Spark step", STEPS),
            optional_text_field("talent[0].id", "Talent identity"),
            optional_text_field("talent[0].name", "Talent name"),
            optional_choice_field("talent[0].attribute", "Talent attribute", ATTRIBUTES),
            optional_choice_field("talent[0].tier", "Talent tier", TIERS),
        ]
    }

    fn build_creation(form: &PanelForm) -> Result<PanelCreationPreview, TabletopError> {
        let beliefs = vec![
            SparkBeliefSeed {
                id: form.text("beliefs[0].id")?.to_owned(),
                statement: form.text("beliefs[0].statement")?.to_owned(),
            },
            SparkBeliefSeed {
                id: form.text("beliefs[1].id")?.to_owned(),
                statement: form.text("beliefs[1].statement")?.to_owned(),
            },
            SparkBeliefSeed {
                id: form.text("beliefs[2].id")?.to_owned(),
                statement: form.text("beliefs[2].statement")?.to_owned(),
            },
        ];
        let relationships = if optional_group(
            form,
            "relationship[0].id",
            &["relationship[0].name", "relationship[0].description"],
        )? {
            vec![SparkRelationshipSeed {
                id: form.text("relationship[0].id")?.to_owned(),
                name: form.text("relationship[0].name")?.to_owned(),
                description: form
                    .optional_text("relationship[0].description")?
                    .unwrap_or_default()
                    .to_owned(),
            }]
        } else {
            Vec::new()
        };
        let attributes = [
            ("body", SparkAttribute::Body),
            ("heart", SparkAttribute::Heart),
            ("mind", SparkAttribute::Mind),
            ("spark", SparkAttribute::Spark),
        ]
        .into_iter()
        .map(|(name, attribute)| {
            let path = match name {
                "body" => "attributes.body",
                "heart" => "attributes.heart",
                "mind" => "attributes.mind",
                _ => "attributes.spark",
            };
            Ok(SparkAttributeSeed {
                attribute,
                step: step(form.choice(path)?),
            })
        })
        .collect::<Result<Vec<_>, TabletopError>>()?;
        let talents = if optional_group(
            form,
            "talent[0].id",
            &["talent[0].name", "talent[0].attribute", "talent[0].tier"],
        )? {
            vec![SparkTalentSeed {
                id: form.text("talent[0].id")?.to_owned(),
                name: form.text("talent[0].name")?.to_owned(),
                attribute: attribute(form.choice("talent[0].attribute")?)?,
                tier: tier(form.choice("talent[0].tier")?)?,
            }]
        } else {
            Vec::new()
        };
        let request = SparkCreationRequest {
            creation_format_version: SPARK_CREATION_FORMAT_VERSION,
            character_id: form.text("character_id")?.to_owned(),
            name: form.text("name")?.to_owned(),
            concept: form.text("concept")?.to_owned(),
            beliefs,
            relationships,
            attributes,
            talents,
            starting_influence: unsigned(form, "starting_influence")?,
        };
        let preview = create_spark_character(&request)?;
        validate_spark_creation_preview(&preview)?;
        artifact(&preview)
    }

    fn artifact(preview: &SparkCreationPreview) -> Result<PanelCreationPreview, TabletopError> {
        Ok(PanelCreationPreview {
            definition: preview.definition.clone(),
            initial_state: preview.initial_state.clone(),
            explanations: preview.explanations.clone(),
            diagnostics: Vec::new(),
            canonical_json: preview.to_json()?,
            canonical_ron: preview.to_ron()?,
        })
    }

    fn build_open_conflict(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([
            ("conflict_id", text(form.text("conflict_id")?)),
            ("max_escalation", number(form.integer("max_escalation")?)),
            ("stakes", text(form.text("stakes")?)),
            (
                "victory_cost_amount",
                number(form.integer("victory_cost_amount")?),
            ),
            (
                "victory_cost_attribute",
                text(form.optional_text("victory_cost_attribute")?.unwrap_or("")),
            ),
            (
                "victory_cost_kind",
                symbol(form.choice("victory_cost_kind")?),
            ),
        ]);
        resolution_request(
            "open_conflict",
            TabletopCapability::ChecksAndConflicts,
            input,
            definition,
            state,
        )
    }

    fn build_declare_action(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let mut fields = BTreeMap::from([
            ("actor_id".to_owned(), text(form.text("actor_id")?)),
            ("attribute".to_owned(), symbol(form.choice("attribute")?)),
            ("conflict_id".to_owned(), text(form.text("conflict_id")?)),
            (
                "declaration_id".to_owned(),
                text(form.text("declaration_id")?),
            ),
            ("description".to_owned(), text(form.text("description")?)),
        ]);
        if let Some(talent_id) = form.optional_text("talent_id")?
            && !talent_id.is_empty()
        {
            fields.insert("talent_id".to_owned(), text(talent_id));
        }
        resolution_request(
            "declare_action",
            TabletopCapability::ChecksAndConflicts,
            DomainValue::Object(fields),
            definition,
            state,
        )
    }

    fn build_add_supporter(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([
            ("conflict_id", text(form.text("conflict_id")?)),
            ("declaration_id", text(form.text("declaration_id")?)),
            ("relationship_id", text(form.text("relationship_id")?)),
        ]);
        resolution_request(
            "add_supporter",
            TabletopCapability::ChecksAndConflicts,
            input,
            definition,
            state,
        )
    }

    fn build_roll_conflict(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([("conflict_id", text(form.text("conflict_id")?))]);
        resolution_request(
            "roll_conflict",
            TabletopCapability::ChecksAndConflicts,
            input,
            definition,
            state,
        )
    }

    fn build_resolve_conflict(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([("conflict_id", text(form.text("conflict_id")?))]);
        resolution_request(
            "resolve_conflict",
            TabletopCapability::ChecksAndConflicts,
            input,
            definition,
            state,
        )
    }

    fn build_boost_roll(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([
            ("amount", number(form.integer("amount")?)),
            (
                "attribute",
                text(form.optional_text("attribute")?.unwrap_or("")),
            ),
            ("conflict_id", text(form.text("conflict_id")?)),
            ("declaration_id", text(form.text("declaration_id")?)),
            ("kind", symbol(form.choice("kind")?)),
            ("rationale", text(form.text("rationale")?)),
        ]);
        resolution_request(
            "boost_roll",
            TabletopCapability::ChecksAndConflicts,
            input,
            definition,
            state,
        )
    }

    fn close_scene_fields() -> Vec<PanelField> {
        vec![
            required_text("scene_id", "Scene identity"),
            required_text("label", "Scene label"),
            required_text("belief_reviews[0].belief_id", "Reviewed Belief"),
            required_text("belief_reviews[0].rationale", "Belief rationale"),
            required_choice("belief_reviews[0].verdict", "Verdict", VERDICTS),
            required_unsigned("influence_amount", "Influence amount"),
            required_choice("influence_mode", "Influence mode", INFLUENCE_MODES),
            required_text("influence_rationale", "Influence rationale"),
            optional_choice_field("recovery[0].attribute", "Recovered attribute", ATTRIBUTES),
            optional_unsigned("recovery[0].amount", "Recovered Harm"),
        ]
    }

    fn build_close_scene(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let reviews = DomainValue::List(vec![object([
            ("belief_id", text(form.text("belief_reviews[0].belief_id")?)),
            ("rationale", text(form.text("belief_reviews[0].rationale")?)),
            ("verdict", symbol(form.choice("belief_reviews[0].verdict")?)),
        ])]);
        let recovery = match form.optional_choice("recovery[0].attribute")? {
            Some(attribute) => DomainValue::List(vec![object([
                (
                    "amount",
                    number(form.optional_integer("recovery[0].amount")?.unwrap_or(0)),
                ),
                ("attribute", symbol(attribute)),
            ])]),
            None => DomainValue::List(Vec::new()),
        };
        let input = object([
            ("belief_reviews", reviews),
            (
                "influence_amount",
                number(form.integer("influence_amount")?),
            ),
            ("influence_mode", symbol(form.choice("influence_mode")?)),
            (
                "influence_rationale",
                text(form.text("influence_rationale")?),
            ),
            ("label", text(form.text("label")?)),
            ("recovery", recovery),
            ("scene_id", text(form.text("scene_id")?)),
        ]);
        resolution_request(
            "close_scene",
            TabletopCapability::Scenes,
            input,
            definition,
            state,
        )
    }

    fn build_replace_belief(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([
            ("belief_id", text(form.text("belief_id")?)),
            ("confirmed_by", text_list(form.text("confirmed_by")?)),
            ("inspiration", text(form.text("inspiration")?)),
            ("new_statement", text(form.text("new_statement")?)),
            ("rationale", text(form.text("rationale")?)),
            ("replacement_id", text(form.text("replacement_id")?)),
        ]);
        resolution_request(
            "replace_belief",
            TabletopCapability::Advancement,
            input,
            definition,
            state,
        )
    }

    fn operation(
        id: &'static str,
        title: &'static str,
        capability: TabletopCapability,
        fields: Vec<PanelField>,
        build_request: fn(
            &PanelForm,
            &AdapterCharacterDefinition,
            &TabletopState,
        ) -> Result<ResolutionRequest, TabletopError>,
    ) -> PanelOperation {
        PanelOperation {
            id,
            title,
            capability,
            fields,
            build_request,
        }
    }

    fn required_text(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: true,
        }
    }

    fn optional_text_field(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: false,
        }
    }

    fn required_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: true,
        }
    }

    fn optional_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: false,
        }
    }

    fn required_choice(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: true,
        }
    }

    fn optional_choice_field(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: false,
        }
    }

    fn unsigned(form: &PanelForm, path: &'static str) -> Result<u32, TabletopError> {
        u32::try_from(form.integer(path)?)
            .map_err(|_| invalid(path, "expected a non-negative integer"))
    }

    fn step(value: &str) -> SparkStep {
        match value {
            "d4" => SparkStep::D4,
            "d6" => SparkStep::D6,
            "d8" => SparkStep::D8,
            "d10" => SparkStep::D10,
            _ => SparkStep::D12,
        }
    }

    fn attribute(value: &str) -> Result<SparkAttribute, TabletopError> {
        match value {
            "body" => Ok(SparkAttribute::Body),
            "heart" => Ok(SparkAttribute::Heart),
            "mind" => Ok(SparkAttribute::Mind),
            "spark" => Ok(SparkAttribute::Spark),
            _ => Err(invalid("talent[0].attribute", "unknown Spark attribute")),
        }
    }

    fn tier(value: &str) -> Result<SparkTalentTier, TabletopError> {
        match value {
            "broad" => Ok(SparkTalentTier::Broad),
            "common" => Ok(SparkTalentTier::Common),
            "deep" => Ok(SparkTalentTier::Deep),
            _ => Err(invalid("talent[0].tier", "unknown Spark Talent tier")),
        }
    }
}

/// Wicked Ones creation, action, condition, campaign, and advancement panels.
pub mod wicked_ones {
    use super::*;
    use weave_tabletop::{
        WICKED_ONES_CREATION_FORMAT_VERSION, WICKED_ONES_PHASES, WickedOnesActionRatings,
        WickedOnesCalling, WickedOnesCreationPreview, WickedOnesCreationRequest,
        WickedOnesGearItem, WickedOnesVileFriend, create_wicked_ones_character,
        validate_wicked_ones_creation_preview, wicked_ones_manifest,
    };

    const CALLINGS: &[&str] = &["forgekeeper", "warbringer", "whisperer", "wildcaller"];
    const ACTIONS: &[&str] = &[
        "analyze", "brawl", "command", "demolish", "endure", "invent", "perceive", "persuade",
        "sneak",
    ];
    const APPROACHES: &[&str] = &["force", "guile", "insight"];
    const POSITIONS: &[&str] = &["controlled", "desperate", "risky"];
    const EFFECTS: &[&str] = &["great", "limited", "standard"];
    const OUTCOMES: &[&str] = &["critical", "failure", "mixed", "success"];
    const DOWNTIME_KINDS: &[&str] = &["lurk", "project", "recovery", "revel"];

    pub struct WickedOnesPanels;

    impl TabletopPanelAdapter for WickedOnesPanels {
        fn adapter_id(&self) -> &'static str {
            weave_tabletop::WICKED_ONES_ADAPTER_ID
        }

        fn adapter_version(&self) -> &'static str {
            weave_tabletop::WICKED_ONES_ADAPTER_VERSION
        }

        fn manifest(&self) -> AdapterManifest {
            wicked_ones_manifest()
        }

        fn creation_panel(&self) -> CreationPanel {
            CreationPanel {
                title: "Wicked One",
                description: "Calling, action ratings, abilities, impulse, revelry, gear, friend, and resources.",
                fields: creation_fields(),
                build: build_creation,
            }
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            vec![
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ChecksAndConflicts,
                    title: "Action loop",
                    description: "Intention, approach, position, effect, teamwork, resistance, and consequences.",
                    operations: vec![operation(
                        "resolve_action",
                        "Resolve action",
                        TabletopCapability::ChecksAndConflicts,
                        resolve_action_fields(),
                        build_resolve_action,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ResourcesAndConditions,
                    title: "Conditions",
                    description: "Stress, Shock, bloodied, feral, flashbacks, and death knells.",
                    operations: vec![
                        operation(
                            "apply_shock",
                            "Apply Shock",
                            TabletopCapability::ResourcesAndConditions,
                            vec![
                                required_unsigned("amount", "Shock amount"),
                                required_text("source", "Shock source"),
                            ],
                            build_apply_shock,
                        ),
                        operation(
                            "go_feral",
                            "Go feral",
                            TabletopCapability::ResourcesAndConditions,
                            vec![required_text("trigger", "Feral trigger")],
                            build_go_feral,
                        ),
                        operation(
                            "death_knell",
                            "Record death knell",
                            TabletopCapability::ResourcesAndConditions,
                            vec![required_text("knell", "Death knell")],
                            build_death_knell,
                        ),
                        operation(
                            "flashback",
                            "Declare flashback",
                            TabletopCapability::ResourcesAndConditions,
                            vec![
                                required_unsigned("stress_cost", "Stress cost"),
                                required_text("summary", "Flashback summary"),
                            ],
                            build_flashback,
                        ),
                        operation(
                            "gain_dark_heart",
                            "Gain dark heart",
                            TabletopCapability::ResourcesAndConditions,
                            vec![required_text("reason", "Reason")],
                            build_gain_dark_heart,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::CampaignState,
                    title: "Campaign",
                    description: "Phases, downtime, dungeon events, calamity, raids, blowback, and recovery.",
                    operations: campaign_operations(),
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::Advancement,
                    title: "Advancement",
                    description: "Wicked and reckless advancement spends.",
                    operations: vec![
                        operation(
                            "spend_wicked_advancement",
                            "Spend Wicked advancement",
                            TabletopCapability::Advancement,
                            vec![
                                required_text("ability", "Ability identity"),
                                required_text("note", "Advancement note"),
                            ],
                            build_spend_wicked,
                        ),
                        operation(
                            "spend_reckless_advancement",
                            "Spend reckless advancement",
                            TabletopCapability::Advancement,
                            vec![
                                required_choice("action", "Action", ACTIONS),
                                required_unsigned("to_rating", "New rating"),
                                required_text("note", "Advancement note"),
                            ],
                            build_spend_reckless,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::EquipmentAndAbilities,
                    title: "Dungeon upgrades",
                    description: "Purchase reviewed dungeon upgrades during downtime.",
                    operations: vec![operation(
                        "purchase_upgrade",
                        "Purchase upgrade",
                        TabletopCapability::EquipmentAndAbilities,
                        vec![
                            required_text("upgrade_id", "Upgrade identity"),
                            required_text("note", "Purchase note"),
                        ],
                        build_purchase_upgrade,
                    )],
                },
            ]
        }
    }

    fn creation_fields() -> Vec<PanelField> {
        let mut fields = vec![
            required_text("character_id", "Character identity"),
            required_text("name", "Name"),
            required_text("race", "Race"),
            required_text("look", "Look"),
            required_choice("calling", "Calling", CALLINGS),
            required_text("dark_impulse", "Dark impulse"),
            required_text("revelry", "Revelry"),
            required_unsigned("starting_resources", "Starting resources"),
            required_unsigned("seed", "Deterministic seed"),
            required_text("abilities[0]", "Ability 1"),
            required_text("abilities[1]", "Ability 2"),
            required_text("gear[0].id", "Gear identity"),
            required_text("gear[0].label", "Gear label"),
            required_unsigned("gear[0].quantity", "Gear quantity"),
            required_text("vile_friend.name", "Vile friend name"),
            optional_text_field("vile_friend.detail", "Vile friend detail"),
        ];
        for action in ACTIONS {
            let path = match *action {
                "analyze" => "actions.analyze",
                "brawl" => "actions.brawl",
                "command" => "actions.command",
                "demolish" => "actions.demolish",
                "endure" => "actions.endure",
                "invent" => "actions.invent",
                "perceive" => "actions.perceive",
                "persuade" => "actions.persuade",
                _ => "actions.sneak",
            };
            fields.push(required_unsigned(path, "Action rating"));
        }
        fields
    }

    fn build_creation(form: &PanelForm) -> Result<PanelCreationPreview, TabletopError> {
        let request = WickedOnesCreationRequest {
            creation_format_version: WICKED_ONES_CREATION_FORMAT_VERSION,
            character_id: form.text("character_id")?.to_owned(),
            name: form.text("name")?.to_owned(),
            race: form.text("race")?.to_owned(),
            look: form.text("look")?.to_owned(),
            calling: calling(form.choice("calling")?)?,
            actions: WickedOnesActionRatings {
                analyze: unsigned_u8(form, "actions.analyze")?,
                invent: unsigned_u8(form, "actions.invent")?,
                perceive: unsigned_u8(form, "actions.perceive")?,
                brawl: unsigned_u8(form, "actions.brawl")?,
                demolish: unsigned_u8(form, "actions.demolish")?,
                sneak: unsigned_u8(form, "actions.sneak")?,
                command: unsigned_u8(form, "actions.command")?,
                persuade: unsigned_u8(form, "actions.persuade")?,
                endure: unsigned_u8(form, "actions.endure")?,
            },
            dark_impulse: form.text("dark_impulse")?.to_owned(),
            revelry: form.text("revelry")?.to_owned(),
            gear: vec![WickedOnesGearItem {
                id: form.text("gear[0].id")?.to_owned(),
                label: form.text("gear[0].label")?.to_owned(),
                quantity: u8::try_from(form.integer("gear[0].quantity")?)
                    .map_err(|_| invalid("gear[0].quantity", "expected 1..99"))?,
            }],
            abilities: vec![
                form.text("abilities[0]")?.to_owned(),
                form.text("abilities[1]")?.to_owned(),
            ],
            vile_friend: WickedOnesVileFriend {
                name: form.text("vile_friend.name")?.to_owned(),
                detail: form
                    .optional_text("vile_friend.detail")?
                    .unwrap_or_default()
                    .to_owned(),
            },
            starting_resources: unsigned(form, "starting_resources")?,
            seed: u64::try_from(form.integer("seed")?)
                .map_err(|_| invalid("seed", "expected a non-negative seed"))?,
        };
        let preview = create_wicked_ones_character(&request)?;
        validate_wicked_ones_creation_preview(&preview)?;
        artifact(&preview)
    }

    fn artifact(
        preview: &WickedOnesCreationPreview,
    ) -> Result<PanelCreationPreview, TabletopError> {
        Ok(PanelCreationPreview {
            definition: preview.definition.clone(),
            initial_state: preview.initial_state.clone(),
            explanations: preview.explanations.clone(),
            diagnostics: Vec::new(),
            canonical_json: preview.to_json()?,
            canonical_ron: preview.to_ron()?,
        })
    }

    fn resolve_action_fields() -> Vec<PanelField> {
        vec![
            required_choice("action", "Action", ACTIONS),
            required_choice("approach", "Approach", APPROACHES),
            required_choice("position", "Position", POSITIONS),
            required_choice("effect", "Effect", EFFECTS),
            required_text("intention", "Intention"),
            required_unsigned("bonus", "Bonus dice"),
            required_boolean("going_hard", "Going hard"),
            optional_text_field("teamwork[0].helper_id", "Helper identity"),
            optional_unsigned("teamwork[0].dice", "Helper dice"),
            optional_choice_field("resistance.action", "Resistance action", ACTIONS),
            optional_unsigned("resistance.bonus", "Resistance bonus"),
        ]
    }

    fn build_resolve_action(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let teamwork = match form.optional_text("teamwork[0].helper_id")? {
            Some(helper) if !helper.is_empty() => DomainValue::List(vec![object([
                (
                    "dice",
                    number(form.optional_integer("teamwork[0].dice")?.unwrap_or(0)),
                ),
                ("helper_id", text(helper)),
            ])]),
            _ => DomainValue::List(Vec::new()),
        };
        let mut fields = BTreeMap::from([
            ("action".to_owned(), symbol(form.choice("action")?)),
            ("approach".to_owned(), symbol(form.choice("approach")?)),
            ("bonus".to_owned(), number(form.integer("bonus")?)),
            ("effect".to_owned(), symbol(form.choice("effect")?)),
            (
                "going_hard".to_owned(),
                DomainValue::Bool(form.boolean("going_hard")?),
            ),
            ("intention".to_owned(), text(form.text("intention")?)),
            ("position".to_owned(), symbol(form.choice("position")?)),
            ("teamwork".to_owned(), teamwork),
        ]);
        if let Some(action) = form.optional_choice("resistance.action")? {
            fields.insert(
                "resistance".to_owned(),
                object([
                    ("action", symbol(action)),
                    (
                        "bonus",
                        number(form.optional_integer("resistance.bonus")?.unwrap_or(0)),
                    ),
                ]),
            );
        }
        resolution_request(
            "resolve_action",
            TabletopCapability::ChecksAndConflicts,
            DomainValue::Object(fields),
            definition,
            state,
        )
    }

    fn campaign_operations() -> Vec<PanelOperation> {
        vec![
            operation(
                "advance_campaign_phase",
                "Advance phase",
                TabletopCapability::CampaignState,
                vec![
                    required_choice("next_phase", "Next phase", &WICKED_ONES_PHASES),
                    required_text("note", "Transition note"),
                ],
                build_advance_phase,
            ),
            operation(
                "perform_downtime_action",
                "Downtime action",
                TabletopCapability::CampaignState,
                vec![
                    required_choice("kind", "Downtime kind", DOWNTIME_KINDS),
                    required_text("note", "Downtime note"),
                    optional_text_field("project_id", "Project, or empty"),
                ],
                build_downtime,
            ),
            operation(
                "resolve_dungeon_event",
                "Resolve dungeon event",
                TabletopCapability::CampaignState,
                vec![
                    required_text("event", "Dungeon event"),
                    required_text("note", "Resolution note"),
                ],
                build_dungeon_event,
            ),
            operation(
                "resolve_calamity",
                "Resolve calamity",
                TabletopCapability::CampaignState,
                vec![
                    required_unsigned("severity", "Severity"),
                    required_text("note", "Calamity note"),
                ],
                build_calamity,
            ),
            operation(
                "plan_raid",
                "Plan raid",
                TabletopCapability::CampaignState,
                vec![
                    required_text("raid_id", "Raid identity"),
                    required_text("label", "Raid label"),
                    required_text("target_id", "Target identity"),
                    required_text("plan", "Plan"),
                    required_text("goal", "Goal"),
                    required_text("advantages", "Advantages, comma separated"),
                    required_text("disadvantages", "Disadvantages, comma separated"),
                ],
                build_plan_raid,
            ),
            operation(
                "engage_raid",
                "Engage raid",
                TabletopCapability::CampaignState,
                vec![required_text("raid_id", "Raid identity")],
                build_engage_raid,
            ),
            operation(
                "resolve_raid_outcome",
                "Resolve raid outcome",
                TabletopCapability::CampaignState,
                vec![
                    required_text("raid_id", "Raid identity"),
                    required_choice("outcome", "Outcome", OUTCOMES),
                    required_text("note", "Outcome note"),
                ],
                build_raid_outcome,
            ),
            operation(
                "perform_recovery",
                "Perform recovery",
                TabletopCapability::CampaignState,
                vec![
                    required_unsigned("amount", "Stress recovered"),
                    required_text("note", "Recovery note"),
                ],
                build_recovery,
            ),
            operation(
                "pause_campaign",
                "Pause campaign",
                TabletopCapability::CampaignState,
                vec![required_text("note", "Pause note")],
                build_pause,
            ),
            operation(
                "resume_campaign",
                "Resume campaign",
                TabletopCapability::CampaignState,
                vec![required_text("note", "Resume note")],
                build_resume,
            ),
        ]
    }

    fn build_apply_shock(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "apply_shock",
            TabletopCapability::ResourcesAndConditions,
            [
                ("amount", number(form.integer("amount")?)),
                ("source", text(form.text("source")?)),
            ],
            definition,
            state,
        )
    }

    fn build_go_feral(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "go_feral",
            TabletopCapability::ResourcesAndConditions,
            [("trigger", text(form.text("trigger")?))],
            definition,
            state,
        )
    }

    fn build_death_knell(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "death_knell",
            TabletopCapability::ResourcesAndConditions,
            [("knell", text(form.text("knell")?))],
            definition,
            state,
        )
    }

    fn build_flashback(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "flashback",
            TabletopCapability::ResourcesAndConditions,
            [
                ("stress_cost", number(form.integer("stress_cost")?)),
                ("summary", text(form.text("summary")?)),
            ],
            definition,
            state,
        )
    }

    fn build_gain_dark_heart(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "gain_dark_heart",
            TabletopCapability::ResourcesAndConditions,
            [("reason", text(form.text("reason")?))],
            definition,
            state,
        )
    }

    fn build_advance_phase(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            [
                ("next_phase", symbol(form.choice("next_phase")?)),
                ("note", text(form.text("note")?)),
            ],
            definition,
            state,
        )
    }

    fn build_downtime(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "perform_downtime_action",
            TabletopCapability::CampaignState,
            [
                ("kind", symbol(form.choice("kind")?)),
                ("note", text(form.text("note")?)),
                (
                    "project_id",
                    text(form.optional_text("project_id")?.unwrap_or("")),
                ),
            ],
            definition,
            state,
        )
    }

    fn build_dungeon_event(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resolve_dungeon_event",
            TabletopCapability::CampaignState,
            [
                ("event", text(form.text("event")?)),
                ("note", text(form.text("note")?)),
            ],
            definition,
            state,
        )
    }

    fn build_calamity(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resolve_calamity",
            TabletopCapability::CampaignState,
            [
                ("note", text(form.text("note")?)),
                ("severity", number(form.integer("severity")?)),
            ],
            definition,
            state,
        )
    }

    fn build_plan_raid(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let input = object([
            ("advantages", text_list(form.text("advantages")?)),
            ("disadvantages", text_list(form.text("disadvantages")?)),
            ("goal", text(form.text("goal")?)),
            ("label", text(form.text("label")?)),
            ("plan", text(form.text("plan")?)),
            ("raid_id", text(form.text("raid_id")?)),
            ("target_id", text(form.text("target_id")?)),
        ]);
        resolution_request(
            "plan_raid",
            TabletopCapability::CampaignState,
            input,
            definition,
            state,
        )
    }

    fn build_engage_raid(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "engage_raid",
            TabletopCapability::CampaignState,
            [("raid_id", text(form.text("raid_id")?))],
            definition,
            state,
        )
    }

    fn build_raid_outcome(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resolve_raid_outcome",
            TabletopCapability::CampaignState,
            [
                ("note", text(form.text("note")?)),
                ("outcome", symbol(form.choice("outcome")?)),
                ("raid_id", text(form.text("raid_id")?)),
            ],
            definition,
            state,
        )
    }

    fn build_recovery(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "perform_recovery",
            TabletopCapability::CampaignState,
            [
                ("amount", number(form.integer("amount")?)),
                ("note", text(form.text("note")?)),
            ],
            definition,
            state,
        )
    }

    fn build_pause(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "pause_campaign",
            TabletopCapability::CampaignState,
            [("note", text(form.text("note")?))],
            definition,
            state,
        )
    }

    fn build_resume(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resume_campaign",
            TabletopCapability::CampaignState,
            [("note", text(form.text("note")?))],
            definition,
            state,
        )
    }

    fn build_spend_wicked(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "spend_wicked_advancement",
            TabletopCapability::Advancement,
            [
                ("ability", text(form.text("ability")?)),
                ("note", text(form.text("note")?)),
            ],
            definition,
            state,
        )
    }

    fn build_spend_reckless(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "spend_reckless_advancement",
            TabletopCapability::Advancement,
            [
                ("action", symbol(form.choice("action")?)),
                ("note", text(form.text("note")?)),
                ("to_rating", number(form.integer("to_rating")?)),
            ],
            definition,
            state,
        )
    }

    fn build_purchase_upgrade(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "purchase_upgrade",
            TabletopCapability::EquipmentAndAbilities,
            [
                ("note", text(form.text("note")?)),
                ("upgrade_id", text(form.text("upgrade_id")?)),
            ],
            definition,
            state,
        )
    }

    fn simple<const N: usize>(
        operation: &'static str,
        capability: TabletopCapability,
        fields: [(&str, DomainValue); N],
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        resolution_request(operation, capability, object(fields), definition, state)
    }

    fn operation(
        id: &'static str,
        title: &'static str,
        capability: TabletopCapability,
        fields: Vec<PanelField>,
        build_request: fn(
            &PanelForm,
            &AdapterCharacterDefinition,
            &TabletopState,
        ) -> Result<ResolutionRequest, TabletopError>,
    ) -> PanelOperation {
        PanelOperation {
            id,
            title,
            capability,
            fields,
            build_request,
        }
    }

    fn required_text(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: true,
        }
    }

    fn optional_text_field(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: false,
        }
    }

    fn required_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: true,
        }
    }

    fn optional_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: false,
        }
    }

    fn required_boolean(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Boolean,
            required: true,
        }
    }

    fn required_choice(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: true,
        }
    }

    fn optional_choice_field(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: false,
        }
    }

    fn unsigned(form: &PanelForm, path: &'static str) -> Result<u32, TabletopError> {
        u32::try_from(form.integer(path)?)
            .map_err(|_| invalid(path, "expected a non-negative integer"))
    }

    fn unsigned_u8(form: &PanelForm, path: &'static str) -> Result<u8, TabletopError> {
        u8::try_from(form.integer(path)?)
            .map_err(|_| invalid(path, "expected a non-negative integer"))
    }

    fn calling(value: &str) -> Result<WickedOnesCalling, TabletopError> {
        match value {
            "forgekeeper" => Ok(WickedOnesCalling::Forgekeeper),
            "warbringer" => Ok(WickedOnesCalling::Warbringer),
            "whisperer" => Ok(WickedOnesCalling::Whisperer),
            "wildcaller" => Ok(WickedOnesCalling::Wildcaller),
            _ => Err(invalid("calling", "unknown Wicked Ones Calling")),
        }
    }
}

/// Open Tale creation, challenge, combat, and advancement panels.
pub mod open_tale {
    use super::*;
    use weave_tabletop::{
        OPEN_TALE_ATTRIBUTE_BUDGET, OPEN_TALE_CREATION_FORMAT_VERSION, OPEN_TALE_MAX_SKILL_RANKS,
        OpenTaleAttributes, OpenTaleCreationRequest, OpenTaleSkillSelection,
        create_open_tale_character, open_tale_manifest, validate_open_tale_creation_preview,
    };

    const ATTRIBUTES: &[&str] = &[
        "dexterity",
        "intelligence",
        "magicy",
        "motion",
        "perception",
        "persuasion",
        "strength",
        "vitality",
        "volition",
    ];
    const MODES: &[&str] = &["advantage", "disadvantage", "normal"];
    const RECOVERY_MODES: &[&str] = &["rest", "treat"];

    pub struct OpenTalePanels;

    impl TabletopPanelAdapter for OpenTalePanels {
        fn adapter_id(&self) -> &'static str {
            weave_tabletop::OPEN_TALE_ADAPTER_ID
        }

        fn adapter_version(&self) -> &'static str {
            weave_tabletop::OPEN_TALE_ADAPTER_VERSION
        }

        fn manifest(&self) -> AdapterManifest {
            open_tale_manifest()
        }

        fn creation_panel(&self) -> CreationPanel {
            CreationPanel {
                title: "Open Tale character",
                description: "Point-budget attributes, skills, wealth, and the exact quality formulas.",
                fields: creation_fields(),
                build: build_creation,
            }
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            vec![
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ChecksAndConflicts,
                    title: "Challenges and combat",
                    description: "Roll checks, record marks, resolve combat turns, and apply damage.",
                    operations: vec![
                        operation(
                            "resolve_challenge",
                            "Resolve challenge",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_choice("attribute", "Attribute", ATTRIBUTES),
                                required_text("challenge_id", "Challenge identity"),
                                required_unsigned("difficulty", "Difficulty"),
                                required_unsigned("distance", "Distance"),
                                required_boolean("heroic_badge", "Spend heroic badge"),
                                required_text("label", "Challenge label"),
                                required_choice("mode", "Dice mode", MODES),
                                required_signed("modifier", "Situational modifier"),
                                optional_text_field("skill_id", "Active skill, or empty"),
                            ],
                            build_resolve_challenge,
                        ),
                        operation(
                            "resolve_combat_turn",
                            "Resolve combat turn",
                            TabletopCapability::ChecksAndConflicts,
                            vec![
                                required_text("combat_id", "Combat identity"),
                                required_boolean("heroic_badge", "Spend heroic badge"),
                                required_choice("mode", "Attack mode", MODES),
                                optional_text_field("weapon_id", "Owned weapon, or empty"),
                            ],
                            build_resolve_combat_turn,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::Encounters,
                    title: "Encounter movement",
                    description: "Start combats and move within the movement allowance.",
                    operations: vec![
                        operation(
                            "start_combat",
                            "Start combat",
                            TabletopCapability::Encounters,
                            vec![
                                required_text("combat_id", "Combat identity"),
                                required_text("label", "Combat label"),
                                required_text("adversary_name", "Adversary name"),
                                required_unsigned("adversary_agility", "Adversary agility"),
                                required_unsigned("adversary_damage", "Adversary damage"),
                                required_unsigned("adversary_defense", "Adversary defense"),
                                required_unsigned("adversary_health", "Adversary health"),
                                required_unsigned("distance", "Starting distance"),
                            ],
                            build_start_combat,
                        ),
                        operation(
                            "move_combat",
                            "Move in combat",
                            TabletopCapability::Encounters,
                            vec![
                                required_text("combat_id", "Combat identity"),
                                required_unsigned("movement", "Distance moved"),
                            ],
                            build_move_combat,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ResourcesAndConditions,
                    title: "Recovery",
                    description: "Restore health and resolve marks through rest or treatment.",
                    operations: vec![operation(
                        "apply_recovery",
                        "Apply recovery",
                        TabletopCapability::ResourcesAndConditions,
                        vec![required_choice("mode", "Recovery mode", RECOVERY_MODES)],
                        build_apply_recovery,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::DerivedValues,
                    title: "Character inspection",
                    description: "Show attributes, qualities, skills, items, wealth, level, and experience.",
                    operations: vec![operation(
                        "inspect_character",
                        "Inspect character",
                        TabletopCapability::DerivedValues,
                        Vec::new(),
                        build_inspect,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::Advancement,
                    title: "Advancement",
                    description: "Award experience and record a milestone below the level cap.",
                    operations: vec![operation(
                        "advance_character",
                        "Advance character",
                        TabletopCapability::Advancement,
                        vec![
                            required_unsigned("experience_gain", "Experience gained"),
                            required_text("milestone", "Milestone"),
                        ],
                        build_advance,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::EquipmentAndAbilities,
                    title: "Purchases",
                    description: "Spend wealth from the bounded item catalog.",
                    operations: vec![operation(
                        "purchase_item",
                        "Purchase item",
                        TabletopCapability::EquipmentAndAbilities,
                        vec![
                            required_text("item_id", "Catalog item identity"),
                            required_unsigned("quantity", "Quantity"),
                        ],
                        build_purchase,
                    )],
                },
            ]
        }
    }

    fn creation_fields() -> Vec<PanelField> {
        let mut fields = vec![
            required_text("character_id", "Character identity"),
            required_text("name", "Name"),
            required_unsigned("seed", "Deterministic seed"),
            required_unsigned("starting_wealth", "Starting wealth"),
            required_unsigned("heroic_badges", "Heroic badges"),
            optional_text_field("skills[0].id", "Skill 1 identity"),
            optional_unsigned("skills[0].rank", "Skill 1 rank"),
            optional_text_field("skills[1].id", "Skill 2 identity"),
            optional_unsigned("skills[1].rank", "Skill 2 rank"),
            optional_text_field("items[0]", "Item 1 identity"),
            optional_text_field("items[1]", "Item 2 identity"),
        ];
        for attribute in ATTRIBUTES {
            let path = match *attribute {
                "dexterity" => "attributes.dexterity",
                "intelligence" => "attributes.intelligence",
                "magicy" => "attributes.magicy",
                "motion" => "attributes.motion",
                "perception" => "attributes.perception",
                "persuasion" => "attributes.persuasion",
                "strength" => "attributes.strength",
                "vitality" => "attributes.vitality",
                _ => "attributes.volition",
            };
            fields.push(required_unsigned(path, "Attribute rating"));
        }
        fields
    }

    fn build_creation(form: &PanelForm) -> Result<PanelCreationPreview, TabletopError> {
        let mut skills = Vec::new();
        for index in 0..2 {
            let id_path = if index == 0 {
                "skills[0].id"
            } else {
                "skills[1].id"
            };
            let rank_path = if index == 0 {
                "skills[0].rank"
            } else {
                "skills[1].rank"
            };
            if optional_group(form, id_path, &[rank_path])? {
                skills.push(OpenTaleSkillSelection {
                    id: form.text(id_path)?.to_owned(),
                    rank: u32::try_from(form.optional_integer(rank_path)?.unwrap_or(1))
                        .map_err(|_| invalid(rank_path, "expected a rank"))?,
                });
            }
        }
        let mut items = Vec::new();
        for path in ["items[0]", "items[1]"] {
            if let Some(id) = form.optional_text(path)?
                && !id.is_empty()
            {
                items.push(id.to_owned());
            }
        }
        let request = OpenTaleCreationRequest {
            attributes: OpenTaleAttributes {
                dexterity: unsigned(form, "attributes.dexterity")?,
                intelligence: unsigned(form, "attributes.intelligence")?,
                magicy: unsigned(form, "attributes.magicy")?,
                motion: unsigned(form, "attributes.motion")?,
                perception: unsigned(form, "attributes.perception")?,
                persuasion: unsigned(form, "attributes.persuasion")?,
                strength: unsigned(form, "attributes.strength")?,
                vitality: unsigned(form, "attributes.vitality")?,
                volition: unsigned(form, "attributes.volition")?,
            },
            character_id: form.text("character_id")?.to_owned(),
            creation_format_version: OPEN_TALE_CREATION_FORMAT_VERSION,
            experience: 0,
            heroic_badges: unsigned(form, "heroic_badges")?,
            items,
            level: 1,
            name: form.text("name")?.to_owned(),
            seed: u64::try_from(form.integer("seed")?)
                .map_err(|_| invalid("seed", "expected a non-negative seed"))?,
            skills,
            starting_wealth: unsigned(form, "starting_wealth")?,
        };
        let preview = create_open_tale_character(&request)?;
        validate_open_tale_creation_preview(&preview)?;
        let skill_ranks = request.skills.iter().map(|skill| skill.rank).sum::<u32>();
        Ok(PanelCreationPreview {
            canonical_json: preview.to_json()?,
            canonical_ron: preview.to_ron()?,
            definition: preview.definition,
            initial_state: preview.initial_state,
            explanations: preview.explanations,
            diagnostics: vec![
                PanelDiagnostic {
                    code: "budget.attributes".to_owned(),
                    path: "attributes".to_owned(),
                    message: format!(
                        "{} of {OPEN_TALE_ATTRIBUTE_BUDGET} attribute points allocated",
                        request.attributes.total()
                    ),
                },
                PanelDiagnostic {
                    code: "budget.skills".to_owned(),
                    path: "skills".to_owned(),
                    message: format!(
                        "{skill_ranks} of {OPEN_TALE_MAX_SKILL_RANKS} skill ranks allocated"
                    ),
                },
                PanelDiagnostic {
                    code: "budget.wealth".to_owned(),
                    path: "items".to_owned(),
                    message: format!(
                        "{} wealth spent, {} remaining",
                        preview.wealth_spent, preview.wealth_remaining
                    ),
                },
            ],
        })
    }

    fn build_resolve_challenge(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resolve_challenge",
            TabletopCapability::ChecksAndConflicts,
            [
                ("attribute", symbol(form.choice("attribute")?)),
                ("challenge_id", text(form.text("challenge_id")?)),
                ("difficulty", number(form.integer("difficulty")?)),
                ("distance", number(form.integer("distance")?)),
                (
                    "heroic_badge",
                    DomainValue::Bool(form.boolean("heroic_badge")?),
                ),
                ("label", text(form.text("label")?)),
                ("mode", symbol(form.choice("mode")?)),
                ("modifier", number(form.integer("modifier")?)),
                (
                    "skill_id",
                    text(form.optional_text("skill_id")?.unwrap_or("")),
                ),
            ],
            definition,
            state,
        )
    }

    fn build_resolve_combat_turn(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "resolve_combat_turn",
            TabletopCapability::ChecksAndConflicts,
            [
                ("combat_id", text(form.text("combat_id")?)),
                (
                    "heroic_badge",
                    DomainValue::Bool(form.boolean("heroic_badge")?),
                ),
                ("mode", symbol(form.choice("mode")?)),
                (
                    "weapon_id",
                    text(form.optional_text("weapon_id")?.unwrap_or("")),
                ),
            ],
            definition,
            state,
        )
    }

    fn build_start_combat(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "start_combat",
            TabletopCapability::Encounters,
            [
                (
                    "adversary_agility",
                    number(form.integer("adversary_agility")?),
                ),
                (
                    "adversary_damage",
                    number(form.integer("adversary_damage")?),
                ),
                (
                    "adversary_defense",
                    number(form.integer("adversary_defense")?),
                ),
                (
                    "adversary_health",
                    number(form.integer("adversary_health")?),
                ),
                ("adversary_name", text(form.text("adversary_name")?)),
                ("combat_id", text(form.text("combat_id")?)),
                ("distance", number(form.integer("distance")?)),
                ("label", text(form.text("label")?)),
            ],
            definition,
            state,
        )
    }

    fn build_move_combat(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "move_combat",
            TabletopCapability::Encounters,
            [
                ("combat_id", text(form.text("combat_id")?)),
                ("movement", number(form.integer("movement")?)),
            ],
            definition,
            state,
        )
    }

    fn build_apply_recovery(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "apply_recovery",
            TabletopCapability::ResourcesAndConditions,
            [("mode", symbol(form.choice("mode")?))],
            definition,
            state,
        )
    }

    fn build_inspect(
        _form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        resolution_request(
            "inspect_character",
            TabletopCapability::DerivedValues,
            DomainValue::Object(BTreeMap::new()),
            definition,
            state,
        )
    }

    fn build_advance(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "advance_character",
            TabletopCapability::Advancement,
            [
                ("experience_gain", number(form.integer("experience_gain")?)),
                ("milestone", text(form.text("milestone")?)),
            ],
            definition,
            state,
        )
    }

    fn build_purchase(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "purchase_item",
            TabletopCapability::EquipmentAndAbilities,
            [
                ("item_id", text(form.text("item_id")?)),
                ("quantity", number(form.integer("quantity")?)),
            ],
            definition,
            state,
        )
    }

    fn simple<const N: usize>(
        operation: &'static str,
        capability: TabletopCapability,
        fields: [(&str, DomainValue); N],
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        resolution_request(operation, capability, object(fields), definition, state)
    }

    fn operation(
        id: &'static str,
        title: &'static str,
        capability: TabletopCapability,
        fields: Vec<PanelField>,
        build_request: fn(
            &PanelForm,
            &AdapterCharacterDefinition,
            &TabletopState,
        ) -> Result<ResolutionRequest, TabletopError>,
    ) -> PanelOperation {
        PanelOperation {
            id,
            title,
            capability,
            fields,
            build_request,
        }
    }

    fn required_text(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: true,
        }
    }

    fn optional_text_field(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: false,
        }
    }

    fn required_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: true,
        }
    }

    fn optional_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: false,
        }
    }

    fn required_signed(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Signed,
            required: true,
        }
    }

    fn required_boolean(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Boolean,
            required: true,
        }
    }

    fn required_choice(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: true,
        }
    }

    fn unsigned(form: &PanelForm, path: &'static str) -> Result<u32, TabletopError> {
        u32::try_from(form.integer(path)?)
            .map_err(|_| invalid(path, "expected a non-negative integer"))
    }
}

/// Open Adventure creation, encounter, module, and advancement panels.
pub mod open_adventure {
    use super::*;
    use weave_tabletop::{
        OPEN_ADVENTURE_CREATION_FORMAT_VERSION, OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN,
        OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES, OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256,
        OPEN_ADVENTURE_MODULE_FORMAT_VERSION, OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION,
        OPEN_ADVENTURE_NAMESPACE, OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES,
        OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256, OPEN_ADVENTURE_SOURCE_URL,
        OPEN_ADVENTURE_VERIFIED_MODULES, OpenAdventureAttribute, OpenAdventureAttributes,
        OpenAdventureCreationRequest, OpenAdventureEquipment, OpenAdventureEquipmentKind,
        OpenAdventureFavoredAction, OpenAdventureModifierTarget, OpenAdventureModuleArtifact,
        OpenAdventureModuleContentClass, OpenAdventureModuleDescriptor,
        OpenAdventureModuleExperience, OpenAdventureModuleSelection, OpenAdventureModusOperandi,
        OpenAdventureOrigin, OpenAdventureSpecialization, OpenAdventureTraitModifier,
        create_open_adventure_character, open_adventure_manifest, open_adventure_module_options,
        validate_open_adventure_creation_preview,
    };

    const ATTRIBUTES: &[&str] = &[
        "agility",
        "empathy",
        "endurance",
        "focus",
        "insight",
        "might",
        "presence",
        "reasoning",
        "will",
    ];
    const EQUIPMENT_KINDS: &[&str] = &["armor", "tool", "weapon"];
    const MODIFIER_TARGETS: &[&str] = &[
        "action_score",
        "body_points",
        "defense",
        "mind_points",
        "reaction",
        "spirit_points",
    ];
    const DISCLOSURES: &[&str] = &["host_only", "public"];
    const CHALLENGE_KINDS: &[&str] = &["environmental", "npc"];
    const POOLS: &[&str] = &["body", "mind", "spirit"];
    const ACTION_KINDS: &[&str] = &["attack", "spell"];
    const TARGET_KINDS: &[&str] = &["challenge", "participant"];
    const ENDINGS: &[&str] = &["defeat", "host_decision", "success", "withdrawal"];
    const EXPERIENCES: &[&str] = &["core", "extended"];

    pub struct OpenAdventurePanels;

    impl TabletopPanelAdapter for OpenAdventurePanels {
        fn adapter_id(&self) -> &'static str {
            weave_tabletop::OPEN_ADVENTURE_ADAPTER_ID
        }

        fn adapter_version(&self) -> &'static str {
            weave_tabletop::OPEN_ADVENTURE_ADAPTER_VERSION
        }

        fn manifest(&self) -> AdapterManifest {
            open_adventure_manifest()
        }

        fn creation_panel(&self) -> CreationPanel {
            CreationPanel {
                title: "Open Adventure character",
                description: "Attributes, lineage, culture, background, specializations, favored actions, equipment, Benefits, and Drawbacks.",
                fields: creation_fields(),
                build: build_creation,
            }
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            vec![
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::Encounters,
                    title: "Encounters",
                    description: "Participants, turn order, actions, wounds, magic, Story Points, and endings.",
                    operations: vec![
                        operation(
                            "begin_challenge",
                            "Begin challenge",
                            TabletopCapability::Encounters,
                            vec![
                                required_text("challenge_id", "Challenge identity"),
                                required_text("label", "Challenge label"),
                                required_choice("kind", "Challenge kind", CHALLENGE_KINDS),
                                required_unsigned("difficulty_score", "Difficulty score"),
                                required_unsigned("challenge_points", "Challenge points"),
                                required_unsigned("effect", "Effect"),
                                required_choice("disclosure", "Disclosure", DISCLOSURES),
                                optional_unsigned("npc_defense", "NPC defense"),
                                optional_unsigned("npc_effect", "NPC effect"),
                                optional_choice_field("npc_target", "NPC target pool", POOLS),
                            ],
                            build_begin_challenge,
                        ),
                        operation(
                            "start_encounter",
                            "Start encounter",
                            TabletopCapability::Encounters,
                            vec![
                                required_text("encounter_id", "Encounter identity"),
                                required_text("document_json", "Encounter document JSON"),
                                required_text("document_sha256", "Document SHA-256"),
                                required_choice("disclosure", "Disclosure", DISCLOSURES),
                            ],
                            build_start_encounter,
                        ),
                        operation(
                            "resolve_encounter_action",
                            "Resolve encounter action",
                            TabletopCapability::Encounters,
                            resolve_encounter_fields(),
                            build_resolve_encounter_action,
                        ),
                        operation(
                            "end_encounter",
                            "End encounter",
                            TabletopCapability::Encounters,
                            vec![
                                required_text("encounter_id", "Encounter identity"),
                                required_choice("ending", "Ending", ENDINGS),
                                required_choice("disclosure", "Disclosure", DISCLOSURES),
                            ],
                            build_end_encounter,
                        ),
                    ],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ChecksAndConflicts,
                    title: "Challenge actions",
                    description: "Exploding action dice against the challenge target.",
                    operations: vec![operation(
                        "resolve_challenge_action",
                        "Resolve challenge action",
                        TabletopCapability::ChecksAndConflicts,
                        vec![
                            required_text("challenge_id", "Challenge identity"),
                            required_choice("attribute", "Attribute", ATTRIBUTES),
                            required_choice("disclosure", "Disclosure", DISCLOSURES),
                            required_signed("situation_modifier", "Situation modifier"),
                            optional_text_field("favored_action_id", "Favored action, or empty"),
                        ],
                        build_resolve_challenge_action,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Play,
                    capability: TabletopCapability::ResourcesAndConditions,
                    title: "Wounds",
                    description: "Restore a Body, Mind, or Spirit pool and clear a wound.",
                    operations: vec![operation(
                        "heal_encounter_participant",
                        "Heal participant",
                        TabletopCapability::ResourcesAndConditions,
                        vec![
                            required_text("encounter_id", "Encounter identity"),
                            required_text("participant_id", "Participant identity"),
                            required_choice("pool", "Pool", POOLS),
                            required_unsigned("amount", "Points restored"),
                            required_choice("disclosure", "Disclosure", DISCLOSURES),
                        ],
                        build_heal_participant,
                    )],
                },
                PanelDescriptor {
                    kind: PanelKind::Progression,
                    capability: TabletopCapability::Advancement,
                    title: "Advancement",
                    description: "Spend experience to raise one attribute bonus.",
                    operations: vec![operation(
                        "advance_encounter_participant",
                        "Advance participant",
                        TabletopCapability::Advancement,
                        vec![
                            required_text("encounter_id", "Encounter identity"),
                            required_text("participant_id", "Participant identity"),
                            required_choice("attribute", "Attribute", ATTRIBUTES),
                            required_unsigned("experience_cost", "Experience cost"),
                            required_choice("disclosure", "Disclosure", DISCLOSURES),
                        ],
                        build_advance_participant,
                    )],
                },
            ]
        }

        fn module_panel(&self) -> Option<ModulePanel> {
            Some(ModulePanel {
                capability: TabletopCapability::DerivedValues,
                title: "Optional modules",
                fields: vec![
                    optional_text_field("enabled_module", "Module identity"),
                    required_choice("experience", "Experience profile", EXPERIENCES),
                ],
                modules: module_entries(),
                apply: apply_modules,
            })
        }
    }

    fn creation_fields() -> Vec<PanelField> {
        let mut fields = vec![
            required_text("character_id", "Character identity"),
            required_text("name", "Name"),
            required_unsigned("seed", "Deterministic seed"),
            required_text("lineage.id", "Lineage identity"),
            required_text("lineage.label", "Lineage label"),
            required_text("lineage.note", "Lineage note"),
            required_text("culture.id", "Culture identity"),
            required_text("culture.label", "Culture label"),
            required_text("culture.note", "Culture note"),
            required_text("background.id", "Background identity"),
            required_text("background.label", "Background label"),
            required_text("background.note", "Background note"),
            required_text("modus_operandi.id", "Modus Operandi identity"),
            required_text("modus_operandi.label", "Modus Operandi label"),
            required_unsigned("modus_operandi.score", "Modus Operandi score"),
            optional_text_field("specializations[0].id", "Specialization identity"),
            optional_text_field("specializations[0].label", "Specialization label"),
            optional_choice_field(
                "specializations[0].attribute",
                "Specialization attribute",
                ATTRIBUTES,
            ),
            optional_unsigned("specializations[0].bonus", "Specialization bonus"),
            optional_text_field("favored_actions[0].id", "Favored action identity"),
            optional_text_field("favored_actions[0].label", "Favored action label"),
            optional_choice_field(
                "favored_actions[0].attribute",
                "Favored action attribute",
                ATTRIBUTES,
            ),
            optional_text_field("equipment[0].id", "Equipment identity"),
            optional_text_field("equipment[0].label", "Equipment label"),
            optional_choice_field("equipment[0].kind", "Equipment kind", EQUIPMENT_KINDS),
            optional_text_field("benefits[0].id", "Benefit identity"),
            optional_text_field("benefits[0].label", "Benefit label"),
            optional_choice_field("benefits[0].target", "Benefit target", MODIFIER_TARGETS),
            optional_signed("benefits[0].modifier", "Benefit modifier"),
            optional_text_field("drawbacks[0].id", "Drawback identity"),
            optional_text_field("drawbacks[0].label", "Drawback label"),
            optional_choice_field("drawbacks[0].target", "Drawback target", MODIFIER_TARGETS),
            optional_signed("drawbacks[0].modifier", "Drawback modifier"),
        ];
        for attribute in ATTRIBUTES {
            let path = match *attribute {
                "agility" => "attributes.agility",
                "empathy" => "attributes.empathy",
                "endurance" => "attributes.endurance",
                "focus" => "attributes.focus",
                "insight" => "attributes.insight",
                "might" => "attributes.might",
                "presence" => "attributes.presence",
                "reasoning" => "attributes.reasoning",
                _ => "attributes.will",
            };
            fields.push(required_unsigned(path, "Attribute rating"));
        }
        fields
    }

    fn build_creation(form: &PanelForm) -> Result<PanelCreationPreview, TabletopError> {
        let specializations = if optional_group(
            form,
            "specializations[0].id",
            &[
                "specializations[0].label",
                "specializations[0].attribute",
                "specializations[0].bonus",
            ],
        )? {
            vec![OpenAdventureSpecialization {
                id: form.text("specializations[0].id")?.to_owned(),
                label: form.text("specializations[0].label")?.to_owned(),
                attribute: attribute(form.choice("specializations[0].attribute")?)?,
                bonus: unsigned(form, "specializations[0].bonus")?,
            }]
        } else {
            Vec::new()
        };
        let favored_actions = if optional_group(
            form,
            "favored_actions[0].id",
            &["favored_actions[0].label", "favored_actions[0].attribute"],
        )? {
            vec![OpenAdventureFavoredAction {
                id: form.text("favored_actions[0].id")?.to_owned(),
                label: form.text("favored_actions[0].label")?.to_owned(),
                attribute: attribute(form.choice("favored_actions[0].attribute")?)?,
            }]
        } else {
            Vec::new()
        };
        let equipment = if optional_group(
            form,
            "equipment[0].id",
            &["equipment[0].label", "equipment[0].kind"],
        )? {
            vec![OpenAdventureEquipment {
                id: form.text("equipment[0].id")?.to_owned(),
                label: form.text("equipment[0].label")?.to_owned(),
                kind: equipment_kind(form.choice("equipment[0].kind")?)?,
            }]
        } else {
            Vec::new()
        };
        let benefits = trait_modifiers(form, "benefits[0]")?;
        let drawbacks = trait_modifiers(form, "drawbacks[0]")?;
        let request = OpenAdventureCreationRequest {
            creation_format_version: OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
            character_id: form.text("character_id")?.to_owned(),
            name: form.text("name")?.to_owned(),
            seed: u64::try_from(form.integer("seed")?)
                .map_err(|_| invalid("seed", "expected a non-negative seed"))?,
            lineage: origin(form, "lineage")?,
            culture: origin(form, "culture")?,
            background: origin(form, "background")?,
            modus_operandi: OpenAdventureModusOperandi {
                id: form.text("modus_operandi.id")?.to_owned(),
                label: form.text("modus_operandi.label")?.to_owned(),
                score: unsigned(form, "modus_operandi.score")?,
            },
            attributes: OpenAdventureAttributes {
                agility: unsigned(form, "attributes.agility")?,
                empathy: unsigned(form, "attributes.empathy")?,
                endurance: unsigned(form, "attributes.endurance")?,
                focus: unsigned(form, "attributes.focus")?,
                insight: unsigned(form, "attributes.insight")?,
                might: unsigned(form, "attributes.might")?,
                presence: unsigned(form, "attributes.presence")?,
                reasoning: unsigned(form, "attributes.reasoning")?,
                will: unsigned(form, "attributes.will")?,
            },
            specializations,
            favored_actions,
            equipment,
            benefits,
            drawbacks,
        };
        let preview = create_open_adventure_character(&request)?;
        validate_open_adventure_creation_preview(&preview)?;
        let derived = &preview.derived;
        Ok(PanelCreationPreview {
            canonical_json: preview.to_json()?,
            canonical_ron: preview.to_ron()?,
            definition: preview.definition,
            initial_state: preview.initial_state,
            explanations: preview.explanations,
            diagnostics: vec![PanelDiagnostic {
                code: "derived.values".to_owned(),
                path: "derived".to_owned(),
                message: format!(
                    "Defense {}; Reaction {}; Body {}; Mind {}; Spirit {}",
                    derived.defense,
                    derived.reaction,
                    derived.body_points,
                    derived.mind_points,
                    derived.spirit_points
                ),
            }],
        })
    }

    fn origin(
        form: &PanelForm,
        prefix: &'static str,
    ) -> Result<OpenAdventureOrigin, TabletopError> {
        let id_path = match prefix {
            "lineage" => "lineage.id",
            "culture" => "culture.id",
            _ => "background.id",
        };
        let label_path = match prefix {
            "lineage" => "lineage.label",
            "culture" => "culture.label",
            _ => "background.label",
        };
        let note_path = match prefix {
            "lineage" => "lineage.note",
            "culture" => "culture.note",
            _ => "background.note",
        };
        Ok(OpenAdventureOrigin {
            id: form.text(id_path)?.to_owned(),
            label: form.text(label_path)?.to_owned(),
            note: form.text(note_path)?.to_owned(),
        })
    }

    fn trait_modifiers(
        form: &PanelForm,
        prefix: &'static str,
    ) -> Result<Vec<OpenAdventureTraitModifier>, TabletopError> {
        let (id_path, label_path, target_path, modifier_path) = match prefix {
            "benefits[0]" => (
                "benefits[0].id",
                "benefits[0].label",
                "benefits[0].target",
                "benefits[0].modifier",
            ),
            _ => (
                "drawbacks[0].id",
                "drawbacks[0].label",
                "drawbacks[0].target",
                "drawbacks[0].modifier",
            ),
        };
        if optional_group(form, id_path, &[label_path, target_path, modifier_path])? {
            Ok(vec![OpenAdventureTraitModifier {
                id: form.text(id_path)?.to_owned(),
                label: form.text(label_path)?.to_owned(),
                target: modifier_target(form.choice(target_path)?)?,
                modifier: i32::try_from(form.integer(modifier_path)?)
                    .map_err(|_| invalid(modifier_path, "expected a signed modifier"))?,
            }])
        } else {
            Ok(Vec::new())
        }
    }

    fn resolve_encounter_fields() -> Vec<PanelField> {
        vec![
            required_text("encounter_id", "Encounter identity"),
            required_text("actor_id", "Actor identity"),
            required_choice("action", "Action", ACTION_KINDS),
            required_choice("target_kind", "Target kind", TARGET_KINDS),
            required_text("target_id", "Target identity"),
            required_signed("situation_modifier", "Situation modifier"),
            required_boolean("spend_story_point", "Spend Story Point"),
            optional_text_field("equipment_id", "Equipment, or empty"),
            optional_text_field("spell_id", "Spell, or empty"),
            required_choice("disclosure", "Disclosure", DISCLOSURES),
        ]
    }

    fn build_begin_challenge(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let mut fields = BTreeMap::from([
            ("challenge_id".to_owned(), text(form.text("challenge_id")?)),
            (
                "challenge_points".to_owned(),
                number(form.integer("challenge_points")?),
            ),
            (
                "difficulty_score".to_owned(),
                number(form.integer("difficulty_score")?),
            ),
            ("disclosure".to_owned(), symbol(form.choice("disclosure")?)),
            ("effect".to_owned(), number(form.integer("effect")?)),
            ("kind".to_owned(), symbol(form.choice("kind")?)),
            ("label".to_owned(), text(form.text("label")?)),
        ]);
        if let Some(defense) = form.optional_integer("npc_defense")? {
            fields.insert("npc_defense".to_owned(), number(defense));
        }
        if let Some(effect) = form.optional_integer("npc_effect")? {
            fields.insert("npc_effect".to_owned(), number(effect));
        }
        if let Some(target) = form.optional_choice("npc_target")? {
            fields.insert("npc_target".to_owned(), symbol(target));
        }
        resolution_request(
            "begin_challenge",
            TabletopCapability::Encounters,
            DomainValue::Object(fields),
            definition,
            state,
        )
    }

    fn build_start_encounter(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "start_encounter",
            TabletopCapability::Encounters,
            [
                ("disclosure", symbol(form.choice("disclosure")?)),
                ("document_json", text(form.text("document_json")?)),
                ("document_sha256", text(form.text("document_sha256")?)),
                ("encounter_id", text(form.text("encounter_id")?)),
            ],
            definition,
            state,
        )
    }

    fn build_resolve_encounter_action(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let mut fields = BTreeMap::from([
            ("action".to_owned(), symbol(form.choice("action")?)),
            ("actor_id".to_owned(), text(form.text("actor_id")?)),
            ("disclosure".to_owned(), symbol(form.choice("disclosure")?)),
            ("encounter_id".to_owned(), text(form.text("encounter_id")?)),
            (
                "situation_modifier".to_owned(),
                number(form.integer("situation_modifier")?),
            ),
            (
                "spend_story_point".to_owned(),
                DomainValue::Bool(form.boolean("spend_story_point")?),
            ),
            ("target_id".to_owned(), text(form.text("target_id")?)),
            (
                "target_kind".to_owned(),
                symbol(form.choice("target_kind")?),
            ),
        ]);
        if let Some(equipment) = form.optional_text("equipment_id")?
            && !equipment.is_empty()
        {
            fields.insert("equipment_id".to_owned(), text(equipment));
        }
        if let Some(spell) = form.optional_text("spell_id")?
            && !spell.is_empty()
        {
            fields.insert("spell_id".to_owned(), text(spell));
        }
        resolution_request(
            "resolve_encounter_action",
            TabletopCapability::Encounters,
            DomainValue::Object(fields),
            definition,
            state,
        )
    }

    fn build_end_encounter(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "end_encounter",
            TabletopCapability::Encounters,
            [
                ("disclosure", symbol(form.choice("disclosure")?)),
                ("encounter_id", text(form.text("encounter_id")?)),
                ("ending", symbol(form.choice("ending")?)),
            ],
            definition,
            state,
        )
    }

    fn build_resolve_challenge_action(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        let mut fields = BTreeMap::from([
            ("attribute".to_owned(), symbol(form.choice("attribute")?)),
            ("challenge_id".to_owned(), text(form.text("challenge_id")?)),
            ("disclosure".to_owned(), symbol(form.choice("disclosure")?)),
            (
                "situation_modifier".to_owned(),
                number(form.integer("situation_modifier")?),
            ),
        ]);
        if let Some(action) = form.optional_text("favored_action_id")?
            && !action.is_empty()
        {
            fields.insert("favored_action_id".to_owned(), text(action));
        }
        resolution_request(
            "resolve_challenge_action",
            TabletopCapability::ChecksAndConflicts,
            DomainValue::Object(fields),
            definition,
            state,
        )
    }

    fn build_heal_participant(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "heal_encounter_participant",
            TabletopCapability::ResourcesAndConditions,
            [
                ("amount", number(form.integer("amount")?)),
                ("disclosure", symbol(form.choice("disclosure")?)),
                ("encounter_id", text(form.text("encounter_id")?)),
                ("participant_id", text(form.text("participant_id")?)),
                ("pool", symbol(form.choice("pool")?)),
            ],
            definition,
            state,
        )
    }

    fn build_advance_participant(
        form: &PanelForm,
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        simple(
            "advance_encounter_participant",
            TabletopCapability::Advancement,
            [
                ("attribute", symbol(form.choice("attribute")?)),
                ("disclosure", symbol(form.choice("disclosure")?)),
                ("encounter_id", text(form.text("encounter_id")?)),
                ("experience_cost", number(form.integer("experience_cost")?)),
                ("participant_id", text(form.text("participant_id")?)),
            ],
            definition,
            state,
        )
    }

    fn module_entries() -> Vec<ModulePanelEntry> {
        available_modules()
            .into_iter()
            .map(|module| ModulePanelEntry {
                verified: allowlist_entry(&module.id, &module.version, &module.sha256).is_some(),
                id: module.id,
                version: module.version,
                title: module.title,
                provides: module.provides_capabilities,
                reason: String::new(),
            })
            .collect()
    }

    fn allowlist_entry(
        id: &str,
        version: &str,
        sha256: &str,
    ) -> Option<&'static weave_tabletop::OpenAdventureVerifiedModule> {
        OPEN_ADVENTURE_VERIFIED_MODULES
            .iter()
            .find(|entry| entry.id == id && entry.version == version && entry.sha256 == sha256)
    }

    fn apply_modules(form: &PanelForm) -> Result<ModulePanelOutcome, TabletopError> {
        let experience = match form.choice("experience")? {
            "extended" => OpenAdventureModuleExperience::Extended,
            _ => OpenAdventureModuleExperience::Core,
        };
        let requested = form
            .optional_text("enabled_module")?
            .unwrap_or_default()
            .trim()
            .to_owned();
        let mut enabled_modules = Vec::new();
        if !requested.is_empty() {
            enabled_modules.push(requested.clone());
        }
        enabled_modules.sort();
        let selection = OpenAdventureModuleSelection {
            selection_format_version: OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION,
            experience,
            enabled_modules: enabled_modules.clone(),
        };
        let available = available_modules();
        let artifacts = vec![
            OpenAdventureModuleArtifact {
                module_id: "org.weave.tabletop.open_adventure.tide_clocks".to_owned(),
                bytes: OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES.to_vec(),
            },
            OpenAdventureModuleArtifact {
                module_id: "org.weave.tabletop.open_adventure.tide_replacement".to_owned(),
                bytes: OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES.to_vec(),
            },
        ];
        let core = open_adventure_manifest();
        match weave_tabletop::apply_open_adventure_modules(
            &available, &artifacts, &selection, &core,
        ) {
            Ok(effective) => {
                let confirmed = effective.enabled_modules.clone();
                let inactive_modules = module_options(experience)
                    .into_iter()
                    .filter(|entry| !confirmed.contains(&entry.id))
                    .collect();
                Ok(ModulePanelOutcome {
                    diagnostics: Vec::new(),
                    effective_capabilities: effective.capabilities,
                    enabled_modules: effective.enabled_modules,
                    inactive_modules,
                })
            }
            Err(diagnostics) => {
                let diagnostics = diagnostics
                    .into_iter()
                    .map(|diagnostic| PanelDiagnostic {
                        code: diagnostic.code,
                        path: diagnostic.path,
                        message: diagnostic.message,
                    })
                    .collect::<Vec<_>>();
                let reason = diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_default();
                let known = module_options(experience)
                    .into_iter()
                    .find(|entry| entry.id == requested);
                let inactive = match known {
                    Some(entry) => entry,
                    None if !requested.is_empty() => ModulePanelEntry {
                        id: requested,
                        version: String::new(),
                        title: String::new(),
                        verified: false,
                        provides: Vec::new(),
                        reason,
                    },
                    None => ModulePanelEntry {
                        id: String::new(),
                        version: String::new(),
                        title: String::new(),
                        verified: false,
                        provides: Vec::new(),
                        reason,
                    },
                };
                Ok(ModulePanelOutcome {
                    diagnostics,
                    effective_capabilities: Vec::new(),
                    enabled_modules: Vec::new(),
                    inactive_modules: vec![inactive],
                })
            }
        }
    }

    fn module_options(experience: OpenAdventureModuleExperience) -> Vec<ModulePanelEntry> {
        open_adventure_module_options(&available_modules(), experience)
            .into_iter()
            .map(|module| ModulePanelEntry {
                verified: allowlist_entry(&module.id, &module.version, &module.sha256).is_some(),
                id: module.id,
                version: module.version,
                title: module.title,
                provides: module.provides_capabilities,
                reason: String::new(),
            })
            .collect()
    }

    fn available_modules() -> Vec<OpenAdventureModuleDescriptor> {
        vec![
            OpenAdventureModuleDescriptor {
                module_format_version: OPEN_ADVENTURE_MODULE_FORMAT_VERSION,
                id: "org.weave.tabletop.open_adventure.tide_clocks".to_owned(),
                version: "1.0.0".to_owned(),
                namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
                title: "Tide Clocks".to_owned(),
                scope: "Synthetic tide-clock module used by the editor module panel.".to_owned(),
                experience: OpenAdventureModuleExperience::Core,
                provides_capabilities: vec![TabletopCapability::Clocks],
                replaces_capabilities: Vec::new(),
                requires_modules: Vec::new(),
                incompatible_modules: Vec::new(),
                source_url: OPEN_ADVENTURE_SOURCE_URL.to_owned(),
                license: OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN.to_owned(),
                content_class: OpenAdventureModuleContentClass::OfficialCommunityOwned,
                sha256: OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256.to_owned(),
            },
            OpenAdventureModuleDescriptor {
                module_format_version: OPEN_ADVENTURE_MODULE_FORMAT_VERSION,
                id: "org.weave.tabletop.open_adventure.tide_replacement".to_owned(),
                version: "1.0.0".to_owned(),
                namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
                title: "Tide Replacement".to_owned(),
                scope: "Synthetic replacement module used by the editor module panel.".to_owned(),
                experience: OpenAdventureModuleExperience::Core,
                provides_capabilities: vec![TabletopCapability::ChecksAndConflicts],
                replaces_capabilities: vec![TabletopCapability::ChecksAndConflicts],
                requires_modules: Vec::new(),
                incompatible_modules: Vec::new(),
                source_url: OPEN_ADVENTURE_SOURCE_URL.to_owned(),
                license: OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN.to_owned(),
                content_class: OpenAdventureModuleContentClass::OfficialCommunityOwned,
                sha256: OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256.to_owned(),
            },
        ]
    }

    fn simple<const N: usize>(
        operation: &'static str,
        capability: TabletopCapability,
        fields: [(&str, DomainValue); N],
        definition: &AdapterCharacterDefinition,
        state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        resolution_request(operation, capability, object(fields), definition, state)
    }

    fn operation(
        id: &'static str,
        title: &'static str,
        capability: TabletopCapability,
        fields: Vec<PanelField>,
        build_request: fn(
            &PanelForm,
            &AdapterCharacterDefinition,
            &TabletopState,
        ) -> Result<ResolutionRequest, TabletopError>,
    ) -> PanelOperation {
        PanelOperation {
            id,
            title,
            capability,
            fields,
            build_request,
        }
    }

    fn required_text(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: true,
        }
    }

    fn optional_text_field(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Text,
            required: false,
        }
    }

    fn required_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: true,
        }
    }

    fn optional_unsigned(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Unsigned,
            required: false,
        }
    }

    fn required_signed(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Signed,
            required: true,
        }
    }

    fn optional_signed(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Signed,
            required: false,
        }
    }

    fn required_boolean(path: &'static str, label: &'static str) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Boolean,
            required: true,
        }
    }

    fn required_choice(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: true,
        }
    }

    fn optional_choice_field(
        path: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    ) -> PanelField {
        PanelField {
            path,
            label,
            kind: PanelFieldKind::Choice(options),
            required: false,
        }
    }

    fn unsigned(form: &PanelForm, path: &'static str) -> Result<u32, TabletopError> {
        u32::try_from(form.integer(path)?)
            .map_err(|_| invalid(path, "expected a non-negative integer"))
    }

    fn attribute(value: &str) -> Result<OpenAdventureAttribute, TabletopError> {
        match value {
            "agility" => Ok(OpenAdventureAttribute::Agility),
            "empathy" => Ok(OpenAdventureAttribute::Empathy),
            "endurance" => Ok(OpenAdventureAttribute::Endurance),
            "focus" => Ok(OpenAdventureAttribute::Focus),
            "insight" => Ok(OpenAdventureAttribute::Insight),
            "might" => Ok(OpenAdventureAttribute::Might),
            "presence" => Ok(OpenAdventureAttribute::Presence),
            "reasoning" => Ok(OpenAdventureAttribute::Reasoning),
            "will" => Ok(OpenAdventureAttribute::Will),
            _ => Err(invalid("attributes", "unknown Open Adventure attribute")),
        }
    }

    fn equipment_kind(value: &str) -> Result<OpenAdventureEquipmentKind, TabletopError> {
        match value {
            "armor" => Ok(OpenAdventureEquipmentKind::Armor),
            "tool" => Ok(OpenAdventureEquipmentKind::Tool),
            "weapon" => Ok(OpenAdventureEquipmentKind::Weapon),
            _ => Err(invalid(
                "equipment[0].kind",
                "unknown Open Adventure equipment kind",
            )),
        }
    }

    fn modifier_target(value: &str) -> Result<OpenAdventureModifierTarget, TabletopError> {
        match value {
            "action_score" => Ok(OpenAdventureModifierTarget::ActionScore),
            "body_points" => Ok(OpenAdventureModifierTarget::BodyPoints),
            "defense" => Ok(OpenAdventureModifierTarget::Defense),
            "mind_points" => Ok(OpenAdventureModifierTarget::MindPoints),
            "reaction" => Ok(OpenAdventureModifierTarget::Reaction),
            "spirit_points" => Ok(OpenAdventureModifierTarget::SpiritPoints),
            _ => Err(invalid(
                "benefits[0].target",
                "unknown Open Adventure modifier target",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_tabletop::{
        OpenAdventureResolver, OpenTaleResolver, SparkResolver, TabletopCapabilityDeclaration,
        TabletopResolver, WickedOnesResolver, spark_manifest,
    };

    fn registry(
        manifest: &AdapterManifest,
        resolver: impl TabletopResolver + 'static,
    ) -> Arc<ResolverRegistry> {
        let mut registry = ResolverRegistry::new();
        registry.register(manifest, resolver).unwrap();
        Arc::new(registry)
    }

    fn spark_session() -> TabletopPanelSession {
        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(spark::SparkPanels);
        let manifest = adapter.manifest();
        TabletopPanelSession::new(adapter, registry(&manifest, SparkResolver)).unwrap()
    }

    fn wicked_session() -> TabletopPanelSession {
        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(wicked_ones::WickedOnesPanels);
        let manifest = adapter.manifest();
        TabletopPanelSession::new(adapter, registry(&manifest, WickedOnesResolver)).unwrap()
    }

    fn open_tale_session() -> TabletopPanelSession {
        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(open_tale::OpenTalePanels);
        let manifest = adapter.manifest();
        TabletopPanelSession::new(adapter, registry(&manifest, OpenTaleResolver)).unwrap()
    }

    fn open_adventure_session() -> TabletopPanelSession {
        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(open_adventure::OpenAdventurePanels);
        let manifest = adapter.manifest();
        TabletopPanelSession::new(adapter, registry(&manifest, OpenAdventureResolver)).unwrap()
    }

    fn spark_form() -> PanelForm {
        PanelForm::new()
            .with_text("character_id", "org.weave.character.panel_lumen")
            .with_text("name", "Panel Lumen")
            .with_text("concept", "A lamp warden testing editor panels.")
            .with_integer("starting_influence", 10)
            .with_text("beliefs[0].id", "steady_light")
            .with_text(
                "beliefs[0].statement",
                "A steady light protects the path behind it.",
            )
            .with_text("beliefs[1].id", "honest_warning")
            .with_text(
                "beliefs[1].statement",
                "A warning is only useful when it is honest.",
            )
            .with_text("beliefs[2].id", "shared_route")
            .with_text(
                "beliefs[2].statement",
                "Every safe route belongs to its travelers.",
            )
            .with_text("relationship[0].id", "panel_sable")
            .with_text("relationship[0].name", "Panel Sable")
            .with_text(
                "relationship[0].description",
                "A signal runner at the editor gate.",
            )
            .with_choice("attributes.body", "d4")
            .with_choice("attributes.heart", "d6")
            .with_choice("attributes.mind", "d10")
            .with_choice("attributes.spark", "d8")
            .with_text("talent[0].id", "lamp_reading")
            .with_text("talent[0].name", "Lamp Reading")
            .with_choice("talent[0].attribute", "mind")
            .with_choice("talent[0].tier", "deep")
    }

    fn wicked_form() -> PanelForm {
        PanelForm::new()
            .with_text("character_id", "org.weave.character.panel_marrow")
            .with_text("name", "Panel Marrow")
            .with_text("race", "Emberkin")
            .with_text("look", "Ash-grey horns and a patched coat")
            .with_choice("calling", "whisperer")
            .with_text("dark_impulse", "Collect every hidden name")
            .with_text("revelry", "Counting stolen coins by candlelight")
            .with_integer("starting_resources", 3)
            .with_integer("seed", 7)
            .with_text("abilities[0]", "silent_step")
            .with_text("abilities[1]", "silver_tongue")
            .with_text("gear[0].id", "chalk_knife")
            .with_text("gear[0].label", "Chalk Knife")
            .with_integer("gear[0].quantity", 1)
            .with_text("vile_friend.name", "Grist")
            .with_text(
                "vile_friend.detail",
                "A patient tunnel rat that remembers grudges",
            )
            .with_integer("actions.analyze", 1)
            .with_integer("actions.invent", 0)
            .with_integer("actions.perceive", 1)
            .with_integer("actions.brawl", 1)
            .with_integer("actions.demolish", 0)
            .with_integer("actions.sneak", 2)
            .with_integer("actions.command", 0)
            .with_integer("actions.persuade", 1)
            .with_integer("actions.endure", 0)
    }

    fn open_tale_form() -> PanelForm {
        PanelForm::new()
            .with_text("character_id", "org.weave.character.panel_alma")
            .with_text("name", "Panel Alma")
            .with_integer("seed", 11)
            .with_integer("starting_wealth", 6)
            .with_integer("heroic_badges", 1)
            .with_text("skills[0].id", "athletics")
            .with_integer("skills[0].rank", 2)
            .with_text("skills[1].id", "composure")
            .with_integer("skills[1].rank", 2)
            .with_text("items[0]", "short_blade")
            .with_text("items[1]", "travel_cloak")
            .with_integer("attributes.dexterity", 3)
            .with_integer("attributes.intelligence", 1)
            .with_integer("attributes.magicy", 1)
            .with_integer("attributes.motion", 3)
            .with_integer("attributes.perception", 2)
            .with_integer("attributes.persuasion", 1)
            .with_integer("attributes.strength", 3)
            .with_integer("attributes.vitality", 3)
            .with_integer("attributes.volition", 1)
    }

    fn open_adventure_form() -> PanelForm {
        PanelForm::new()
            .with_text("character_id", "org.weave.character.panel_ivy")
            .with_text("name", "Panel Ivy")
            .with_integer("seed", 13)
            .with_text("lineage.id", "glassborn")
            .with_text("lineage.label", "Glassborn")
            .with_text("lineage.note", "Raised among tide-cut observatories.")
            .with_text("culture.id", "lantern_ward")
            .with_text("culture.label", "Lantern Ward")
            .with_text("culture.note", "Learned to read weather by lamplight.")
            .with_text("background.id", "survey_runner")
            .with_text("background.label", "Survey Runner")
            .with_text(
                "background.note",
                "Mapped shifting shorelines for a living.",
            )
            .with_text("modus_operandi.id", "careful_craft")
            .with_text("modus_operandi.label", "Careful Craft")
            .with_integer("modus_operandi.score", 3)
            .with_text("specializations[0].id", "shoreline_lore")
            .with_text("specializations[0].label", "Shoreline Lore")
            .with_choice("specializations[0].attribute", "reasoning")
            .with_integer("specializations[0].bonus", 2)
            .with_text("favored_actions[0].id", "chart_the_channel")
            .with_text("favored_actions[0].label", "Chart the Channel")
            .with_choice("favored_actions[0].attribute", "reasoning")
            .with_text("equipment[0].id", "sounding_line")
            .with_text("equipment[0].label", "Sounding Line")
            .with_choice("equipment[0].kind", "tool")
            .with_text("benefits[0].id", "long_practice")
            .with_text("benefits[0].label", "Long Practice")
            .with_choice("benefits[0].target", "action_score")
            .with_integer("benefits[0].modifier", 3)
            .with_text("drawbacks[0].id", "loud_boots")
            .with_text("drawbacks[0].label", "Loud Boots")
            .with_choice("drawbacks[0].target", "reaction")
            .with_integer("drawbacks[0].modifier", -2)
            .with_integer("attributes.agility", 3)
            .with_integer("attributes.empathy", 2)
            .with_integer("attributes.endurance", 3)
            .with_integer("attributes.focus", 3)
            .with_integer("attributes.insight", 4)
            .with_integer("attributes.might", 3)
            .with_integer("attributes.presence", 2)
            .with_integer("attributes.reasoning", 3)
            .with_integer("attributes.will", 4)
    }

    #[test]
    fn spark_creation_panel_builds_typed_preview_and_accepts() {
        let mut session = spark_session();
        let panel = session.creation_panel().unwrap();
        assert_eq!(
            panel
                .steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            [
                "identity",
                "beliefs",
                "relationships",
                "attributes",
                "talents"
            ]
        );
        session.update_creation_form(spark_form()).unwrap();
        {
            let preview = session.preview_creation().unwrap();
            assert!(!preview.explanations.is_empty());
            assert!(preview.canonical_json.contains("creation_format_version"));
            assert!(preview.canonical_ron.contains("creation_format_version"));
        }
        session.accept_creation().unwrap();
        assert!(session.is_creation_accepted());
        assert_eq!(
            session.definition().unwrap().adapter.id,
            weave_tabletop::SPARK_ADAPTER_ID
        );
    }

    #[test]
    fn wicked_ones_creation_panel_builds_typed_preview_and_accepts() {
        let mut session = wicked_session();
        let panel = session.creation_panel().unwrap();
        assert_eq!(
            panel
                .steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            [
                "core_identity",
                "actions",
                "abilities_and_friend",
                "gear_and_resources"
            ]
        );
        session.update_creation_form(wicked_form()).unwrap();
        {
            let preview = session.preview_creation().unwrap();
            assert!(!preview.explanations.is_empty());
        }
        session.accept_creation().unwrap();
        assert_eq!(
            session.definition().unwrap().adapter.id,
            weave_tabletop::WICKED_ONES_ADAPTER_ID
        );
    }

    #[test]
    fn open_tale_creation_panel_builds_typed_preview_and_accepts() {
        let mut session = open_tale_session();
        let panel = session.creation_panel().unwrap();
        assert_eq!(
            panel
                .steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            ["attributes", "identity", "skills_and_wealth"]
        );
        session.update_creation_form(open_tale_form()).unwrap();
        {
            let preview = session.preview_creation().unwrap();
            assert!(!preview.explanations.is_empty());
            assert!(
                preview
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "budget.attributes")
            );
        }
        session.accept_creation().unwrap();
        assert_eq!(
            session.definition().unwrap().adapter.id,
            weave_tabletop::OPEN_TALE_ADAPTER_ID
        );
    }

    #[test]
    fn open_adventure_creation_panel_builds_typed_preview_and_accepts() {
        let mut session = open_adventure_session();
        let panel = session.creation_panel().unwrap();
        assert_eq!(
            panel
                .steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            ["attributes", "details", "identity"]
        );
        session.update_creation_form(open_adventure_form()).unwrap();
        {
            let preview = session.preview_creation().unwrap();
            assert!(!preview.explanations.is_empty());
            assert!(
                preview
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "derived.values")
            );
        }
        session.accept_creation().unwrap();
        assert_eq!(
            session.definition().unwrap().adapter.id,
            weave_tabletop::OPEN_ADVENTURE_ADAPTER_ID
        );
    }

    #[test]
    fn validation_failures_name_the_exact_field_path_before_submission() {
        let mut session = spark_session();
        let wrong_type = spark_form().with_text("attributes.body", "d20");
        let error = session.update_creation_form(wrong_type).unwrap_err();
        assert_eq!(
            error,
            TabletopError::InvalidField {
                path: "attributes.body".to_owned(),
                reason: "expected a declared choice",
            }
        );

        let mut session = open_tale_session();
        let overspent = open_tale_form()
            .with_integer("attributes.dexterity", 6)
            .with_integer("attributes.strength", 0);
        session.update_creation_form(overspent).unwrap();
        let error = session.preview_creation().unwrap_err();
        assert_eq!(
            error,
            TabletopError::InvalidField {
                path: "attributes.dexterity".to_owned(),
                reason: "attribute exceeds the starting maximum",
            }
        );
    }

    #[test]
    fn absent_capability_returns_tt106_and_hides_undeclared_panels() {
        let session = spark_session();
        let error = session.panel(TabletopCapability::Encounters).unwrap_err();
        assert_eq!(
            error,
            TabletopError::UndeclaredCapability {
                capability: TabletopCapability::Encounters,
            }
        );
        assert!(error.to_string().starts_with("TT106"));
        assert!(
            !session
                .panels()
                .iter()
                .any(|panel| panel.capability == TabletopCapability::Encounters)
        );
        assert!(session.module_panel().is_err());
    }

    #[test]
    fn spark_conflict_operation_records_typed_events_and_replays() {
        let mut session = spark_session();
        session.update_creation_form(spark_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        let form = PanelForm::new()
            .with_text("conflict_id", "panel_gate_conflict")
            .with_text("stakes", "Who holds the editor gate")
            .with_integer("max_escalation", 3)
            .with_choice("victory_cost_kind", "influence")
            .with_integer("victory_cost_amount", 2)
            .with_text("victory_cost_attribute", "");
        let outcome = session.execute_operation("open_conflict", &form).unwrap();
        assert!(outcome.events.iter().any(|event| {
            event.kind == "conflict_opened"
                && event.capability == TabletopCapability::ChecksAndConflicts
        }));
        assert!(
            session
                .events()
                .iter()
                .any(|event| event.kind == "conflict_opened")
        );
        session.replay_record(0).unwrap();
    }

    #[test]
    fn wicked_ones_condition_operation_records_typed_events() {
        let mut session = wicked_session();
        session.update_creation_form(wicked_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        let form = PanelForm::new()
            .with_integer("amount", 4)
            .with_text("source", "A collapsing scaffold in the panel test.");
        let outcome = session.execute_operation("apply_shock", &form).unwrap();
        assert!(outcome.events.iter().any(|event| {
            event.kind == "shock_applied"
                && event.capability == TabletopCapability::ResourcesAndConditions
        }));
        assert!(
            outcome
                .events
                .iter()
                .any(|event| event.kind == "condition_changed")
        );
    }

    #[test]
    fn open_tale_challenge_operation_records_typed_events() {
        let mut session = open_tale_session();
        session.update_creation_form(open_tale_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        let form = PanelForm::new()
            .with_choice("attribute", "strength")
            .with_text("challenge_id", "panel_ford")
            .with_integer("difficulty", 12)
            .with_integer("distance", 2)
            .with_boolean("heroic_badge", false)
            .with_text("label", "Cross the ford")
            .with_choice("mode", "normal")
            .with_integer("modifier", 0)
            .with_text("skill_id", "athletics");
        let outcome = session
            .execute_operation("resolve_challenge", &form)
            .unwrap();
        assert!(outcome.events.iter().any(|event| {
            event.kind == "challenge_updated"
                && event.capability == TabletopCapability::ChecksAndConflicts
        }));
        assert!(
            outcome
                .events
                .iter()
                .any(|event| event.kind == "roll_recorded")
        );
        assert!(!outcome.receipt.events.is_empty());
    }

    #[test]
    fn open_adventure_encounter_and_module_panels_report_typed_events() {
        let mut session = open_adventure_session();
        session.update_creation_form(open_adventure_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        let form = PanelForm::new()
            .with_text("challenge_id", "panel_tide_challenge")
            .with_text("label", "Hold the tide gate")
            .with_choice("kind", "environmental")
            .with_integer("difficulty_score", 12)
            .with_integer("challenge_points", 4)
            .with_integer("effect", 1)
            .with_choice("disclosure", "public");
        let outcome = session.execute_operation("begin_challenge", &form).unwrap();
        assert!(outcome.events.iter().any(|event| {
            event.kind == "challenge_begun" && event.capability == TabletopCapability::Encounters
        }));

        let module = session.module_panel().unwrap();
        assert_eq!(module.modules.len(), 2);
        let verified = session
            .apply_modules(
                &PanelForm::new()
                    .with_text(
                        "enabled_module",
                        "org.weave.tabletop.open_adventure.tide_clocks",
                    )
                    .with_choice("experience", "core"),
            )
            .unwrap();
        assert!(verified.diagnostics.is_empty());
        assert!(
            verified
                .effective_capabilities
                .contains(&TabletopCapability::Clocks)
        );

        let rejected = session
            .apply_modules(
                &PanelForm::new()
                    .with_text(
                        "enabled_module",
                        "org.weave.tabletop.open_adventure.unverified",
                    )
                    .with_choice("experience", "core"),
            )
            .unwrap();
        assert!(
            rejected
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "OA202")
        );
        assert!(rejected.effective_capabilities.is_empty());
        assert!(
            rejected.enabled_modules.is_empty(),
            "a rejected module must never be reported enabled"
        );
        assert!(
            rejected
                .inactive_modules
                .iter()
                .any(|module| { !module.verified && module.id.ends_with(".unverified") })
        );
    }

    /// Test-only adapter that declares a panel under a capability its manifest does not declare.
    struct UndeclaredPanelAdapter(spark::SparkPanels);

    impl TabletopPanelAdapter for UndeclaredPanelAdapter {
        fn adapter_id(&self) -> &'static str {
            self.0.adapter_id()
        }

        fn adapter_version(&self) -> &'static str {
            self.0.adapter_version()
        }

        fn manifest(&self) -> AdapterManifest {
            self.0.manifest()
        }

        fn creation_panel(&self) -> CreationPanel {
            self.0.creation_panel()
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            let mut panels = self.0.panels();
            panels.push(PanelDescriptor {
                kind: PanelKind::Play,
                capability: TabletopCapability::Encounters,
                title: "Dormant controls",
                description: "Must never be exposed without the declared capability.",
                operations: vec![PanelOperation {
                    id: "dormant_operation",
                    title: "Dormant operation",
                    capability: TabletopCapability::Encounters,
                    fields: Vec::new(),
                    build_request: dormant_request,
                }],
            });
            panels
        }
    }

    fn dormant_request(
        _form: &PanelForm,
        _definition: &AdapterCharacterDefinition,
        _state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        Err(invalid(
            "operation",
            "dormant controls must not build requests",
        ))
    }

    fn manifest_with_encounters() -> AdapterManifest {
        let mut manifest = spark_manifest();
        manifest.capabilities.push(TabletopCapabilityDeclaration {
            capability: TabletopCapability::Encounters,
            version: 1,
        });
        manifest
            .capabilities
            .sort_by_key(|declaration| declaration.capability);
        manifest
    }

    struct ModuleCapabilityAdapter {
        capability: TabletopCapability,
    }

    impl TabletopPanelAdapter for ModuleCapabilityAdapter {
        fn adapter_id(&self) -> &'static str {
            weave_tabletop::SPARK_ADAPTER_ID
        }

        fn adapter_version(&self) -> &'static str {
            weave_tabletop::SPARK_ADAPTER_VERSION
        }

        fn manifest(&self) -> AdapterManifest {
            manifest_with_encounters()
        }

        fn creation_panel(&self) -> CreationPanel {
            spark::SparkPanels.creation_panel()
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            spark::SparkPanels.panels()
        }

        fn module_panel(&self) -> Option<ModulePanel> {
            Some(ModulePanel {
                capability: self.capability,
                title: "Module gate",
                fields: Vec::new(),
                modules: Vec::new(),
                apply: test_module_outcome,
            })
        }
    }

    fn test_module_outcome(_form: &PanelForm) -> Result<ModulePanelOutcome, TabletopError> {
        Ok(ModulePanelOutcome {
            diagnostics: Vec::new(),
            effective_capabilities: vec![TabletopCapability::Encounters],
            enabled_modules: Vec::new(),
            inactive_modules: Vec::new(),
        })
    }

    #[test]
    fn module_panel_requires_its_own_declared_capability() {
        let declared: Arc<dyn TabletopPanelAdapter> = Arc::new(ModuleCapabilityAdapter {
            capability: TabletopCapability::Encounters,
        });
        let manifest = declared.manifest();
        let session =
            TabletopPanelSession::new(declared, registry(&manifest, SparkResolver)).unwrap();
        assert!(
            !has_capability(session.manifest(), TabletopCapability::DerivedValues),
            "the fixture must not declare DerivedValues"
        );
        let panel = session.module_panel().unwrap();
        assert_eq!(panel.capability, TabletopCapability::Encounters);
        assert!(
            session
                .apply_modules(&PanelForm::new())
                .unwrap()
                .effective_capabilities
                .contains(&TabletopCapability::Encounters)
        );

        let absent: Arc<dyn TabletopPanelAdapter> = Arc::new(ModuleCapabilityAdapter {
            capability: TabletopCapability::DerivedValues,
        });
        let manifest = absent.manifest();
        let session =
            TabletopPanelSession::new(absent, registry(&manifest, SparkResolver)).unwrap();
        let error = session.module_panel().unwrap_err();
        assert_eq!(
            error,
            TabletopError::UndeclaredCapability {
                capability: TabletopCapability::DerivedValues,
            }
        );
        assert!(error.to_string().starts_with("TT106"));
        assert_eq!(
            session.apply_modules(&PanelForm::new()).unwrap_err(),
            TabletopError::UndeclaredCapability {
                capability: TabletopCapability::DerivedValues,
            }
        );
    }

    #[test]
    fn operation_under_undeclared_capability_returns_tt106() {
        let adapter: Arc<dyn TabletopPanelAdapter> =
            Arc::new(UndeclaredPanelAdapter(spark::SparkPanels));
        let manifest = adapter.manifest();
        let mut session =
            TabletopPanelSession::new(adapter, registry(&manifest, SparkResolver)).unwrap();
        let error = session
            .execute_operation("dormant_operation", &PanelForm::new())
            .unwrap_err();
        assert_eq!(
            error,
            TabletopError::UndeclaredCapability {
                capability: TabletopCapability::Encounters,
            }
        );
        assert!(error.to_string().starts_with("TT106"));
        assert!(
            !session
                .panels()
                .iter()
                .any(|panel| panel.capability == TabletopCapability::Encounters)
        );
    }

    #[test]
    fn operation_forms_reject_unknown_fields_with_the_exact_path() {
        let mut session = spark_session();
        session.update_creation_form(spark_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        let form = PanelForm::new()
            .with_text("conflict_id", "panel_gate_conflict")
            .with_text("stakes", "Who holds the editor gate")
            .with_integer("max_escalation", 3)
            .with_choice("victory_cost_kind", "influence")
            .with_integer("victory_cost_amount", 2)
            .with_text("victory_cost_attribute", "")
            .with_text("unexpected", "value");
        let error = session
            .execute_operation("open_conflict", &form)
            .unwrap_err();
        assert_eq!(
            error,
            TabletopError::InvalidField {
                path: "unexpected".to_owned(),
                reason: "unknown panel field",
            }
        );
    }
}
