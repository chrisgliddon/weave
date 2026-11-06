use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use weave_domain::validate_typed_value;

use crate::{
    ADAPTER_SWITCH_FORMAT_VERSION, AdapterCharacterDefinition, AdapterManifest, AdapterSwitchAudit,
    AdapterSwitchPreview, AdapterSwitchRequest, ArchivedAdapterProjection, EntropyState,
    HostAudience, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest, ResolvedAdapter,
    ResolverEvent, ResolverOutput, SwitchArchiveReport, SwitchDecision, SwitchDecisionKind,
    SwitchDiagnostic, SwitchDiagnosticSeverity, SwitchOutgoingPolicy, TabletopCapability,
    TabletopCharacterProjection, TabletopError, TabletopEvent, TabletopState,
    canonical_fingerprint, resolved_adapter, validate_adapter_manifest,
    validate_character_projection, validate_resolution_receipt,
    validate_resolution_receipt_for_request, validate_resolution_request, validate_tabletop_state,
};

/// Deterministic, serializable SHA-256 counter stream.
///
/// This is replay entropy, not a cryptographic secret generator. Every draw advances the explicit
/// cursor exactly once, so save/restore and cross-host replay consume the same sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntropyStream {
    state: EntropyState,
}

impl EntropyStream {
    /// Start or restore one exact entropy stream.
    pub fn from_state(state: EntropyState) -> Result<Self, TabletopError> {
        if state.algorithm != "sha256_counter_v1" {
            return Err(TabletopError::UnsupportedVersion {
                path: "entropy.algorithm",
            });
        }
        Ok(Self { state })
    }

    /// Draw one deterministic unsigned integer and advance the cursor once.
    pub fn next_u64(&mut self) -> Result<u64, TabletopError> {
        if self.state.cursor == u64::MAX {
            return Err(TabletopError::InvalidField {
                path: "entropy.cursor".to_owned(),
                reason: "entropy stream is exhausted",
            });
        }
        let mut hasher = Sha256::new();
        hasher.update(b"weave.tabletop.entropy.sha256_counter_v1\0");
        hasher.update(self.state.seed.to_be_bytes());
        hasher.update(self.state.cursor.to_be_bytes());
        let digest = hasher.finalize();
        self.state.cursor += 1;
        Ok(u64::from_be_bytes(
            digest[..8]
                .try_into()
                .expect("SHA-256 prefix is eight bytes"),
        ))
    }

    /// Draw uniformly within `0..upper`, using rejection sampling without modulo bias.
    pub fn draw_bounded(&mut self, upper: u64) -> Result<u64, TabletopError> {
        if upper == 0 {
            return Err(TabletopError::InvalidField {
                path: "entropy.upper".to_owned(),
                reason: "expected a positive bound",
            });
        }
        let zone = u64::MAX - (u64::MAX % upper);
        loop {
            let value = self.next_u64()?;
            if value < zone {
                return Ok(value % upper);
            }
        }
    }

    /// Serializable state after the draws consumed so far.
    #[must_use]
    pub fn state(&self) -> EntropyState {
        self.state.clone()
    }
}

/// Trusted, explicitly compiled resolver implementation.
///
/// Adapter packages cannot provide or load implementations of this trait. Hosts register trusted
/// implementations by exact adapter id/version; all input, output, state, entropy, event, and
/// visibility validation remains in this crate.
pub trait TabletopResolver: Send + Sync {
    fn adapter_id(&self) -> &str;
    fn adapter_version(&self) -> &str;
    fn resolve(
        &self,
        operation: &str,
        definition: &weave_domain::DomainValue,
        request: &weave_domain::DomainValue,
        state: &weave_domain::DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError>;
}

impl<T: TabletopResolver + ?Sized> TabletopResolver for Box<T> {
    fn adapter_id(&self) -> &str {
        self.as_ref().adapter_id()
    }

    fn adapter_version(&self) -> &str {
        self.as_ref().adapter_version()
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &weave_domain::DomainValue,
        request: &weave_domain::DomainValue,
        state: &weave_domain::DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        self.as_ref()
            .resolve(operation, definition, request, state, entropy)
    }
}

/// Exact resolver registry that replaces adapter-specific host branches.
#[derive(Default)]
pub struct ResolverRegistry {
    resolvers: BTreeMap<(String, String), Box<dyn TabletopResolver>>,
}

impl ResolverRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one trusted resolver after its coordinate matches a validated manifest.
    pub fn register(
        &mut self,
        manifest: &AdapterManifest,
        resolver: impl TabletopResolver + 'static,
    ) -> Result<(), TabletopError> {
        validate_adapter_manifest(manifest)?;
        if resolver.adapter_id() != manifest.id || resolver.adapter_version() != manifest.version {
            return Err(TabletopError::ResolverMismatch);
        }
        let key = (manifest.id.clone(), manifest.version.clone());
        if self.resolvers.insert(key, Box::new(resolver)).is_some() {
            return Err(TabletopError::DuplicateResolver);
        }
        Ok(())
    }

