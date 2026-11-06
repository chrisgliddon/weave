//! Portable Character packs with separate authoring and compact runtime projections.
//!
//! A [`CharacterAuthoringPack`] retains everything an author or reviewer needs: effective
//! attributed profiles, immutable templates and sparse operation manifests, approved extension
//! values, dialogue templates, assistance lineage, health diagnostics, and licence notices.
//!
//! A [`CharacterRuntimePack`] is the lossy export for a game or other runtime. It keeps only
//! approved identity, personality, presentation, expression, relationship, alignment, role, and
//! date-context values plus the references required to render them. It never contains
//! suggestions, rejected or pending records, editor locks, review hashes, explanations,
//! provenance, or other authoring metadata.
//!
//! Both formats serialize deterministically to strict JSON and RON. Identifiers, versions, and
//! references are validated; unsupported versions fail closed with an actionable diagnostic.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    DomainValue, Provenance, ProvenanceSource, ProvenanceTransformation, TypeExpression,
    parse_strict_json, to_pretty_json, to_pretty_ron, validate_provenance, validate_typed_value,
};

use crate::{
    ALIGNMENT_EXTENSION_NAMESPACE, AlignmentPublicDecision, AlignmentView, AssistanceReceipt,
    Attributed, BirthDate, CHARACTER_COLLECTION_FORMAT_VERSION,
    CHARACTER_FINAL_REVIEW_FORMAT_VERSION, CharacterCollection, CharacterCorpusError,
    CharacterDiagnostic, CharacterDiagnosticCode, CharacterError, CharacterExtension,
    CharacterFinalReview, CharacterHealthDiagnostic, CharacterHealthDiagnosticScope,
    CharacterOverlay, CharacterProfile, CharacterTemplate, CharacterTemplateRef,
    DATE_CONTEXT_EXTENSION_NAMESPACE, DateContext, DateContextCueKind, DateContextUncertainty,
    DiagnosticSeverity, EXPRESSION_EXTENSION_NAMESPACE, ExpressionApplicability,
    ExpressionConstraintEffect, ExpressionContextPredicate, ExpressionData,
    ExpressionDialogueTemplate, ExpressionError, ExpressionMedium, ExpressionPack,
    ExpressionPackRef, ExpressionPlaceholderValue, ExpressionSpeakerRequirement,
    ExpressionTemplateAssignment, ExpressionTermKind, ExpressionTextAuthority, ExtensionHeader,
    Freshness, HexacoProfile, HexacoTrait, IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
    IdentityContextKind, IdentityPresentation, InnerLifeCategory, PreferencePolarity,
    PresentationAssetReference, PresentationCatalogValue, ProjectionKind, ProjectionPublicDecision,
    PronounSet, RELATIONSHIP_EXTENSION_NAMESPACE, ROLE_PROJECTION_EXTENSION_NAMESPACE,
    RelationshipEdge, RelationshipEdgeOrigin, RelationshipEdges, ReviewState, RoleProjections,
    TraitMeasurement, ValueState, VersionedExtension, VoiceCategory, expression_pack_ref,
    synthesize_character, template_fingerprint, validate_assistance_receipt,
    validate_character_collection, validate_expression_pack, validate_overlay, validate_profile,
    validate_template,
};

/// Current supported portable Character-pack format version.
pub const CHARACTER_PACK_FORMAT_VERSION: u32 = 1;

/// Versioned portable Character-pack format. Version 1 is the only supported value.
pub type PackFormatVersion = u32;

/// Maximum number of records in any one pack collection.
pub const CHARACTER_PACK_MAX_RECORDS: usize = 4_096;

/// Maximum number of character documents in one pack.
pub const CHARACTER_PACK_MAX_CHARACTERS: usize = CHARACTER_PACK_MAX_RECORDS;

/// Current packed tabletop projection format version.
pub const CHARACTER_PACK_TABLETOP_PROJECTION_FORMAT_VERSION: u32 = 1;
/// Current packed adapter manifest format version.
pub const CHARACTER_PACK_ADAPTER_MANIFEST_FORMAT_VERSION: u32 = 1;
/// Current packed tabletop mutable-state format version.
pub const CHARACTER_PACK_TABLETOP_STATE_FORMAT_VERSION: u32 = 1;
/// Entropy algorithm supported by packed tabletop state.
pub const CHARACTER_PACK_TABLETOP_ENTROPY_ALGORITHM: &str = "sha256_counter_v1";

const AUTHORING_PACK_SCHEMA_ID: &str = "urn:weave:schema:character-authoring-pack:1";
const RUNTIME_PACK_SCHEMA_ID: &str = "urn:weave:schema:character-runtime-pack:1";
const MAX_TEXT: usize = 65_536;

/// Redaction-safe Character-pack failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterPackError {
    diagnostic: CharacterDiagnostic,
}

impl CharacterPackError {
    /// Typed diagnostic usable by source tooling, the editor, and engine consumers.
    #[must_use]
    pub const fn diagnostic(&self) -> &CharacterDiagnostic {
        &self.diagnostic
    }
}

impl std::fmt::Display for CharacterPackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Character pack {:?} at `{}`: {}",
            self.diagnostic.code, self.diagnostic.path, self.diagnostic.message
        )
    }
}

impl std::error::Error for CharacterPackError {}

fn pack_error(
    code: CharacterDiagnosticCode,
    path: impl Into<String>,
    message: &'static str,
) -> CharacterPackError {
    CharacterPackError {
        diagnostic: CharacterDiagnostic {
            code,
            severity: DiagnosticSeverity::Error,
            path: path.into(),
            message: message.to_owned(),
        },
    }
}

fn encoding_error() -> CharacterPackError {
    pack_error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "Character pack does not match the strict serialized contract",
    )
}

fn from_character_error(error: CharacterError) -> CharacterPackError {
    CharacterPackError {
        diagnostic: error.diagnostic().clone(),
    }
}

fn from_corpus_error(error: CharacterCorpusError) -> CharacterPackError {
    CharacterPackError {
        diagnostic: error.diagnostic().clone(),
    }
}

fn ensure_record_capacity(path: &str, count: usize) -> Result<(), CharacterPackError> {
    if count > CHARACTER_PACK_MAX_RECORDS {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "pack collection exceeds the 4096-record limit; split the pack or raise the version",
        ));
    }
    Ok(())
}

fn duplicate_ron_key_error() -> CharacterPackError {
    pack_error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "Character pack RON contains a duplicate map key",
    )
}

struct RonFrame {
    is_map: bool,
    expecting_key: bool,
    keys: BTreeSet<String>,
}

impl RonFrame {
    fn map() -> Self {
        Self {
            is_map: true,
            expecting_key: true,
            keys: BTreeSet::new(),
        }
    }

    fn opaque() -> Self {
        Self {
            is_map: false,
            expecting_key: false,
            keys: BTreeSet::new(),
        }
    }
}

/// Reject duplicate keys in RON map literals before serde collapses them into one entry.
///
/// Duplicate struct fields are already rejected by the RON deserializer. Map literals keep the
/// last value silently, so this scanner tracks `{ ... }` frames and compares decoded keys.
fn reject_duplicate_ron_map_keys(source: &str) -> Result<(), CharacterPackError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut stack: Vec<RonFrame> = Vec::new();
    while index < bytes.len() {
        skip_ron_trivia(bytes, &mut index)?;
        let Some(&byte) = bytes.get(index) else {
            break;
        };
        match byte {
            b'{' => {
                stack.push(RonFrame::map());
                index += 1;
            }
            b'(' | b'[' => {
                stack.push(RonFrame::opaque());
                index += 1;
            }
            b'}' | b')' | b']' => {
                stack.pop();
                index += 1;
            }
            b',' => {
                if let Some(frame) = stack.last_mut()
                    && frame.is_map
                {
                    frame.expecting_key = true;
                }
                index += 1;
            }
            b':' => {
                if let Some(frame) = stack.last_mut()
                    && frame.is_map
                {
                    frame.expecting_key = false;
                }
                index += 1;
            }
            b'"' => {
                let (key, next) = read_ron_string(bytes, index)?;
                index = next;
                record_ron_map_key(&mut stack, format!("k:{key}"))?;
            }
            b'\'' => {
                let (key, next) = read_ron_char(bytes, index)?;
                index = next;
                record_ron_map_key(&mut stack, format!("c:{key}"))?;
            }
            b'r' => {
                if let Some((hashes, quote)) = ron_raw_string_start(bytes, index) {
                    let (key, next) = read_ron_raw_string(bytes, quote, hashes)?;
                    index = next;
                    record_ron_map_key(&mut stack, format!("k:{key}"))?;
                } else {
                    let (key, next) = read_ron_identifier(bytes, index);
                    index = next;
                    record_ron_map_key(&mut stack, format!("k:{key}"))?;
                }
            }
            _ if is_ron_identifier_start(byte) => {
                let (key, next) = read_ron_identifier(bytes, index);
                index = next;
                record_ron_map_key(&mut stack, format!("k:{key}"))?;
            }
            _ if byte.is_ascii_digit() || byte == b'-' || byte == b'+' => {
                let (key, next) = read_ron_number(bytes, index);
                index = next;
                record_ron_map_key(&mut stack, format!("n:{key}"))?;
            }
            _ => index += 1,
        }
    }
    Ok(())
}

fn record_ron_map_key(stack: &mut [RonFrame], key: String) -> Result<(), CharacterPackError> {
    let Some(frame) = stack.last_mut() else {
        return Ok(());
    };
    if !frame.is_map || !frame.expecting_key {
        return Ok(());
    }
    frame.expecting_key = false;
    if !frame.keys.insert(key) {
        return Err(duplicate_ron_key_error());
    }
    Ok(())
}

fn skip_ron_trivia(bytes: &[u8], index: &mut usize) -> Result<(), CharacterPackError> {
    loop {
        while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
            *index += 1;
        }
        if bytes.get(*index..*index + 2) == Some(b"//") {
            *index += 2;
            while *index < bytes.len() && bytes[*index] != b'\n' {
                *index += 1;
            }
        } else if bytes.get(*index..*index + 2) == Some(b"/*") {
            *index += 2;
            let mut depth = 1usize;
            while *index < bytes.len() && depth > 0 {
                if bytes.get(*index..*index + 2) == Some(b"/*") {
                    depth += 1;
                    *index += 2;
                } else if bytes.get(*index..*index + 2) == Some(b"*/") {
                    depth -= 1;
                    *index += 2;
                } else {
                    *index += 1;
                }
            }
            if depth != 0 {
                return Err(encoding_error());
            }
        } else {
            return Ok(());
        }
    }
}

fn is_ron_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn read_ron_identifier(bytes: &[u8], start: usize) -> (String, usize) {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    (
        String::from_utf8_lossy(&bytes[start..end]).into_owned(),
        end,
    )
}

fn read_ron_number(bytes: &[u8], start: usize) -> (String, usize) {
    let mut end = start;
    while end < bytes.len() && !is_ron_token_end(bytes[end]) {
        end += 1;
    }
    (
        String::from_utf8_lossy(&bytes[start..end]).into_owned(),
        end,
    )
}

fn is_ron_token_end(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b':' | b'{' | b'}' | b'(' | b')' | b'[' | b']' | b'/' | b'#'
        )
}

fn read_ron_string(bytes: &[u8], start: usize) -> Result<(String, usize), CharacterPackError> {
    let mut value = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Ok((value, index + 1)),
            b'\\' => {
                index += 1;
                let (character, next) = read_ron_escape(bytes, index)?;
                value.push(character);
                index = next;
            }
            byte if byte.is_ascii() => {
                value.push(char::from(byte));
                index += 1;
            }
            _ => {
                let (character, next) = read_ron_utf8_char(bytes, index)?;
                value.push(character);
                index = next;
            }
        }
    }
    Err(encoding_error())
}

fn read_ron_char(bytes: &[u8], start: usize) -> Result<(String, usize), CharacterPackError> {
    let mut index = start + 1;
    let (character, next) = if bytes.get(index) == Some(&b'\\') {
        read_ron_escape(bytes, index + 1)?
    } else {
        read_ron_utf8_char(bytes, index)?
    };
    index = next;
    if bytes.get(index) != Some(&b'\'') {
        return Err(encoding_error());
    }
    Ok((character.to_string(), index + 1))
}

fn read_ron_utf8_char(bytes: &[u8], index: usize) -> Result<(char, usize), CharacterPackError> {
    // The source is valid UTF-8, so the lead byte determines a bounded slice. Do not revalidate
    // the entire remaining buffer per character.
    let first = *bytes.get(index).ok_or_else(encoding_error)?;
    let length = match first {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return Err(encoding_error()),
    };
    let end = index + length;
    let window = bytes.get(index..end).ok_or_else(encoding_error)?;
    let text = std::str::from_utf8(window).map_err(|_| encoding_error())?;
    let character = text.chars().next().ok_or_else(encoding_error)?;
    Ok((character, end))
}

fn read_ron_escape(bytes: &[u8], index: usize) -> Result<(char, usize), CharacterPackError> {
    let Some(&byte) = bytes.get(index) else {
        return Err(encoding_error());
    };
    match byte {
        b'n' => Ok(('\n', index + 1)),
        b'r' => Ok(('\r', index + 1)),
        b't' => Ok(('\t', index + 1)),
        b'0' => Ok(('\0', index + 1)),
        b'\\' => Ok(('\\', index + 1)),
        b'"' => Ok(('"', index + 1)),
        b'\'' => Ok(('\'', index + 1)),
        b'/' => Ok(('/', index + 1)),
        b'x' => {
            let value = read_ron_hex(bytes, index + 1, 2)?;
            let character = char::from_u32(value).ok_or_else(encoding_error)?;
            Ok((character, index + 3))
        }
        b'u' => {
            if bytes.get(index + 1) != Some(&b'{') {
                return Err(encoding_error());
            }
            let mut end = index + 2;
            while end < bytes.len() && bytes[end] != b'}' {
                end += 1;
            }
            if end == bytes.len() {
                return Err(encoding_error());
            }
            let digits =
                std::str::from_utf8(&bytes[index + 2..end]).map_err(|_| encoding_error())?;
            let value = u32::from_str_radix(digits, 16).map_err(|_| encoding_error())?;
            let character = char::from_u32(value).ok_or_else(encoding_error)?;
            Ok((character, end + 1))
        }
        other => Ok((other as char, index + 1)),
    }
}

