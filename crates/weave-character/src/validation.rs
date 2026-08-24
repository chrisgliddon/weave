use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use semver::Version;
use weave_domain::{DomainValue, Provenance, validate_provenance};

use crate::expression::{
    validate_behavioral_signature_data, validate_expression_data, validate_expression_profile_links,
};
use crate::model::{
    AlignmentPackRef, AlignmentView, AppearanceDescriptor, Attributed, AuthoredNote, BirthDate,
    CHARACTER_OVERLAY_FORMAT_VERSION, CHARACTER_PROFILE_FORMAT_VERSION,
    CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar, CharacterDiagnostic, CharacterDiagnosticCode,
    CharacterExtension, CharacterOperation, CharacterOperationAction, CharacterOverlay,
    CharacterProfile, CharacterSuggestion, CharacterTemplate, Confidence, DateContext,
    DateContextPackRef, DiagnosticSeverity, ExtensionHeader, ExtensionWriteBack, Freshness,
    HexacoProfile, HexacoTrait, IdentityContextNote, IdentityPresentation, OceanView,
    OpaqueExtensionData, OpaqueInterpretation, PresentationAssetReference,
    PresentationCatalogAssignment, PresentationCatalogRef, PresentationCatalogValue,
    PresentationPalette, PronounSet, RelationshipConsent, RelationshipConsentState,
    RelationshipDate, RelationshipEdge, RelationshipEdgeOrigin, RelationshipEdges,
    RelationshipKindPackRef, ReviewState, TraitMeasurement, ValueState, VersionedExtension,
    VoiceDirection, relationship_graph_format_version,
};
use crate::projection::validate_role_projections_data;

const MAX_TEXT: usize = 65_536;
const MAX_COLLECTION: usize = 65_536;
const MAX_DOMAIN_DEPTH: usize = 32;
const MAX_DOMAIN_NODES: usize = 262_144;
const OCEAN_ALGORITHM: &str = "weave_hexaco_to_ocean_compatibility";
const OCEAN_ALGORITHM_VERSION: u32 = 1;

/// Redaction-safe Character contract failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterError {
    diagnostic: CharacterDiagnostic,
}

impl CharacterError {
    /// Typed diagnostic usable by source tooling, the editor, and command-line callers.
    #[must_use]
    pub const fn diagnostic(&self) -> &CharacterDiagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for CharacterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Character contract {:?} at `{}`: {}",
            self.diagnostic.code, self.diagnostic.path, self.diagnostic.message
        )
    }
}

impl std::error::Error for CharacterError {}

/// Validate one complete portable Character Profile.
pub fn validate_profile(profile: &CharacterProfile) -> Result<(), CharacterError> {
    if profile.profile_format_version != CHARACTER_PROFILE_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "profile_format_version",
            "unsupported Character Profile format version",
        ));
    }
    validate_namespaced_id("id", &profile.id)?;
    validate_profile_provenance(&profile.provenance)?;
    let lineage = lineage_ids(&profile.provenance);

    validate_attributed_text(
        "canon.identity.display_name",
        &profile.canon.identity.display_name,
        &lineage,
        CanonicalPolicy::Canon,
        1,
        256,
    )?;
    if let Some(aliases) = &profile.canon.identity.aliases {
        validate_attributed(
            "canon.identity.aliases",
            aliases,
            &lineage,
            CanonicalPolicy::Canon,
        )?;
        validate_sorted_texts("canon.identity.aliases.value", &aliases.value, 1, 256)?;
    }
    if let Some(date) = &profile.canon.birth_date {
        validate_attributed("canon.birth_date", date, &lineage, CanonicalPolicy::Canon)?;
        validate_birth_date("canon.birth_date.value", &date.value)?;
    }
    validate_hexaco(&profile.canon.personality, &lineage)?;
    validate_authored_notes("canon.inner_life", &profile.canon.inner_life, &lineage)?;
    validate_voice("canon.voice", &profile.canon.voice, &lineage)?;

    if profile.extensions.len() > 4_096 {
        return Err(invalid_value("extensions", "too many extension records"));
    }
    for (namespace, extension) in &profile.extensions {
        validate_extension(namespace, extension, &profile.id, &lineage)?;
    }
    validate_expression_profile_links(profile)?;
    if profile.suggestions.len() > 16_384 {
        return Err(invalid_value("suggestions", "too many suggestions"));
    }
    for (id, suggestion) in &profile.suggestions {
        let path = format!("suggestions.{id}");
        validate_suggestion(&path, id, suggestion, &lineage)?;
    }

    let expected = derive_ocean(&profile.canon.personality);
    if profile.derived.ocean != expected {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "derived.ocean",
            "derived OCEAN view does not match canonical HEXACO inputs",
        ));
    }
    Ok(())
}

/// Validate one immutable Character template.
pub fn validate_template(template: &CharacterTemplate) -> Result<(), CharacterError> {
    if template.template_format_version != CHARACTER_TEMPLATE_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "template_format_version",
            "unsupported Character template format version",
        ));
    }
    validate_namespaced_id("id", &template.id)?;
    validate_semver("version", &template.version)?;
    validate_profile(&template.profile)
}

/// Validate one sparse overlay without applying it.
pub fn validate_overlay(overlay: &CharacterOverlay) -> Result<(), CharacterError> {
    if overlay.overlay_format_version != CHARACTER_OVERLAY_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "overlay_format_version",
            "unsupported Character overlay format version",
        ));
    }
    validate_namespaced_id("id", &overlay.id)?;
    validate_namespaced_id("character_id", &overlay.character_id)?;
    if let Some(template) = &overlay.template {
        validate_namespaced_id("template.id", &template.id)?;
        validate_semver("template.version", &template.version)?;
        validate_sha256("template.sha256", &template.sha256)?;
    }
    validate_profile_provenance(&overlay.provenance)?;
    let lineage = lineage_ids(&overlay.provenance);
    if overlay.operations.len() > 16_384 {
        return Err(invalid_value("operations", "too many overlay operations"));
    }
    let mut prior: Option<(String, &str)> = None;
    let mut operation_ids = BTreeSet::new();
    for (index, operation) in overlay.operations.iter().enumerate() {
        let path = format!("operations[{index}]");
        validate_local_id(&format!("{path}.id"), &operation.id)?;
        if !operation_ids.insert(operation.id.as_str()) {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                format!("{path}.id"),
                "overlay operation identifiers must be unique",
            ));
        }
        validate_text(&format!("{path}.rationale"), &operation.rationale, 1, 2_048)?;
        if let Some(expected) = &operation.expected_prior_sha256 {
            validate_sha256(&format!("{path}.expected_prior_sha256"), expected)?;
        }
        validate_operation(&path, operation, &overlay.character_id, &lineage)?;
        let target = operation_target(&operation.action);
        if prior.as_ref().is_some_and(|(prior_target, prior_id)| {
            (prior_target.as_str(), *prior_id) >= (target.as_str(), operation.id.as_str())
        }) {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                "operations",
                "overlay operations must be uniquely ordered by target path and operation id",
            ));
        }
        if prior
            .as_ref()
            .is_some_and(|(prior_target, _)| prior_target == &target)
        {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                "operations",
                "an overlay cannot target one field more than once",
            ));
        }
        prior = Some((target, &operation.id));
    }
    Ok(())
}

/// Recompute the non-authoritative derived views in place.
pub fn recompute_derived(profile: &mut CharacterProfile) {
    profile.derived.ocean = derive_ocean(&profile.canon.personality);
}