    /// Validate, execute, and atomically fingerprint one transition.
    pub fn execute(
        &self,
        manifest: &AdapterManifest,
        request: &ResolutionRequest,
        before: &TabletopState,
    ) -> Result<ResolutionReceipt, TabletopError> {
        validate_tabletop_state(before, manifest)?;
        validate_resolution_request(request, before, manifest)?;
        if before.revision == u64::MAX {
            return Err(TabletopError::InvalidField {
                path: "revision".to_owned(),
                reason: "state revision is exhausted",
            });
        }
        let operation = &manifest.operations[&request.operation];
        let Some(resolver) = self
            .resolvers
            .get(&(manifest.id.clone(), manifest.version.clone()))
        else {
            return Err(TabletopError::ResolverNotRegistered);
        };
        let before_hash = canonical_fingerprint(before)?;
        let start_cursor = before.entropy.cursor;
        let mut entropy = EntropyStream::from_state(before.entropy.clone())?;
        let output = resolver.resolve(
            &request.operation,
            &request.definition,
            &request.input,
            &before.value,
            &mut entropy,
        )?;
        validate_typed_value(
            "resolver.state",
            &output.state,
            &manifest.state_type,
            &manifest.types,
        )
        .map_err(|_| TabletopError::SchemaMismatch {
            path: "resolver.state",
        })?;
        if !operation.consumes_entropy && entropy.state().cursor != start_cursor {
            return Err(TabletopError::UnexpectedEntropy);
        }
        if operation.consumes_entropy && entropy.state().cursor == start_cursor {
            return Err(TabletopError::UnexpectedEntropy);
        }
        let events = validate_events(manifest, request, output.events)?;
        let after_state = TabletopState {
            state_format_version: before.state_format_version,
            adapter: before.adapter.clone(),
            owner_id: before.owner_id.clone(),
            definition_sha256: before.definition_sha256.clone(),
            revision: before.revision + 1,
            entropy: entropy.state(),
            value: output.state,
        };
        validate_tabletop_state(&after_state, manifest)?;
        let receipt = ResolutionReceipt {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            adapter: before.adapter.clone(),
            operation: request.operation.clone(),
            capability: request.capability,
            request_sha256: canonical_fingerprint(request)?,
            before_state_sha256: before_hash,
            before_revision: before.revision,
            entropy_before: before.entropy.clone(),
            after_state_sha256: canonical_fingerprint(&after_state)?,
            entropy_consumed: after_state.entropy.cursor.saturating_sub(start_cursor),
            after_state,
            events,
        };
        validate_resolution_receipt_for_request(&receipt, manifest, request)?;
        Ok(receipt)
    }

    /// Re-execute one recorded transition and require byte-equivalent structured output.
    pub fn replay(
        &self,
        manifest: &AdapterManifest,
        request: &ResolutionRequest,
        before: &TabletopState,
        expected: &ResolutionReceipt,
    ) -> Result<(), TabletopError> {
        let replayed = self.execute(manifest, request, before)?;
        if &replayed != expected {
            return Err(TabletopError::ReplayMismatch);
        }
        Ok(())
    }
}

/// Retain every event envelope while revealing payloads only to the declared audience.
pub fn project_receipt_for_audience(
    receipt: &ResolutionReceipt,
    manifest: &AdapterManifest,
    audience: HostAudience,
) -> Result<ResolutionReceipt, TabletopError> {
    validate_resolution_receipt(receipt, manifest)?;
    let allowed = match audience {
        HostAudience::Runtime => &manifest.visibility.runtime,
        HostAudience::Authoring => &manifest.visibility.authoring,
        HostAudience::AuthorityHost => &manifest.visibility.authority_host,
    };
    let mut projected = receipt.clone();
    for event in &mut projected.events {
        if !allowed.contains(&event.visibility) {
            event.payload = None;
        }
    }
    validate_resolution_receipt(&projected, manifest)?;
    Ok(projected)
}

