use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};
use weave_domain::{Provenance, to_pretty_json};

use crate::model::{
    Attributed, CHARACTER_PROFILE_FORMAT_VERSION, CHARACTER_SYNTHESIS_FORMAT_VERSION,
    CharacterCanon, CharacterDerivedViews, CharacterDiagnosticCode, CharacterExtension,
    CharacterIdentity, CharacterOperation, CharacterOperationAction, CharacterOverlay,
    CharacterProfile, CharacterSynthesisResult, CharacterTemplate, CharacterTemplateRef,
    ExtensionHeader, ExtensionWriteBack, Freshness, HexacoProfile, IdentityPresentation, LockState,
    ReviewState, SynthesisOrigin, ValueState, VersionedExtension,
};
use crate::validation::{
    ALL_HEXACO_TRAITS, CharacterError, derive_ocean, error, operation_target, recompute_derived,
    trait_path, trait_value, trait_value_mut, validate_overlay, validate_profile,
    validate_template,
};

/// Compute the canonical SHA-256 identity of one validated immutable template.
pub fn template_fingerprint(template: &CharacterTemplate) -> Result<String, CharacterError> {
    validate_template(template)?;
    let json = to_pretty_json(template).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidEncoding,
            "template",
            "could not serialize the Character template",
        )
    })?;
    Ok(sha256(json.as_bytes()))
}

/// Deterministically synthesize one effective profile without mutating either input.
pub fn synthesize_character(
    template: Option<&CharacterTemplate>,
    overlay: &CharacterOverlay,
) -> Result<CharacterSynthesisResult, CharacterError> {
    validate_overlay(overlay)?;
    if let Some(template) = template {
        validate_template(template)?;
    }
    validate_template_selection(template, overlay.template.as_ref())?;

    let blank_display_operation = if template.is_none() {
        overlay.operations.iter().find(|operation| {
            matches!(
                operation.action,
                CharacterOperationAction::SetDisplayName { .. }
            )
        })
    } else {
        None
    };
    let mut profile = match template {
        Some(template) => {
            let mut profile = template.profile.clone();
            let template_character_id = profile.id.clone();
            profile.id.clone_from(&overlay.character_id);
            retarget_template_owner_links(
                &mut profile,
                &template_character_id,
                &overlay.character_id,
            );
            profile.provenance = merge_provenance(&profile.provenance, &overlay.provenance)?;
            profile
        }
        None => blank_profile(
            overlay,
            blank_display_operation.ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidValue,
                    "operations",
                    "a blank overlay requires one display-name operation",
                )
            })?,
        )?,
    };

    let mut origins = template
        .map(|template| template_origins(template, &profile))
        .unwrap_or_default();
    let blank_operation_id = blank_display_operation.map(|operation| operation.id.as_str());
    if let Some(operation) = blank_display_operation {
        origins.insert(
            operation_target(&operation.action),
            overlay_origin(overlay, operation),
        );
    }

    for operation in &overlay.operations {
        if blank_operation_id == Some(operation.id.as_str()) {
            continue;
        }
        apply_operation(&mut profile, &mut origins, overlay, operation)?;
    }
    recompute_derived(&mut profile);
    validate_profile(&profile)?;

    Ok(CharacterSynthesisResult {
        synthesis_format_version: CHARACTER_SYNTHESIS_FORMAT_VERSION,
        template: template.cloned(),
        overlay: overlay.clone(),
        effective_profile: profile,
        origins,
    })
}