/// Derive the exact v1 lossy OCEAN compatibility view from canonical HEXACO.
#[must_use]
pub fn derive_ocean(profile: &HexacoProfile) -> OceanView {
    OceanView {
        algorithm: OCEAN_ALGORITHM.to_owned(),
        algorithm_version: OCEAN_ALGORITHM_VERSION,
        lossy: true,
        independent_evidence: false,
        openness: factor_projection(profile, HexacoTrait::Openness),
        conscientiousness: factor_projection(profile, HexacoTrait::Conscientiousness),
        extraversion: factor_projection(profile, HexacoTrait::Extraversion),
        agreeableness: factor_projection(profile, HexacoTrait::Agreeableness),
        neuroticism: factor_projection(profile, HexacoTrait::Emotionality),
        omitted_factor: HexacoTrait::HonestyHumility,
    }
}

fn factor_projection(
    profile: &HexacoProfile,
    factor: HexacoTrait,
) -> Option<crate::model::DerivedTrait> {
    if let Some(value) = trait_value(profile, factor) {
        return Some(crate::model::DerivedTrait {
            score: measurement_score(value.value),
            confidence: value.confidence,
            input_paths: vec![trait_path(factor)],
        });
    }
    let facets = factor_facets(factor);
    let values = facets
        .iter()
        .map(|facet| trait_value(profile, *facet))
        .collect::<Option<Vec<_>>>()?;
    let score = values
        .iter()
        .map(|value| measurement_score(value.value))
        .sum::<f64>()
        / values.len() as f64;
    Some(crate::model::DerivedTrait {
        score: round_projection(score),
        confidence: values
            .iter()
            .map(|value| value.confidence)
            .min()
            .unwrap_or(Confidence::Unknown),
        input_paths: facets.iter().map(|facet| trait_path(*facet)).collect(),
    })
}

fn measurement_score(measurement: TraitMeasurement) -> f64 {
    match measurement {
        TraitMeasurement::Score { score } => round_projection(score),
        TraitMeasurement::Band { band } => band.projection_anchor(),
    }
}

fn round_projection(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn validate_hexaco(
    profile: &HexacoProfile,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    for trait_id in ALL_HEXACO_TRAITS {
        if let Some(value) = trait_value(profile, trait_id) {
            let path = trait_path(trait_id);
            validate_attributed(&path, value, lineage, CanonicalPolicy::Personality)?;
            if let TraitMeasurement::Score { score } = value.value
                && (!score.is_finite() || !(0.0..=1.0).contains(&score))
            {
                return Err(invalid_value(
                    format!("{path}.value.score"),
                    "trait scores must be finite values from 0.0 through 1.0",
                ));
            }
        }
    }
    Ok(())
}

fn validate_authored_notes(
    root: &str,
    records: &BTreeMap<String, AuthoredNote>,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if records.len() > 4_096 {
        return Err(invalid_value(root, "too many authored inner-life records"));
    }
    for (id, record) in records {
        let path = format!("{root}.{id}");
        validate_local_id(&path, id)?;
        if record.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "record identifier must equal its containing map key",
            ));
        }
        validate_attributed_text(
            &format!("{path}.content"),
            &record.content,
            lineage,
            CanonicalPolicy::Canon,
            1,
            8_192,
        )?;
    }
    Ok(())
}

fn validate_voice(
    root: &str,
    records: &BTreeMap<String, VoiceDirection>,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if records.len() > 4_096 {
        return Err(invalid_value(root, "too many authored voice records"));
    }
    for (id, record) in records {
        let path = format!("{root}.{id}");
        validate_local_id(&path, id)?;
        if record.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "record identifier must equal its containing map key",
            ));
        }
        validate_attributed_text(
            &format!("{path}.content"),
            &record.content,
            lineage,
            CanonicalPolicy::Canon,
            1,
            8_192,
        )?;
    }
    Ok(())
}

fn validate_suggestion(
    path: &str,
    map_id: &str,
    suggestion: &CharacterSuggestion,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    validate_local_id(&format!("{path}.id"), &suggestion.id)?;
    if suggestion.id != map_id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.id"),
            "suggestion identifier must equal its containing map key",
        ));
    }
    validate_profile_path(&format!("{path}.target_path"), &suggestion.target_path)?;
    validate_attributed(
        &format!("{path}.proposal"),
        &suggestion.proposal,
        lineage,
        CanonicalPolicy::Suggestion,
    )?;
    validate_domain_value(
        &format!("{path}.proposal.value"),
        &suggestion.proposal.value,
    )
}

fn validate_extension(
    namespace: &str,
    extension: &CharacterExtension,
    profile_id: &str,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    validate_namespaced_id("extensions.namespace", namespace)?;
    match extension {
        CharacterExtension::IdentityPresentation(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_identity_presentation(namespace, &record.value, lineage)
        }
        CharacterExtension::Expression(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_expression_data(namespace, profile_id, &record.value, lineage)
        }
        CharacterExtension::BehavioralSignatures(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_behavioral_signature_data(namespace, profile_id, &record.value, lineage)
        }
        CharacterExtension::RoleProjections(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_role_projections_data(namespace, profile_id, &record.value)
        }
        CharacterExtension::Relationships(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_relationships(namespace, profile_id, &record.value, lineage)
        }
        CharacterExtension::AlignmentView(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_alignment_view(namespace, &record.value, &record.header.lineage)
        }
        CharacterExtension::DateContext(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_date_context(namespace, &record.value, lineage, &record.header.lineage)
        }
        CharacterExtension::Tabletop(record) | CharacterExtension::Opaque(record) => {
            validate_extension_record(namespace, record, lineage, true)?;
            validate_opaque(namespace, &record.value)
        }
    }
}

fn validate_extension_record<T>(
    map_namespace: &str,
    record: &VersionedExtension<T>,
    lineage: &BTreeSet<&str>,
    allow_unknown_version: bool,
) -> Result<(), CharacterError> {
    let path = format!("extensions.{map_namespace}.header");
    validate_extension_header(&path, &record.header, lineage)?;
    if record.header.namespace != map_namespace {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.namespace"),
            "extension namespace must equal its containing map key",
        ));
    }
    if record.header.extension_version == 0
        || (!allow_unknown_version && record.header.extension_version != 1)
    {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{path}.extension_version"),
            "unsupported typed extension version",
        ));
    }
    Ok(())
}

fn validate_extension_header(
    path: &str,
    header: &ExtensionHeader,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.namespace"), &header.namespace)?;
    validate_text(&format!("{path}.authority"), &header.authority, 1, 512)?;
    validate_text(&format!("{path}.rationale"), &header.rationale, 1, 2_048)?;
    validate_lineage(&format!("{path}.lineage"), &header.lineage, lineage)?;
    if header.canonical_personality_write_back != ExtensionWriteBack::Forbidden {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            format!("{path}.canonical_personality_write_back"),
            "extensions cannot write back into canonical personality evidence",
        ));
    }
    validate_state(path, header.state, header.review, header.freshness, true)
}