/// Preview a no-conversion switch while preserving canonical data and inactive projections.
pub fn preview_adapter_switch(
    projection: &TabletopCharacterProjection,
    target: Option<&AdapterManifest>,
) -> Result<AdapterSwitchPreview, TabletopError> {
    let from = projection
        .active
        .as_ref()
        .map(|active| active.adapter.clone());
    let to = target.map(resolved_adapter).transpose()?;
    let same = from == to;
    let coordinate_changed = !same;
    let restore_existing_archive = coordinate_changed
        && to.as_ref().is_some_and(|coordinate| {
            projection
                .inactive
                .get(&coordinate.namespace_key())
                .is_some_and(|archived| &archived.adapter == coordinate)
        });
    let reviewed_migration_id = match (&from, target) {
        (Some(from), Some(target)) if from.id == target.id && coordinate_changed => target
            .migrations
            .iter()
            .find(|migration| {
                migration.reviewed
                    && migration.preserves_canonical_character
                    && migration.from_adapter_id == from.id
                    && migration.from_schema_version == from.schema_version
                    && migration.to_schema_version == target.schema_version
                    && semver::VersionReq::parse(&migration.from_version).is_ok_and(|requirement| {
                        semver::Version::parse(&from.version)
                            .is_ok_and(|version| requirement.matches(&version))
                    })
            })
            .map(|migration| migration.id.clone()),
        _ => None,
    };
    let mut warnings = Vec::new();
    if coordinate_changed && from.is_some() {
        warnings.push(
            "Current adapter definition and state must be archived without conversion.".to_owned(),
        );
    }
    if let (Some(from), Some(to)) = (&from, &to) {
        if from.id != to.id {
            warnings.push(
                "No explicit reviewed migration exists; cross-system conversion is forbidden."
                    .to_owned(),
            );
        } else if coordinate_changed && reviewed_migration_id.is_none() {
            warnings
                .push("No reviewed same-adapter migration matches this schema upgrade.".to_owned());
        }
    }
    let archive = if coordinate_changed {
        projection
            .active
            .as_ref()
            .map(|active| SwitchArchiveReport {
                adapter: active.adapter.clone(),
                namespace: active.adapter.namespace_key(),
                definition_sha256: active.definition_sha256.clone(),
            })
    } else {
        None
    };
    let restore = if restore_existing_archive {
        to.as_ref()
            .and_then(|coordinate| {
                projection
                    .inactive
                    .get(&coordinate.namespace_key())
                    .filter(|archived| &archived.adapter == coordinate)
            })
            .map(|archived| SwitchArchiveReport {
                adapter: archived.adapter.clone(),
                namespace: archived.adapter.namespace_key(),
                definition_sha256: archived.definition_sha256.clone(),
            })
    } else {
        None
    };
    let mut diagnostics = Vec::new();
    if let (Some(from), Some(to)) = (&from, &to)
        && from.id != to.id
    {
        diagnostics.push(SwitchDiagnostic {
            code: "TT101".to_owned(),
            severity: SwitchDiagnosticSeverity::Warning,
            message:
                "Cross-system definitions, attributes, resources, and progression are never converted."
                    .to_owned(),
        });
    }
    Ok(AdapterSwitchPreview {
        canonical_profile_sha256: projection.canonical_profile_sha256.clone(),
        from,
        to,
        archive_current: coordinate_changed && projection.active.is_some(),
        restore_existing_archive,
        automatic_conversion: false,
        reviewed_migration_id,
        warnings,
        target_installed: target.is_some(),
        capabilities_gained: Vec::new(),
        capabilities_lost: Vec::new(),
        non_transferable_fields: Vec::new(),
        incompatible_active_resources: Vec::new(),
        pending_conflicts: Vec::new(),
        archive,
        restore,
        discard_inactive: Vec::new(),
        retained_inactive: projection.inactive.keys().cloned().collect(),
        active_state_sha256: None,
        state_archive_required: false,
        required_decisions: Vec::new(),
        diagnostics,
    })
}

/// Exact installed-manifest lookup used by explicit switches.
struct IndexedManifests<'a> {
    entries: Vec<(ResolvedAdapter, &'a AdapterManifest)>,
}

impl<'a> IndexedManifests<'a> {
    fn new(manifests: &'a [AdapterManifest]) -> Result<Self, TabletopError> {
        let mut entries = Vec::new();
        let mut namespaces = BTreeSet::new();
        for manifest in manifests {
            validate_adapter_manifest(manifest)?;
            let coordinate = resolved_adapter(manifest)?;
            if entries
                .iter()
                .any(|(existing, _): &(ResolvedAdapter, &AdapterManifest)| existing == &coordinate)
            {
                return Err(TabletopError::InvalidField {
                    path: "installed".to_owned(),
                    reason: "duplicate installed adapter release",
                });
            }
            if !namespaces.insert(coordinate.namespace_key()) {
                return Err(TabletopError::InvalidField {
                    path: "installed".to_owned(),
                    reason: "duplicate installed adapter namespace",
                });
            }
            entries.push((coordinate, manifest));
        }
        Ok(Self { entries })
    }

    fn exact(&self, coordinate: &ResolvedAdapter) -> Result<&'a AdapterManifest, TabletopError> {
        self.exact_optional(coordinate)
            .ok_or(TabletopError::AdapterNotInstalled)
    }

    fn exact_optional(&self, coordinate: &ResolvedAdapter) -> Option<&'a AdapterManifest> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == coordinate)
            .map(|(_, manifest)| *manifest)
    }
}

impl TabletopCharacterProjection {
    /// Capabilities declared by the active adapter only. Inactive archives never contribute.
    pub fn active_capabilities(
        &self,
        installed: &[AdapterManifest],
    ) -> Result<Vec<TabletopCapability>, TabletopError> {
        let Some(active) = &self.active else {
            return Ok(Vec::new());
        };
        let indexed = IndexedManifests::new(installed)?;
        let manifest = indexed.exact(&active.adapter)?;
        Ok(capability_set(manifest).into_iter().collect())
    }
}