fn read_ron_hex(bytes: &[u8], start: usize, length: usize) -> Result<u32, CharacterPackError> {
    let end = start + length;
    let digits = bytes.get(start..end).ok_or_else(encoding_error)?;
    let text = std::str::from_utf8(digits).map_err(|_| encoding_error())?;
    u32::from_str_radix(text, 16).map_err(|_| encoding_error())
}

fn ron_raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    let mut hashes = 0;
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'"') {
        Some((hashes, cursor))
    } else {
        None
    }
}

fn read_ron_raw_string(
    bytes: &[u8],
    quote: usize,
    hashes: usize,
) -> Result<(String, usize), CharacterPackError> {
    let mut index = quote + 1;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let close_end = index + 1 + hashes;
            if bytes
                .get(index + 1..close_end)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
            {
                let text =
                    std::str::from_utf8(&bytes[quote + 1..index]).map_err(|_| encoding_error())?;
                return Ok((text.to_owned(), close_end));
            }
        }
        index += 1;
    }
    Err(encoding_error())
}

fn from_expression_error(error: ExpressionError) -> CharacterPackError {
    let diagnostic = error.diagnostic();
    pack_error(
        CharacterDiagnosticCode::InvalidValue,
        diagnostic.path.clone(),
        "nested expression artifact failed validation",
    )
}

/// Fail closed when a pack declares a version this crate cannot interpret.
pub fn validate_pack_format_version(version: PackFormatVersion) -> Result<(), CharacterPackError> {
    if version == CHARACTER_PACK_FORMAT_VERSION {
        Ok(())
    } else {
        Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "pack_format_version",
            "unsupported character pack format version; re-export the pack as version 1",
        ))
    }
}

/// Pack-level coordinate and licence shared by authoring and runtime exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackMetadata {
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub license: CharacterPackLicense,
}

impl CharacterPackMetadata {
    fn validate(&self) -> Result<(), CharacterPackError> {
        validate_namespaced_id("id", &self.id)?;
        validate_semver("version", &self.version)?;
        validate_text("title", &self.title, 1, 256)?;
        validate_text("description", &self.description, 1, 2_048)
    }
}

/// Licence data that always travels with a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackLicense {
    /// SPDX expression for the pack's own material.
    pub license: String,
    /// Public HTTPS location of the licence text.
    pub license_url: String,
    /// Full licence text retained with the pack.
    pub license_text: String,
    /// Separately licensed included components, each with its own full text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<CharacterPackLicenseNotice>,
}

/// One separately licensed component retained inside a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackLicenseNotice {
    pub id: String,
    /// Provenance source id or other stable component coordinate.
    pub component: String,
    pub license: String,
    pub license_url: String,
    pub text: String,
}

/// Exact tabletop adapter identity and release pinned by a pack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackAdapterCoordinate {
    pub id: String,
    pub version: String,
    pub schema_version: u32,
    pub content_sha256: String,
}

/// Closed tabletop capability vocabulary supported by v1 projections.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CharacterPackAdapterCapability {
    CharacterCreation,
    DerivedValues,
    ChecksAndConflicts,
    ResourcesAndConditions,
    EquipmentAndAbilities,
    Advancement,
    Encounters,
    Clocks,
    Scenes,
    Factions,
    WorldState,
    CampaignState,
}

/// One declared capability contract version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackAdapterCapabilityDeclaration {
    pub capability: CharacterPackAdapterCapability,
    pub version: u32,
}

/// Pinned, portable adapter manifest summary carried by a pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackAdapterManifest {
    pub manifest_format_version: u32,
    pub coordinate: CharacterPackAdapterCoordinate,
    pub namespace: String,
    pub title: String,
    /// Declared capabilities in stable capability order.
    pub capabilities: Vec<CharacterPackAdapterCapabilityDeclaration>,
    /// Declared creation-step ids in stable order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creation_step_ids: Vec<String>,
    pub definition_type: TypeExpression,
    pub state_type: TypeExpression,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: BTreeMap<String, TypeExpression>,
    /// Adapter licence boundary and notices.
    pub license: CharacterPackLicense,
}

/// One immutable adapter-owned character definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackAdapterDefinition {
    pub adapter: CharacterPackAdapterCoordinate,
    pub definition: DomainValue,
    pub definition_sha256: String,
    /// Completed creation-step ids in stable order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_creation_steps: Vec<String>,
}

/// Compact mutable adapter state required to run without the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackTabletopState {
    pub state_format_version: u32,
    pub adapter: CharacterPackAdapterCoordinate,
    pub owner_id: String,
    pub definition_sha256: String,
    pub revision: u64,
    pub entropy_algorithm: String,
    pub entropy_seed: u64,
    pub entropy_cursor: u64,
    pub value: DomainValue,
}

/// One inactive adapter projection retained only in authoring exports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackInactiveTabletop {
    pub adapter: CharacterPackAdapterCoordinate,
    pub definition: DomainValue,
    pub definition_sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_creation_steps: Vec<String>,
    pub reason: String,
}

/// Active adapter projection with authoring provenance and archived projections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackTabletopProjection {
    pub projection_format_version: u32,
    pub character_id: String,
    pub canonical_profile_sha256: String,
    /// Always false; tabletop projections cannot write canonical Character evidence.
    pub canonical_character_write_back: bool,
    pub manifest: CharacterPackAdapterManifest,
    /// Adapter provenance retained only in authoring exports.
    pub provenance: Provenance,
    pub active: CharacterPackAdapterDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<CharacterPackTabletopState>,
    /// Archived adapter projections keyed by their exact adapter namespace key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inactive: BTreeMap<String, CharacterPackInactiveTabletop>,
}

/// Approved runtime adapter view without provenance or archived projections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterPackActiveTabletop {
    pub projection_format_version: u32,
    pub character_id: String,
    pub canonical_profile_sha256: String,
    /// Always false; tabletop projections cannot write canonical Character evidence.
    pub canonical_character_write_back: bool,
    pub manifest: CharacterPackAdapterManifest,
    pub active: CharacterPackAdapterDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<CharacterPackTabletopState>,
}

/// One effective profile together with its authoring lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringEntry {
    /// Effective, fully attributed profile.
    pub profile: CharacterProfile,
    /// Exact immutable template this profile was synthesized from, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<CharacterTemplateRef>,
    /// Sparse operation manifest applied over the template or blank profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_id: Option<String>,
    /// Sorted assistance receipts that contributed to this character.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assistance_receipt_ids: Vec<String>,
    /// Accepted or needs-changes final authoring review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_review: Option<CharacterFinalReview>,
    /// Active tabletop adapter projection plus authoring-only archives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabletop: Option<CharacterPackTabletopProjection>,
}

impl CharacterAuthoringEntry {
    /// Build an entry for a directly exported profile with no template or override lineage.
    #[must_use]
    pub fn direct(profile: CharacterProfile) -> Self {
        Self {
            profile,
            template: None,
            override_id: None,
            assistance_receipt_ids: Vec::new(),
            final_review: None,
            tabletop: None,
        }
    }
}

/// Deterministic authoring export for one profile or a stable collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringPack {
    pub pack_format_version: PackFormatVersion,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub license: CharacterPackLicense,
    /// Machine-readable lineage for every included source and transformation.
    pub provenance: Provenance,
    /// Effective profiles sorted by stable character id.
    pub characters: Vec<CharacterAuthoringEntry>,
    /// Immutable templates sorted by id and semantic version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<CharacterTemplate>,
    /// Sparse operation manifests sorted by overlay id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<CharacterOverlay>,
    /// Expression packs required by assigned dialogue templates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expression_packs: Vec<ExpressionPack>,
    /// Assistance receipts sorted by receipt id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assistance_receipts: Vec<AssistanceReceipt>,
    /// Read-only health diagnostics sorted by diagnostic id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub health_diagnostics: Vec<CharacterHealthDiagnostic>,
}

impl CharacterAuthoringPack {
    /// Assemble a validated authoring pack from a reviewed collection.
    ///
    /// `expression_packs` must contain every pack referenced by an included profile.
    pub fn from_collection(
        metadata: CharacterPackMetadata,
        collection: &CharacterCollection,
        expression_packs: Vec<ExpressionPack>,
    ) -> Result<Self, CharacterPackError> {
        validate_character_collection(collection).map_err(from_corpus_error)?;
        let provenance = merge_collection_provenance(collection)?;
        let mut pack = Self {
            pack_format_version: CHARACTER_PACK_FORMAT_VERSION,
            id: metadata.id,
            version: metadata.version,
            title: metadata.title,
            description: metadata.description,
            license: metadata.license,
            provenance,
            characters: collection
                .characters
                .values()
                .cloned()
                .map(CharacterAuthoringEntry::direct)
                .collect(),
            templates: Vec::new(),
            overrides: Vec::new(),
            expression_packs,
            assistance_receipts: Vec::new(),
            health_diagnostics: Vec::new(),
        };
        pack.normalize();
        pack.validate()?;
        Ok(pack)
    }

    /// Assemble a validated pack from one self-contained profile.
    ///
    /// A single-profile pack is the one-entry form of the same deterministic collection
    /// contract used by [`Self::from_collection`].
    pub fn from_profile(
        metadata: CharacterPackMetadata,
        profile: CharacterProfile,
        expression_packs: Vec<ExpressionPack>,
    ) -> Result<Self, CharacterPackError> {
        validate_profile(&profile).map_err(from_character_error)?;
        let collection = CharacterCollection {
            collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
            id: format!("{}.single", metadata.id),
            revision: 0,
            characters: BTreeMap::from([(profile.id.clone(), profile)]),
        };
        Self::from_collection(metadata, &collection, expression_packs)
    }

    /// Parse and validate strict authoring-pack JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterPackError> {
        let value = parse_strict_json(source).map_err(|_| encoding_error())?;
        validate_authoring_pack(&value)?;
        Ok(value)
    }

    /// Parse and validate authoring-pack RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterPackError> {
        reject_duplicate_ron_map_keys(source)?;
        let value = ron::from_str(source).map_err(|_| encoding_error())?;
        validate_authoring_pack(&value)?;
        Ok(value)
    }

    /// Serialize canonical authoring-pack JSON.
    pub fn to_json(&self) -> Result<String, CharacterPackError> {
        validate_authoring_pack(self)?;
        to_pretty_json(self).map_err(|_| encoding_error())
    }

    /// Serialize canonical authoring-pack RON.
    pub fn to_ron(&self) -> Result<String, CharacterPackError> {
        validate_authoring_pack(self)?;
        to_pretty_ron(self).map_err(|_| encoding_error())
    }

    /// Validate the complete authoring pack.
    pub fn validate(&self) -> Result<(), CharacterPackError> {
        validate_authoring_pack(self)
    }

    /// Sort every collection so repeated exports are byte-stable.
    pub fn normalize(&mut self) {
        self.characters
            .sort_by_key(|entry| entry.profile.id.clone());
        self.templates.sort_by_key(template_order_key);
        self.overrides.sort_by_key(|overlay| overlay.id.clone());
        self.expression_packs.sort_by_key(expression_pack_order_key);
        self.assistance_receipts
            .sort_by_key(|receipt| receipt.id.clone());
        self.health_diagnostics
            .sort_by_key(|diagnostic| diagnostic.id.clone());
        for entry in &mut self.characters {
            entry.assistance_receipt_ids.sort();
            entry.assistance_receipt_ids.dedup();
            if let Some(tabletop) = &mut entry.tabletop {
                normalize_tabletop_projection(tabletop);
            }
        }
    }

    /// Validate the pack format version and return a normalized current-version pack.
    pub fn migrate(mut self) -> Result<Self, CharacterPackError> {
        validate_pack_format_version(self.pack_format_version)?;
        self.pack_format_version = CHARACTER_PACK_FORMAT_VERSION;
        self.normalize();
        self.validate()?;
        Ok(self)
    }

    /// Export the compact runtime projection of this authoring pack.
    ///
    /// The runtime pack keeps approved values and the references needed to render them. It
    /// carries the pack metadata and licence unchanged.
    pub fn to_runtime_pack(&self) -> Result<CharacterRuntimePack, CharacterPackError> {
        validate_authoring_pack(self)?;
        let mut characters = Vec::with_capacity(self.characters.len());
        for entry in &self.characters {
            characters.push(runtime_profile(entry, self)?);
        }
        let mut pack = CharacterRuntimePack {
            pack_format_version: CHARACTER_PACK_FORMAT_VERSION,
            id: self.id.clone(),
            version: self.version.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            license: self.license.clone(),
            characters,
        };
        pack.normalize();
        pack.validate()?;
        Ok(pack)
    }
}

/// Compact runtime export for one profile or a stable collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterRuntimePack {
    pub pack_format_version: PackFormatVersion,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub license: CharacterPackLicense,
    /// Approved profiles sorted by stable character id.
    pub characters: Vec<CharacterRuntimeProfile>,
}