fn validate_identity_presentation(
    namespace: &str,
    value: &IdentityPresentation,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    validate_sorted_namespaced_refs(
        &format!("extensions.{namespace}.value.identity_refs"),
        &value.identity_refs,
    )?;
    validate_sorted_relative_paths(
        &format!("extensions.{namespace}.value.presentation_refs"),
        &value.presentation_refs,
    )?;
    let root = format!("extensions.{namespace}.value");
    if let Some(pronouns) = &value.pronouns {
        validate_attributed(
            &format!("{root}.pronouns"),
            pronouns,
            lineage,
            CanonicalPolicy::Presentation,
        )?;
        validate_pronouns(&format!("{root}.pronouns.value"), &pronouns.value)?;
    }
    validate_identity_context_notes(
        &format!("{root}.context_notes"),
        &value.context_notes,
        lineage,
    )?;
    validate_appearance_descriptors(&format!("{root}.appearance"), &value.appearance, lineage)?;
    if let Some(palette) = &value.palette {
        validate_attributed(
            &format!("{root}.palette"),
            palette,
            lineage,
            CanonicalPolicy::Presentation,
        )?;
        validate_presentation_palette(&format!("{root}.palette.value"), &palette.value)?;
    }
    if let Some(tags) = &value.style_tags {
        validate_attributed(
            &format!("{root}.style_tags"),
            tags,
            lineage,
            CanonicalPolicy::Presentation,
        )?;
        validate_sorted_local_ids(&format!("{root}.style_tags.value"), &tags.value)?;
    }
    if value.assets.len() > 4_096 {
        return Err(invalid_value(
            format!("{root}.assets"),
            "too many presentation assets",
        ));
    }
    for (id, asset) in &value.assets {
        let path = format!("{root}.assets.{id}");
        validate_local_id(&path, id)?;
        validate_attributed(&path, asset, lineage, CanonicalPolicy::Presentation)?;
        if asset.value.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.value.id"),
                "asset identifier must equal its containing map key",
            ));
        }
        validate_presentation_asset_reference(&format!("{path}.value"), &asset.value)?;
    }
    if value.catalog_assignments.len() > 4_096 {
        return Err(invalid_value(
            format!("{root}.catalog_assignments"),
            "too many presentation assignments",
        ));
    }
    for (slot_id, assignment) in &value.catalog_assignments {
        let path = format!("{root}.catalog_assignments.{slot_id}");
        validate_local_id(&path, slot_id)?;
        validate_attributed(&path, assignment, lineage, CanonicalPolicy::Presentation)?;
        if assignment.value.slot_id != *slot_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.value.slot_id"),
                "assignment slot must equal its containing map key",
            ));
        }
        validate_presentation_catalog_assignment(&format!("{path}.value"), &assignment.value)?;
    }
    Ok(())
}

fn validate_pronouns(path: &str, value: &PronounSet) -> Result<(), CharacterError> {
    for (field, form) in [
        ("subject", &value.subject),
        ("object", &value.object),
        ("possessive_determiner", &value.possessive_determiner),
        ("possessive_pronoun", &value.possessive_pronoun),
        ("reflexive", &value.reflexive),
    ] {
        validate_text(&format!("{path}.{field}"), form, 1, 64)?;
    }
    Ok(())
}

fn validate_identity_context_notes(
    root: &str,
    records: &BTreeMap<String, IdentityContextNote>,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if records.len() > 4_096 {
        return Err(invalid_value(root, "too many identity context notes"));
    }
    for (id, record) in records {
        let path = format!("{root}.{id}");
        validate_local_id(&path, id)?;
        if record.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "identity context note id must equal its containing map key",
            ));
        }
        validate_attributed_text(
            &format!("{path}.content"),
            &record.content,
            lineage,
            CanonicalPolicy::Presentation,
            1,
            8_192,
        )?;
    }
    Ok(())
}

fn validate_appearance_descriptors(
    root: &str,
    records: &BTreeMap<String, AppearanceDescriptor>,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if records.len() > 4_096 {
        return Err(invalid_value(root, "too many appearance descriptors"));
    }
    for (id, record) in records {
        let path = format!("{root}.{id}");
        validate_local_id(&path, id)?;
        if record.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "appearance descriptor id must equal its containing map key",
            ));
        }
        validate_namespaced_id(&format!("{path}.category"), &record.category)?;
        validate_attributed_text(
            &format!("{path}.content"),
            &record.content,
            lineage,
            CanonicalPolicy::Presentation,
            1,
            2_048,
        )?;
    }
    Ok(())
}

pub(crate) fn validate_presentation_palette(
    path: &str,
    value: &PresentationPalette,
) -> Result<(), CharacterError> {
    if value.colors.is_empty() || value.colors.len() > 64 {
        return Err(invalid_value(
            format!("{path}.colors"),
            "a palette requires one through 64 named color slots",
        ));
    }
    for (slot, color) in &value.colors {
        validate_local_id(&format!("{path}.colors.{slot}"), slot)?;
        validate_presentation_color(&format!("{path}.colors.{slot}"), color)?;
    }
    Ok(())
}

pub(crate) fn validate_presentation_color(path: &str, value: &str) -> Result<(), CharacterError> {
    if !matches!(value.len(), 7 | 9)
        || !value.starts_with('#')
        || !value[1..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_lowercase())
    {
        return Err(invalid_value(
            path,
            "colors use canonical uppercase #RRGGBB or #RRGGBBAA notation",
        ));
    }
    Ok(())
}

pub(crate) fn validate_presentation_asset_reference(
    path: &str,
    value: &PresentationAssetReference,
) -> Result<(), CharacterError> {
    validate_local_id(&format!("{path}.id"), &value.id)?;
    validate_relative_path(&format!("{path}.path"), &value.path)?;
    validate_text(&format!("{path}.media_type"), &value.media_type, 3, 128)?;
    if value.media_type.contains(char::is_whitespace) || value.media_type.matches('/').count() != 1
    {
        return Err(invalid_value(
            format!("{path}.media_type"),
            "asset media type must be one portable type/subtype token",
        ));
    }
    if let Some(sha256) = &value.sha256 {
        validate_sha256(&format!("{path}.sha256"), sha256)?;
    }
    validate_text(&format!("{path}.alt_text"), &value.alt_text, 1, 512)
}

pub(crate) fn validate_presentation_catalog_ref(
    path: &str,
    value: &PresentationCatalogRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

pub(crate) fn validate_presentation_catalog_value(
    path: &str,
    value: &PresentationCatalogValue,
) -> Result<(), CharacterError> {
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
            validate_local_id(&format!("{path}.tag"), tag)
        }
        PresentationCatalogValue::Asset { asset } => {
            validate_presentation_asset_reference(&format!("{path}.asset"), asset)
        }
    }
}

fn validate_presentation_catalog_assignment(
    path: &str,
    value: &PresentationCatalogAssignment,
) -> Result<(), CharacterError> {
    validate_local_id(&format!("{path}.slot_id"), &value.slot_id)?;
    validate_presentation_catalog_ref(&format!("{path}.catalog"), &value.catalog)?;
    validate_local_id(&format!("{path}.entry_id"), &value.entry_id)?;
    validate_presentation_catalog_value(&format!("{path}.value"), &value.value)?;
    validate_sha256(&format!("{path}.proposal_sha256"), &value.proposal_sha256)?;
    validate_sha256(&format!("{path}.review_sha256"), &value.review_sha256)
}

fn validate_relationships(
    namespace: &str,
    profile_id: &str,
    value: &RelationshipEdges,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    let root = format!("extensions.{namespace}.value");
    if value.graph_format_version != relationship_graph_format_version() {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{root}.graph_format_version"),
            "unsupported relationship graph value version",
        ));
    }
    if let Some(pack) = &value.kind_pack {
        validate_relationship_kind_pack_ref(&format!("{root}.kind_pack"), pack)?;
    }
    if value.edges.len() > 65_536 {
        return Err(invalid_value(
            format!("{root}.edges"),
            "relationship graph contains too many edges",
        ));
    }
    for (id, edge) in &value.edges {
        let path = format!("{root}.edges.{id}");
        validate_local_id(&path, id)?;
        if edge.id != *id || edge.source_character_id != profile_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "relationship edge identity or source character does not match its owner",
            ));
        }
        validate_namespaced_id(
            &format!("{path}.target_character_id"),
            &edge.target_character_id,
        )?;
        validate_namespaced_id(&format!("{path}.kind"), &edge.kind)?;
        validate_relationship_edge(&path, edge, lineage)?;
    }
    Ok(())
}