/// Retarget profile-owned links when one immutable template is instantiated as another character.
///
/// This changes only explicit owner/self references. External relationship targets and every
/// extension authority field remain exactly as authored in the template.
fn retarget_template_owner_links(profile: &mut CharacterProfile, old_id: &str, new_id: &str) {
    if old_id == new_id {
        return;
    }
    for extension in profile.extensions.values_mut() {
        match extension {
            CharacterExtension::Expression(record) => {
                if record.value.character_id == old_id {
                    record.value.character_id = new_id.to_owned();
                }
                for value in record.value.lexicon.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                    retarget_applicability(&mut value.applicability, old_id, new_id);
                }
                for value in record.value.preferences.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                    retarget_applicability(&mut value.applicability, old_id, new_id);
                }
                for value in record.value.vocabulary_pools.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                    retarget_applicability(&mut value.applicability, old_id, new_id);
                }
                for value in record.value.voice_constraints.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                    retarget_applicability(&mut value.applicability, old_id, new_id);
                }
                for value in record.value.template_assignments.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                }
                let prefix = format!("{old_id}.signature.");
                for value in &mut record.value.behavioral_signature_refs {
                    if let Some(suffix) = value.strip_prefix(&prefix) {
                        *value = format!("{new_id}.signature.{suffix}");
                    }
                }
            }
            CharacterExtension::BehavioralSignatures(record) => {
                retarget_owner(&mut record.value.character_id, old_id, new_id);
                for value in record.value.signatures.values_mut() {
                    retarget_owner(&mut value.character_id, old_id, new_id);
                    retarget_applicability(&mut value.applicability, old_id, new_id);
                }
            }
            CharacterExtension::Relationships(record) => {
                for edge in record.value.edges.values_mut() {
                    retarget_owner(&mut edge.source_character_id, old_id, new_id);
                    retarget_owner(&mut edge.target_character_id, old_id, new_id);
                }
            }
            _ => {}
        }
    }
}

fn retarget_owner(value: &mut String, old_id: &str, new_id: &str) {
    if value == old_id {
        *value = new_id.to_owned();
    }
}

fn retarget_applicability(
    applicability: &mut crate::ExpressionApplicability,
    old_id: &str,
    new_id: &str,
) {
    for predicate in &mut applicability.predicates {
        let crate::ExpressionContextPredicate::Relationship {
            other_character_id: Some(character_id),
            ..
        } = predicate
        else {
            continue;
        };
        retarget_owner(character_id, old_id, new_id);
    }
}

/// Independently reproduce and validate a serialized synthesis proof.
pub fn validate_synthesis_result(result: &CharacterSynthesisResult) -> Result<(), CharacterError> {
    if result.synthesis_format_version != CHARACTER_SYNTHESIS_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "synthesis_format_version",
            "unsupported Character synthesis format version",
        ));
    }
    let expected = synthesize_character(result.template.as_ref(), &result.overlay)?;
    if expected.effective_profile != result.effective_profile || expected.origins != result.origins
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "effective_profile",
            "synthesis proof does not match its immutable inputs",
        ));
    }
    Ok(())
}

fn validate_template_selection(
    template: Option<&CharacterTemplate>,
    selected: Option<&CharacterTemplateRef>,
) -> Result<(), CharacterError> {
    match (template, selected) {
        (None, None) => Ok(()),
        (Some(template), Some(selected)) => {
            if template.id != selected.id || template.version != selected.version {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    "template",
                    "overlay references a different Character template",
                ));
            }
            if template_fingerprint(template)? != selected.sha256 {
                return Err(error(
                    CharacterDiagnosticCode::StaleInput,
                    "template.sha256",
                    "overlay template fingerprint is stale",
                ));
            }
            Ok(())
        }
        _ => Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "template",
            "overlay and synthesis input must agree on template presence",
        )),
    }
}

fn blank_profile(
    overlay: &CharacterOverlay,
    display_operation: &CharacterOperation,
) -> Result<CharacterProfile, CharacterError> {
    validate_expected_prior(display_operation, None)?;
    let CharacterOperationAction::SetDisplayName { value } = &display_operation.action else {
        return Err(error(
            CharacterDiagnosticCode::InvalidValue,
            "operations",
            "a blank overlay requires one display-name operation",
        ));
    };
    let personality = HexacoProfile::default();
    Ok(CharacterProfile {
        profile_format_version: CHARACTER_PROFILE_FORMAT_VERSION,
        id: overlay.character_id.clone(),
        canon: CharacterCanon {
            identity: CharacterIdentity {
                display_name: value.clone(),
                aliases: None,
            },
            birth_date: None,
            personality: personality.clone(),
            inner_life: BTreeMap::new(),
            voice: BTreeMap::new(),
        },
        extensions: BTreeMap::new(),
        suggestions: BTreeMap::new(),
        derived: CharacterDerivedViews {
            ocean: derive_ocean(&personality),
        },
        provenance: overlay.provenance.clone(),
    })
}

