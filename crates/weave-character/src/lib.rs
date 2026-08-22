//! Portable, provenance-aware character profiles and deterministic template synthesis.
//!
//! The contract keeps canonical HEXACO evidence distinct from suggestions, lossy OCEAN
//! projections, presentation, alignment, date context, relationships, and tabletop records.
//! Inputs are data-only, strictly versioned, and suitable for source tooling, editors, command-line
//! workflows, RON/JSON interchange, and game-engine consumers without network or provider access.

mod alignment;
mod authoring;
mod domain;
mod model;
mod operations;
mod presentation;
mod synthesis;
mod temporal;
mod validation;

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use weave_domain::{parse_strict_json, to_pretty_json, to_pretty_ron};

pub use alignment::*;
pub use authoring::*;
pub use domain::{
    CHARACTER_DOMAIN_MODULE_VERSION, CharacterDomainError, character_domain_pack,
    character_module_manifest, character_profile_domain_value,
};
pub use model::*;
pub use operations::*;
pub use presentation::*;
pub use synthesis::{synthesize_character, template_fingerprint, validate_synthesis_result};
pub use temporal::*;
pub use validation::{
    CharacterError, derive_ocean, recompute_derived, validate_overlay, validate_profile,
    validate_template,
};

/// Stable identity reserved for the public Weave Character domain module.
pub const CHARACTER_MODULE_ID: &str = "org.weave.character";

/// Stable namespace for optional identity and presentation data.
pub const IDENTITY_PRESENTATION_EXTENSION_NAMESPACE: &str =
    "org.weave.character.identity_presentation";

const PROFILE_SCHEMA_ID: &str = "urn:weave:schema:character-profile:1";
const TEMPLATE_SCHEMA_ID: &str = "urn:weave:schema:character-template:1";
const OVERLAY_SCHEMA_ID: &str = "urn:weave:schema:character-overlay:1";
const SYNTHESIS_SCHEMA_ID: &str = "urn:weave:schema:character-synthesis:1";
const DIAGNOSTIC_SCHEMA_ID: &str = "urn:weave:schema:character-diagnostic:1";

impl CharacterProfile {
    /// Parse and validate strict Character Profile JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterError> {
        parse_json(source, validate_profile)
    }

    /// Parse and validate Character Profile RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
        parse_ron(source, validate_profile)
    }

    /// Serialize canonical, validated Character Profile JSON.
    pub fn to_json(&self) -> Result<String, CharacterError> {
        serialize_json(self, validate_profile)
    }

    /// Serialize canonical, validated Character Profile RON.
    pub fn to_ron(&self) -> Result<String, CharacterError> {
        serialize_ron(self, validate_profile)
    }
}

impl CharacterTemplate {
    /// Parse and validate strict immutable-template JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterError> {
        parse_json(source, validate_template)
    }

    /// Parse and validate immutable-template RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
        parse_ron(source, validate_template)
    }

    /// Serialize canonical immutable-template JSON.
    pub fn to_json(&self) -> Result<String, CharacterError> {
        serialize_json(self, validate_template)
    }

    /// Serialize canonical immutable-template RON.
    pub fn to_ron(&self) -> Result<String, CharacterError> {
        serialize_ron(self, validate_template)
    }
}

impl CharacterOverlay {
    /// Parse and validate strict sparse-overlay JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterError> {
        parse_json(source, validate_overlay)
    }

    /// Parse and validate sparse-overlay RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
        parse_ron(source, validate_overlay)
    }

    /// Serialize canonical sparse-overlay JSON.
    pub fn to_json(&self) -> Result<String, CharacterError> {
        serialize_json(self, validate_overlay)
    }

    /// Serialize canonical sparse-overlay RON.
    pub fn to_ron(&self) -> Result<String, CharacterError> {
        serialize_ron(self, validate_overlay)
    }
}