fn validate_relationship_kind_pack_ref(
    path: &str,
    value: &RelationshipKindPackRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_relationship_edge(
    path: &str,
    edge: &RelationshipEdge,
    profile_lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if let Some(validity) = &edge.validity {
        if validity.start.is_none() && validity.end.is_none() {
            return Err(invalid_value(
                format!("{path}.validity"),
                "relationship validity requires at least one date bound",
            ));
        }
        if let Some(start) = validity.start {
            validate_relationship_date(&format!("{path}.validity.start"), start)?;
        }
        if let Some(end) = validity.end {
            validate_relationship_date(&format!("{path}.validity.end"), end)?;
        }
        if matches!((validity.start, validity.end), (Some(start), Some(end)) if start > end) {
            return Err(invalid_value(
                format!("{path}.validity"),
                "relationship validity start must not follow its end",
            ));
        }
    }
    if let Some(inverse_edge_id) = &edge.inverse_edge_id {
        validate_local_id(&format!("{path}.inverse_edge_id"), inverse_edge_id)?;
    }
    for (id, note) in &edge.notes {
        let note_path = format!("{path}.notes.{id}");
        validate_local_id(&note_path, id)?;
        if note.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{note_path}.id"),
                "relationship note id must equal its containing map key",
            ));
        }
        validate_text(&format!("{note_path}.content"), &note.content, 1, 4_096)?;
        validate_optional_lineage(
            &format!("{note_path}.lineage"),
            &note.lineage,
            profile_lineage,
        )?;
    }
    if let Some(consent) = &edge.consent {
        validate_relationship_consent(&format!("{path}.consent"), consent, profile_lineage)?;
    }
    for (id, exception) in &edge.safeguard_exceptions {
        let exception_path = format!("{path}.safeguard_exceptions.{id}");
        validate_local_id(&exception_path, id)?;
        if exception.id != *id
            || !matches!(exception.code.as_str(), "R107" | "R108" | "R109" | "R110")
        {
            return Err(invalid_value(
                exception_path,
                "relationship safeguard exception id or code is invalid",
            ));
        }
        validate_namespaced_id(
            &format!("{path}.safeguard_exceptions.{id}.reviewer"),
            &exception.reviewer,
        )?;
        validate_text(
            &format!("{path}.safeguard_exceptions.{id}.rationale"),
            &exception.rationale,
            1,
            2_048,
        )?;
        validate_optional_lineage(
            &format!("{path}.safeguard_exceptions.{id}.lineage"),
            &exception.lineage,
            profile_lineage,
        )?;
    }
    if edge
        .affinity_score_micros
        .is_some_and(|score| score > 1_000_000)
    {
        return Err(invalid_value(
            format!("{path}.affinity_score_micros"),
            "relationship advisory score must be bounded millionths",
        ));
    }
    if edge.evidence.len() > 256 {
        return Err(invalid_value(
            format!("{path}.evidence"),
            "relationship edge contains too many evidence contributions",
        ));
    }
    let mut evidence_ids = BTreeSet::new();
    for (index, evidence) in edge.evidence.iter().enumerate() {
        let evidence_path = format!("{path}.evidence[{index}]");
        validate_local_id(&format!("{evidence_path}.id"), &evidence.id)?;
        if !evidence_ids.insert(evidence.id.as_str()) {
            return Err(invalid_value(
                format!("{evidence_path}.id"),
                "relationship evidence ids must be unique while order is retained",
            ));
        }
        if evidence.contribution_micros.unsigned_abs() > 1_000_000 {
            return Err(invalid_value(
                format!("{evidence_path}.contribution_micros"),
                "relationship evidence contribution must be bounded millionths",
            ));
        }
        validate_sorted_relationship_paths(
            &format!("{evidence_path}.input_paths"),
            &evidence.input_paths,
        )?;
        validate_sha256(
            &format!("{evidence_path}.input_sha256"),
            &evidence.input_sha256,
        )?;
        validate_text(
            &format!("{evidence_path}.explanation"),
            &evidence.explanation,
            1,
            2_048,
        )?;
    }
    validate_optional_lineage(&format!("{path}.lineage"), &edge.lineage, profile_lineage)?;
    if let Some(rationale) = &edge.rationale {
        validate_text(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    match edge.origin {
        RelationshipEdgeOrigin::Authored => {
            if !matches!(
                edge.review,
                ReviewState::NotRequired | ReviewState::Accepted
            ) {
                return Err(invalid_value(
                    format!("{path}.review"),
                    "authored relationship review state is invalid",
                ));
            }
        }
        RelationshipEdgeOrigin::Imported => {
            if edge.rationale.is_none()
                || !matches!(
                    edge.review,
                    ReviewState::NotRequired | ReviewState::Accepted
                )
            {
                return Err(invalid_value(
                    path,
                    "imported relationships require rationale and a resolved review state",
                ));
            }
        }
        RelationshipEdgeOrigin::ComputedAffinity => {
            if edge.affinity_score_micros.is_none()
                || edge.evidence.is_empty()
                || edge.review != ReviewState::Accepted
            {
                return Err(invalid_value(
                    path,
                    "applied computed affinity requires accepted review, score, and evidence",
                ));
            }
        }
        RelationshipEdgeOrigin::SuggestedNarrative => {
            if edge.rationale.is_none()
                || !matches!(edge.review, ReviewState::Pending | ReviewState::Rejected)
            {
                return Err(invalid_value(
                    path,
                    "narrative suggestion requires rationale and a pending or rejected review",
                ));
            }
        }
        RelationshipEdgeOrigin::ReviewedSuggestion => {
            if edge.rationale.is_none() || edge.review != ReviewState::Accepted {
                return Err(invalid_value(
                    path,
                    "reviewed narrative suggestion requires accepted review and rationale",
                ));
            }
        }
    }
    if !edge.safeguard_exceptions.is_empty() && edge.review != ReviewState::Accepted {
        return Err(invalid_value(
            format!("{path}.safeguard_exceptions"),
            "safeguard exceptions require an accepted review",
        ));
    }
    Ok(())
}

fn validate_relationship_consent(
    path: &str,
    consent: &RelationshipConsent,
    profile_lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if consent.state == RelationshipConsentState::Affirmed {
        validate_namespaced_id(
            &format!("{path}.reviewed_by"),
            consent.reviewed_by.as_deref().unwrap_or_default(),
        )?;
        validate_text(
            &format!("{path}.rationale"),
            consent.rationale.as_deref().unwrap_or_default(),
            1,
            2_048,
        )?;
    } else {
        if let Some(reviewer) = &consent.reviewed_by {
            validate_namespaced_id(&format!("{path}.reviewed_by"), reviewer)?;
        }
        if let Some(rationale) = &consent.rationale {
            validate_text(&format!("{path}.rationale"), rationale, 1, 2_048)?;
        }
    }
    validate_optional_lineage(
        &format!("{path}.lineage"),
        &consent.lineage,
        profile_lineage,
    )
}

fn validate_optional_lineage(
    path: &str,
    values: &[String],
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if values.is_empty() {
        Ok(())
    } else {
        validate_lineage(path, values, lineage)
    }
}

fn validate_sorted_relationship_paths(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if values.is_empty() || values.len() > 4_096 {
        return Err(invalid_value(
            path,
            "relationship evidence requires bounded input paths",
        ));
    }
    let mut prior = None;
    for value in values {
        validate_text(path, value, 1, 1_024)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(
                path,
                "relationship input paths must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_relationship_date(path: &str, value: RelationshipDate) -> Result<(), CharacterError> {
    let valid_day = match value.month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => value.day <= 31,
        4 | 6 | 9 | 11 => value.day <= 30,
        2 => {
            let leap = value.year.rem_euclid(4) == 0
                && (value.year.rem_euclid(100) != 0 || value.year.rem_euclid(400) == 0);
            value.day <= if leap { 29 } else { 28 }
        }
        _ => false,
    };
    if !(-999_999..=999_999).contains(&value.year) || value.day == 0 || !valid_day {
        return Err(invalid_value(
            path,
            "relationship date is not a valid proleptic-Gregorian date",
        ));
    }
    Ok(())
}

fn validate_alignment_view(
    namespace: &str,
    value: &AlignmentView,
    header_lineage: &[String],
) -> Result<(), CharacterError> {
    let path = format!("extensions.{namespace}.value");
    validate_namespaced_id(&format!("{path}.view_id"), &value.view_id)?;
    validate_alignment_pack_ref(&format!("{path}.pack"), &value.pack)?;
    if value.view_id != value.pack.id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.view_id"),
            "alignment view identity must equal its exact pack identity",
        ));
    }
    validate_sha256(&format!("{path}.review_sha256"), &value.review_sha256)?;
    validate_sha256(&format!("{path}.applied_sha256"), &value.applied_sha256)?;
    let transformation_id = format!("alignment_apply_{}", &value.applied_sha256[..16]);
    if header_lineage.binary_search(&transformation_id).is_err() {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            format!("{path}.applied_sha256"),
            "alignment application transformation is absent from extension lineage",
        ));
    }
    if value.values.len() > 4_096 {
        return Err(invalid_value(
            format!("{path}.values"),
            "alignment view contains too many public values",
        ));
    }
    let mut expected_paths = BTreeSet::new();
    for (id, approved) in &value.values {
        let value_path = format!("{path}.values.{id}");
        validate_local_id(&value_path, id)?;
        if approved.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{value_path}.id"),
                "approved alignment value identifier must equal its containing map key",
            ));
        }
        validate_local_id(&format!("{value_path}.label_id"), &approved.label_id)?;
        validate_text(&format!("{value_path}.label"), &approved.label, 1, 256)?;
        if approved
            .score_micros
            .is_some_and(|score| !(-1_000_000..=1_000_000).contains(&score))
        {
            return Err(invalid_value(
                format!("{value_path}.score_micros"),
                "alignment score must be signed millionths",
            ));
        }
        if approved.coverage_micros > 1_000_000 {
            return Err(invalid_value(
                format!("{value_path}.coverage_micros"),
                "alignment coverage must be from zero through one million",
            ));
        }
        validate_text(
            &format!("{value_path}.explanation"),
            &approved.explanation,
            1,
            2_048,
        )?;
        validate_sorted_paths(&format!("{value_path}.input_paths"), &approved.input_paths)?;
        if approved
            .input_paths
            .iter()
            .any(|input| !input.starts_with("canon.personality."))
        {
            return Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                format!("{value_path}.input_paths"),
                "alignment views may read only canonical personality evidence",
            ));
        }
        expected_paths.extend(approved.input_paths.iter().cloned());
    }
    validate_sorted_paths(&format!("{path}.input_paths"), &value.input_paths)?;
    if value.input_paths != expected_paths.into_iter().collect::<Vec<_>>() {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.input_paths"),
            "alignment view inputs must exactly cover approved public values",
        ));
    }
    Ok(())
}

