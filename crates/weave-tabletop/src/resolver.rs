use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use weave_domain::validate_typed_value;

use crate::{
    AdapterManifest, AdapterSwitchPreview, EntropyState, HostAudience, RESOLVER_FORMAT_VERSION,
    ResolutionReceipt, ResolutionRequest, ResolvedAdapter, ResolverEvent, ResolverOutput,
    TabletopCharacterProjection, TabletopError, TabletopEvent, TabletopState,
    canonical_fingerprint, resolved_adapter, validate_adapter_manifest,
    validate_resolution_receipt, validate_resolution_receipt_for_request,
    validate_resolution_request, validate_tabletop_state,
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
    let restore_existing_archive = to.as_ref().is_some_and(|coordinate| {
        projection
            .inactive
            .contains_key(&coordinate.namespace_key())
    });
    let reviewed_migration_id = match (&from, target) {
        (Some(from), Some(target)) if from.id != target.id || from.version != target.version => {
            target
                .migrations
                .iter()
                .find(|migration| {
                    migration.reviewed
                        && migration.preserves_canonical_character
                        && migration.from_adapter_id == from.id
                        && migration.from_schema_version == from.schema_version
                        && semver::VersionReq::parse(&migration.from_version).is_ok_and(
                            |requirement| {
                                semver::Version::parse(&from.version)
                                    .is_ok_and(|version| requirement.matches(&version))
                            },
                        )
                })
                .map(|migration| migration.id.clone())
        }
        _ => None,
    };
    let mut warnings = Vec::new();
    if !same && from.is_some() {
        warnings.push(
            "Current adapter definition and state must be archived without conversion.".to_owned(),
        );
    }
    if from.is_some() && to.is_some() && reviewed_migration_id.is_none() && !same {
        warnings.push(
            "No explicit reviewed migration exists; cross-system conversion is forbidden."
                .to_owned(),
        );
    }
    Ok(AdapterSwitchPreview {
        canonical_profile_sha256: projection.canonical_profile_sha256.clone(),
        from,
        to,
        archive_current: !same && projection.active.is_some(),
        restore_existing_archive,
        automatic_conversion: false,
        reviewed_migration_id,
        warnings,
    })
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
