//! Stable, host-independent contracts for selectable tabletop ruleset adapters.
//!
//! Adapter packages are declarative and inert. They describe capabilities, immutable character
//! definitions, mutable state, creation panels, typed resolver requests/events, visibility,
//! migrations, and exact public provenance. Trusted applications may explicitly compile and
//! register resolver implementations through [`TabletopResolver`]; packages never load code.

mod domain;
mod model;
mod resolver;
mod validation;

use schemars::JsonSchema;
use serde::Deserialize;

pub use domain::{tabletop_domain_manifest, tabletop_domain_pack};
pub use model::*;
pub use resolver::{
    EntropyStream, ResolverRegistry, TabletopResolver, preview_adapter_switch,
    project_receipt_for_audience,
};
pub use validation::{
    canonical_fingerprint, has_capability, resolved_adapter, sha256_bytes,
    validate_adapter_manifest, validate_adapter_selection, validate_character_projection,
    validate_resolution_receipt, validate_resolution_receipt_for_request,
    validate_resolution_request, validate_tabletop_state, verify_adapter_source,
};

/// Redaction-safe tabletop contract failure with a stable diagnostic code.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TabletopError {
    #[error("TT100: unsupported tabletop contract version at `{path}`")]
    UnsupportedVersion { path: &'static str },
    #[error("TT101: invalid tabletop field `{path}`: {reason}")]
    InvalidField { path: String, reason: &'static str },
    #[error("TT102: installed adapters contain more than one primary selection")]
    PrimaryConflict,
    #[error("TT103: selected tabletop adapter is not installed exactly")]
    AdapterNotInstalled,
    #[error("TT104: tabletop content fingerprint changed at `{path}`")]
    ContentHashMismatch { path: &'static str },
    #[error("TT105: adapter source license is outside the public allowlist")]
    LicenseRejected,
    #[error("TT106: adapter does not declare required capability `{capability:?}`")]
    UndeclaredCapability { capability: TabletopCapability },
    #[error("TT107: portable value does not match the declared schema at `{path}`")]
    SchemaMismatch { path: &'static str },
    #[error("TT108: adapter projection attempted canonical Character write-back")]
    CanonicalWriteBack,
    #[error("TT109: mutable tabletop state changed since the request was created")]
    StaleState,
    #[error("TT110: resolver implementation does not match its manifest")]
    ResolverMismatch,
    #[error("TT111: resolver is not explicitly registered for this adapter release")]
    ResolverNotRegistered,
    #[error("TT112: resolver is already registered for this adapter release")]
    DuplicateResolver,
    #[error("TT113: resolver emitted an undeclared event kind")]
    UndeclaredEvent,
    #[error("TT114: resolver entropy use disagrees with its operation declaration")]
    UnexpectedEntropy,
    #[error("TT115: deterministic replay did not reproduce the recorded receipt")]
    ReplayMismatch,
    #[error("TT199: invalid or unserializable tabletop artifact")]
    Artifact,
}

macro_rules! impl_artifact_io {
    ($type:ty) => {
        impl $type {
            /// Parse strict JSON and reject duplicate object keys.
            pub fn from_json(source: &str) -> Result<Self, TabletopError> {
                weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
            }

            /// Parse one RON artifact.
            pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
                ron::from_str(source).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable human-readable JSON with a trailing newline.
            pub fn to_json(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable human-readable RON with a trailing newline.
            pub fn to_ron(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
            }
        }
    };
}

impl_artifact_io!(AdapterManifest);
impl_artifact_io!(AdapterSelection);
impl_artifact_io!(TabletopCharacterProjection);
impl_artifact_io!(TabletopState);
impl_artifact_io!(ResolutionRequest);
impl_artifact_io!(ResolutionReceipt);
impl_artifact_io!(AdapterSwitchPreview);

/// Canonical adapter-manifest JSON Schema.
pub fn adapter_manifest_schema() -> Result<String, TabletopError> {
    schema::<AdapterManifest>(
        "urn:weave:schema:tabletop-adapter-manifest:1",
        "Weave Tabletop Adapter Manifest v1",
        "manifest_format_version",
        ADAPTER_MANIFEST_FORMAT_VERSION,
    )
}

/// Canonical installed/primary selection JSON Schema.
pub fn adapter_selection_schema() -> Result<String, TabletopError> {
    schema::<AdapterSelection>(
        "urn:weave:schema:tabletop-adapter-selection:1",
        "Weave Tabletop Adapter Selection v1",
        "selection_format_version",
        ADAPTER_SELECTION_FORMAT_VERSION,
    )
}

/// Canonical immutable Character projection JSON Schema.
pub fn character_projection_schema() -> Result<String, TabletopError> {
    schema::<TabletopCharacterProjection>(
        "urn:weave:schema:tabletop-character-projection:1",
        "Weave Tabletop Character Projection v1",
        "projection_format_version",
        CHARACTER_PROJECTION_FORMAT_VERSION,
    )
}

/// Canonical mutable adapter state JSON Schema.
pub fn tabletop_state_schema() -> Result<String, TabletopError> {
    schema::<TabletopState>(
        "urn:weave:schema:tabletop-state:1",
        "Weave Tabletop State v1",
        "state_format_version",
        ADAPTER_STATE_FORMAT_VERSION,
    )
}

/// Canonical resolver request JSON Schema.
pub fn resolution_request_schema() -> Result<String, TabletopError> {
    schema::<ResolutionRequest>(
        "urn:weave:schema:tabletop-resolution-request:1",
        "Weave Tabletop Resolution Request v1",
        "resolver_format_version",
        RESOLVER_FORMAT_VERSION,
    )
}

/// Canonical resolver receipt and typed event JSON Schema.
pub fn resolution_receipt_schema() -> Result<String, TabletopError> {
    schema::<ResolutionReceipt>(
        "urn:weave:schema:tabletop-resolution-receipt:1",
        "Weave Tabletop Resolution Receipt v1",
        "resolver_format_version",
        RESOLVER_FORMAT_VERSION,
    )
}

fn schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version_property: &str,
    version: u32,
) -> Result<String, TabletopError> {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| TabletopError::Artifact)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-tabletop-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut(version_property))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    weave_domain::to_pretty_json(&value).map_err(|_| TabletopError::Artifact)
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                sort_json_keys(value);
            }
            values.sort_keys();
        }
        _ => {}
    }
}

/// Semantic equality helper used by conformance fixtures.
pub fn equivalent_json_and_ron<T>(json: &str, ron: &str) -> Result<bool, TabletopError>
where
    T: for<'de> Deserialize<'de> + PartialEq,
{
    let json: T = weave_domain::parse_strict_json(json).map_err(|_| TabletopError::Artifact)?;
    let ron: T = ron::from_str(ron).map_err(|_| TabletopError::Artifact)?;
    Ok(json == ron)
}