impl TabletopCharacterProjection {
    /// Preview an explicit switch against installed releases, a request, and optional play state.
    ///
    /// The preview never mutates the projection or the state. It reports capability deltas,
    /// incompatible resources, archive and restore plans, dry-run diagnostics, and every author
    /// decision that [`apply_switch`](Self::apply_switch) will require.
    pub fn preview_switch(
        &self,
        installed: &[AdapterManifest],
        request: &AdapterSwitchRequest,
        active_state: Option<&TabletopState>,
    ) -> Result<AdapterSwitchPreview, TabletopError> {
        validate_switch_request(request)?;
        if self.canonical_character_write_back {
            return Err(TabletopError::CanonicalWriteBack);
        }
        let indexed = IndexedManifests::new(installed)?;
        validate_archived_namespaces(self)?;
        let target_manifest = match &request.target {
            Some(coordinate) => Some(indexed.exact(coordinate)?),
            None => None,
        };
        let mut preview = preview_adapter_switch(self, target_manifest)?;
        let from = preview.from.clone();
        let to = preview.to.clone();
        let same = from == to;
        let coordinate_changed = !same;
        let cross_system = matches!((&from, &to), (Some(from), Some(to)) if from.id != to.id);
        let upgrading =
            matches!((&from, &to), (Some(from), Some(to)) if from.id == to.id && from != to);
        let from_manifest = from
            .as_ref()
            .and_then(|coordinate| indexed.exact_optional(coordinate));

        // A same-adapter upgrade migrates in place. Archiving the outgoing release can be
        // impossible because only one release per adapter id may be installed. A discard policy
        // never plans an archive either.
        let archiving_outgoing = coordinate_changed
            && self.active.is_some()
            && !upgrading
            && request.outgoing_policy == SwitchOutgoingPolicy::Archive;
        preview.archive_current = archiving_outgoing;
        preview.archive = if archiving_outgoing {
            self.active.as_ref().map(|active| SwitchArchiveReport {
                adapter: active.adapter.clone(),
                namespace: active.adapter.namespace_key(),
                definition_sha256: active.definition_sha256.clone(),
            })
        } else {
            None
        };

        let from_caps = from_manifest.map(capability_set).unwrap_or_default();
        let target_caps = target_manifest.map(capability_set).unwrap_or_default();
        if coordinate_changed && !(upgrading && from_manifest.is_none()) {
            preview.capabilities_gained = target_caps
                .difference(&from_caps)
                .copied()
                .collect::<Vec<_>>();
            preview.capabilities_lost = from_caps
                .difference(&target_caps)
                .copied()
                .collect::<Vec<_>>();
        }
        if upgrading && from_manifest.is_none() {
            preview.diagnostics.push(SwitchDiagnostic {
                code: "TT103".to_owned(),
                severity: SwitchDiagnosticSeverity::Warning,
                message:
                    "The outgoing release is not installed, so the capability delta cannot be computed."
                        .to_owned(),
            });
        }

        let mut non_transferable = BTreeSet::new();
        if cross_system && let Some(active) = &self.active {
            let fields = top_level_fields(&active.definition);
            if fields.is_empty() {
                non_transferable.insert("definition".to_owned());
            } else {
                for field in fields {
                    non_transferable.insert(format!("definition.{field}"));
                }
            }
        }
        if upgrading
            && let (Some(active), Some(target_definition)) =
                (&self.active, &request.target_definition)
        {
            let after = top_level_fields(&target_definition.definition)
                .into_iter()
                .collect::<BTreeSet<_>>();
            for field in top_level_fields(&active.definition) {
                if !after.contains(&field) {
                    non_transferable.insert(format!("definition.{field}"));
                }
            }
        }
        preview.non_transferable_fields = non_transferable.into_iter().collect();

        let mut incompatible = BTreeSet::new();
        if coordinate_changed
            && let Some(state) = active_state
            && (cross_system || upgrading)
        {
            for field in top_level_fields(&state.value) {
                incompatible.insert(format!("state.{field}"));
            }
        }
        preview.incompatible_active_resources = incompatible.into_iter().collect();
        preview.active_state_sha256 = active_state.map(canonical_fingerprint).transpose()?;
        preview.state_archive_required = active_state.is_some()
            && self.active.is_some()
            && coordinate_changed
            && !upgrading
            && request.outgoing_policy == SwitchOutgoingPolicy::Archive;

        let mut pending = Vec::new();
        if let Some(state) = active_state {
            match &from {
                Some(from_coordinate) => {
                    if &state.adapter != from_coordinate {
                        pending.push(format!(
                            "Active state belongs to adapter `{}` release `{}`, not the active projection.",
                            state.adapter.id, state.adapter.version
                        ));
                    } else if let Some(manifest) = from_manifest {
                        if validate_tabletop_state(state, manifest).is_err() {
                            pending.push(
                                "Active play state fails validation for the outgoing adapter."
                                    .to_owned(),
                            );
                        }
                    } else {
                        pending.push(
                            "The outgoing release is not installed; active state cannot be validated."
                                .to_owned(),
                        );
                    }
                }
                None => pending.push(
                    "Active play state exists while no adapter projection is active.".to_owned(),
                ),
            }
        }
        if upgrading && !preview.restore_existing_archive {
            if preview.reviewed_migration_id.is_none() {
                pending.push(
                    "No reviewed migration matches this same-adapter schema upgrade.".to_owned(),
                );
                preview.diagnostics.push(SwitchDiagnostic {
                    code: "TT101".to_owned(),
                    severity: SwitchDiagnosticSeverity::Error,
                    message:
                        "Same-adapter schema upgrades require a reviewed migration declaration."
                            .to_owned(),
                });
            }
            if to.as_ref().is_some_and(|target| {
                from.as_ref()
                    .is_some_and(|from| from.schema_version > target.schema_version)
            }) {
                pending.push("Adapter schema downgrades are not permitted.".to_owned());
                preview.diagnostics.push(SwitchDiagnostic {
                    code: "TT101".to_owned(),
                    severity: SwitchDiagnosticSeverity::Error,
                    message: "Adapter schema downgrades are not permitted.".to_owned(),
                });
            }
            if request.target_definition.is_none() {
                pending
                    .push("A migrated target definition is required for this upgrade.".to_owned());
            }
        }
        if coordinate_changed
            && to.is_some()
            && request.target_definition.is_none()
            && !preview.restore_existing_archive
        {
            pending.push(if cross_system {
                "A target projection is required; cross-system conversion is forbidden.".to_owned()
            } else {
                "An installed target projection is required for this activation.".to_owned()
            });
        }

        preview.discard_inactive = request.discard_inactive.clone();
        let discarded = request
            .discard_inactive
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for key in &request.discard_inactive {
            if !self.inactive.contains_key(key) {
                return Err(TabletopError::InvalidField {
                    path: format!("discard_inactive.{key}"),
                    reason: "inactive projection does not exist",
                });
            }
            if preview.restore_existing_archive
                && to
                    .as_ref()
                    .is_some_and(|coordinate| coordinate.namespace_key() == *key)
            {
                return Err(TabletopError::InvalidField {
                    path: format!("discard_inactive.{key}"),
                    reason: "cannot discard the archive selected for restoration",
                });
            }
        }
        preview.retained_inactive = self
            .inactive
            .keys()
            .filter(|key| !discarded.contains(key.as_str()))
            .cloned()
            .collect();

        if let (Some(target_definition), Some(target)) = (&request.target_definition, &to) {
            match validate_target_definition(self, target_definition, target, installed) {
                Ok(()) => preview.diagnostics.push(SwitchDiagnostic {
                    code: "TT107".to_owned(),
                    severity: SwitchDiagnosticSeverity::Info,
                    message: if upgrading {
                        "Migration dry run: the target definition satisfies the target adapter schema."
                            .to_owned()
                    } else {
                        "Target definition dry run: the supplied projection satisfies the target adapter schema."
                            .to_owned()
                    },
                }),
                Err(error) => {
                    preview.diagnostics.push(SwitchDiagnostic {
                        code: "TT107".to_owned(),
                        severity: SwitchDiagnosticSeverity::Error,
                        message: format!("Target definition dry run failed: {error}"),
                    });
                    pending.push(
                        "The target definition fails validation for the target adapter.".to_owned(),
                    );
                }
            }
            if let Some(archived) = self.inactive.get(&target.namespace_key())
                && !archived_matches_definition(archived, target_definition)
            {
                preview.diagnostics.push(SwitchDiagnostic {
                    code: "TT104".to_owned(),
                    severity: SwitchDiagnosticSeverity::Error,
                    message: "The supplied target definition conflicts with the retained archive."
                        .to_owned(),
                });
                pending.push(
                    "The supplied target definition conflicts with the retained archive."
                        .to_owned(),
                );
            }
        }
        if same
            && let (Some(active), Some(target_definition)) =
                (&self.active, &request.target_definition)
            && !same_definition(active, target_definition)
        {
            preview.diagnostics.push(SwitchDiagnostic {
                code: "TT104".to_owned(),
                severity: SwitchDiagnosticSeverity::Error,
                message: "The supplied definition differs from the active coordinate.".to_owned(),
            });
            pending.push("The supplied definition differs from the active projection.".to_owned());
        }

        preview.pending_conflicts = pending;
        preview.required_decisions =
            required_decisions(&preview, upgrading, request, self.active.is_some());
        Ok(preview)
    }