fn validate_alignment_pack_ref(path: &str, value: &AlignmentPackRef) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_date_context(
    namespace: &str,
    value: &DateContext,
    lineage: &BTreeSet<&str>,
    header_lineage: &[String],
) -> Result<(), CharacterError> {
    let path = format!("extensions.{namespace}.value");
    let primary = DateContextPackRef {
        id: value.context_pack.clone(),
        version: value.context_version.clone(),
        sha256: value.context_hash.clone(),
    };
    validate_date_context_pack_ref(&format!("{path}.context_pack"), &primary)?;
    let mut prior: Option<&DateContextPackRef> = Some(&primary);
    for pack in &value.additional_context_packs {
        validate_date_context_pack_ref(&format!("{path}.additional_context_packs"), pack)?;
        if prior.is_some_and(|prior| prior >= pack) {
            return Err(invalid_value(
                format!("{path}.additional_context_packs"),
                "context packs must be unique and sorted after the primary pack",
            ));
        }
        prior = Some(pack);
    }
    validate_sorted_local_ids(
        &format!("{path}.accepted_record_ids"),
        &value.accepted_record_ids,
    )?;
    let mut cue_record_ids = BTreeSet::new();
    let mut review_sha256 = None;
    for (id, cue) in &value.accepted_cues {
        let cue_path = format!("{path}.accepted_cues.{id}");
        validate_date_context_candidate_id(&cue_path, id)?;
        if cue.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{cue_path}.id"),
                "accepted cue identifier must equal its containing map key",
            ));
        }
        validate_local_id(&format!("{cue_path}.record_id"), &cue.record_id)?;
        if value
            .accepted_record_ids
            .binary_search(&cue.record_id)
            .is_err()
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{cue_path}.record_id"),
                "accepted cue record is absent from accepted_record_ids",
            ));
        }
        cue_record_ids.insert(cue.record_id.clone());
        validate_date_context_pack_ref(&format!("{cue_path}.pack"), &cue.pack)?;
        if cue.pack != primary && !value.additional_context_packs.contains(&cue.pack) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{cue_path}.pack"),
                "accepted cue pack is absent from the retained context coordinates",
            ));
        }
        validate_text(&format!("{cue_path}.content"), &cue.content, 1, 2_048)?;
        validate_unit_interval(&format!("{cue_path}.relevance"), cue.relevance)?;
        let relevance_micros = cue.relevance * 1_000_000.0;
        if (relevance_micros - relevance_micros.round()).abs() > 0.000_001 {
            return Err(invalid_value(
                format!("{cue_path}.relevance"),
                "accepted cue relevance must retain exact integer millionths",
            ));
        }
        validate_lineage(
            &format!("{cue_path}.fact_source_ids"),
            &cue.fact_source_ids,
            lineage,
        )?;
        validate_lineage(
            &format!("{cue_path}.cue_source_ids"),
            &cue.cue_source_ids,
            lineage,
        )?;
        validate_lineage(&format!("{cue_path}.source_ids"), &cue.source_ids, lineage)?;
        let union = cue
            .fact_source_ids
            .iter()
            .chain(&cue.cue_source_ids)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if cue.source_ids != union {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("{cue_path}.source_ids"),
                "accepted cue lineage must be the sorted union of fact and cue sources",
            ));
        }
        if cue
            .source_ids
            .iter()
            .any(|source| header_lineage.binary_search(source).is_err())
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("{cue_path}.source_ids"),
                "accepted cue lineage must be retained by the extension header",
            ));
        }
        validate_sha256(&format!("{cue_path}.review_sha256"), &cue.review_sha256)?;
        if review_sha256.is_some_and(|prior| prior != cue.review_sha256.as_str()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("{cue_path}.review_sha256"),
                "accepted cues in one extension must share one complete review",
            ));
        }
        review_sha256 = Some(cue.review_sha256.as_str());
        let transformation_id = format!("temporal_review_{}", &cue.review_sha256[..16]);
        if header_lineage.binary_search(&transformation_id).is_err() {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("{cue_path}.review_sha256"),
                "accepted cue review transformation is absent from extension lineage",
            ));
        }
    }
    if !value.accepted_cues.is_empty()
        && cue_record_ids.into_iter().collect::<Vec<_>>() != value.accepted_record_ids
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.accepted_record_ids"),
            "accepted record ids must exactly cover the reviewed cues",
        ));
    }
    Ok(())
}

fn validate_date_context_pack_ref(
    path: &str,
    value: &DateContextPackRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_date_context_candidate_id(path: &str, value: &str) -> Result<(), CharacterError> {
    let digest = value.strip_prefix("cue_").unwrap_or_default();
    if digest.len() != 24
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a stable temporal candidate identifier",
        ));
    }
    Ok(())
}

fn validate_opaque(namespace: &str, value: &OpaqueExtensionData) -> Result<(), CharacterError> {
    if value.interpretation != OpaqueInterpretation::PreservedInactive {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            format!("extensions.{namespace}.value.interpretation"),
            "opaque extensions must remain preserved and inactive",
        ));
    }
    validate_domain_value(
        &format!("extensions.{namespace}.value.payload"),
        &value.payload,
    )
}