fn apply_operation(
    profile: &mut CharacterProfile,
    origins: &mut BTreeMap<String, SynthesisOrigin>,
    overlay: &CharacterOverlay,
    operation: &CharacterOperation,
) -> Result<(), CharacterError> {
    let target = operation_target(&operation.action);
    let prior_hash = current_value_hash(profile, &operation.action)?;
    validate_expected_prior(operation, prior_hash.as_deref())?;
    validate_authority(profile, operation)?;

    match &operation.action {
        CharacterOperationAction::SetDisplayName { value } => {
            profile.canon.identity.display_name = value.clone();
        }
        CharacterOperationAction::SetAliases { value } => {
            profile.canon.identity.aliases = Some(value.clone());
        }
        CharacterOperationAction::ClearAliases => {
            profile.canon.identity.aliases = None;
        }
        CharacterOperationAction::SetPronouns { value } => {
            identity_presentation_mut(profile)?.pronouns = Some(value.clone());
        }
        CharacterOperationAction::ClearPronouns => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.pronouns = None;
            }
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => {
            identity_presentation_mut(profile)?
                .context_notes
                .insert(record.id.clone(), record.clone());
        }
        CharacterOperationAction::RemoveIdentityContextNote { id } => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.context_notes.remove(id);
            }
        }
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => {
            identity_presentation_mut(profile)?
                .appearance
                .insert(record.id.clone(), record.clone());
        }
        CharacterOperationAction::RemoveAppearanceDescriptor { id } => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.appearance.remove(id);
            }
        }
        CharacterOperationAction::SetPresentationPalette { value } => {
            identity_presentation_mut(profile)?.palette = Some(value.clone());
        }
        CharacterOperationAction::ClearPresentationPalette => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.palette = None;
            }
        }
        CharacterOperationAction::SetPresentationStyleTags { value } => {
            identity_presentation_mut(profile)?.style_tags = Some(value.clone());
        }
        CharacterOperationAction::ClearPresentationStyleTags => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.style_tags = None;
            }
        }
        CharacterOperationAction::UpsertPresentationAsset { value } => {
            identity_presentation_mut(profile)?
                .assets
                .insert(value.value.id.clone(), value.clone());
        }
        CharacterOperationAction::RemovePresentationAsset { id } => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.assets.remove(id);
            }
        }
        CharacterOperationAction::UpsertPresentationAssignment { value } => {
            identity_presentation_mut(profile)?
                .catalog_assignments
                .insert(value.value.slot_id.clone(), value.clone());
        }
        CharacterOperationAction::RemovePresentationAssignment { slot_id } => {
            if let Some(presentation) = existing_identity_presentation_mut(profile)? {
                presentation.catalog_assignments.remove(slot_id);
            }
        }
        CharacterOperationAction::SetBirthDate { value } => {
            profile.canon.birth_date = Some(value.clone());
        }
        CharacterOperationAction::ClearBirthDate => {
            profile.canon.birth_date = None;
        }
        CharacterOperationAction::SetHexacoTrait { trait_id, value } => {
            *trait_value_mut(&mut profile.canon.personality, *trait_id) = Some(value.clone());
        }
        CharacterOperationAction::ClearHexacoTrait { trait_id } => {
            *trait_value_mut(&mut profile.canon.personality, *trait_id) = None;
        }
        CharacterOperationAction::UpsertInnerLife { record } => {
            profile
                .canon
                .inner_life
                .insert(record.id.clone(), record.clone());
        }
        CharacterOperationAction::RemoveInnerLife { id } => {
            profile.canon.inner_life.remove(id);
        }
        CharacterOperationAction::UpsertVoice { record } => {
            profile
                .canon
                .voice
                .insert(record.id.clone(), record.clone());
        }
        CharacterOperationAction::RemoveVoice { id } => {
            profile.canon.voice.remove(id);
        }
        CharacterOperationAction::UpsertSuggestion { suggestion } => {
            profile
                .suggestions
                .insert(suggestion.id.clone(), suggestion.clone());
        }
        CharacterOperationAction::RemoveSuggestion { id } => {
            profile.suggestions.remove(id);
        }
        CharacterOperationAction::UpsertExtension {
            namespace,
            extension,
        } => {
            profile
                .extensions
                .insert(namespace.clone(), extension.clone());
        }
        CharacterOperationAction::RemoveExtension { namespace } => {
            profile.extensions.remove(namespace);
        }
    }

    if current_value_hash(profile, &operation.action)?.is_some() {
        origins.insert(target, overlay_origin(overlay, operation));
    } else {
        origins.remove(&target);
    }
    Ok(())
}