    /// Apply a fully acknowledged switch atomically.
    ///
    /// Every check runs against a clone. Any failure leaves the projection byte-identical, so a
    /// failed same-adapter migration rolls back without a partial write.
    pub fn apply_switch(
        &mut self,
        installed: &[AdapterManifest],
        request: &AdapterSwitchRequest,
        active_state: Option<&TabletopState>,
    ) -> Result<AdapterSwitchAudit, TabletopError> {
        let preview = self.preview_switch(installed, request, active_state)?;
        if preview
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == SwitchDiagnosticSeverity::Error)
        {
            return Err(TabletopError::InvalidField {
                path: "switch".to_owned(),
                reason: "preview reported a blocking switch diagnostic",
            });
        }
        let accepted = request
            .accepted_decisions
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for decision in &preview.required_decisions {
            if decision.required && !accepted.contains(decision.id.as_str()) {
                return Err(TabletopError::InvalidField {
                    path: format!("accepted_decisions.{}", decision.id),
                    reason: "required author decision is missing",
                });
            }
        }
        for id in &accepted {
            if !preview
                .required_decisions
                .iter()
                .any(|decision| decision.id == *id)
            {
                return Err(TabletopError::InvalidField {
                    path: format!("accepted_decisions.{id}"),
                    reason: "decision is not required by this switch",
                });
            }
        }
        let indexed = IndexedManifests::new(installed)?;
        let from = self.active.as_ref().map(|active| active.adapter.clone());
        let to = request.target.clone();
        let same = from == to;
        let upgrading =
            matches!((&from, &to), (Some(from), Some(to)) if from.id == to.id && from != to);
        if !same
            && !upgrading
            && request.outgoing_policy == SwitchOutgoingPolicy::Archive
            && let Some(from_coordinate) = &from
        {
            indexed.exact(from_coordinate)?;
        }
        if to.is_some()
            && !same
            && !preview.restore_existing_archive
            && request.target_definition.is_none()
        {
            return Err(TabletopError::InvalidField {
                path: "target_definition".to_owned(),
                reason: "a target projection is required",
            });
        }