fn validate_operation(
    path: &str,
    operation: &CharacterOperation,
    character_id: &str,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    match &operation.action {
        CharacterOperationAction::SetDisplayName { value } => validate_attributed_text(
            &format!("{path}.action.value"),
            value,
            lineage,
            CanonicalPolicy::Canon,
            1,
            256,
        ),
        CharacterOperationAction::SetAliases { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Canon,
            )?;
            validate_sorted_texts(&format!("{path}.action.value.value"), &value.value, 1, 256)
        }
        CharacterOperationAction::SetPronouns { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Presentation,
            )?;
            validate_pronouns(&format!("{path}.action.value.value"), &value.value)
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => {
            validate_identity_context_notes(
                &format!("{path}.action.record"),
                &BTreeMap::from([(record.id.clone(), record.clone())]),
                lineage,
            )
        }
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => {
            validate_appearance_descriptors(
                &format!("{path}.action.record"),
                &BTreeMap::from([(record.id.clone(), record.clone())]),
                lineage,
            )
        }
        CharacterOperationAction::SetPresentationPalette { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Presentation,
            )?;
            validate_presentation_palette(&format!("{path}.action.value.value"), &value.value)
        }
        CharacterOperationAction::SetPresentationStyleTags { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Presentation,
            )?;
            validate_sorted_local_ids(&format!("{path}.action.value.value"), &value.value)
        }
        CharacterOperationAction::UpsertPresentationAsset { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Presentation,
            )?;
            validate_presentation_asset_reference(
                &format!("{path}.action.value.value"),
                &value.value,
            )
        }
        CharacterOperationAction::UpsertPresentationAssignment { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Presentation,
            )?;
            validate_presentation_catalog_assignment(
                &format!("{path}.action.value.value"),
                &value.value,
            )
        }
        CharacterOperationAction::SetBirthDate { value } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Canon,
            )?;
            validate_birth_date(&format!("{path}.action.value.value"), &value.value)
        }
        CharacterOperationAction::SetHexacoTrait { value, .. } => {
            validate_attributed(
                &format!("{path}.action.value"),
                value,
                lineage,
                CanonicalPolicy::Personality,
            )?;
            if let TraitMeasurement::Score { score } = value.value
                && (!score.is_finite() || !(0.0..=1.0).contains(&score))
            {
                return Err(invalid_value(
                    format!("{path}.action.value.value.score"),
                    "trait scores must be finite values from 0.0 through 1.0",
                ));
            }
            Ok(())
        }
        CharacterOperationAction::UpsertInnerLife { record } => {
            let records = BTreeMap::from([(record.id.clone(), record.clone())]);
            validate_authored_notes(&format!("{path}.action.record"), &records, lineage)
        }
        CharacterOperationAction::UpsertVoice { record } => {
            let records = BTreeMap::from([(record.id.clone(), record.clone())]);
            validate_voice(&format!("{path}.action.record"), &records, lineage)
        }
        CharacterOperationAction::UpsertSuggestion { suggestion } => validate_suggestion(
            &format!("{path}.action.suggestion"),
            &suggestion.id,
            suggestion,
            lineage,
        ),
        CharacterOperationAction::UpsertExtension {
            namespace,
            extension,
        } => validate_extension(namespace, extension, character_id, lineage),
        CharacterOperationAction::RemoveInnerLife { id }
        | CharacterOperationAction::RemoveVoice { id }
        | CharacterOperationAction::RemoveSuggestion { id }
        | CharacterOperationAction::RemoveIdentityContextNote { id }
        | CharacterOperationAction::RemoveAppearanceDescriptor { id }
        | CharacterOperationAction::RemovePresentationAsset { id } => {
            validate_local_id(&format!("{path}.action.id"), id)
        }
        CharacterOperationAction::RemovePresentationAssignment { slot_id } => {
            validate_local_id(&format!("{path}.action.slot_id"), slot_id)
        }
        CharacterOperationAction::RemoveExtension { namespace } => {
            validate_namespaced_id(&format!("{path}.action.namespace"), namespace)
        }
        CharacterOperationAction::ClearAliases
        | CharacterOperationAction::ClearPronouns
        | CharacterOperationAction::ClearPresentationPalette
        | CharacterOperationAction::ClearPresentationStyleTags
        | CharacterOperationAction::ClearBirthDate
        | CharacterOperationAction::ClearHexacoTrait { .. } => Ok(()),
    }
}