fn identity_presentation_mut(
    profile: &mut CharacterProfile,
) -> Result<&mut IdentityPresentation, CharacterError> {
    if !profile
        .extensions
        .contains_key(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        let mut lineage = profile
            .provenance
            .sources
            .iter()
            .map(|source| source.id.clone())
            .chain(
                profile
                    .provenance
                    .transformations
                    .iter()
                    .map(|transformation| transformation.id.clone()),
            )
            .collect::<Vec<_>>();
        lineage.sort();
        lineage.dedup();
        profile.extensions.insert(
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE.to_owned(),
            CharacterExtension::IdentityPresentation(VersionedExtension {
                header: ExtensionHeader {
                    namespace: crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE.to_owned(),
                    extension_version: 1,
                    authority: "weave-character sparse presentation authoring".to_owned(),
                    rationale:
                        "Typed identity presentation fields remain separate from character canon."
                            .to_owned(),
                    state: ValueState::Authored,
                    review: ReviewState::NotRequired,
                    lock: LockState::Unlocked,
                    freshness: Freshness::Current,
                    lineage,
                    canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                },
                value: IdentityPresentation::default(),
            }),
        );
    }
    existing_identity_presentation_mut(profile)?.ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "extensions.org.weave.character.identity_presentation",
            "identity presentation namespace has an incompatible extension kind",
        )
    })
}

fn existing_identity_presentation_mut(
    profile: &mut CharacterProfile,
) -> Result<Option<&mut IdentityPresentation>, CharacterError> {
    match profile
        .extensions
        .get_mut(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::IdentityPresentation(record)) => Ok(Some(&mut record.value)),
        Some(_) => Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "extensions.org.weave.character.identity_presentation",
            "identity presentation namespace has an incompatible extension kind",
        )),
        None => Ok(None),
    }
}

fn validate_expected_prior(
    operation: &CharacterOperation,
    actual: Option<&str>,
) -> Result<(), CharacterError> {
    match (actual, operation.expected_prior_sha256.as_deref()) {
        (None, None) => {
            if incoming_authority(&operation.action)
                .is_some_and(|(_, state)| state == ValueState::Overridden)
            {
                return Err(error(
                    CharacterDiagnosticCode::ConflictingOverlay,
                    operation_target(&operation.action),
                    "an overridden value requires an existing fingerprinted value",
                ));
            }
            Ok(())
        }
        (Some(actual), Some(expected)) if actual == expected => Ok(()),
        (Some(_), None) => Err(error(
            CharacterDiagnosticCode::ConflictingOverlay,
            operation_target(&operation.action),
            "replacing or removing a present value requires its exact fingerprint",
        )),
        _ => Err(error(
            CharacterDiagnosticCode::StaleInput,
            operation_target(&operation.action),
            "operation fingerprint does not match the current value",
        )),
    }
}

fn validate_authority(
    profile: &CharacterProfile,
    operation: &CharacterOperation,
) -> Result<(), CharacterError> {
    let Some((existing_lock, existing_state)) = existing_authority(profile, &operation.action)
    else {
        return Ok(());
    };
    let incoming = incoming_authority(&operation.action);
    if existing_lock == LockState::Locked
        && incoming.is_none_or(|(_, state)| state != ValueState::Overridden)
    {
        return Err(error(
            CharacterDiagnosticCode::LockedField,
            operation_target(&operation.action),
            "locked field requires an explicit overridden value",
        ));
    }
    if let Some((_, incoming_state)) = incoming
        && incoming_state.precedence() < existing_state.precedence()
    {
        return Err(error(
            CharacterDiagnosticCode::ConflictingOverlay,
            operation_target(&operation.action),
            "lower-precedence value cannot replace the current value",
        ));
    }
    Ok(())
}