        let mut next = self.clone();
        let mut archived_projection = None;
        if !same && let Some(active) = self.active.as_ref() {
            let superseded = upgrading;
            if request.outgoing_policy == SwitchOutgoingPolicy::Archive && !superseded {
                let key = active.adapter.namespace_key();
                if next.inactive.contains_key(&key) {
                    return Err(TabletopError::InvalidField {
                        path: "inactive".to_owned(),
                        reason: "outgoing archive key already exists",
                    });
                }
                let report = SwitchArchiveReport {
                    adapter: active.adapter.clone(),
                    namespace: key.clone(),
                    definition_sha256: active.definition_sha256.clone(),
                };
                next.inactive.insert(
                    key,
                    ArchivedAdapterProjection {
                        adapter: active.adapter.clone(),
                        definition: active.definition.clone(),
                        definition_sha256: active.definition_sha256.clone(),
                        completed_creation_steps: active.completed_creation_steps.clone(),
                        reason: match &to {
                            Some(target) => {
                                format!("Archived when switching to adapter `{}`.", target.id)
                            }
                            None => "Archived when tabletop rules were deactivated.".to_owned(),
                        },
                    },
                );
                archived_projection = Some(report);
            }
        }
        next.active = None;
        for key in &request.discard_inactive {
            if next.inactive.remove(key).is_none() {
                return Err(TabletopError::InvalidField {
                    path: format!("discard_inactive.{key}"),
                    reason: "inactive projection does not exist",
                });
            }
        }

        let mut restored_projection = None;
        if let Some(target) = &request.target {
            indexed.exact(target)?;
            let target_key = target.namespace_key();
            if let Some(stored) = next.inactive.get(&target_key)
                && &stored.adapter != target
            {
                return Err(TabletopError::ContentHashMismatch {
                    path: "inactive.adapter",
                });
            }
            let archived = next.inactive.get(&target_key).cloned();
            if same && request.target_definition.is_none() {
                next.active = self.active.clone();
            } else {
                let definition = match (&request.target_definition, archived) {
                    (Some(supplied), Some(stored)) => {
                        if !archived_matches_definition(&stored, supplied) {
                            return Err(TabletopError::InvalidField {
                                path: "target_definition".to_owned(),
                                reason: "target definition conflicts with the retained archive",
                            });
                        }
                        restored_projection = Some(SwitchArchiveReport {
                            adapter: stored.adapter.clone(),
                            namespace: target_key.clone(),
                            definition_sha256: stored.definition_sha256.clone(),
                        });
                        next.inactive.remove(&target_key);
                        supplied.clone()
                    }
                    (Some(supplied), None) => {
                        validate_target_definition(self, supplied, target, installed)?;
                        supplied.clone()
                    }
                    (None, Some(stored)) => {
                        restored_projection = Some(SwitchArchiveReport {
                            adapter: stored.adapter.clone(),
                            namespace: target_key.clone(),
                            definition_sha256: stored.definition_sha256.clone(),
                        });
                        next.inactive.remove(&target_key);
                        AdapterCharacterDefinition {
                            adapter: stored.adapter,
                            definition: stored.definition,
                            definition_sha256: stored.definition_sha256,
                            completed_creation_steps: stored.completed_creation_steps,
                        }
                    }
                    (None, None) => {
                        return Err(TabletopError::InvalidField {
                            path: "target_definition".to_owned(),
                            reason: "a target projection is required",
                        });
                    }
                };
                next.active = Some(definition);
            }
        }

        if next.canonical_profile_sha256 != self.canonical_profile_sha256 {
            return Err(TabletopError::CanonicalWriteBack);
        }
        next.canonical_character_write_back = self.canonical_character_write_back;
        if next.canonical_character_write_back {
            return Err(TabletopError::CanonicalWriteBack);
        }
        validate_character_projection(&next, installed)?;
        let result_projection_sha256 = canonical_fingerprint(&next)?;
        let (preserved_state_sha256, discarded_state_sha256) =
            match (active_state, request.outgoing_policy) {
                (Some(state), SwitchOutgoingPolicy::Discard) if !same => {
                    (None, Some(canonical_fingerprint(state)?))
                }
                (Some(state), _) => (Some(canonical_fingerprint(state)?), None),
                (None, _) => (None, None),
            };
        let retained_inactive = next.inactive.keys().cloned().collect();
        let migrated_from_definition_sha256 = if upgrading {
            self.active
                .as_ref()
                .map(|active| active.definition_sha256.clone())
        } else {
            None
        };
        *self = next;
        Ok(AdapterSwitchAudit {
            switch_format_version: ADAPTER_SWITCH_FORMAT_VERSION,
            character_id: self.character_id.clone(),
            canonical_profile_sha256: self.canonical_profile_sha256.clone(),
            canonical_character_write_back: self.canonical_character_write_back,
            initiator: request.initiator,
            initiator_id: request.initiator_id.clone(),
            from,
            to,
            accepted_decisions: request.accepted_decisions.clone(),
            diagnostics: preview.diagnostics,
            archived_projection,
            restored_projection,
            discarded_current: !same
                && (upgrading || request.outgoing_policy == SwitchOutgoingPolicy::Discard),
            discarded_inactive: request.discard_inactive.clone(),
            retained_inactive,
            preserved_state_sha256,
            discarded_state_sha256,
            migration_id: preview.reviewed_migration_id,
            migrated_from_definition_sha256,
            result_projection_sha256,
        })
    }
}