impl CharacterRuntimePack {
    /// Parse and validate strict runtime-pack JSON.
    pub fn from_json(source: &str) -> Result<Self, CharacterPackError> {
        let value = parse_strict_json(source).map_err(|_| encoding_error())?;
        validate_runtime_pack(&value)?;
        Ok(value)
    }

    /// Parse and validate runtime-pack RON.
    pub fn from_ron(source: &str) -> Result<Self, CharacterPackError> {
        reject_duplicate_ron_map_keys(source)?;
        let value = ron::from_str(source).map_err(|_| encoding_error())?;
        validate_runtime_pack(&value)?;
        Ok(value)
    }

    /// Serialize canonical runtime-pack JSON.
    pub fn to_json(&self) -> Result<String, CharacterPackError> {
        validate_runtime_pack(self)?;
        to_pretty_json(self).map_err(|_| encoding_error())
    }

    /// Serialize canonical runtime-pack RON.
    pub fn to_ron(&self) -> Result<String, CharacterPackError> {
        validate_runtime_pack(self)?;
        to_pretty_ron(self).map_err(|_| encoding_error())
    }

    /// Validate the complete runtime pack.
    pub fn validate(&self) -> Result<(), CharacterPackError> {
        validate_runtime_pack(self)
    }

    /// Sort every collection so repeated exports are byte-stable.
    pub fn normalize(&mut self) {
        self.characters.sort_by_key(|profile| profile.id.clone());
        for character in &mut self.characters {
            character.aliases.sort();
            character.aliases.dedup();
            if let Some(presentation) = &mut character.presentation {
                presentation
                    .context_notes
                    .sort_by_key(|note| note.id.clone());
                presentation
                    .appearance
                    .sort_by_key(|appearance| appearance.id.clone());
                presentation.assets.sort_by_key(|asset| asset.id.clone());
                presentation.style_tags.sort();
                presentation.style_tags.dedup();
            }
            if let Some(expression) = &mut character.expression {
                for template in expression.dialogue.values_mut() {
                    template.variants.sort_by_key(|variant| variant.id.clone());
                }
                for pool in expression.vocabulary_pools.values_mut() {
                    pool.term_ids.sort();
                    pool.term_ids.dedup();
                }
            }
            if let Some(tabletop) = &mut character.tabletop {
                normalize_tabletop_active(tabletop);
            }
        }
    }

    /// Validate the pack format version and return a normalized current-version pack.
    pub fn migrate(mut self) -> Result<Self, CharacterPackError> {
        validate_pack_format_version(self.pack_format_version)?;
        self.pack_format_version = CHARACTER_PACK_FORMAT_VERSION;
        self.normalize();
        self.validate()?;
        Ok(self)
    }
}

/// Approved runtime identity, personality, presentation, and extension values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterRuntimeProfile {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Present trait measurements keyed by canonical trait identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub personality: BTreeMap<HexacoTrait, TraitMeasurement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<BirthDate>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inner_life: BTreeMap<String, RuntimeInnerLifeRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub voice: BTreeMap<String, RuntimeVoiceRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<RuntimePresentation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, RuntimeRole>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alignment: BTreeMap<String, RuntimeAlignment>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relationships: BTreeMap<String, RuntimeRelationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<RuntimeExpression>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub date_context: BTreeMap<String, RuntimeDateCue>,
    /// Approved active adapter view; inactive projections remain authoring-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabletop: Option<CharacterPackActiveTabletop>,
}

/// One approved inner-life record without attribution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInnerLifeRecord {
    pub category: InnerLifeCategory,
    pub content: String,
}

/// One approved voice record without attribution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVoiceRecord {
    pub category: VoiceCategory,
    pub content: String,
}

/// Approved presentation values required to render a character offline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimePresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<PronounSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_notes: Vec<RuntimeContextNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appearance: Vec<RuntimeAppearance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub palette: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<PresentationAssetReference>,
    /// Reviewed catalog values keyed by presentation slot, without proposal or review hashes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub catalog: BTreeMap<String, RuntimeCatalogAssignment>,
}

/// One approved identity-adjacent note without attribution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContextNote {
    pub id: String,
    pub kind: IdentityContextKind,
    pub content: String,
}

/// One approved appearance descriptor without attribution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAppearance {
    pub id: String,
    pub category: String,
    pub content: String,
}

/// One approved catalog value. Catalog identity is retained; review fingerprints are not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCatalogAssignment {
    pub catalog_id: String,
    pub catalog_version: String,
    pub entry_id: String,
    pub value: PresentationCatalogValue,
}

/// One approved classification, vocation, social-role, or narrative-role value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRole {
    pub kind: ProjectionKind,
    pub taxonomy: String,
    pub entry_id: String,
    pub label: String,
    pub decision: ProjectionPublicDecision,
    pub lossy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<i32>,
    pub coverage_micros: u32,
}

/// One approved alignment shorthand value without explanation or input-path metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAlignment {
    pub label_id: String,
    pub label: String,
    pub decision: AlignmentPublicDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<i32>,
    pub coverage_micros: u32,
}

/// One approved relationship edge without scores, evidence, or lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRelationship {
    pub source_character_id: String,
    pub target_character_id: String,
    pub kind: String,
    pub origin: RelationshipEdgeOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse_edge_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, String>,
}

/// Approved expression records and resolved dialogue templates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeExpression {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lexicon: BTreeMap<String, RuntimeLexiconTerm>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preferences: BTreeMap<String, RuntimePreference>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vocabulary_pools: BTreeMap<String, RuntimeVocabularyPool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub voice_constraints: BTreeMap<String, RuntimeVoiceConstraint>,
    /// Dialogue templates resolved from included expression packs, keyed by template id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dialogue: BTreeMap<String, RuntimeDialogueTemplate>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub template_assignments: BTreeMap<String, RuntimeTemplateAssignment>,
}

/// One approved normalized lexicon term.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLexiconTerm {
    pub category: String,
    pub kind: ExpressionTermKind,
    pub surface: String,
    pub normalized: String,
    pub strength: f64,
    pub applicability: ExpressionApplicability,
}

/// One approved preference record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimePreference {
    pub category: String,
    pub target: String,
    pub polarity: PreferencePolarity,
    pub strength: f64,
    pub applicability: ExpressionApplicability,
}

/// One approved vocabulary pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVocabularyPool {
    pub category: String,
    pub term_ids: Vec<String>,
    pub applicability: ExpressionApplicability,
}

/// One approved voice constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeVoiceConstraint {
    pub category: String,
    pub medium: ExpressionMedium,
    pub effect: ExpressionConstraintEffect,
    pub target: String,
    pub instruction: String,
    pub strength: f64,
    pub applicability: ExpressionApplicability,
}

/// One approved template assignment retained as a required reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTemplateAssignment {
    pub scenario_id: String,
    pub template_id: String,
}

/// One dialogue template with only approved variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDialogueTemplate {
    pub scenario_id: String,
    pub speaker_requirement: ExpressionSpeakerRequirement,
    pub placeholders: BTreeMap<String, RuntimePlaceholderDeclaration>,
    pub variants: Vec<RuntimeDialogueVariant>,
    pub fallback_variant_id: String,
}

/// One declared runtime placeholder and its closed value source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimePlaceholderDeclaration {
    pub value: ExpressionPlaceholderValue,
    pub required: bool,
}

/// One approved dialogue variant without source coordinates or lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDialogueVariant {
    pub id: String,
    pub content: String,
    pub priority: i32,
    pub authority: ExpressionTextAuthority,
    pub applicability: ExpressionApplicability,
}

/// One accepted date-context cue without pack hashes or source lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDateCue {
    pub record_id: String,
    pub kind: DateContextCueKind,
    pub content: String,
    pub relevance: f64,
    pub uncertainty: DateContextUncertainty,
}

/// Generate the canonical authoring-pack JSON Schema.
pub fn character_authoring_pack_schema() -> Result<String, CharacterPackError> {
    pack_schema::<CharacterAuthoringPack>(
        AUTHORING_PACK_SCHEMA_ID,
        "Weave Character Authoring Pack v1",
    )
}

/// Generate the canonical runtime-pack JSON Schema.
pub fn character_runtime_pack_schema() -> Result<String, CharacterPackError> {
    pack_schema::<CharacterRuntimePack>(RUNTIME_PACK_SCHEMA_ID, "Weave Character Runtime Pack v1")
}

fn pack_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterPackError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
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
        if let Some(properties) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            && let Some(version) = properties
                .get_mut("pack_format_version")
                .and_then(serde_json::Value::as_object_mut)
        {
            version.insert(
                "const".to_owned(),
                serde_json::Value::from(CHARACTER_PACK_FORMAT_VERSION),
            );
        }
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

/// Deterministic SHA-256 of one packed portable value.
///
/// This matches the tabletop contract's canonical fingerprint: compact JSON bytes hashed with
/// SHA-256.
pub fn character_pack_value_fingerprint(value: &DomainValue) -> Result<String, CharacterPackError> {
    canonical_serialized_sha256(value)
}

/// Deterministic SHA-256 of one canonical Character profile.
pub fn character_pack_profile_fingerprint(
    profile: &CharacterProfile,
) -> Result<String, CharacterPackError> {
    canonical_serialized_sha256(profile)
}

/// Deterministic SHA-256 of one packed adapter manifest.
///
/// The coordinate's own `content_sha256` is excluded so the hash can be recomputed before the
/// coordinate is pinned.
pub fn character_pack_adapter_manifest_fingerprint(
    manifest: &CharacterPackAdapterManifest,
) -> Result<String, CharacterPackError> {
    let mut value = serde_json::to_value(manifest).map_err(|_| encoding_error())?;
    if let Some(object) = value.as_object_mut() {
        if let Some(coordinate) = object
            .get_mut("coordinate")
            .and_then(serde_json::Value::as_object_mut)
        {
            coordinate.remove("content_sha256");
        }
        // Sort the two ordered vectors so the fingerprint is invariant to input order. Pack
        // validation still requires the canonical order in the stored manifest.
        if let Some(capabilities) = object
            .get_mut("capabilities")
            .and_then(serde_json::Value::as_array_mut)
        {
            capabilities.sort_by(|left, right| {
                left.get("capability")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("capability")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default(),
                    )
            });
        }
        if let Some(steps) = object
            .get_mut("creation_step_ids")
            .and_then(serde_json::Value::as_array_mut)
        {
            steps.sort_by(|left, right| {
                left.as_str()
                    .unwrap_or_default()
                    .cmp(right.as_str().unwrap_or_default())
            });
        }
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| encoding_error())?;
    Ok(sha256_hex(&bytes))
}

/// Stable archive key for one packed adapter identity and release.
#[must_use]
pub fn character_pack_adapter_namespace_key(adapter: &CharacterPackAdapterCoordinate) -> String {
    format!(
        "{}_v{}",
        adapter.id.replace('.', "_"),
        adapter.version.replace('.', "_")
    )
}

fn canonical_serialized_sha256(value: &impl Serialize) -> Result<String, CharacterPackError> {
    let bytes = serde_json::to_vec(value).map_err(|_| encoding_error())?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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

/// Validate a complete authoring pack.
pub fn validate_authoring_pack(pack: &CharacterAuthoringPack) -> Result<(), CharacterPackError> {
    validate_pack_format_version(pack.pack_format_version)?;
    CharacterPackMetadata {
        id: pack.id.clone(),
        version: pack.version.clone(),
        title: pack.title.clone(),
        description: pack.description.clone(),
        license: pack.license.clone(),
    }
    .validate()?;
    validate_pack_license("license", &pack.license, &pack.provenance)?;
    validate_provenance(&pack.provenance).map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "pack provenance is malformed or incomplete",
        )
    })?;

    if pack.characters.is_empty() {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            "characters",
            "a pack requires at least one character entry",
        ));
    }
    ensure_record_capacity("characters", pack.characters.len())?;
    ensure_record_capacity("templates", pack.templates.len())?;
    ensure_record_capacity("overrides", pack.overrides.len())?;
    ensure_record_capacity("expression_packs", pack.expression_packs.len())?;
    ensure_record_capacity("assistance_receipts", pack.assistance_receipts.len())?;
    ensure_record_capacity("health_diagnostics", pack.health_diagnostics.len())?;
    ensure_strict_order("characters", &pack.characters, |entry| {
        entry.profile.id.clone()
    })?;
    let mut character_ids = BTreeSet::new();
    let mut character_profiles = BTreeMap::new();
    for entry in &pack.characters {
        character_ids.insert(entry.profile.id.as_str());
        character_profiles.insert(entry.profile.id.as_str(), &entry.profile);
    }

    ensure_strict_order("templates", &pack.templates, template_coordinate)?;
    for template in &pack.templates {
        validate_template(template).map_err(from_character_error)?;
    }

    ensure_strict_order("overrides", &pack.overrides, |overlay| overlay.id.clone())?;
    for overlay in &pack.overrides {
        validate_overlay(overlay).map_err(from_character_error)?;
        if !character_ids.contains(overlay.character_id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("overrides.{}", overlay.id),
                "override targets a character that is absent from the pack",
            ));
        }
    }

    ensure_strict_order("expression_packs", &pack.expression_packs, |pack| {
        format!("{}@{}", pack.id, version_key(&pack.version))
    })?;
    for expression_pack in &pack.expression_packs {
        validate_expression_pack(expression_pack).map_err(from_expression_error)?;
    }

    ensure_strict_order(
        "assistance_receipts",
        &pack.assistance_receipts,
        |receipt| receipt.id.clone(),
    )?;
    for receipt in &pack.assistance_receipts {
        validate_assistance_receipt(receipt).map_err(from_character_error)?;
        if !character_ids.contains(receipt.output_profile.id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("assistance_receipts.{}", receipt.id),
                "assistance receipt targets a character that is absent from the pack",
            ));
        }
    }

    ensure_strict_order(
        "health_diagnostics",
        &pack.health_diagnostics,
        |diagnostic| diagnostic.id.clone(),
    )?;
    for diagnostic in &pack.health_diagnostics {
        validate_health_diagnostic(diagnostic, &character_ids)?;
    }

    let template_ids = pack
        .templates
        .iter()
        .map(template_coordinate)
        .collect::<BTreeSet<_>>();
    let override_ids = pack
        .overrides
        .iter()
        .map(|overlay| overlay.id.clone())
        .collect::<BTreeSet<_>>();
    let receipt_ids = pack
        .assistance_receipts
        .iter()
        .map(|receipt| receipt.id.clone())
        .collect::<BTreeSet<_>>();

    for entry in &pack.characters {
        let path = format!("characters.{}", entry.profile.id);
        validate_profile(&entry.profile).map_err(from_character_error)?;
        validate_authoring_lineage(
            &path,
            entry,
            pack,
            &template_ids,
            &override_ids,
            &receipt_ids,
        )?;
        validate_authoring_relationships(&path, &entry.profile, &character_profiles)?;
        validate_authoring_tabletop(&path, &entry.profile, entry.tabletop.as_ref())?;
    }

    Ok(())
}