fn existing_authority(
    profile: &CharacterProfile,
    action: &CharacterOperationAction,
) -> Option<(LockState, ValueState)> {
    match action {
        CharacterOperationAction::SetDisplayName { .. } => {
            Some(authority(&profile.canon.identity.display_name))
        }
        CharacterOperationAction::SetAliases { .. } | CharacterOperationAction::ClearAliases => {
            profile.canon.identity.aliases.as_ref().map(authority)
        }
        CharacterOperationAction::SetPronouns { .. } | CharacterOperationAction::ClearPronouns => {
            presentation_authority(
                profile,
                identity_presentation(profile).and_then(|value| value.pronouns.as_ref()),
            )
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => presentation_authority(
            profile,
            identity_presentation(profile)
                .and_then(|value| value.context_notes.get(&record.id))
                .map(|record| &record.content),
        ),
        CharacterOperationAction::RemoveIdentityContextNote { id } => presentation_authority(
            profile,
            identity_presentation(profile)
                .and_then(|value| value.context_notes.get(id))
                .map(|record| &record.content),
        ),
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => presentation_authority(
            profile,
            identity_presentation(profile)
                .and_then(|value| value.appearance.get(&record.id))
                .map(|record| &record.content),
        ),
        CharacterOperationAction::RemoveAppearanceDescriptor { id } => presentation_authority(
            profile,
            identity_presentation(profile)
                .and_then(|value| value.appearance.get(id))
                .map(|record| &record.content),
        ),
        CharacterOperationAction::SetPresentationPalette { .. }
        | CharacterOperationAction::ClearPresentationPalette => presentation_authority(
            profile,
            identity_presentation(profile).and_then(|value| value.palette.as_ref()),
        ),
        CharacterOperationAction::SetPresentationStyleTags { .. }
        | CharacterOperationAction::ClearPresentationStyleTags => presentation_authority(
            profile,
            identity_presentation(profile).and_then(|value| value.style_tags.as_ref()),
        ),
        CharacterOperationAction::UpsertPresentationAsset { value } => presentation_authority(
            profile,
            identity_presentation(profile)
                .and_then(|presentation| presentation.assets.get(&value.value.id)),
        ),
        CharacterOperationAction::RemovePresentationAsset { id } => presentation_authority(
            profile,
            identity_presentation(profile).and_then(|value| value.assets.get(id)),
        ),
        CharacterOperationAction::UpsertPresentationAssignment { value } => presentation_authority(
            profile,
            identity_presentation(profile).and_then(|presentation| {
                presentation.catalog_assignments.get(&value.value.slot_id)
            }),
        ),
        CharacterOperationAction::RemovePresentationAssignment { slot_id } => {
            presentation_authority(
                profile,
                identity_presentation(profile)
                    .and_then(|value| value.catalog_assignments.get(slot_id)),
            )
        }
        CharacterOperationAction::SetBirthDate { .. }
        | CharacterOperationAction::ClearBirthDate => {
            profile.canon.birth_date.as_ref().map(authority)
        }
        CharacterOperationAction::SetHexacoTrait { trait_id, .. }
        | CharacterOperationAction::ClearHexacoTrait { trait_id } => {
            trait_value(&profile.canon.personality, *trait_id).map(authority)
        }
        CharacterOperationAction::UpsertInnerLife { record } => profile
            .canon
            .inner_life
            .get(&record.id)
            .map(|record| authority(&record.content)),
        CharacterOperationAction::RemoveInnerLife { id } => profile
            .canon
            .inner_life
            .get(id)
            .map(|record| authority(&record.content)),
        CharacterOperationAction::UpsertVoice { record } => profile
            .canon
            .voice
            .get(&record.id)
            .map(|record| authority(&record.content)),
        CharacterOperationAction::RemoveVoice { id } => profile
            .canon
            .voice
            .get(id)
            .map(|record| authority(&record.content)),
        CharacterOperationAction::UpsertSuggestion { suggestion } => profile
            .suggestions
            .get(&suggestion.id)
            .map(|record| authority(&record.proposal)),
        CharacterOperationAction::RemoveSuggestion { id } => profile
            .suggestions
            .get(id)
            .map(|record| authority(&record.proposal)),
        CharacterOperationAction::UpsertExtension { namespace, .. }
        | CharacterOperationAction::RemoveExtension { namespace } => {
            profile.extensions.get(namespace).map(extension_authority)
        }
    }
}

fn incoming_authority(action: &CharacterOperationAction) -> Option<(LockState, ValueState)> {
    match action {
        CharacterOperationAction::SetDisplayName { value } => Some(authority(value)),
        CharacterOperationAction::SetAliases { value } => Some(authority(value)),
        CharacterOperationAction::SetPronouns { value } => Some(authority(value)),
        CharacterOperationAction::UpsertIdentityContextNote { record } => {
            Some(authority(&record.content))
        }
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => {
            Some(authority(&record.content))
        }
        CharacterOperationAction::SetPresentationPalette { value } => Some(authority(value)),
        CharacterOperationAction::SetPresentationStyleTags { value } => Some(authority(value)),
        CharacterOperationAction::UpsertPresentationAsset { value } => Some(authority(value)),
        CharacterOperationAction::UpsertPresentationAssignment { value } => Some(authority(value)),
        CharacterOperationAction::SetBirthDate { value } => Some(authority(value)),
        CharacterOperationAction::SetHexacoTrait { value, .. } => Some(authority(value)),
        CharacterOperationAction::UpsertInnerLife { record } => Some(authority(&record.content)),
        CharacterOperationAction::UpsertVoice { record } => Some(authority(&record.content)),
        CharacterOperationAction::UpsertSuggestion { suggestion } => {
            Some(authority(&suggestion.proposal))
        }
        CharacterOperationAction::UpsertExtension { extension, .. } => {
            Some(extension_authority(extension))
        }
        CharacterOperationAction::ClearAliases
        | CharacterOperationAction::ClearPronouns
        | CharacterOperationAction::RemoveIdentityContextNote { .. }
        | CharacterOperationAction::RemoveAppearanceDescriptor { .. }
        | CharacterOperationAction::ClearPresentationPalette
        | CharacterOperationAction::ClearPresentationStyleTags
        | CharacterOperationAction::RemovePresentationAsset { .. }
        | CharacterOperationAction::RemovePresentationAssignment { .. }
        | CharacterOperationAction::ClearBirthDate
        | CharacterOperationAction::ClearHexacoTrait { .. }
        | CharacterOperationAction::RemoveInnerLife { .. }
        | CharacterOperationAction::RemoveVoice { .. }
        | CharacterOperationAction::RemoveSuggestion { .. }
        | CharacterOperationAction::RemoveExtension { .. } => None,
    }
}

fn identity_presentation(profile: &CharacterProfile) -> Option<&IdentityPresentation> {
    match profile
        .extensions
        .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::IdentityPresentation(record)) => Some(&record.value),
        _ => None,
    }
}