/// Canonical target path for deterministic ordering and conflict detection.
pub(crate) fn operation_target(action: &CharacterOperationAction) -> String {
    match action {
        CharacterOperationAction::SetDisplayName { .. } => "canon.identity.display_name".to_owned(),
        CharacterOperationAction::SetAliases { .. } | CharacterOperationAction::ClearAliases => {
            "canon.identity.aliases".to_owned()
        }
        CharacterOperationAction::SetPronouns { .. } | CharacterOperationAction::ClearPronouns => {
            format!(
                "extensions.{}.value.pronouns",
                crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
            )
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => format!(
            "extensions.{}.value.context_notes.{}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
            record.id
        ),
        CharacterOperationAction::RemoveIdentityContextNote { id } => format!(
            "extensions.{}.value.context_notes.{id}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => format!(
            "extensions.{}.value.appearance.{}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
            record.id
        ),
        CharacterOperationAction::RemoveAppearanceDescriptor { id } => format!(
            "extensions.{}.value.appearance.{id}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::SetPresentationPalette { .. }
        | CharacterOperationAction::ClearPresentationPalette => format!(
            "extensions.{}.value.palette",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::SetPresentationStyleTags { .. }
        | CharacterOperationAction::ClearPresentationStyleTags => format!(
            "extensions.{}.value.style_tags",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::UpsertPresentationAsset { value } => format!(
            "extensions.{}.value.assets.{}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
            value.value.id
        ),
        CharacterOperationAction::RemovePresentationAsset { id } => format!(
            "extensions.{}.value.assets.{id}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::UpsertPresentationAssignment { value } => format!(
            "extensions.{}.value.catalog_assignments.{}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
            value.value.slot_id
        ),
        CharacterOperationAction::RemovePresentationAssignment { slot_id } => format!(
            "extensions.{}.value.catalog_assignments.{slot_id}",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        ),
        CharacterOperationAction::SetBirthDate { .. }
        | CharacterOperationAction::ClearBirthDate => "canon.birth_date".to_owned(),
        CharacterOperationAction::SetHexacoTrait { trait_id, .. }
        | CharacterOperationAction::ClearHexacoTrait { trait_id } => trait_path(*trait_id),
        CharacterOperationAction::UpsertInnerLife { record } => {
            format!("canon.inner_life.{}", record.id)
        }
        CharacterOperationAction::RemoveInnerLife { id } => format!("canon.inner_life.{id}"),
        CharacterOperationAction::UpsertVoice { record } => {
            format!("canon.voice.{}", record.id)
        }
        CharacterOperationAction::RemoveVoice { id } => format!("canon.voice.{id}"),
        CharacterOperationAction::UpsertSuggestion { suggestion } => {
            format!("suggestions.{}", suggestion.id)
        }
        CharacterOperationAction::RemoveSuggestion { id } => format!("suggestions.{id}"),
        CharacterOperationAction::UpsertExtension { namespace, .. }
        | CharacterOperationAction::RemoveExtension { namespace } => {
            format!("extensions.{namespace}")
        }
    }
}

fn validate_attributed_text(
    path: &str,
    value: &Attributed<String>,
    lineage: &BTreeSet<&str>,
    policy: CanonicalPolicy,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterError> {
    validate_attributed(path, value, lineage, policy)?;
    validate_text(&format!("{path}.value"), &value.value, minimum, maximum)
}

fn validate_attributed<T>(
    path: &str,
    value: &Attributed<T>,
    lineage: &BTreeSet<&str>,
    policy: CanonicalPolicy,
) -> Result<(), CharacterError> {
    validate_lineage(&format!("{path}.lineage"), &value.lineage, lineage)?;
    validate_state(
        path,
        value.state,
        value.review,
        value.freshness,
        policy == CanonicalPolicy::Extension,
    )?;
    match policy {
        CanonicalPolicy::Canon | CanonicalPolicy::Personality | CanonicalPolicy::Presentation => {
            if matches!(value.state, ValueState::Derived | ValueState::Suggested) {
                return Err(error(
                    CharacterDiagnosticCode::ForbiddenWriteBack,
                    format!("{path}.state"),
                    "derived and suggested values cannot enter authored Character fields directly",
                ));
            }
            if value.freshness != Freshness::Current {
                return Err(error(
                    CharacterDiagnosticCode::StaleInput,
                    format!("{path}.freshness"),
                    "stale values cannot be applied to character canon",
                ));
            }
        }
        CanonicalPolicy::Suggestion => {
            if value.state != ValueState::Suggested
                || !matches!(value.review, ReviewState::Pending | ReviewState::Rejected)
            {
                return Err(error(
                    CharacterDiagnosticCode::InvalidValue,
                    format!("{path}.state"),
                    "suggestion records must remain suggested and pending or rejected",
                ));
            }
        }
        CanonicalPolicy::Extension => {}
    }
    if matches!(
        value.state,
        ValueState::Derived | ValueState::Suggested | ValueState::Reviewed | ValueState::Overridden
    ) {
        let rationale = value.rationale.as_deref().unwrap_or_default();
        validate_text(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    Ok(())
}

fn validate_state(
    path: &str,
    state: ValueState,
    review: ReviewState,
    freshness: Freshness,
    allow_stale: bool,
) -> Result<(), CharacterError> {
    if !allow_stale && freshness == Freshness::Stale {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            format!("{path}.freshness"),
            "stale values cannot be applied",
        ));
    }
    let valid_review = match state {
        ValueState::Suggested => matches!(review, ReviewState::Pending | ReviewState::Rejected),
        ValueState::Derived => review == ReviewState::NotRequired,
        ValueState::Reviewed | ValueState::Overridden => review == ReviewState::Accepted,
        ValueState::Authored | ValueState::Imported => {
            matches!(review, ReviewState::NotRequired | ReviewState::Accepted)
        }
    };
    if !valid_review {
        return Err(invalid_value(
            format!("{path}.review"),
            "review state is incompatible with value state",
        ));
    }
    Ok(())
}

fn validate_birth_date(path: &str, value: &BirthDate) -> Result<(), CharacterError> {
    match *value {
        BirthDate::Year {
            calendar: Calendar::ProlepticGregorian,
            year,
        } => validate_year(path, year),
        BirthDate::MonthDay {
            calendar: Calendar::ProlepticGregorian,
            month,
            day,
        } => validate_month_day(path, None, month, day),
        BirthDate::Full {
            calendar: Calendar::ProlepticGregorian,
            year,
            month,
            day,
        } => {
            validate_year(path, year)?;
            validate_month_day(path, Some(year), month, day)
        }
    }
}

fn validate_year(path: &str, year: i32) -> Result<(), CharacterError> {
    if !(1..=9999).contains(&year) {
        return Err(invalid_value(
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
) -> Result<(), CharacterError> {
    let leap = year.is_none_or(is_leap_year);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if days == 0 || day == 0 || day > days {
        return Err(invalid_value(path, "date contains an invalid month or day"));
    }
    Ok(())
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn validate_profile_provenance(provenance: &Provenance) -> Result<(), CharacterError> {
    validate_provenance(provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "profile provenance is malformed or incomplete",
        )
    })
}

fn lineage_ids(provenance: &Provenance) -> BTreeSet<&str> {
    provenance
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .chain(
            provenance
                .transformations
                .iter()
                .map(|transformation| transformation.id.as_str()),
        )
        .collect()
}

fn validate_lineage(
    path: &str,
    values: &[String],
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if values.is_empty() || values.len() > 4_096 {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            path,
            "value requires at least one bounded lineage reference",
        ));
    }
    let mut prior = None;
    for value in values {
        if prior.is_some_and(|prior: &str| prior >= value.as_str())
            || !lineage.contains(value.as_str())
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                path,
                "lineage references must be declared, unique, and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_domain_value(path: &str, value: &DomainValue) -> Result<(), CharacterError> {
    fn walk(value: &DomainValue, depth: usize, nodes: &mut usize) -> bool {
        *nodes += 1;
        if depth > MAX_DOMAIN_DEPTH || *nodes > MAX_DOMAIN_NODES {
            return false;
        }
        match value {
            DomainValue::Null | DomainValue::Bool(_) => true,
            DomainValue::Number(number) => number.is_finite(),
            DomainValue::String(value) | DomainValue::Symbol(value) => value.len() <= MAX_TEXT,
            DomainValue::List(values) => {
                values.len() <= MAX_COLLECTION
                    && values.iter().all(|value| walk(value, depth + 1, nodes))
            }
            DomainValue::Object(values) => {
                values.len() <= MAX_COLLECTION
                    && values
                        .iter()
                        .all(|(key, value)| valid_local_id(key) && walk(value, depth + 1, nodes))
            }
        }
    }
    let mut nodes = 0;
    if !walk(value, 0, &mut nodes) {
        return Err(invalid_value(
            path,
            "extension value exceeds portable value bounds",
        ));
    }
    Ok(())
}

fn validate_sorted_texts(
    path: &str,
    values: &[String],
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterError> {
    if values.len() > 4_096 {
        return Err(invalid_value(path, "too many values"));
    }
    let mut prior = None;
    for value in values {
        validate_text(path, value, minimum, maximum)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(path, "values must be unique and sorted"));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_namespaced_refs(path: &str, values: &[String]) -> Result<(), CharacterError> {
    validate_sorted(values, path, |value| validate_namespaced_id(path, value))
}

fn validate_sorted_local_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    validate_sorted(values, path, |value| validate_local_id(path, value))
}

fn validate_sorted_paths(path: &str, values: &[String]) -> Result<(), CharacterError> {
    validate_sorted(values, path, |value| validate_profile_path(path, value))
}

fn validate_sorted_relative_paths(path: &str, values: &[String]) -> Result<(), CharacterError> {
    validate_sorted(values, path, |value| validate_relative_path(path, value))
}

pub(crate) fn validate_relative_path(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "presentation references must be safe project-relative paths",
        ));
    }
    Ok(())
}

fn validate_sorted(
    values: &[String],
    path: &str,
    mut validate: impl FnMut(&String) -> Result<(), CharacterError>,
) -> Result<(), CharacterError> {
    if values.len() > 4_096 {
        return Err(invalid_value(path, "too many references"));
    }
    let mut prior = None;
    for value in values {
        validate(value)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(path, "references must be unique and sorted"));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_profile_path(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.is_empty() || value.len() > 512 || value.split('.').any(|part| !valid_local_id(part)) {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated character field path",
        ));
    }
    Ok(())
}

pub(crate) fn validate_namespaced_id(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.len() > 256
        || value.split('.').count() < 2
        || value.split('.').any(|part| !valid_local_id(part))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated lowercase stable identifier",
        ));
    }
    Ok(())
}

pub(crate) fn validate_local_id(path: &str, value: &str) -> Result<(), CharacterError> {
    if !valid_local_id(value) {
        return Err(error(
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

pub(crate) fn validate_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterError> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) || value.contains('\0') {
        return Err(invalid_value(path, "text length or contents are invalid"));
    }
    Ok(())
}

fn validate_unit_interval(path: &str, value: f64) -> Result<(), CharacterError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(invalid_value(
            path,
            "strength must be a finite value from 0.0 through 1.0",
        ));
    }
    Ok(())
}