fn capability_set(manifest: &AdapterManifest) -> BTreeSet<TabletopCapability> {
    manifest
        .capabilities
        .iter()
        .filter(|declaration| declaration.version == 1)
        .map(|declaration| declaration.capability)
        .collect()
}

fn capability_key(capability: TabletopCapability) -> &'static str {
    match capability {
        TabletopCapability::CharacterCreation => "character_creation",
        TabletopCapability::DerivedValues => "derived_values",
        TabletopCapability::ChecksAndConflicts => "checks_and_conflicts",
        TabletopCapability::ResourcesAndConditions => "resources_and_conditions",
        TabletopCapability::EquipmentAndAbilities => "equipment_and_abilities",
        TabletopCapability::Advancement => "advancement",
        TabletopCapability::Encounters => "encounters",
        TabletopCapability::Clocks => "clocks",
        TabletopCapability::Scenes => "scenes",
        TabletopCapability::Factions => "factions",
        TabletopCapability::WorldState => "world_state",
        TabletopCapability::CampaignState => "campaign_state",
    }
}

fn top_level_fields(value: &weave_domain::DomainValue) -> Vec<String> {
    match value {
        weave_domain::DomainValue::Object(fields) => fields.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

fn validate_archived_namespaces(
    projection: &TabletopCharacterProjection,
) -> Result<(), TabletopError> {
    let mut namespaces = BTreeSet::new();
    for (key, archived) in &projection.inactive {
        if key != &archived.adapter.namespace_key() {
            return Err(TabletopError::InvalidField {
                path: format!("inactive.{key}"),
                reason: "archive key must match the exact adapter coordinate",
            });
        }
        if !namespaces.insert(archived.adapter.namespace_key()) {
            return Err(TabletopError::InvalidField {
                path: "inactive".to_owned(),
                reason: "duplicate archived adapter namespace",
            });
        }
    }
    if let Some(active) = &projection.active
        && namespaces.contains(&active.adapter.namespace_key())
    {
        return Err(TabletopError::InvalidField {
            path: "inactive".to_owned(),
            reason: "active adapter namespace collides with an archive",
        });
    }
    Ok(())
}

fn same_definition(left: &AdapterCharacterDefinition, right: &AdapterCharacterDefinition) -> bool {
    left.adapter == right.adapter
        && left.definition == right.definition
        && left.definition_sha256 == right.definition_sha256
        && left.completed_creation_steps == right.completed_creation_steps
}

fn archived_matches_definition(
    archived: &ArchivedAdapterProjection,
    supplied: &AdapterCharacterDefinition,
) -> bool {
    archived.adapter == supplied.adapter
        && archived.definition == supplied.definition
        && archived.definition_sha256 == supplied.definition_sha256
        && archived.completed_creation_steps == supplied.completed_creation_steps
}

fn validate_target_definition(
    projection: &TabletopCharacterProjection,
    definition: &AdapterCharacterDefinition,
    target: &ResolvedAdapter,
    installed: &[AdapterManifest],
) -> Result<(), TabletopError> {
    if &definition.adapter != target {
        return Err(TabletopError::ContentHashMismatch {
            path: "target_definition.adapter",
        });
    }
    let probe = TabletopCharacterProjection {
        projection_format_version: projection.projection_format_version,
        character_id: projection.character_id.clone(),
        canonical_profile_sha256: projection.canonical_profile_sha256.clone(),
        canonical_character_write_back: false,
        active: Some(definition.clone()),
        inactive: BTreeMap::new(),
        suggestions: Vec::new(),
    };
    validate_character_projection(&probe, installed)
}

fn required_decisions(
    preview: &AdapterSwitchPreview,
    upgrading: bool,
    request: &AdapterSwitchRequest,
    has_active: bool,
) -> Vec<SwitchDecision> {
    let mut decisions = Vec::new();
    let coordinate_changed = preview.from != preview.to;
    if coordinate_changed
        && has_active
        && (upgrading || request.outgoing_policy == SwitchOutgoingPolicy::Discard)
    {
        decisions.push(SwitchDecision {
            id: "discard_current_projection".to_owned(),
            kind: SwitchDecisionKind::DiscardCurrentProjection,
            required: true,
            description: "Replace the outgoing projection without retaining an archive of it."
                .to_owned(),
        });
    }
    for capability in &preview.capabilities_lost {
        let key = capability_key(*capability);
        decisions.push(SwitchDecision {
            id: format!("accept_capability_loss_{key}"),
            kind: SwitchDecisionKind::AcceptCapabilityLoss,
            required: true,
            description: format!("Accept that the target adapter does not declare `{key}`."),
        });
    }
    if !preview.non_transferable_fields.is_empty() {
        decisions.push(SwitchDecision {
            id: "accept_non_transferable_fields".to_owned(),
            kind: SwitchDecisionKind::AcceptNonTransferableFields,
            required: true,
            description: "Accept that the listed definition fields do not transfer.".to_owned(),
        });
    }
    if !preview.incompatible_active_resources.is_empty() {
        decisions.push(SwitchDecision {
            id: "resolve_incompatible_resources".to_owned(),
            kind: SwitchDecisionKind::ResolveIncompatibleResources,
            required: true,
            description: "Choose how the listed active resources are preserved or discarded."
                .to_owned(),
        });
    }
    if !preview.pending_conflicts.is_empty() {
        decisions.push(SwitchDecision {
            id: "resolve_pending_conflicts".to_owned(),
            kind: SwitchDecisionKind::ResolvePendingConflicts,
            required: true,
            description: "Acknowledge every listed conflict before the switch applies.".to_owned(),
        });
    }
    if coordinate_changed
        && preview.to.is_some()
        && !preview.restore_existing_archive
        && request.target_definition.is_none()
    {
        decisions.push(SwitchDecision {
            id: "provide_target_projection".to_owned(),
            kind: SwitchDecisionKind::ProvideTargetProjection,
            required: true,
            description: "Supply an independently created target projection for this switch."
                .to_owned(),
        });
    }
    if upgrading
        && !preview.restore_existing_archive
        && let Some(migration_id) = &preview.reviewed_migration_id
    {
        decisions.push(SwitchDecision {
            id: "apply_reviewed_migration".to_owned(),
            kind: SwitchDecisionKind::ApplyReviewedMigration,
            required: true,
            description: format!("Apply reviewed migration `{migration_id}` to the definition."),
        });
    }
    decisions.sort_by(|left, right| left.id.cmp(&right.id));
    decisions.dedup_by(|left, right| left.id == right.id);
    decisions
}

fn validate_switch_request(request: &AdapterSwitchRequest) -> Result<(), TabletopError> {
    if request.switch_format_version != ADAPTER_SWITCH_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "switch_format_version",
        });
    }
    validate_actor_identifier("initiator_id", &request.initiator_id)?;
    if !strictly_sorted(&request.discard_inactive) {
        return Err(TabletopError::InvalidField {
            path: "discard_inactive".to_owned(),
            reason: "expected unique ordered namespaces",
        });
    }
    for key in &request.discard_inactive {
        validate_contract_identifier("discard_inactive", key)?;
    }
    if !strictly_sorted(&request.accepted_decisions) {
        return Err(TabletopError::InvalidField {
            path: "accepted_decisions".to_owned(),
            reason: "expected unique ordered decision ids",
        });
    }
    for id in &request.accepted_decisions {
        validate_contract_identifier("accepted_decisions", id)?;
    }
    if request.target.is_none() && request.target_definition.is_some() {
        return Err(TabletopError::InvalidField {
            path: "target_definition".to_owned(),
            reason: "deactivation cannot supply a target definition",
        });
    }
    Ok(())
}