fn presentation_authority<T>(
    profile: &CharacterProfile,
    value: Option<&Attributed<T>>,
) -> Option<(LockState, ValueState)> {
    value.map(authority).or_else(|| {
        let CharacterExtension::IdentityPresentation(record) = profile
            .extensions
            .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)?
        else {
            return None;
        };
        (record.header.lock == LockState::Locked).then(|| {
            extension_authority(
                profile
                    .extensions
                    .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
                    .expect("presentation extension just matched"),
            )
        })
    })
}

fn authority<T>(value: &Attributed<T>) -> (LockState, ValueState) {
    (value.lock, value.state)
}

fn extension_authority(extension: &CharacterExtension) -> (LockState, ValueState) {
    let header = match extension {
        CharacterExtension::IdentityPresentation(record) => &record.header,
        CharacterExtension::Expression(record) => &record.header,
        CharacterExtension::BehavioralSignatures(record) => &record.header,
        CharacterExtension::RoleProjections(record) => &record.header,
        CharacterExtension::Relationships(record) => &record.header,
        CharacterExtension::AlignmentView(record) => &record.header,
        CharacterExtension::DateContext(record) => &record.header,
        CharacterExtension::Tabletop(record) | CharacterExtension::Opaque(record) => &record.header,
    };
    (header.lock, header.state)
}