pub(crate) fn validate_semver(path: &str, value: &str) -> Result<(), CharacterError> {
    Version::parse(value)
        .map(|_| ())
        .map_err(|_| invalid_value(path, "expected a semantic version"))
}

pub(crate) fn validate_sha256(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid_value(path, "expected a lowercase SHA-256"));
    }
    Ok(())
}

pub(crate) fn invalid_value(path: impl Into<String>, message: &'static str) -> CharacterError {
    error(CharacterDiagnosticCode::InvalidValue, path, message)
}

pub(crate) fn error(
    code: CharacterDiagnosticCode,
    path: impl Into<String>,
    message: &'static str,
) -> CharacterError {
    CharacterError {
        diagnostic: CharacterDiagnostic {
            code,
            severity: DiagnosticSeverity::Error,
            path: path.into(),
            message: message.to_owned(),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalPolicy {
    Canon,
    Personality,
    Presentation,
    Suggestion,
    Extension,
}

pub(crate) const ALL_HEXACO_TRAITS: [HexacoTrait; 30] = [
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
];

pub(crate) fn factor_facets(factor: HexacoTrait) -> [HexacoTrait; 4] {
    match factor {
        HexacoTrait::HonestyHumility => [
            HexacoTrait::Sincerity,
            HexacoTrait::Fairness,
            HexacoTrait::GreedAvoidance,
            HexacoTrait::Modesty,
        ],
        HexacoTrait::Emotionality => [
            HexacoTrait::Fearfulness,
            HexacoTrait::Anxiety,
            HexacoTrait::Dependence,
            HexacoTrait::Sentimentality,
        ],
        HexacoTrait::Extraversion => [
            HexacoTrait::SocialSelfEsteem,
            HexacoTrait::SocialBoldness,
            HexacoTrait::Sociability,
            HexacoTrait::Liveliness,
        ],
        HexacoTrait::Agreeableness => [
            HexacoTrait::Forgivingness,
            HexacoTrait::Gentleness,
            HexacoTrait::Flexibility,
            HexacoTrait::Patience,
        ],
        HexacoTrait::Conscientiousness => [
            HexacoTrait::Organization,
            HexacoTrait::Diligence,
            HexacoTrait::Perfectionism,
            HexacoTrait::Prudence,
        ],
        HexacoTrait::Openness => [
            HexacoTrait::AestheticAppreciation,
            HexacoTrait::Inquisitiveness,
            HexacoTrait::Creativity,
            HexacoTrait::Unconventionality,
        ],
        _ => unreachable!("only factor identifiers have facet sets"),
    }
}

pub(crate) fn trait_path(trait_id: HexacoTrait) -> String {
    let (factor, field) = match trait_id {
        HexacoTrait::HonestyHumility => ("honesty_humility", "factor"),
        HexacoTrait::Sincerity => ("honesty_humility", "sincerity"),
        HexacoTrait::Fairness => ("honesty_humility", "fairness"),
        HexacoTrait::GreedAvoidance => ("honesty_humility", "greed_avoidance"),
        HexacoTrait::Modesty => ("honesty_humility", "modesty"),
        HexacoTrait::Emotionality => ("emotionality", "factor"),
        HexacoTrait::Fearfulness => ("emotionality", "fearfulness"),
        HexacoTrait::Anxiety => ("emotionality", "anxiety"),
        HexacoTrait::Dependence => ("emotionality", "dependence"),
        HexacoTrait::Sentimentality => ("emotionality", "sentimentality"),
        HexacoTrait::Extraversion => ("extraversion", "factor"),
        HexacoTrait::SocialSelfEsteem => ("extraversion", "social_self_esteem"),
        HexacoTrait::SocialBoldness => ("extraversion", "social_boldness"),
        HexacoTrait::Sociability => ("extraversion", "sociability"),
        HexacoTrait::Liveliness => ("extraversion", "liveliness"),
        HexacoTrait::Agreeableness => ("agreeableness", "factor"),
        HexacoTrait::Forgivingness => ("agreeableness", "forgivingness"),
        HexacoTrait::Gentleness => ("agreeableness", "gentleness"),
        HexacoTrait::Flexibility => ("agreeableness", "flexibility"),
        HexacoTrait::Patience => ("agreeableness", "patience"),
        HexacoTrait::Conscientiousness => ("conscientiousness", "factor"),
        HexacoTrait::Organization => ("conscientiousness", "organization"),
        HexacoTrait::Diligence => ("conscientiousness", "diligence"),
        HexacoTrait::Perfectionism => ("conscientiousness", "perfectionism"),
        HexacoTrait::Prudence => ("conscientiousness", "prudence"),
        HexacoTrait::Openness => ("openness", "factor"),
        HexacoTrait::AestheticAppreciation => ("openness", "aesthetic_appreciation"),
        HexacoTrait::Inquisitiveness => ("openness", "inquisitiveness"),
        HexacoTrait::Creativity => ("openness", "creativity"),
        HexacoTrait::Unconventionality => ("openness", "unconventionality"),
    };
    format!("canon.personality.{factor}.{field}")
}

pub(crate) fn trait_value(
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

pub(crate) fn trait_value_mut(
    profile: &mut HexacoProfile,
    trait_id: HexacoTrait,
) -> &mut Option<Attributed<TraitMeasurement>> {
    match trait_id {
        HexacoTrait::HonestyHumility => &mut profile.honesty_humility.factor,
        HexacoTrait::Sincerity => &mut profile.honesty_humility.sincerity,
        HexacoTrait::Fairness => &mut profile.honesty_humility.fairness,
        HexacoTrait::GreedAvoidance => &mut profile.honesty_humility.greed_avoidance,
        HexacoTrait::Modesty => &mut profile.honesty_humility.modesty,
        HexacoTrait::Emotionality => &mut profile.emotionality.factor,
        HexacoTrait::Fearfulness => &mut profile.emotionality.fearfulness,
        HexacoTrait::Anxiety => &mut profile.emotionality.anxiety,
        HexacoTrait::Dependence => &mut profile.emotionality.dependence,
        HexacoTrait::Sentimentality => &mut profile.emotionality.sentimentality,
        HexacoTrait::Extraversion => &mut profile.extraversion.factor,
        HexacoTrait::SocialSelfEsteem => &mut profile.extraversion.social_self_esteem,
        HexacoTrait::SocialBoldness => &mut profile.extraversion.social_boldness,
        HexacoTrait::Sociability => &mut profile.extraversion.sociability,
        HexacoTrait::Liveliness => &mut profile.extraversion.liveliness,
        HexacoTrait::Agreeableness => &mut profile.agreeableness.factor,
        HexacoTrait::Forgivingness => &mut profile.agreeableness.forgivingness,
        HexacoTrait::Gentleness => &mut profile.agreeableness.gentleness,
        HexacoTrait::Flexibility => &mut profile.agreeableness.flexibility,
        HexacoTrait::Patience => &mut profile.agreeableness.patience,
        HexacoTrait::Conscientiousness => &mut profile.conscientiousness.factor,
        HexacoTrait::Organization => &mut profile.conscientiousness.organization,
        HexacoTrait::Diligence => &mut profile.conscientiousness.diligence,
        HexacoTrait::Perfectionism => &mut profile.conscientiousness.perfectionism,
        HexacoTrait::Prudence => &mut profile.conscientiousness.prudence,
        HexacoTrait::Openness => &mut profile.openness.factor,
        HexacoTrait::AestheticAppreciation => &mut profile.openness.aesthetic_appreciation,
        HexacoTrait::Inquisitiveness => &mut profile.openness.inquisitiveness,
        HexacoTrait::Creativity => &mut profile.openness.creativity,
        HexacoTrait::Unconventionality => &mut profile.openness.unconventionality,
    }
}