fn validate_authoring_lineage(
    path: &str,
    entry: &CharacterAuthoringEntry,
    pack: &CharacterAuthoringPack,
    template_ids: &BTreeSet<String>,
    override_ids: &BTreeSet<String>,
    receipt_ids: &BTreeSet<String>,
) -> Result<(), CharacterPackError> {
    let templates = pack.templates.as_slice();
    let overrides = pack.overrides.as_slice();
    let expression_packs = pack.expression_packs.as_slice();
    if !entry.assistance_receipt_ids.is_sorted() {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.assistance_receipt_ids"),
            "assistance receipt ids must be sorted and unique",
        ));
    }
    let mut prior = None;
    for receipt_id in &entry.assistance_receipt_ids {
        validate_namespaced_id(&format!("{path}.assistance_receipt_ids"), receipt_id)?;
        if prior.is_some_and(|prior: &String| prior >= receipt_id) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.assistance_receipt_ids"),
                "assistance receipt ids must be sorted and unique",
            ));
        }
        prior = Some(receipt_id);
        if !receipt_ids.contains(receipt_id) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.assistance_receipt_ids.{receipt_id}"),
                "assistance receipt id is absent from the pack",
            ));
        }
    }

    if let Some(review) = &entry.final_review {
        if review.review_format_version != CHARACTER_FINAL_REVIEW_FORMAT_VERSION {
            return Err(pack_error(
                CharacterDiagnosticCode::UnsupportedVersion,
                format!("{path}.final_review.review_format_version"),
                "unsupported final authoring review version",
            ));
        }
        if review.draft_id != entry.profile.id {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.final_review.draft_id"),
                "final review targets a different character",
            ));
        }
        validate_text(
            &format!("{path}.final_review.reviewer"),
            &review.reviewer,
            1,
            256,
        )?;
        validate_text(
            &format!("{path}.final_review.rationale"),
            &review.rationale,
            1,
            2_048,
        )?;
    }

    let template = match &entry.template {
        Some(reference) => {
            let coordinate = format!("{}@{}", reference.id, reference.version);
            if !template_ids.contains(&coordinate) {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{path}.template"),
                    "template coordinate is absent from the pack",
                ));
            }
            let template = templates
                .iter()
                .find(|template| {
                    template.id == reference.id && template.version == reference.version
                })
                .ok_or_else(|| {
                    pack_error(
                        CharacterDiagnosticCode::InvalidReference,
                        format!("{path}.template"),
                        "template coordinate is absent from the pack",
                    )
                })?;
            if template_fingerprint(template).map_err(from_character_error)? != reference.sha256 {
                return Err(pack_error(
                    CharacterDiagnosticCode::StaleInput,
                    format!("{path}.template.sha256"),
                    "template fingerprint does not match the exact template in the pack",
                ));
            }
            Some(template)
        }
        None => None,
    };

    let overlay = match &entry.override_id {
        Some(id) => {
            if !override_ids.contains(id) {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{path}.override_id"),
                    "override id is absent from the pack",
                ));
            }
            let overlay = overrides
                .iter()
                .find(|overlay| overlay.id == *id)
                .ok_or_else(|| {
                    pack_error(
                        CharacterDiagnosticCode::InvalidReference,
                        format!("{path}.override_id"),
                        "override id is absent from the pack",
                    )
                })?;
            if overlay.character_id != entry.profile.id {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{path}.override_id"),
                    "override targets a different character",
                ));
            }
            Some(overlay)
        }
        None => None,
    };

    match (template, overlay) {
        (None, None) => {}
        (Some(template), None) => {
            if template.profile != entry.profile {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidValue,
                    path,
                    "entry profile does not match its declared template without overrides",
                ));
            }
        }
        (template, Some(overlay)) => {
            let synthesized =
                synthesize_character(template, overlay).map_err(from_character_error)?;
            if synthesized.effective_profile != entry.profile {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidValue,
                    path,
                    "entry profile does not reproduce from its template and override",
                ));
            }
        }
    }

    validate_expression_references(path, &entry.profile, expression_packs)
}

fn validate_authoring_relationships(
    path: &str,
    profile: &CharacterProfile,
    character_profiles: &BTreeMap<&str, &CharacterProfile>,
) -> Result<(), CharacterPackError> {
    let Some(CharacterExtension::Relationships(record)) =
        profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE)
    else {
        return Ok(());
    };
    for (id, edge) in &record.value.edges {
        let Some(target) = character_profiles.get(edge.target_character_id.as_str()) else {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.relationships.{id}.target_character_id"),
                "relationship target is absent from the pack",
            ));
        };
        if let Some(inverse_id) = &edge.inverse_edge_id
            && !reciprocal_authoring_edge(&record.value, target, edge, inverse_id)
        {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.relationships.{id}.inverse_edge_id"),
                "inverse relationship must be an exact reciprocal edge in this profile or the target profile",
            ));
        }
    }
    Ok(())
}

fn reciprocal_authoring_edge(
    owner_edges: &RelationshipEdges,
    target: &CharacterProfile,
    edge: &RelationshipEdge,
    inverse_id: &str,
) -> bool {
    let owner_inverse = owner_edges.edges.get(inverse_id);
    let target_inverse =
        authoring_relationship_edges(target).and_then(|edges| edges.edges.get(inverse_id));
    owner_inverse
        .into_iter()
        .chain(target_inverse)
        .any(|inverse| {
            inverse.source_character_id == edge.target_character_id
                && inverse.target_character_id == edge.source_character_id
                && inverse.inverse_edge_id.as_deref() == Some(edge.id.as_str())
        })
}

fn authoring_relationship_edges(profile: &CharacterProfile) -> Option<&RelationshipEdges> {
    match profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Relationships(record)) => Some(&record.value),
        _ => None,
    }
}

fn validate_expression_references(
    path: &str,
    profile: &CharacterProfile,
    expression_packs: &[ExpressionPack],
) -> Result<(), CharacterPackError> {
    let Some(CharacterExtension::Expression(record)) =
        profile.extensions.get(EXPRESSION_EXTENSION_NAMESPACE)
    else {
        return Ok(());
    };
    for reference in &record.value.source_pack_refs {
        resolve_expression_pack(expression_packs, reference, &format!("{path}.expression"))?;
    }
    for assignment in record.value.template_assignments.values() {
        let expression_pack = resolve_expression_pack(
            expression_packs,
            &assignment.pack,
            &format!("{path}.expression"),
        )?;
        let Some(template) = expression_pack.templates.get(&assignment.template_id) else {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.expression.template_assignments.{}", assignment.id),
                "dialogue template id is absent from the referenced expression pack",
            ));
        };
        if template.scenario_id != assignment.scenario_id {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.expression.template_assignments.{}", assignment.id),
                "dialogue template scenario does not match its assignment",
            ));
        }
    }
    Ok(())
}

fn resolve_expression_pack<'a>(
    packs: &'a [ExpressionPack],
    reference: &ExpressionPackRef,
    path: &str,
) -> Result<&'a ExpressionPack, CharacterPackError> {
    let pack = packs
        .iter()
        .find(|pack| pack.id == reference.id && pack.version == reference.version)
        .ok_or_else(|| {
            pack_error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "required expression pack is absent from the pack",
            )
        })?;
    let actual = expression_pack_ref(pack).map_err(from_expression_error)?;
    if actual.sha256 != reference.sha256 {
        return Err(pack_error(
            CharacterDiagnosticCode::StaleInput,
            path,
            "expression pack fingerprint does not match the reference",
        ));
    }
    Ok(pack)
}

/// Validate a complete runtime pack and every required reference.
pub fn validate_runtime_pack(pack: &CharacterRuntimePack) -> Result<(), CharacterPackError> {
    validate_pack_format_version(pack.pack_format_version)?;
    CharacterPackMetadata {
        id: pack.id.clone(),
        version: pack.version.clone(),
        title: pack.title.clone(),
        description: pack.description.clone(),
        license: pack.license.clone(),
    }
    .validate()?;
    validate_pack_license("license", &pack.license, &empty_provenance())?;

    if pack.characters.is_empty() {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            "characters",
            "a pack requires at least one character entry",
        ));
    }
    ensure_record_capacity("characters", pack.characters.len())?;
    ensure_strict_order("characters", &pack.characters, |profile| profile.id.clone())?;

    let ids = pack
        .characters
        .iter()
        .map(|profile| profile.id.clone())
        .collect::<BTreeSet<_>>();
    let profiles = pack
        .characters
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();

    for profile in &pack.characters {
        validate_runtime_profile(profile, &ids, &profiles)?;
    }
    Ok(())
}

fn validate_runtime_profile(
    profile: &CharacterRuntimeProfile,
    ids: &BTreeSet<String>,
    profiles: &BTreeMap<&str, &CharacterRuntimeProfile>,
) -> Result<(), CharacterPackError> {
    let path = format!("characters.{}", profile.id);
    validate_namespaced_id(&format!("{path}.id"), &profile.id)?;
    validate_text(
        &format!("{path}.display_name"),
        &profile.display_name,
        1,
        256,
    )?;
    validate_sorted_texts(&format!("{path}.aliases"), &profile.aliases, 1, 256)?;

    for (trait_id, measurement) in &profile.personality {
        let trait_path = format!("{path}.personality.{}", trait_name(*trait_id));
        match measurement {
            TraitMeasurement::Score { score } => {
                if !score.is_finite() || !(0.0..=1.0).contains(score) {
                    return Err(pack_error(
                        CharacterDiagnosticCode::InvalidValue,
                        trait_path,
                        "trait scores must be finite values from 0.0 through 1.0",
                    ));
                }
            }
            TraitMeasurement::Band { .. } => {}
        }
    }
    if let Some(birth_date) = &profile.birth_date {
        validate_birth_date(&format!("{path}.birth_date"), birth_date)?;
    }

    for (id, record) in &profile.inner_life {
        validate_local_id(&format!("{path}.inner_life"), id)?;
        validate_text(
            &format!("{path}.inner_life.{id}.content"),
            &record.content,
            1,
            8_192,
        )?;
    }
    for (id, record) in &profile.voice {
        validate_local_id(&format!("{path}.voice"), id)?;
        validate_text(
            &format!("{path}.voice.{id}.content"),
            &record.content,
            1,
            8_192,
        )?;
    }

    if let Some(presentation) = &profile.presentation {
        validate_runtime_presentation(&format!("{path}.presentation"), presentation)?;
    }
    for (id, role) in &profile.roles {
        let role_path = format!("{path}.roles.{id}");
        validate_local_id(&format!("{path}.roles"), id)?;
        validate_text(&format!("{role_path}.taxonomy"), &role.taxonomy, 1, 256)?;
        validate_text(&format!("{role_path}.entry_id"), &role.entry_id, 1, 256)?;
        validate_text(&format!("{role_path}.label"), &role.label, 1, 256)?;
        if role.coverage_micros > 1_000_000 {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{role_path}.coverage_micros"),
                "role coverage must be at most one million",
            ));
        }
    }
    for (id, alignment) in &profile.alignment {
        let alignment_path = format!("{path}.alignment.{id}");
        validate_local_id(&format!("{path}.alignment"), id)?;
        validate_text(
            &format!("{alignment_path}.label_id"),
            &alignment.label_id,
            1,
            256,
        )?;
        validate_text(&format!("{alignment_path}.label"), &alignment.label, 1, 256)?;
        if alignment.coverage_micros > 1_000_000 {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{alignment_path}.coverage_micros"),
                "alignment coverage must be at most one million",
            ));
        }
    }

    for (id, relationship) in &profile.relationships {
        let relationship_path = format!("{path}.relationships.{id}");
        validate_local_id(&format!("{path}.relationships"), id)?;
        if relationship.source_character_id != profile.id {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{relationship_path}.source_character_id"),
                "relationship source must equal its owning character",
            ));
        }
        let Some(target) = profiles.get(relationship.target_character_id.as_str()) else {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{relationship_path}.target_character_id"),
                "relationship target is absent from the pack",
            ));
        };
        validate_namespaced_id(&format!("{relationship_path}.kind"), &relationship.kind)?;
        if let Some(inverse_id) = &relationship.inverse_edge_id {
            validate_local_id(&format!("{relationship_path}.inverse_edge_id"), inverse_id)?;
            let owner_inverse = profile.relationships.get(inverse_id);
            let peer_inverse = target.relationships.get(inverse_id);
            let valid = owner_inverse
                .into_iter()
                .chain(peer_inverse)
                .any(|inverse| {
                    inverse.source_character_id == relationship.target_character_id
                        && inverse.target_character_id == relationship.source_character_id
                        && inverse.inverse_edge_id.as_deref() == Some(id.as_str())
                });
            if !valid {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{relationship_path}.inverse_edge_id"),
                    "inverse relationship must be an exact reciprocal edge in this profile or the target profile",
                ));
            }
        }
        for (note_id, note) in &relationship.notes {
            validate_local_id(&format!("{relationship_path}.notes"), note_id)?;
            validate_text(
                &format!("{relationship_path}.notes.{note_id}"),
                note,
                1,
                2_048,
            )?;
        }
    }

    if let Some(expression) = &profile.expression {
        validate_runtime_expression(&format!("{path}.expression"), expression, ids)?;
    }
    if let Some(tabletop) = &profile.tabletop {
        validate_runtime_tabletop(&path, profile, tabletop)?;
    }
    for (id, cue) in &profile.date_context {
        let cue_path = format!("{path}.date_context.{id}");
        validate_local_id(&format!("{path}.date_context"), id)?;
        validate_text(&format!("{cue_path}.record_id"), &cue.record_id, 1, 256)?;
        validate_text(&format!("{cue_path}.content"), &cue.content, 1, 8_192)?;
        if !cue.relevance.is_finite() || !(0.0..=1.0).contains(&cue.relevance) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{cue_path}.relevance"),
                "date-context relevance must be a finite value from 0.0 through 1.0",
            ));
        }
    }
    Ok(())
}