impl CharacterSynthesisResult {
    /// Parse and independently reproduce strict synthesis-result JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterError> {
        parse_json(source, validate_synthesis_result)
    }

    /// Parse and independently reproduce synthesis-result RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
        parse_ron(source, validate_synthesis_result)
    }

    /// Serialize canonical, independently reproducible synthesis-result JSON.
    pub fn to_json(&self) -> Result<String, CharacterError> {
        serialize_json(self, validate_synthesis_result)
    }

    /// Serialize canonical, independently reproducible synthesis-result RON.
    pub fn to_ron(&self) -> Result<String, CharacterError> {
        serialize_ron(self, validate_synthesis_result)
    }
}

/// Generate the canonical Character Profile v1 JSON Schema.
pub fn character_profile_schema() -> Result<String, CharacterError> {
    schema::<CharacterProfile>(
        PROFILE_SCHEMA_ID,
        "Weave Character Profile v1",
        Some(("profile_format_version", CHARACTER_PROFILE_FORMAT_VERSION)),
    )
}

/// Generate the canonical immutable Character template v1 JSON Schema.
pub fn character_template_schema() -> Result<String, CharacterError> {
    schema::<CharacterTemplate>(
        TEMPLATE_SCHEMA_ID,
        "Weave Character Template v1",
        Some(("template_format_version", CHARACTER_TEMPLATE_FORMAT_VERSION)),
    )
}

/// Generate the canonical sparse Character overlay v1 JSON Schema.
pub fn character_overlay_schema() -> Result<String, CharacterError> {
    schema::<CharacterOverlay>(
        OVERLAY_SCHEMA_ID,
        "Weave Character Overlay v1",
        Some(("overlay_format_version", CHARACTER_OVERLAY_FORMAT_VERSION)),
    )
}

/// Generate the canonical deterministic Character synthesis v1 JSON Schema.
pub fn character_synthesis_schema() -> Result<String, CharacterError> {
    schema::<CharacterSynthesisResult>(
        SYNTHESIS_SCHEMA_ID,
        "Weave Character Synthesis v1",
        Some((
            "synthesis_format_version",
            CHARACTER_SYNTHESIS_FORMAT_VERSION,
        )),
    )
}

/// Generate the stable Character diagnostic JSON Schema.
pub fn character_diagnostic_schema() -> Result<String, CharacterError> {
    schema::<CharacterDiagnostic>(DIAGNOSTIC_SCHEMA_ID, "Weave Character Diagnostic v1", None)
}

fn parse_json<T: DeserializeOwned>(
    source: &str,
    validate: impl FnOnce(&T) -> Result<(), CharacterError>,
) -> Result<T, CharacterError> {
    let value = parse_strict_json(source).map_err(|_| invalid_encoding())?;
    validate(&value)?;
    Ok(value)
}

fn parse_ron<T: DeserializeOwned>(
    source: &str,
    validate: impl FnOnce(&T) -> Result<(), CharacterError>,
) -> Result<T, CharacterError> {
    let value = ron::from_str(source).map_err(|_| invalid_encoding())?;
    validate(&value)?;
    Ok(value)
}

fn serialize_json<T: Serialize>(
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), CharacterError>,
) -> Result<String, CharacterError> {
    validate(value)?;
    to_pretty_json(value).map_err(|_| invalid_encoding())
}

fn serialize_ron<T: Serialize>(
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), CharacterError>,
) -> Result<String, CharacterError> {
    validate(value)?;
    to_pretty_ron(value).map_err(|_| invalid_encoding())
}

fn schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version: Option<(&str, u32)>,
) -> Result<String, CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| invalid_encoding())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
        if let Some((property, version)) = version
            && let Some(property) = root
                .get_mut("properties")
                .and_then(serde_json::Value::as_object_mut)
                .and_then(|properties| properties.get_mut(property))
                .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| invalid_encoding())
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

fn invalid_encoding() -> CharacterError {
    validation::error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "Character document does not match the strict serialized contract",
    )
}