fn strictly_sorted(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn validate_contract_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    let valid = is_local_identifier(value);
    if valid {
        Ok(())
    } else {
        Err(TabletopError::InvalidField {
            path: path.to_owned(),
            reason: "expected a lowercase identifier",
        })
    }
}

fn validate_actor_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    let valid_global = value.len() <= 255
        && value.split('.').count() >= 3
        && value.split('.').all(is_local_identifier);
    if is_local_identifier(value) || valid_global {
        Ok(())
    } else {
        Err(TabletopError::InvalidField {
            path: path.to_owned(),
            reason: "expected a lowercase actor identifier",
        })
    }
}

fn is_local_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && value.len() <= 128
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_events(
    manifest: &AdapterManifest,
    request: &ResolutionRequest,
    events: Vec<ResolverEvent>,
) -> Result<Vec<TabletopEvent>, TabletopError> {
    if events.len() > 65_536 {
        return Err(TabletopError::InvalidField {
            path: "events".to_owned(),
            reason: "too many resolver events",
        });
    }
    let operation = &manifest.operations[&request.operation];
    events
        .into_iter()
        .enumerate()
        .map(|(sequence, event)| {
            if !operation.event_kinds.contains(&event.kind) {
                return Err(TabletopError::UndeclaredEvent);
            }
            let Some(value_type) = manifest.event_types.get(&event.kind) else {
                return Err(TabletopError::UndeclaredEvent);
            };
            validate_typed_value(
                "events.payload",
                &event.payload,
                value_type,
                &manifest.types,
            )
            .map_err(|_| TabletopError::SchemaMismatch {
                path: "events.payload",
            })?;
            Ok(TabletopEvent {
                sequence: sequence as u64,
                kind: event.kind,
                capability: request.capability,
                visibility: event.visibility,
                payload_sha256: canonical_fingerprint(&event.payload)?,
                payload: Some(event.payload),
            })
        })
        .collect()
}

impl ResolvedAdapter {
    /// Stable archive key for one adapter identity and release.
    #[must_use]
    pub fn namespace_key(&self) -> String {
        format!(
            "{}_v{}",
            self.id.replace('.', "_"),
            self.version.replace('.', "_")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventVisibility;

    #[test]
    fn entropy_save_restore_is_exact() {
        let initial = EntropyState {
            algorithm: "sha256_counter_v1".to_owned(),
            seed: 42,
            cursor: 0,
        };
        let mut first = EntropyStream::from_state(initial).unwrap();
        let first_value = first.draw_bounded(10).unwrap();
        let saved = first.state();
        let second_value = first.draw_bounded(10).unwrap();
        let mut restored = EntropyStream::from_state(saved).unwrap();
        assert_eq!(restored.draw_bounded(10).unwrap(), second_value);
        assert!(first_value < 10);
    }

    #[test]
    fn event_visibility_order_matches_public_to_host_only() {
        assert!(EventVisibility::Public < EventVisibility::Authoring);
        assert!(EventVisibility::Authoring < EventVisibility::HostOnly);
    }
}