fn validate_runtime_presentation(
    path: &str,
    presentation: &RuntimePresentation,
) -> Result<(), CharacterPackError> {
    if let Some(pronouns) = &presentation.pronouns {
        for (field, form) in [
            ("subject", &pronouns.subject),
            ("object", &pronouns.object),
            ("possessive_determiner", &pronouns.possessive_determiner),
            ("possessive_pronoun", &pronouns.possessive_pronoun),
            ("reflexive", &pronouns.reflexive),
        ] {
            validate_text(&format!("{path}.pronouns.{field}"), form, 1, 64)?;
        }
    }
    let mut prior = None;
    for note in &presentation.context_notes {
        validate_local_id(&format!("{path}.context_notes"), &note.id)?;
        if prior.is_some_and(|prior: &str| prior >= note.id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.context_notes"),
                "context notes must be unique and sorted by id",
            ));
        }
        prior = Some(note.id.as_str());
        validate_text(
            &format!("{path}.context_notes.{}.content", note.id),
            &note.content,
            1,
            8_192,
        )?;
    }
    let mut prior = None;
    for appearance in &presentation.appearance {
        validate_local_id(&format!("{path}.appearance"), &appearance.id)?;
        if prior.is_some_and(|prior: &str| prior >= appearance.id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.appearance"),
                "appearance descriptors must be unique and sorted by id",
            ));
        }
        prior = Some(appearance.id.as_str());
        validate_namespaced_id(
            &format!("{path}.appearance.{}.category", appearance.id),
            &appearance.category,
        )?;
        validate_text(
            &format!("{path}.appearance.{}.content", appearance.id),
            &appearance.content,
            1,
            2_048,
        )?;
    }
    if !presentation.palette.is_empty() {
        if presentation.palette.len() > 64 {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.palette"),
                "a palette requires at most 64 named color slots",
            ));
        }
        for (slot, color) in &presentation.palette {
            validate_local_id(&format!("{path}.palette"), slot)?;
            validate_presentation_color(&format!("{path}.palette.{slot}"), color)?;
        }
    }
    validate_sorted_texts(
        &format!("{path}.style_tags"),
        &presentation.style_tags,
        1,
        64,
    )?;
    let mut prior = None;
    for asset in &presentation.assets {
        validate_local_id(&format!("{path}.assets"), &asset.id)?;
        if prior.is_some_and(|prior: &str| prior >= asset.id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.assets"),
                "presentation assets must be unique and sorted by id",
            ));
        }
        prior = Some(asset.id.as_str());
        validate_presentation_asset(&format!("{path}.assets.{}", asset.id), asset)?;
    }
    for (slot, assignment) in &presentation.catalog {
        let catalog_path = format!("{path}.catalog.{slot}");
        validate_local_id(&format!("{path}.catalog"), slot)?;
        validate_namespaced_id(
            &format!("{catalog_path}.catalog_id"),
            &assignment.catalog_id,
        )?;
        validate_semver(
            &format!("{catalog_path}.catalog_version"),
            &assignment.catalog_version,
        )?;
        validate_text(
            &format!("{catalog_path}.entry_id"),
            &assignment.entry_id,
            1,
            256,
        )?;
        validate_catalog_value(&format!("{catalog_path}.value"), &assignment.value)?;
    }
    Ok(())
}

fn validate_catalog_value(
    path: &str,
    value: &PresentationCatalogValue,
) -> Result<(), CharacterPackError> {
    match value {
        PresentationCatalogValue::Appearance {
            category,
            descriptor,
        } => {
            validate_namespaced_id(&format!("{path}.category"), category)?;
            validate_text(&format!("{path}.descriptor"), descriptor, 1, 2_048)
        }
        PresentationCatalogValue::PaletteColor {
            palette_slot,
            color,
        } => {
            validate_local_id(&format!("{path}.palette_slot"), palette_slot)?;
            validate_presentation_color(&format!("{path}.color"), color)
        }
        PresentationCatalogValue::StyleTag { tag } => {
            validate_text(&format!("{path}.tag"), tag, 1, 64)
        }
        PresentationCatalogValue::Asset { asset } => {
            validate_presentation_asset(&format!("{path}.asset"), asset)
        }
    }
}

fn validate_runtime_expression(
    path: &str,
    expression: &RuntimeExpression,
    ids: &BTreeSet<String>,
) -> Result<(), CharacterPackError> {
    for (id, term) in &expression.lexicon {
        let term_path = format!("{path}.lexicon.{id}");
        validate_local_id(&format!("{path}.lexicon"), id)?;
        validate_text(&format!("{term_path}.category"), &term.category, 1, 256)?;
        validate_text(&format!("{term_path}.surface"), &term.surface, 1, 256)?;
        validate_text(&format!("{term_path}.normalized"), &term.normalized, 1, 256)?;
        validate_strength(&format!("{term_path}.strength"), term.strength)?;
        validate_applicability(&format!("{term_path}.applicability"), &term.applicability)?;
    }
    for (id, preference) in &expression.preferences {
        let preference_path = format!("{path}.preferences.{id}");
        validate_local_id(&format!("{path}.preferences"), id)?;
        validate_text(
            &format!("{preference_path}.category"),
            &preference.category,
            1,
            256,
        )?;
        validate_text(
            &format!("{preference_path}.target"),
            &preference.target,
            1,
            512,
        )?;
        validate_strength(&format!("{preference_path}.strength"), preference.strength)?;
        validate_applicability(
            &format!("{preference_path}.applicability"),
            &preference.applicability,
        )?;
    }
    for (id, pool) in &expression.vocabulary_pools {
        let pool_path = format!("{path}.vocabulary_pools.{id}");
        validate_local_id(&format!("{path}.vocabulary_pools"), id)?;
        validate_text(&format!("{pool_path}.category"), &pool.category, 1, 256)?;
        validate_sorted_texts(&format!("{pool_path}.term_ids"), &pool.term_ids, 1, 128)?;
        for term_id in &pool.term_ids {
            if !expression.lexicon.contains_key(term_id) {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{pool_path}.term_ids.{term_id}"),
                    "vocabulary pool references a term that is absent from the runtime pack",
                ));
            }
        }
        validate_applicability(&format!("{pool_path}.applicability"), &pool.applicability)?;
    }
    for (id, constraint) in &expression.voice_constraints {
        let constraint_path = format!("{path}.voice_constraints.{id}");
        validate_local_id(&format!("{path}.voice_constraints"), id)?;
        validate_text(
            &format!("{constraint_path}.category"),
            &constraint.category,
            1,
            256,
        )?;
        validate_text(
            &format!("{constraint_path}.target"),
            &constraint.target,
            1,
            512,
        )?;
        validate_text(
            &format!("{constraint_path}.instruction"),
            &constraint.instruction,
            1,
            2_048,
        )?;
        validate_strength(&format!("{constraint_path}.strength"), constraint.strength)?;
        validate_applicability(
            &format!("{constraint_path}.applicability"),
            &constraint.applicability,
        )?;
    }
    for (id, template) in &expression.dialogue {
        let template_path = format!("{path}.dialogue.{id}");
        validate_local_id(&format!("{path}.dialogue"), id)?;
        validate_text(
            &format!("{template_path}.scenario_id"),
            &template.scenario_id,
            1,
            128,
        )?;
        if let ExpressionSpeakerRequirement::ExactCharacter { character_id } =
            &template.speaker_requirement
            && !ids.contains(character_id)
        {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{template_path}.speaker_requirement"),
                "dialogue speaker character is absent from the pack",
            ));
        }
        for (placeholder_id, placeholder) in &template.placeholders {
            validate_local_id(&format!("{template_path}.placeholders"), placeholder_id)?;
            if let ExpressionPlaceholderValue::LexiconTerm { term_id } = &placeholder.value
                && !expression.lexicon.contains_key(term_id)
            {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{template_path}.placeholders.{placeholder_id}"),
                    "dialogue placeholder references an absent lexicon term",
                ));
            }
        }
        let mut prior = None;
        let mut variant_ids = BTreeSet::new();
        for variant in &template.variants {
            validate_local_id(&format!("{template_path}.variants"), &variant.id)?;
            if prior.is_some_and(|prior: &str| prior >= variant.id.as_str()) {
                return Err(pack_error(
                    CharacterDiagnosticCode::InvalidValue,
                    format!("{template_path}.variants"),
                    "dialogue variants must be unique and sorted by id",
                ));
            }
            prior = Some(variant.id.as_str());
            variant_ids.insert(variant.id.as_str());
            validate_text(
                &format!("{template_path}.variants.{}.content", variant.id),
                &variant.content,
                1,
                8_192,
            )?;
            validate_applicability(
                &format!("{template_path}.variants.{}.applicability", variant.id),
                &variant.applicability,
            )?;
        }
        if !variant_ids.contains(template.fallback_variant_id.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{template_path}.fallback_variant_id"),
                "dialogue fallback variant is absent from the template",
            ));
        }
    }
    for (id, assignment) in &expression.template_assignments {
        let assignment_path = format!("{path}.template_assignments.{id}");
        validate_local_id(&format!("{path}.template_assignments"), id)?;
        validate_text(
            &format!("{assignment_path}.scenario_id"),
            &assignment.scenario_id,
            1,
            128,
        )?;
        validate_text(
            &format!("{assignment_path}.template_id"),
            &assignment.template_id,
            1,
            128,
        )?;
        let Some(template) = expression.dialogue.get(&assignment.template_id) else {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                assignment_path,
                "assigned dialogue template is absent from the runtime pack",
            ));
        };
        if template.scenario_id != assignment.scenario_id {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                assignment_path,
                "assigned dialogue template scenario does not match",
            ));
        }
    }
    Ok(())
}

fn validate_applicability(
    path: &str,
    applicability: &ExpressionApplicability,
) -> Result<(), CharacterPackError> {
    validate_sorted_texts(
        &format!("{path}.scenario_ids"),
        &applicability.scenario_ids,
        1,
        128,
    )?;
    for (index, predicate) in applicability.predicates.iter().enumerate() {
        let predicate_path = format!("{path}.predicates[{index}]");
        match predicate {
            ExpressionContextPredicate::PersonalityBand { bands, .. } => {
                if bands.is_empty() {
                    return Err(pack_error(
                        CharacterDiagnosticCode::InvalidValue,
                        predicate_path,
                        "personality applicability requires at least one band",
                    ));
                }
            }
            ExpressionContextPredicate::Relationship {
                relationship_kind_id,
                other_character_id,
            } => {
                validate_namespaced_id(
                    &format!("{predicate_path}.relationship_kind_id"),
                    relationship_kind_id,
                )?;
                if let Some(other) = other_character_id {
                    validate_namespaced_id(&format!("{predicate_path}.other_character_id"), other)?;
                }
            }
            // Missing optional context makes a predicate inapplicable; it is never an error.
            ExpressionContextPredicate::DateContext { cue_id } => {
                validate_local_id(&format!("{predicate_path}.cue_id"), cue_id)?;
            }
            ExpressionContextPredicate::WorldContext { tag } => {
                validate_text(&format!("{predicate_path}.tag"), tag, 1, 256)?;
            }
        }
    }
    Ok(())
}

