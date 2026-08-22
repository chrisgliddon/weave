use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use semver::Version;
use weave_domain::{DomainValue, Provenance, validate_provenance};

use crate::model::{
    Attributed, AuthoredNote, BehavioralSignatures, BirthDate, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_PROFILE_FORMAT_VERSION, CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar,
    CharacterDiagnostic, CharacterDiagnosticCode, CharacterExtension, CharacterOperation,
    CharacterOperationAction, CharacterOverlay, CharacterProfile, CharacterSuggestion,
    CharacterTemplate, Confidence, DateContext, DateContextPackRef, DiagnosticSeverity,
    ExpressionData, ExtensionHeader, ExtensionWriteBack, Freshness, HexacoProfile, HexacoTrait,
    IdentityPresentation, OceanView, OpaqueExtensionData, OpaqueInterpretation, RelationshipEdges,
    ReviewState, RoleProjections, TraitMeasurement, ValueState, VersionedExtension, VoiceDirection,
};

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
            validate_identity_presentation(namespace, &record.value)
        }
        CharacterExtension::Expression(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_expression(namespace, &record.value)
        }
        CharacterExtension::BehavioralSignatures(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_behavioral_signatures(namespace, &record.value)
        }
        CharacterExtension::RoleProjections(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_role_projections(namespace, &record.value)
        }
        CharacterExtension::Relationships(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_relationships(namespace, profile_id, &record.value)
        }
        CharacterExtension::AlignmentView(record) => {
            validate_extension_record(namespace, record, lineage, false)?;
            validate_namespaced_id(
                &format!("extensions.{namespace}.value.view_id"),
                &record.value.view_id,
            )?;
            validate_sorted_paths(
                &format!("extensions.{namespace}.value.input_paths"),
                &record.value.input_paths,
            )?;
            validate_domain_value(
                &format!("extensions.{namespace}.value.values"),
                &record.value.values,
            )
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
) -> Result<(), CharacterError> {
    validate_sorted_namespaced_refs(
        &format!("extensions.{namespace}.value.identity_refs"),
        &value.identity_refs,
    )?;
    validate_sorted_relative_paths(
        &format!("extensions.{namespace}.value.presentation_refs"),
        &value.presentation_refs,
    )
}

fn validate_expression(namespace: &str, value: &ExpressionData) -> Result<(), CharacterError> {
    for (id, term) in &value.lexicon {
        let path = format!("extensions.{namespace}.value.lexicon.{id}");
        validate_local_id(&path, id)?;
        if term.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "lexicon identifier must equal its containing map key",
            ));
        }
        validate_namespaced_id(&format!("{path}.category"), &term.category)?;
        validate_normalized_text(&format!("{path}.normalized"), &term.normalized)?;
        validate_unit_interval(&format!("{path}.strength"), term.strength)?;
    }
    for (id, preference) in &value.preferences {
        let path = format!("extensions.{namespace}.value.preferences.{id}");
        validate_local_id(&path, id)?;
        if preference.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "preference identifier must equal its containing map key",
            ));
        }
        validate_namespaced_id(&format!("{path}.category"), &preference.category)?;
        validate_normalized_text(&format!("{path}.target"), &preference.target)?;
        validate_unit_interval(&format!("{path}.strength"), preference.strength)?;
    }
    validate_sorted_namespaced_refs(
        &format!("extensions.{namespace}.value.behavioral_signature_refs"),
        &value.behavioral_signature_refs,
    )?;
    validate_sorted_namespaced_refs(
        &format!("extensions.{namespace}.value.source_pack_refs"),
        &value.source_pack_refs,
    )
}

fn validate_behavioral_signatures(
    namespace: &str,
    value: &BehavioralSignatures,
) -> Result<(), CharacterError> {
    for (id, signature) in &value.signatures {
        let path = format!("extensions.{namespace}.value.signatures.{id}");
        validate_local_id(&path, id)?;
        if signature.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "signature identifier must equal its containing map key",
            ));
        }
        validate_text(&format!("{path}.cue"), &signature.cue, 1, 2_048)?;
        validate_unit_interval(&format!("{path}.strength"), signature.strength)?;
    }
    Ok(())
}

fn validate_role_projections(
    namespace: &str,
    value: &RoleProjections,
) -> Result<(), CharacterError> {
    for (id, role) in &value.roles {
        let path = format!("extensions.{namespace}.value.roles.{id}");
        validate_local_id(&path, id)?;
        if role.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "role identifier must equal its containing map key",
            ));
        }
        validate_namespaced_id(&format!("{path}.taxonomy"), &role.taxonomy)?;
        validate_text(&format!("{path}.role"), &role.role, 1, 256)?;
        validate_text(&format!("{path}.rationale"), &role.rationale, 1, 2_048)?;
        validate_sorted_paths(&format!("{path}.input_paths"), &role.input_paths)?;
    }
    Ok(())
}

fn validate_relationships(
    namespace: &str,
    profile_id: &str,
    value: &RelationshipEdges,
) -> Result<(), CharacterError> {
    for (id, edge) in &value.edges {
        let path = format!("extensions.{namespace}.value.edges.{id}");
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
        if edge.target_character_id == profile_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.target_character_id"),
                "relationship edge cannot target its owning character",
            ));
        }
        validate_namespaced_id(&format!("{path}.kind"), &edge.kind)?;
    }
    Ok(())
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
        | CharacterOperationAction::RemoveSuggestion { id } => {
            validate_local_id(&format!("{path}.action.id"), id)
        }
        CharacterOperationAction::RemoveExtension { namespace } => {
            validate_namespaced_id(&format!("{path}.action.namespace"), namespace)
        }
        CharacterOperationAction::ClearAliases
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
        CanonicalPolicy::Canon | CanonicalPolicy::Personality => {
            if matches!(value.state, ValueState::Derived | ValueState::Suggested) {
                return Err(error(
                    CharacterDiagnosticCode::ForbiddenWriteBack,
                    format!("{path}.state"),
                    "derived and suggested values cannot enter character canon directly",
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
    validate_sorted(values, path, |value| {
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
    })
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

fn validate_namespaced_id(path: &str, value: &str) -> Result<(), CharacterError> {
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

fn validate_local_id(path: &str, value: &str) -> Result<(), CharacterError> {
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

fn validate_text(
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

fn validate_normalized_text(path: &str, value: &str) -> Result<(), CharacterError> {
    validate_text(path, value, 1, 1_024)?;
    if value.trim() != value
        || value.chars().any(char::is_uppercase)
        || value.split_whitespace().collect::<Vec<_>>().join(" ") != value
    {
        return Err(invalid_value(
            path,
            "normalized text must be lowercase with canonical single spacing",
        ));
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

fn validate_semver(path: &str, value: &str) -> Result<(), CharacterError> {
    Version::parse(value)
        .map(|_| ())
        .map_err(|_| invalid_value(path, "expected a semantic version"))
}

fn validate_sha256(path: &str, value: &str) -> Result<(), CharacterError> {
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