pub(crate) fn current_value_hash(
    profile: &CharacterProfile,
    action: &CharacterOperationAction,
) -> Result<Option<String>, CharacterError> {
    match action {
        CharacterOperationAction::SetDisplayName { .. } => {
            hash_optional(Some(&profile.canon.identity.display_name))
        }
        CharacterOperationAction::SetAliases { .. } | CharacterOperationAction::ClearAliases => {
            hash_optional(profile.canon.identity.aliases.as_ref())
        }
        CharacterOperationAction::SetPronouns { .. } | CharacterOperationAction::ClearPronouns => {
            hash_optional(identity_presentation(profile).and_then(|value| value.pronouns.as_ref()))
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => hash_optional(
            identity_presentation(profile).and_then(|value| value.context_notes.get(&record.id)),
        ),
        CharacterOperationAction::RemoveIdentityContextNote { id } => hash_optional(
            identity_presentation(profile).and_then(|value| value.context_notes.get(id)),
        ),
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => hash_optional(
            identity_presentation(profile).and_then(|value| value.appearance.get(&record.id)),
        ),
        CharacterOperationAction::RemoveAppearanceDescriptor { id } => {
            hash_optional(identity_presentation(profile).and_then(|value| value.appearance.get(id)))
        }
        CharacterOperationAction::SetPresentationPalette { .. }
        | CharacterOperationAction::ClearPresentationPalette => {
            hash_optional(identity_presentation(profile).and_then(|value| value.palette.as_ref()))
        }
        CharacterOperationAction::SetPresentationStyleTags { .. }
        | CharacterOperationAction::ClearPresentationStyleTags => hash_optional(
            identity_presentation(profile).and_then(|value| value.style_tags.as_ref()),
        ),
        CharacterOperationAction::UpsertPresentationAsset { value } => hash_optional(
            identity_presentation(profile)
                .and_then(|presentation| presentation.assets.get(&value.value.id)),
        ),
        CharacterOperationAction::RemovePresentationAsset { id } => {
            hash_optional(identity_presentation(profile).and_then(|value| value.assets.get(id)))
        }
        CharacterOperationAction::UpsertPresentationAssignment { value } => {
            hash_optional(identity_presentation(profile).and_then(|presentation| {
                presentation.catalog_assignments.get(&value.value.slot_id)
            }))
        }
        CharacterOperationAction::RemovePresentationAssignment { slot_id } => hash_optional(
            identity_presentation(profile).and_then(|value| value.catalog_assignments.get(slot_id)),
        ),
        CharacterOperationAction::SetBirthDate { .. }
        | CharacterOperationAction::ClearBirthDate => {
            hash_optional(profile.canon.birth_date.as_ref())
        }
        CharacterOperationAction::SetHexacoTrait { trait_id, .. }
        | CharacterOperationAction::ClearHexacoTrait { trait_id } => {
            hash_optional(trait_value(&profile.canon.personality, *trait_id))
        }
        CharacterOperationAction::UpsertInnerLife { record } => {
            hash_optional(profile.canon.inner_life.get(&record.id))
        }
        CharacterOperationAction::RemoveInnerLife { id } => {
            hash_optional(profile.canon.inner_life.get(id))
        }
        CharacterOperationAction::UpsertVoice { record } => {
            hash_optional(profile.canon.voice.get(&record.id))
        }
        CharacterOperationAction::RemoveVoice { id } => hash_optional(profile.canon.voice.get(id)),
        CharacterOperationAction::UpsertSuggestion { suggestion } => {
            hash_optional(profile.suggestions.get(&suggestion.id))
        }
        CharacterOperationAction::RemoveSuggestion { id } => {
            hash_optional(profile.suggestions.get(id))
        }
        CharacterOperationAction::UpsertExtension { namespace, .. }
        | CharacterOperationAction::RemoveExtension { namespace } => {
            hash_optional(profile.extensions.get(namespace))
        }
    }
}

fn hash_optional<T: Serialize>(value: Option<&T>) -> Result<Option<String>, CharacterError> {
    value
        .map(|value| {
            serde_json::to_vec(value)
                .map(|bytes| sha256(&bytes))
                .map_err(|_| {
                    error(
                        CharacterDiagnosticCode::InvalidEncoding,
                        "operation",
                        "could not fingerprint the current Character value",
                    )
                })
        })
        .transpose()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn merge_provenance(
    base: &Provenance,
    overlay: &Provenance,
) -> Result<Provenance, CharacterError> {
    let mut sources = BTreeMap::new();
    for source in base.sources.iter().chain(&overlay.sources) {
        if let Some(existing) = sources.insert(source.id.clone(), source.clone())
            && existing != *source
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.sources",
                "template and overlay reuse one source identifier differently",
            ));
        }
    }
    let mut transformations = BTreeMap::new();
    for transformation in base.transformations.iter().chain(&overlay.transformations) {
        if sources.contains_key(&transformation.id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.transformations",
                "source and transformation identifiers must remain distinct",
            ));
        }
        if let Some(existing) =
            transformations.insert(transformation.id.clone(), transformation.clone())
            && existing != *transformation
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.transformations",
                "template and overlay reuse one transformation identifier differently",
            ));
        }
    }
    let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (claim, references) in base.claims.iter().chain(&overlay.claims) {
        claims
            .entry(claim.clone())
            .or_default()
            .extend(references.iter().cloned());
    }
    Ok(Provenance {
        sources: sources.into_values().collect(),
        transformations: transformations.into_values().collect(),
        claims: claims
            .into_iter()
            .map(|(claim, references)| (claim, references.into_iter().collect()))
            .collect(),
    })
}