fn runtime_profile(
    entry: &CharacterAuthoringEntry,
    pack: &CharacterAuthoringPack,
) -> Result<CharacterRuntimeProfile, CharacterPackError> {
    let profile = &entry.profile;
    let mut personality = BTreeMap::new();
    for trait_id in all_traits() {
        if let Some(value) = trait_measurement(&profile.canon.personality, trait_id)
            && value.freshness == Freshness::Current
            && approved_review(value.review)
        {
            personality.insert(trait_id, value.value);
        }
    }

    let mut inner_life = BTreeMap::new();
    for (id, record) in &profile.canon.inner_life {
        if record.content.freshness == Freshness::Current && approved_review(record.content.review)
        {
            inner_life.insert(
                id.clone(),
                RuntimeInnerLifeRecord {
                    category: record.category,
                    content: record.content.value.clone(),
                },
            );
        }
    }
    let mut voice = BTreeMap::new();
    for (id, record) in &profile.canon.voice {
        if record.content.freshness == Freshness::Current && approved_review(record.content.review)
        {
            voice.insert(
                id.clone(),
                RuntimeVoiceRecord {
                    category: record.category,
                    content: record.content.value.clone(),
                },
            );
        }
    }

    let presentation = presentation_extension(profile)
        .filter(|record| extension_is_runtime_ready(&record.header))
        .map(|record| runtime_presentation(&record.value));

    let mut roles = BTreeMap::new();
    if let Some(record) = role_extension(profile)
        && extension_is_runtime_ready(&record.header)
    {
        for (id, role) in &record.value.roles {
            roles.insert(
                id.clone(),
                RuntimeRole {
                    kind: role.kind,
                    taxonomy: role.taxonomy.clone(),
                    entry_id: role.entry_id.clone(),
                    label: role.label.clone(),
                    decision: role.decision,
                    lossy: role.lossy,
                    score_micros: role.score_micros,
                    coverage_micros: role.coverage_micros,
                },
            );
        }
    }

    let mut alignment = BTreeMap::new();
    if let Some(record) = alignment_extension(profile)
        && extension_is_runtime_ready(&record.header)
    {
        for (id, value) in &record.value.values {
            alignment.insert(
                id.clone(),
                RuntimeAlignment {
                    label_id: value.label_id.clone(),
                    label: value.label.clone(),
                    decision: value.decision,
                    score_micros: value.score_micros,
                    coverage_micros: value.coverage_micros,
                },
            );
        }
    }

    let mut relationships = BTreeMap::new();
    if let Some(record) = relationship_extension(profile)
        && extension_is_runtime_ready(&record.header)
    {
        for (id, edge) in &record.value.edges {
            if !runtime_relationship_ready(edge) {
                continue;
            }
            relationships.insert(
                id.clone(),
                RuntimeRelationship {
                    source_character_id: edge.source_character_id.clone(),
                    target_character_id: edge.target_character_id.clone(),
                    kind: edge.kind.clone(),
                    origin: edge.origin,
                    inverse_edge_id: edge.inverse_edge_id.clone(),
                    notes: edge
                        .notes
                        .iter()
                        .map(|(note_id, note)| (note_id.clone(), note.content.clone()))
                        .collect(),
                },
            );
        }
    }

    let expression = match expression_extension(profile) {
        Some(record) if extension_is_runtime_ready(&record.header) => {
            Some(runtime_expression(&record.value, pack)?)
        }
        _ => None,
    };

    let mut date_context = BTreeMap::new();
    if let Some(record) = date_context_extension(profile)
        && extension_is_runtime_ready(&record.header)
    {
        for (id, cue) in &record.value.accepted_cues {
            date_context.insert(
                id.clone(),
                RuntimeDateCue {
                    record_id: cue.record_id.clone(),
                    kind: cue.kind,
                    content: cue.content.clone(),
                    relevance: cue.relevance,
                    uncertainty: cue.uncertainty,
                },
            );
        }
    }

    Ok(CharacterRuntimeProfile {
        id: profile.id.clone(),
        display_name: profile.canon.identity.display_name.value.clone(),
        aliases: profile
            .canon
            .identity
            .aliases
            .as_ref()
            .map(|aliases| aliases.value.clone())
            .unwrap_or_default(),
        personality,
        birth_date: profile
            .canon
            .birth_date
            .as_ref()
            .filter(|date| date.freshness == Freshness::Current && approved_review(date.review))
            .map(|date| date.value.clone()),
        inner_life,
        voice,
        presentation,
        roles,
        alignment,
        relationships,
        expression,
        date_context,
        tabletop: entry.tabletop.as_ref().map(runtime_tabletop),
    })
}

fn normalize_adapter_manifest(manifest: &mut CharacterPackAdapterManifest) {
    manifest
        .capabilities
        .sort_by_key(|declaration| declaration.capability);
    manifest.creation_step_ids.sort();
    manifest.creation_step_ids.dedup();
}

fn normalize_tabletop_projection(projection: &mut CharacterPackTabletopProjection) {
    normalize_adapter_manifest(&mut projection.manifest);
    projection.active.completed_creation_steps.sort();
    projection.active.completed_creation_steps.dedup();
    for inactive in projection.inactive.values_mut() {
        inactive.completed_creation_steps.sort();
        inactive.completed_creation_steps.dedup();
    }
}

fn normalize_tabletop_active(active_view: &mut CharacterPackActiveTabletop) {
    normalize_adapter_manifest(&mut active_view.manifest);
    active_view.active.completed_creation_steps.sort();
    active_view.active.completed_creation_steps.dedup();
}

fn runtime_tabletop(projection: &CharacterPackTabletopProjection) -> CharacterPackActiveTabletop {
    CharacterPackActiveTabletop {
        projection_format_version: projection.projection_format_version,
        character_id: projection.character_id.clone(),
        canonical_profile_sha256: projection.canonical_profile_sha256.clone(),
        canonical_character_write_back: projection.canonical_character_write_back,
        manifest: projection.manifest.clone(),
        active: projection.active.clone(),
        state: projection.state.clone(),
    }
}

fn runtime_presentation(value: &IdentityPresentation) -> RuntimePresentation {
    let context_notes = value
        .context_notes
        .values()
        .filter(|note| {
            note.content.freshness == Freshness::Current && approved_review(note.content.review)
        })
        .map(|note| RuntimeContextNote {
            id: note.id.clone(),
            kind: note.kind,
            content: note.content.value.clone(),
        })
        .collect();
    let appearance = value
        .appearance
        .values()
        .filter(|record| {
            record.content.freshness == Freshness::Current && approved_review(record.content.review)
        })
        .map(|record| RuntimeAppearance {
            id: record.id.clone(),
            category: record.category.clone(),
            content: record.content.value.clone(),
        })
        .collect();
    let palette = value
        .palette
        .as_ref()
        .filter(|palette| {
            palette.freshness == Freshness::Current && approved_review(palette.review)
        })
        .map(|palette| palette.value.colors.clone())
        .unwrap_or_default();
    let style_tags = value
        .style_tags
        .as_ref()
        .filter(|tags| tags.freshness == Freshness::Current && approved_review(tags.review))
        .map(|tags| tags.value.clone())
        .unwrap_or_default();
    let assets = value
        .assets
        .values()
        .filter(|asset| asset.freshness == Freshness::Current && approved_review(asset.review))
        .map(|asset| asset.value.clone())
        .collect();
    let catalog = value
        .catalog_assignments
        .iter()
        .filter(|(_, assignment)| {
            assignment.freshness == Freshness::Current && approved_review(assignment.review)
        })
        .map(|(slot, assignment)| {
            (
                slot.clone(),
                RuntimeCatalogAssignment {
                    catalog_id: assignment.value.catalog.id.clone(),
                    catalog_version: assignment.value.catalog.version.clone(),
                    entry_id: assignment.value.entry_id.clone(),
                    value: assignment.value.value.clone(),
                },
            )
        })
        .collect();
    RuntimePresentation {
        pronouns: value
            .pronouns
            .as_ref()
            .filter(|pronouns| {
                pronouns.freshness == Freshness::Current && approved_review(pronouns.review)
            })
            .map(|pronouns| pronouns.value.clone()),
        context_notes,
        appearance,
        palette,
        style_tags,
        assets,
        catalog,
    }
}

fn runtime_expression(
    data: &ExpressionData,
    pack: &CharacterAuthoringPack,
) -> Result<RuntimeExpression, CharacterPackError> {
    let mut expression = RuntimeExpression::default();
    for (id, term) in &data.lexicon {
        if approved_review(term.review) {
            expression.lexicon.insert(
                id.clone(),
                RuntimeLexiconTerm {
                    category: term.category.clone(),
                    kind: term.kind,
                    surface: term.surface.clone(),
                    normalized: term.normalized.clone(),
                    strength: term.strength,
                    applicability: term.applicability.clone(),
                },
            );
        }
    }
    for (id, preference) in &data.preferences {
        if approved_review(preference.review) {
            expression.preferences.insert(
                id.clone(),
                RuntimePreference {
                    category: preference.category.clone(),
                    target: preference.target.clone(),
                    polarity: preference.polarity,
                    strength: preference.strength,
                    applicability: preference.applicability.clone(),
                },
            );
        }
    }
    for (id, pool) in &data.vocabulary_pools {
        if approved_review(pool.review) {
            let term_ids = pool
                .term_ids
                .iter()
                .filter(|term_id| expression.lexicon.contains_key(*term_id))
                .cloned()
                .collect::<Vec<_>>();
            if !term_ids.is_empty() {
                expression.vocabulary_pools.insert(
                    id.clone(),
                    RuntimeVocabularyPool {
                        category: pool.category.clone(),
                        term_ids,
                        applicability: pool.applicability.clone(),
                    },
                );
            }
        }
    }
    for (id, constraint) in &data.voice_constraints {
        if approved_review(constraint.review) {
            expression.voice_constraints.insert(
                id.clone(),
                RuntimeVoiceConstraint {
                    category: constraint.category.clone(),
                    medium: constraint.medium,
                    effect: constraint.effect,
                    target: constraint.target.clone(),
                    instruction: constraint.instruction.clone(),
                    strength: constraint.strength,
                    applicability: constraint.applicability.clone(),
                },
            );
        }
    }
    for (id, assignment) in &data.template_assignments {
        if !runtime_assignment_ready(assignment) {
            continue;
        }
        let expression_pack =
            resolve_expression_pack(&pack.expression_packs, &assignment.pack, "expression")?;
        let Some(template) = expression_pack.templates.get(&assignment.template_id) else {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("expression.template_assignments.{id}"),
                "assigned dialogue template is absent from the referenced expression pack",
            ));
        };
        let runtime_template = runtime_dialogue_template(template)?;
        expression
            .dialogue
            .insert(template.id.clone(), runtime_template);
        expression.template_assignments.insert(
            id.clone(),
            RuntimeTemplateAssignment {
                scenario_id: assignment.scenario_id.clone(),
                template_id: assignment.template_id.clone(),
            },
        );
    }
    Ok(expression)
}

fn runtime_dialogue_template(
    template: &ExpressionDialogueTemplate,
) -> Result<RuntimeDialogueTemplate, CharacterPackError> {
    let variants = template
        .variants
        .values()
        .filter(|variant| {
            approved_review(variant.review)
                && variant.authority != ExpressionTextAuthority::Suggested
        })
        .map(|variant| RuntimeDialogueVariant {
            id: variant.id.clone(),
            content: variant.content.clone(),
            priority: variant.priority,
            authority: variant.authority,
            applicability: variant.applicability.clone(),
        })
        .collect();
    let placeholders = template
        .placeholders
        .iter()
        .map(|(id, declaration)| {
            (
                id.clone(),
                RuntimePlaceholderDeclaration {
                    value: declaration.value.clone(),
                    required: declaration.required,
                },
            )
        })
        .collect();
    Ok(RuntimeDialogueTemplate {
        scenario_id: template.scenario_id.clone(),
        speaker_requirement: template.speaker_requirement.clone(),
        placeholders,
        variants,
        fallback_variant_id: template.fallback_variant_id.clone(),
    })
}

fn runtime_relationship_ready(edge: &RelationshipEdge) -> bool {
    edge.freshness == Freshness::Current
        && approved_review(edge.review)
        && matches!(
            edge.origin,
            RelationshipEdgeOrigin::Authored
                | RelationshipEdgeOrigin::Imported
                | RelationshipEdgeOrigin::ReviewedSuggestion
        )
}

fn runtime_assignment_ready(assignment: &ExpressionTemplateAssignment) -> bool {
    approved_review(assignment.review)
        && matches!(
            assignment.state,
            ValueState::Authored
                | ValueState::Imported
                | ValueState::Reviewed
                | ValueState::Overridden
        )
}

fn approved_review(review: ReviewState) -> bool {
    matches!(review, ReviewState::NotRequired | ReviewState::Accepted)
}

fn extension_is_runtime_ready(header: &ExtensionHeader) -> bool {
    header.freshness == Freshness::Current && approved_review(header.review)
}

fn presentation_extension(
    profile: &CharacterProfile,
) -> Option<&VersionedExtension<IdentityPresentation>> {
    match profile
        .extensions
        .get(IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::IdentityPresentation(record)) => Some(record),
        _ => None,
    }
}

fn expression_extension(profile: &CharacterProfile) -> Option<&VersionedExtension<ExpressionData>> {
    match profile.extensions.get(EXPRESSION_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Expression(record)) => Some(record),
        _ => None,
    }
}

fn role_extension(profile: &CharacterProfile) -> Option<&VersionedExtension<RoleProjections>> {
    match profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::RoleProjections(record)) => Some(record),
        _ => None,
    }
}

fn alignment_extension(profile: &CharacterProfile) -> Option<&VersionedExtension<AlignmentView>> {
    match profile.extensions.get(ALIGNMENT_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::AlignmentView(record)) => Some(record),
        _ => None,
    }
}

fn relationship_extension(
    profile: &CharacterProfile,
) -> Option<&VersionedExtension<RelationshipEdges>> {
    match profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Relationships(record)) => Some(record),
        _ => None,
    }
}

fn date_context_extension(profile: &CharacterProfile) -> Option<&VersionedExtension<DateContext>> {
    match profile.extensions.get(DATE_CONTEXT_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::DateContext(record)) => Some(record),
        _ => None,
    }
}

fn validate_health_diagnostic(
    diagnostic: &CharacterHealthDiagnostic,
    character_ids: &BTreeSet<&str>,
) -> Result<(), CharacterPackError> {
    validate_local_id("health_diagnostics", &diagnostic.id)?;
    validate_text(
        &format!("health_diagnostics.{}.path", diagnostic.id),
        &diagnostic.path,
        1,
        512,
    )?;
    validate_text(
        &format!("health_diagnostics.{}.explanation", diagnostic.id),
        &diagnostic.explanation,
        1,
        2_048,
    )?;
    validate_text(
        &format!("health_diagnostics.{}.remediation", diagnostic.id),
        &diagnostic.remediation,
        1,
        2_048,
    )?;
    if let Some(suppressed_by) = &diagnostic.suppressed_by {
        validate_local_id(
            &format!("health_diagnostics.{}.suppressed_by", diagnostic.id),
            suppressed_by,
        )?;
    }
    if let Some(source) = &diagnostic.source {
        validate_text(
            &format!("health_diagnostics.{}.source.document_id", diagnostic.id),
            &source.document_id,
            1,
            256,
        )?;
        validate_relative_path(
            &format!("health_diagnostics.{}.source.file", diagnostic.id),
            &source.file,
        )?;
        if source.line == Some(0) || source.column == Some(0) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("health_diagnostics.{}.source", diagnostic.id),
                "source line and column must be positive when present",
            ));
        }
    }
    let character_id = match &diagnostic.scope {
        CharacterHealthDiagnosticScope::Collection => None,
        CharacterHealthDiagnosticScope::Document { document_id } => {
            validate_text(
                &format!("health_diagnostics.{}.scope.document_id", diagnostic.id),
                document_id,
                1,
                256,
            )?;
            None
        }
        CharacterHealthDiagnosticScope::Character { character_id }
        | CharacterHealthDiagnosticScope::CharacterDocument { character_id, .. } => {
            Some(character_id.as_str())
        }
    };
    if let Some(character_id) = character_id
        && !character_ids.contains(character_id)
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("health_diagnostics.{}.scope", diagnostic.id),
            "health diagnostic references a character that is absent from the pack",
        ));
    }
    Ok(())
}

fn validate_authoring_tabletop(
    path: &str,
    profile: &CharacterProfile,
    projection: Option<&CharacterPackTabletopProjection>,
) -> Result<(), CharacterPackError> {
    let Some(projection) = projection else {
        return Ok(());
    };
    let root = format!("{path}.tabletop");
    if projection.projection_format_version != CHARACTER_PACK_TABLETOP_PROJECTION_FORMAT_VERSION {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{root}.projection_format_version"),
            "unsupported tabletop projection format version; re-export as version 1",
        ));
    }
    if projection.character_id != profile.id {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{root}.character_id"),
            "tabletop projection targets a different character",
        ));
    }
    validate_sha256(
        &format!("{root}.canonical_profile_sha256"),
        &projection.canonical_profile_sha256,
    )?;
    if projection.canonical_profile_sha256 != character_pack_profile_fingerprint(profile)? {
        return Err(pack_error(
            CharacterDiagnosticCode::StaleInput,
            format!("{root}.canonical_profile_sha256"),
            "tabletop projection does not reference the exact canonical profile",
        ));
    }
    if projection.canonical_character_write_back {
        return Err(pack_error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            format!("{root}.canonical_character_write_back"),
            "tabletop projections cannot write canonical Character evidence",
        ));
    }
    validate_provenance(&projection.provenance).map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidLineage,
            format!("{root}.provenance"),
            "adapter provenance is malformed or incomplete",
        )
    })?;
    validate_tabletop_manifest(
        &format!("{root}.manifest"),
        &projection.manifest,
        Some(&projection.provenance),
    )?;
    validate_adapter_definition(
        &format!("{root}.active"),
        &projection.active,
        &projection.manifest,
    )?;
    if let Some(state) = &projection.state {
        validate_adapter_state(
            &format!("{root}.state"),
            state,
            &projection.manifest,
            &projection.active,
            &profile.id,
        )?;
    }
    if projection.inactive.len() > CHARACTER_PACK_MAX_RECORDS {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{root}.inactive"),
            "inactive adapter projections exceed the 4096-record limit",
        ));
    }
    for (key, inactive) in &projection.inactive {
        validate_inactive_tabletop(&format!("{root}.inactive.{key}"), key, inactive, projection)?;
    }
    Ok(())
}

fn validate_runtime_tabletop(
    path: &str,
    profile: &CharacterRuntimeProfile,
    projection: &CharacterPackActiveTabletop,
) -> Result<(), CharacterPackError> {
    let root = format!("{path}.tabletop");
    if projection.projection_format_version != CHARACTER_PACK_TABLETOP_PROJECTION_FORMAT_VERSION {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{root}.projection_format_version"),
            "unsupported tabletop projection format version; re-export as version 1",
        ));
    }
    if projection.character_id != profile.id {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{root}.character_id"),
            "runtime tabletop projection targets a different character",
        ));
    }
    validate_sha256(
        &format!("{root}.canonical_profile_sha256"),
        &projection.canonical_profile_sha256,
    )?;
    if projection.canonical_character_write_back {
        return Err(pack_error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            format!("{root}.canonical_character_write_back"),
            "tabletop projections cannot write canonical Character evidence",
        ));
    }
    validate_tabletop_manifest(&format!("{root}.manifest"), &projection.manifest, None)?;
    validate_adapter_definition(
        &format!("{root}.active"),
        &projection.active,
        &projection.manifest,
    )?;
    if let Some(state) = &projection.state {
        validate_adapter_state(
            &format!("{root}.state"),
            state,
            &projection.manifest,
            &projection.active,
            &profile.id,
        )?;
    }
    Ok(())
}

fn validate_tabletop_manifest(
    path: &str,
    manifest: &CharacterPackAdapterManifest,
    provenance: Option<&Provenance>,
) -> Result<(), CharacterPackError> {
    if manifest.manifest_format_version != CHARACTER_PACK_ADAPTER_MANIFEST_FORMAT_VERSION {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{path}.manifest_format_version"),
            "unsupported tabletop adapter manifest format version; re-export as version 1",
        ));
    }
    validate_adapter_coordinate(&format!("{path}.coordinate"), &manifest.coordinate)?;
    validate_local_id(&format!("{path}.namespace"), &manifest.namespace)?;
    validate_text(&format!("{path}.title"), &manifest.title, 1, 160)?;
    if manifest.capabilities.is_empty() || manifest.capabilities.len() > 64 {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.capabilities"),
            "an adapter manifest requires between one and 64 capabilities",
        ));
    }
    let mut prior = None;
    for (index, declaration) in manifest.capabilities.iter().enumerate() {
        if declaration.version != 1 {
            return Err(pack_error(
                CharacterDiagnosticCode::UnsupportedVersion,
                format!("{path}.capabilities[{index}].version"),
                "unsupported tabletop capability version; re-export as version 1",
            ));
        }
        if prior.is_some_and(|prior| prior >= declaration.capability) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.capabilities"),
                "capabilities must be unique and sorted",
            ));
        }
        prior = Some(declaration.capability);
    }
    validate_sorted_texts(
        &format!("{path}.creation_step_ids"),
        &manifest.creation_step_ids,
        1,
        128,
    )?;
    if manifest.types.len() > 256 {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.types"),
            "an adapter manifest declares too many named types",
        ));
    }
    for key in manifest.types.keys() {
        validate_local_id(&format!("{path}.types"), key)?;
    }
    let empty = empty_provenance();
    validate_pack_license(
        &format!("{path}.license"),
        &manifest.license,
        provenance.unwrap_or(&empty),
    )?;
    if manifest.license.license != "MIT" && manifest.license.notices.is_empty() {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidLineage,
            format!("{path}.license.notices"),
            "separately licensed adapter requires its own licence notice and text",
        ));
    }
    if character_pack_adapter_manifest_fingerprint(manifest)? != manifest.coordinate.content_sha256
    {
        return Err(pack_error(
            CharacterDiagnosticCode::StaleInput,
            format!("{path}.coordinate.content_sha256"),
            "adapter manifest content hash does not match the pinned coordinate",
        ));
    }
    Ok(())
}

fn validate_adapter_coordinate(
    path: &str,
    adapter: &CharacterPackAdapterCoordinate,
) -> Result<(), CharacterPackError> {
    validate_namespaced_id(&format!("{path}.id"), &adapter.id)?;
    validate_semver(&format!("{path}.version"), &adapter.version)?;
    if adapter.schema_version != 1 {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{path}.schema_version"),
            "unsupported tabletop adapter schema version; migrate the adapter projection to schema version 1",
        ));
    }
    validate_sha256(&format!("{path}.content_sha256"), &adapter.content_sha256)
}

fn validate_adapter_definition(
    path: &str,
    definition: &CharacterPackAdapterDefinition,
    manifest: &CharacterPackAdapterManifest,
) -> Result<(), CharacterPackError> {
    validate_adapter_coordinate(&format!("{path}.adapter"), &definition.adapter)?;
    if definition.adapter != manifest.coordinate {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.adapter"),
            "adapter definition coordinate does not match the pinned manifest",
        ));
    }
    validate_sha256(
        &format!("{path}.definition_sha256"),
        &definition.definition_sha256,
    )?;
    if definition.definition_sha256 != character_pack_value_fingerprint(&definition.definition)? {
        return Err(pack_error(
            CharacterDiagnosticCode::StaleInput,
            format!("{path}.definition_sha256"),
            "adapter definition hash does not match its immutable definition",
        ));
    }
    validate_typed_value(
        &format!("{path}.definition"),
        &definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.definition"),
            "adapter definition does not match the pinned definition type",
        )
    })?;
    validate_completed_steps(
        &format!("{path}.completed_creation_steps"),
        &definition.completed_creation_steps,
        &manifest.creation_step_ids,
    )
}

fn validate_adapter_state(
    path: &str,
    state: &CharacterPackTabletopState,
    manifest: &CharacterPackAdapterManifest,
    active: &CharacterPackAdapterDefinition,
    character_id: &str,
) -> Result<(), CharacterPackError> {
    if state.state_format_version != CHARACTER_PACK_TABLETOP_STATE_FORMAT_VERSION {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{path}.state_format_version"),
            "unsupported tabletop state format version; re-export as version 1",
        ));
    }
    validate_adapter_coordinate(&format!("{path}.adapter"), &state.adapter)?;
    if state.adapter != manifest.coordinate {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.adapter"),
            "tabletop state coordinate does not match the pinned manifest",
        ));
    }
    if state.owner_id != character_id {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.owner_id"),
            "tabletop state owner does not match the character",
        ));
    }
    validate_sha256(
        &format!("{path}.definition_sha256"),
        &state.definition_sha256,
    )?;
    if state.definition_sha256 != active.definition_sha256 {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.definition_sha256"),
            "tabletop state does not reference the active definition",
        ));
    }
    if state.entropy_algorithm != CHARACTER_PACK_TABLETOP_ENTROPY_ALGORITHM {
        return Err(pack_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{path}.entropy_algorithm"),
            "unsupported tabletop entropy algorithm",
        ));
    }
    validate_typed_value(
        &format!("{path}.value"),
        &state.value,
        &manifest.state_type,
        &manifest.types,
    )
    .map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.value"),
            "tabletop state does not match the pinned state type",
        )
    })
}

fn validate_inactive_tabletop(
    path: &str,
    key: &str,
    inactive: &CharacterPackInactiveTabletop,
    projection: &CharacterPackTabletopProjection,
) -> Result<(), CharacterPackError> {
    validate_local_id(path, key)?;
    validate_adapter_coordinate(&format!("{path}.adapter"), &inactive.adapter)?;
    if key != character_pack_adapter_namespace_key(&inactive.adapter) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "inactive adapter key must equal its exact coordinate namespace key",
        ));
    }
    if inactive.adapter == projection.active.adapter {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "the active adapter cannot also be archived",
        ));
    }
    validate_sha256(
        &format!("{path}.definition_sha256"),
        &inactive.definition_sha256,
    )?;
    if inactive.definition_sha256 != character_pack_value_fingerprint(&inactive.definition)? {
        return Err(pack_error(
            CharacterDiagnosticCode::StaleInput,
            format!("{path}.definition_sha256"),
            "inactive adapter definition hash does not match its immutable definition",
        ));
    }
    validate_typed_value(
        &format!("{path}.definition"),
        &inactive.definition,
        &projection.manifest.definition_type,
        &projection.manifest.types,
    )
    .map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.definition"),
            "inactive adapter definition does not match the pinned definition type",
        )
    })?;
    validate_completed_steps(
        &format!("{path}.completed_creation_steps"),
        &inactive.completed_creation_steps,
        &projection.manifest.creation_step_ids,
    )?;
    validate_text(&format!("{path}.reason"), &inactive.reason, 1, 2_048)
}

fn validate_completed_steps(
    path: &str,
    steps: &[String],
    declared: &[String],
) -> Result<(), CharacterPackError> {
    validate_sorted_texts(path, steps, 1, 128)?;
    let declared = declared.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if steps.iter().any(|step| !declared.contains(step.as_str())) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "completed creation step is not declared by the adapter manifest",
        ));
    }
    Ok(())
}