fn template_origins(
    template: &CharacterTemplate,
    effective_profile: &CharacterProfile,
) -> BTreeMap<String, SynthesisOrigin> {
    profile_field_paths(effective_profile)
        .into_iter()
        .map(|path| {
            let origin = SynthesisOrigin::Template {
                template_id: template.id.clone(),
                template_version: template.version.clone(),
                source_path: path.clone(),
            };
            (path, origin)
        })
        .collect()
}

pub(crate) fn profile_field_paths(profile: &CharacterProfile) -> Vec<String> {
    let mut paths = vec!["canon.identity.display_name".to_owned()];
    if profile.canon.identity.aliases.is_some() {
        paths.push("canon.identity.aliases".to_owned());
    }
    if profile.canon.birth_date.is_some() {
        paths.push("canon.birth_date".to_owned());
    }
    paths.extend(
        ALL_HEXACO_TRAITS
            .into_iter()
            .filter(|trait_id| trait_value(&profile.canon.personality, *trait_id).is_some())
            .map(trait_path),
    );
    paths.extend(
        profile
            .canon
            .inner_life
            .keys()
            .map(|id| format!("canon.inner_life.{id}")),
    );
    paths.extend(
        profile
            .canon
            .voice
            .keys()
            .map(|id| format!("canon.voice.{id}")),
    );
    paths.extend(
        profile
            .extensions
            .keys()
            .filter(|namespace| {
                namespace.as_str() != crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
            })
            .map(|namespace| format!("extensions.{namespace}")),
    );
    if let Some(presentation) = identity_presentation(profile) {
        let root = format!(
            "extensions.{}.value",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
        );
        if !presentation.identity_refs.is_empty() {
            paths.push(format!("{root}.identity_refs"));
        }
        if !presentation.presentation_refs.is_empty() {
            paths.push(format!("{root}.presentation_refs"));
        }
        if presentation.pronouns.is_some() {
            paths.push(format!("{root}.pronouns"));
        }
        paths.extend(
            presentation
                .context_notes
                .keys()
                .map(|id| format!("{root}.context_notes.{id}")),
        );
        paths.extend(
            presentation
                .appearance
                .keys()
                .map(|id| format!("{root}.appearance.{id}")),
        );
        if presentation.palette.is_some() {
            paths.push(format!("{root}.palette"));
        }
        if presentation.style_tags.is_some() {
            paths.push(format!("{root}.style_tags"));
        }
        paths.extend(
            presentation
                .assets
                .keys()
                .map(|id| format!("{root}.assets.{id}")),
        );
        paths.extend(
            presentation
                .catalog_assignments
                .keys()
                .map(|id| format!("{root}.catalog_assignments.{id}")),
        );
    }
    paths.extend(
        profile
            .suggestions
            .keys()
            .map(|id| format!("suggestions.{id}")),
    );
    paths.sort();
    paths
}

fn overlay_origin(overlay: &CharacterOverlay, operation: &CharacterOperation) -> SynthesisOrigin {
    SynthesisOrigin::Overlay {
        overlay_id: overlay.id.clone(),
        operation_id: operation.id.clone(),
    }
}