fn merge_collection_provenance(
    collection: &CharacterCollection,
) -> Result<Provenance, CharacterPackError> {
    let mut sources: BTreeMap<String, ProvenanceSource> = BTreeMap::new();
    let mut transformations: BTreeMap<String, ProvenanceTransformation> = BTreeMap::new();
    let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for profile in collection.characters.values() {
        for source in &profile.provenance.sources {
            match sources.get(&source.id) {
                Some(existing) if existing != source => {
                    return Err(pack_error(
                        CharacterDiagnosticCode::InvalidLineage,
                        "provenance.sources",
                        "included profiles declare conflicting provenance sources",
                    ));
                }
                Some(_) => {}
                None => {
                    sources.insert(source.id.clone(), source.clone());
                }
            }
        }
        for transformation in &profile.provenance.transformations {
            match transformations.get(&transformation.id) {
                Some(existing) if existing != transformation => {
                    return Err(pack_error(
                        CharacterDiagnosticCode::InvalidLineage,
                        "provenance.transformations",
                        "included profiles declare conflicting transformations",
                    ));
                }
                Some(_) => {}
                None => {
                    transformations.insert(transformation.id.clone(), transformation.clone());
                }
            }
        }
        for (claim, references) in &profile.provenance.claims {
            claims
                .entry(claim.clone())
                .or_default()
                .extend(references.iter().cloned());
        }
    }
    let provenance = Provenance {
        sources: sources.into_values().collect(),
        transformations: transformations.into_values().collect(),
        claims: claims
            .into_iter()
            .map(|(claim, references)| (claim, references.into_iter().collect()))
            .collect(),
    };
    validate_provenance(&provenance).map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "merged collection provenance is malformed or incomplete",
        )
    })?;
    Ok(provenance)
}

fn empty_provenance() -> Provenance {
    Provenance {
        sources: Vec::new(),
        transformations: Vec::new(),
        claims: BTreeMap::new(),
    }
}

fn validate_pack_license(
    path: &str,
    license: &CharacterPackLicense,
    provenance: &Provenance,
) -> Result<(), CharacterPackError> {
    validate_text(&format!("{path}.license"), &license.license, 1, 64)?;
    validate_https(&format!("{path}.license_url"), &license.license_url)?;
    validate_text(
        &format!("{path}.license_text"),
        &license.license_text,
        1,
        MAX_TEXT,
    )?;
    ensure_strict_order(&format!("{path}.notices"), &license.notices, |notice| {
        notice.id.clone()
    })?;
    for notice in &license.notices {
        validate_local_id(&format!("{path}.notices"), &notice.id)?;
        validate_text(
            &format!("{path}.notices.{}.component", notice.id),
            &notice.component,
            1,
            256,
        )?;
        validate_text(
            &format!("{path}.notices.{}.license", notice.id),
            &notice.license,
            1,
            64,
        )?;
        validate_https(
            &format!("{path}.notices.{}.license_url", notice.id),
            &notice.license_url,
        )?;
        validate_text(
            &format!("{path}.notices.{}.text", notice.id),
            &notice.text,
            1,
            MAX_TEXT,
        )?;
    }
    for source in &provenance.sources {
        if source.license == "MIT" {
            continue;
        }
        let covered = license.notices.iter().any(|notice| {
            notice.component == source.id
                && notice.license == source.license
                && !notice.text.is_empty()
        });
        if !covered {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("{path}.notices"),
                "separately licensed source requires its own licence notice and text",
            ));
        }
    }
    Ok(())
}

fn validate_presentation_asset(
    path: &str,
    asset: &PresentationAssetReference,
) -> Result<(), CharacterPackError> {
    validate_local_id(&format!("{path}.id"), &asset.id)?;
    validate_relative_path(&format!("{path}.path"), &asset.path)?;
    validate_text(&format!("{path}.media_type"), &asset.media_type, 3, 128)?;
    if asset.media_type.contains(char::is_whitespace) || asset.media_type.matches('/').count() != 1
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.media_type"),
            "asset media type must be one portable type/subtype token",
        ));
    }
    if let Some(sha256) = &asset.sha256 {
        validate_sha256(&format!("{path}.sha256"), sha256)?;
    }
    validate_text(&format!("{path}.alt_text"), &asset.alt_text, 1, 512)
}

fn validate_presentation_color(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if !matches!(value.len(), 7 | 9)
        || !value.starts_with('#')
        || !value[1..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_lowercase())
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "colors use canonical uppercase #RRGGBB or #RRGGBBAA notation",
        ));
    }
    Ok(())
}

fn validate_birth_date(path: &str, value: &BirthDate) -> Result<(), CharacterPackError> {
    match *value {
        BirthDate::Year { year, .. } => validate_year(path, year),
        BirthDate::MonthDay { month, day, .. } => validate_month_day(path, None, month, day),
        BirthDate::Full {
            year, month, day, ..
        } => {
            validate_year(path, year)?;
            validate_month_day(path, Some(year), month, day)
        }
    }
}

fn validate_year(path: &str, year: i32) -> Result<(), CharacterPackError> {
    if !(1..=9999).contains(&year) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.year"),
            "year must be from 1 through 9999",
        ));
    }
    Ok(())
}

fn validate_month_day(
    path: &str,
    year: Option<i32>,
    month: u8,
    day: u8,
) -> Result<(), CharacterPackError> {
    let leap = year.is_none_or(is_leap_year);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if days == 0 || day == 0 || day > days {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "date contains an invalid month or day",
        ));
    }
    Ok(())
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn validate_strength(path: &str, value: f64) -> Result<(), CharacterPackError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "strength must be a finite value from 0.0 through 1.0",
        ));
    }
    Ok(())
}

fn validate_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterPackError> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) || value.contains('\0') {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "text length or contents are invalid",
        ));
    }
    Ok(())
}

fn validate_sorted_texts(
    path: &str,
    values: &[String],
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterPackError> {
    if values.len() > 4_096 {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "too many values",
        ));
    }
    let mut prior = None;
    for value in values {
        validate_text(path, value, minimum, maximum)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(pack_error(
                CharacterDiagnosticCode::InvalidValue,
                path,
                "values must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_namespaced_id(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if value.len() > 256
        || value.split('.').count() < 2
        || value.split('.').any(|part| !valid_local_id(part))
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated lowercase stable identifier",
        ));
    }
    Ok(())
}

fn validate_local_id(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if !valid_local_id(value) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a lowercase stable identifier",
        ));
    }
    Ok(())
}

fn valid_local_id(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && value.len() <= 128
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_semver(path: &str, value: &str) -> Result<(), CharacterPackError> {
    Version::parse(value).map(|_| ()).map_err(|_| {
        pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "expected a semantic version",
        )
    })
}

fn validate_sha256(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "expected a lowercase SHA-256",
        ));
    }
    Ok(())
}

fn validate_https(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if !value.starts_with("https://") || value.len() > 512 || value.contains(char::is_whitespace) {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "expected a public HTTPS URL",
        ));
    }
    Ok(())
}

fn validate_relative_path(path: &str, value: &str) -> Result<(), CharacterPackError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(pack_error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "presentation references must be safe project-relative paths without drive letters, schemes, or control characters",
        ));
    }
    Ok(())
}

fn ensure_strict_order<T, K: Ord>(
    path: &str,
    values: &[T],
    key: impl Fn(&T) -> K,
) -> Result<(), CharacterPackError> {
    let mut prior: Option<K> = None;
    for value in values {
        let current = key(value);
        if prior.as_ref().is_some_and(|prior| *prior >= current) {
            return Err(pack_error(
                CharacterDiagnosticCode::ConflictingOverlay,
                path,
                "entries must have unique identifiers in deterministic order",
            ));
        }
        prior = Some(current);
    }
    Ok(())
}

fn template_coordinate(template: &CharacterTemplate) -> String {
    format!("{}@{}", template.id, version_key(&template.version))
}

fn template_order_key(template: &CharacterTemplate) -> (String, Version, String) {
    (
        template.id.clone(),
        version_key(&template.version),
        template.version.clone(),
    )
}

fn expression_pack_order_key(pack: &ExpressionPack) -> (String, Version, String) {
    (
        pack.id.clone(),
        version_key(&pack.version),
        pack.version.clone(),
    )
}

fn version_key(version: &str) -> Version {
    Version::parse(version).unwrap_or_else(|_| Version::new(u64::MAX, 0, 0))
}

fn all_traits() -> [HexacoTrait; 30] {
    [
        HexacoTrait::HonestyHumility,
        HexacoTrait::Sincerity,
        HexacoTrait::Fairness,
        HexacoTrait::GreedAvoidance,
        HexacoTrait::Modesty,
        HexacoTrait::Emotionality,
        HexacoTrait::Fearfulness,
        HexacoTrait::Anxiety,
        HexacoTrait::Dependence,
        HexacoTrait::Sentimentality,
        HexacoTrait::Extraversion,
        HexacoTrait::SocialSelfEsteem,
        HexacoTrait::SocialBoldness,
        HexacoTrait::Sociability,
        HexacoTrait::Liveliness,
        HexacoTrait::Agreeableness,
        HexacoTrait::Forgivingness,
        HexacoTrait::Gentleness,
        HexacoTrait::Flexibility,
        HexacoTrait::Patience,
        HexacoTrait::Conscientiousness,
        HexacoTrait::Organization,
        HexacoTrait::Diligence,
        HexacoTrait::Perfectionism,
        HexacoTrait::Prudence,
        HexacoTrait::Openness,
        HexacoTrait::AestheticAppreciation,
        HexacoTrait::Inquisitiveness,
        HexacoTrait::Creativity,
        HexacoTrait::Unconventionality,
    ]
}

fn trait_measurement(
    profile: &HexacoProfile,
    trait_id: HexacoTrait,
) -> Option<&Attributed<TraitMeasurement>> {
    match trait_id {
        HexacoTrait::HonestyHumility => profile.honesty_humility.factor.as_ref(),
        HexacoTrait::Sincerity => profile.honesty_humility.sincerity.as_ref(),
        HexacoTrait::Fairness => profile.honesty_humility.fairness.as_ref(),
        HexacoTrait::GreedAvoidance => profile.honesty_humility.greed_avoidance.as_ref(),
        HexacoTrait::Modesty => profile.honesty_humility.modesty.as_ref(),
        HexacoTrait::Emotionality => profile.emotionality.factor.as_ref(),
        HexacoTrait::Fearfulness => profile.emotionality.fearfulness.as_ref(),
        HexacoTrait::Anxiety => profile.emotionality.anxiety.as_ref(),
        HexacoTrait::Dependence => profile.emotionality.dependence.as_ref(),
        HexacoTrait::Sentimentality => profile.emotionality.sentimentality.as_ref(),
        HexacoTrait::Extraversion => profile.extraversion.factor.as_ref(),
        HexacoTrait::SocialSelfEsteem => profile.extraversion.social_self_esteem.as_ref(),
        HexacoTrait::SocialBoldness => profile.extraversion.social_boldness.as_ref(),
        HexacoTrait::Sociability => profile.extraversion.sociability.as_ref(),
        HexacoTrait::Liveliness => profile.extraversion.liveliness.as_ref(),
        HexacoTrait::Agreeableness => profile.agreeableness.factor.as_ref(),
        HexacoTrait::Forgivingness => profile.agreeableness.forgivingness.as_ref(),
        HexacoTrait::Gentleness => profile.agreeableness.gentleness.as_ref(),
        HexacoTrait::Flexibility => profile.agreeableness.flexibility.as_ref(),
        HexacoTrait::Patience => profile.agreeableness.patience.as_ref(),
        HexacoTrait::Conscientiousness => profile.conscientiousness.factor.as_ref(),
        HexacoTrait::Organization => profile.conscientiousness.organization.as_ref(),
        HexacoTrait::Diligence => profile.conscientiousness.diligence.as_ref(),
        HexacoTrait::Perfectionism => profile.conscientiousness.perfectionism.as_ref(),
        HexacoTrait::Prudence => profile.conscientiousness.prudence.as_ref(),
        HexacoTrait::Openness => profile.openness.factor.as_ref(),
        HexacoTrait::AestheticAppreciation => profile.openness.aesthetic_appreciation.as_ref(),
        HexacoTrait::Inquisitiveness => profile.openness.inquisitiveness.as_ref(),
        HexacoTrait::Creativity => profile.openness.creativity.as_ref(),
        HexacoTrait::Unconventionality => profile.openness.unconventionality.as_ref(),
    }
}

fn trait_name(trait_id: HexacoTrait) -> &'static str {
    match trait_id {
        HexacoTrait::HonestyHumility => "honesty_humility",
        HexacoTrait::Sincerity => "sincerity",
        HexacoTrait::Fairness => "fairness",
        HexacoTrait::GreedAvoidance => "greed_avoidance",
        HexacoTrait::Modesty => "modesty",
        HexacoTrait::Emotionality => "emotionality",
        HexacoTrait::Fearfulness => "fearfulness",
        HexacoTrait::Anxiety => "anxiety",
        HexacoTrait::Dependence => "dependence",
        HexacoTrait::Sentimentality => "sentimentality",
        HexacoTrait::Extraversion => "extraversion",
        HexacoTrait::SocialSelfEsteem => "social_self_esteem",
        HexacoTrait::SocialBoldness => "social_boldness",
        HexacoTrait::Sociability => "sociability",
        HexacoTrait::Liveliness => "liveliness",
        HexacoTrait::Agreeableness => "agreeableness",
        HexacoTrait::Forgivingness => "forgivingness",
        HexacoTrait::Gentleness => "gentleness",
        HexacoTrait::Flexibility => "flexibility",
        HexacoTrait::Patience => "patience",
        HexacoTrait::Conscientiousness => "conscientiousness",
        HexacoTrait::Organization => "organization",
        HexacoTrait::Diligence => "diligence",
        HexacoTrait::Perfectionism => "perfectionism",
        HexacoTrait::Prudence => "prudence",
        HexacoTrait::Openness => "openness",
        HexacoTrait::AestheticAppreciation => "aesthetic_appreciation",
        HexacoTrait::Inquisitiveness => "inquisitiveness",
        HexacoTrait::Creativity => "creativity",
        HexacoTrait::Unconventionality => "unconventionality",
    }
}
