use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use weave_character::{
    Agreeableness, AlignmentView, Attributed, AuthoredNote, BehavioralSignature,
    BehavioralSignatures, BirthDate, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_PROFILE_FORMAT_VERSION, CHARACTER_TEMPLATE_FORMAT_VERSION, Calendar, CharacterCanon,
    CharacterDerivedViews, CharacterExtension, CharacterIdentity, CharacterOperation,
    CharacterOperationAction, CharacterOverlay, CharacterProfile, CharacterSuggestion,
    CharacterTemplate, CharacterTemplateRef, Confidence, Conscientiousness, DateContext,
    Emotionality, ExpressionData, ExtensionHeader, ExtensionWriteBack, Extraversion, Freshness,
    HexacoProfile, HonestyHumility, IdentityPresentation, InnerLifeCategory, LockState,
    NormalizedExpressionTerm, NormalizedPreference, OpaqueExtensionData, OpaqueInterpretation,
    Openness, PreferencePolarity, RelationshipEdge, RelationshipEdges, ReviewState, RoleProjection,
    RoleProjections, TraitMeasurement, ValueState, VersionedExtension, VoiceCategory,
    VoiceDirection, character_diagnostic_schema, character_overlay_schema,
    character_profile_schema, character_synthesis_schema, character_template_schema,
    recompute_derived, synthesize_character, template_fingerprint,
};
use weave_domain::{DomainValue, Provenance, ProvenanceKind, ProvenanceSource, to_pretty_json};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--check".to_owned());
    let write = match mode.as_str() {
        "--write" => true,
        "--check" => false,
        _ => return Err("expected --write or --check".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root")?
        .to_path_buf();
    let fixture = root.join("examples/domain-modules/weave-character");
    let schemas = root.join("schemas");

    let profile = complete_profile();
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: "org.weave.character.template.glasswind_wayfinder".to_owned(),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = overlay(&template)?;
    let synthesis = synthesize_character(Some(&template), &overlay)?;
    let mut omitted = profile.clone();
    omitted.extensions.clear();
    omitted.suggestions.clear();
    recompute_derived(&mut omitted);
    let unknown_extension = synthesis.effective_profile.clone();

    write_pair(&fixture, "profile.character", &profile, write)?;
    write_pair(&fixture, "template.character", &template, write)?;
    write_pair(&fixture, "overlay.character", &overlay, write)?;
    write_pair(&fixture, "synthesis.character", &synthesis, write)?;
    write_pair(&fixture, "omitted-extensions.character", &omitted, write)?;
    write_pair(
        &fixture,
        "unknown-extension-preserved.character",
        &unknown_extension,
        write,
    )?;

    let invalid = fixture.join("invalid");
    let mut unknown_profile_version = serde_json::to_value(&profile)?;
    unknown_profile_version["profile_format_version"] = serde_json::Value::from(2);
    write_json_value(
        &invalid.join("unknown-profile-version.character.json"),
        &unknown_profile_version,
        write,
    )?;

    let mut conflicting_overlay = overlay.clone();
    let mut duplicate = conflicting_overlay.operations[0].clone();
    duplicate.id = "rename_again".to_owned();
    conflicting_overlay.operations.insert(1, duplicate);
    write_raw_json(
        &invalid.join("conflicting-overlay.character.json"),
        &conflicting_overlay,
        write,
    )?;

    let mut stale_overlay = overlay.clone();
    stale_overlay
        .template
        .as_mut()
        .expect("checked template reference")
        .sha256 = "0".repeat(64);
    write_raw_json(
        &invalid.join("stale-overlay.character.json"),
        &stale_overlay,
        write,
    )?;

    let mut invalid_reference = profile.clone();
    let CharacterExtension::Relationships(relationships) = invalid_reference
        .extensions
        .get_mut("org.weave.character.relationships")
        .expect("relationship fixture")
    else {
        return Err("relationship fixture changed kind".into());
    };
    relationships
        .value
        .edges
        .get_mut("mentor_sable")
        .expect("relationship edge")
        .target_character_id
        .clone_from(&profile.id);
    write_raw_json(
        &invalid.join("invalid-reference.character.json"),
        &invalid_reference,
        write,
    )?;

    let mut unknown_typed_extension = profile.clone();
    let CharacterExtension::AlignmentView(alignment) = unknown_typed_extension
        .extensions
        .get_mut("org.weave.character.alignment")
        .expect("alignment fixture")
    else {
        return Err("alignment fixture changed kind".into());
    };
    alignment.header.extension_version = 2;
    write_raw_json(
        &invalid.join("unknown-typed-extension-version.character.json"),
        &unknown_typed_extension,
        write,
    )?;

    let mut derived_in_canon = profile.clone();
    derived_in_canon
        .canon
        .personality
        .openness
        .creativity
        .as_mut()
        .expect("creativity fixture")
        .state = ValueState::Derived;
    write_raw_json(
        &invalid.join("derived-canonical-evidence.character.json"),
        &derived_in_canon,
        write,
    )?;

    for (name, contents) in [
        (
            "weave-character-profile-v1.schema.json",
            character_profile_schema()?,
        ),
        (
            "weave-character-template-v1.schema.json",
            character_template_schema()?,
        ),
        (
            "weave-character-overlay-v1.schema.json",
            character_overlay_schema()?,
        ),
        (
            "weave-character-synthesis-v1.schema.json",
            character_synthesis_schema()?,
        ),
        (
            "weave-character-diagnostic-v1.schema.json",
            character_diagnostic_schema()?,
        ),
    ] {
        write_or_check(&schemas.join(name), contents.as_bytes(), write)?;
    }
    Ok(())
}

fn complete_profile() -> CharacterProfile {
    let lineage = vec!["character_original".to_owned()];
    let mut personality = HexacoProfile {
        honesty_humility: HonestyHumility {
            factor: Some(trait_value(0.72, &lineage)),
            sincerity: Some(trait_value(0.76, &lineage)),
            fairness: Some(trait_value(0.81, &lineage)),
            greed_avoidance: Some(trait_value(0.66, &lineage)),
            modesty: Some(trait_value(0.65, &lineage)),
        },
        emotionality: Emotionality {
            factor: Some(trait_value(0.57, &lineage)),
            fearfulness: Some(trait_value(0.42, &lineage)),
            anxiety: Some(trait_value(0.58, &lineage)),
            dependence: Some(trait_value(0.49, &lineage)),
            sentimentality: Some(trait_value(0.79, &lineage)),
        },
        extraversion: Extraversion {
            factor: Some(trait_value(0.68, &lineage)),
            social_self_esteem: Some(trait_value(0.73, &lineage)),
            social_boldness: Some(trait_value(0.61, &lineage)),
            sociability: Some(trait_value(0.64, &lineage)),
            liveliness: Some(trait_value(0.74, &lineage)),
        },
        agreeableness: Agreeableness {
            factor: Some(trait_value(0.63, &lineage)),
            forgivingness: Some(trait_value(0.55, &lineage)),
            gentleness: Some(trait_value(0.71, &lineage)),
            flexibility: Some(trait_value(0.59, &lineage)),
            patience: Some(trait_value(0.67, &lineage)),
        },
        conscientiousness: Conscientiousness {
            factor: Some(trait_value(0.78, &lineage)),
            organization: Some(trait_value(0.75, &lineage)),
            diligence: Some(trait_value(0.84, &lineage)),
            perfectionism: Some(trait_value(0.69, &lineage)),
            prudence: Some(trait_value(0.82, &lineage)),
        },
        openness: Openness {
            factor: Some(trait_value(0.83, &lineage)),
            aesthetic_appreciation: Some(trait_value(0.88, &lineage)),
            inquisitiveness: Some(trait_value(0.81, &lineage)),
            creativity: Some(trait_value(0.86, &lineage)),
            unconventionality: Some(trait_value(0.77, &lineage)),
        },
    };
    personality
        .conscientiousness
        .prudence
        .as_mut()
        .expect("prudence fixture")
        .lock = LockState::Locked;

    let provenance = original_provenance("character_original", "profile");
    let mut profile = CharacterProfile {
        profile_format_version: CHARACTER_PROFILE_FORMAT_VERSION,
        id: "org.weave.character.ari_vale".to_owned(),
        canon: CharacterCanon {
            identity: CharacterIdentity {
                display_name: authored("Ari Vale".to_owned(), &lineage),
                aliases: Some(authored(
                    vec!["Ari".to_owned(), "Vale".to_owned()],
                    &lineage,
                )),
            },
            birth_date: Some(authored(
                BirthDate::Full {
                    calendar: Calendar::ProlepticGregorian,
                    year: 998,
                    month: 3,
                    day: 14,
                },
                &lineage,
            )),
            personality,
            inner_life: BTreeMap::from([(
                "keeps_promises".to_owned(),
                AuthoredNote {
                    id: "keeps_promises".to_owned(),
                    category: InnerLifeCategory::Value,
                    content: authored(
                        "Promises become paths Ari can follow through uncertainty.".to_owned(),
                        &lineage,
                    ),
                },
            )]),
            voice: BTreeMap::from([(
                "measured_warmth".to_owned(),
                VoiceDirection {
                    id: "measured_warmth".to_owned(),
                    category: VoiceCategory::Cadence,
                    content: authored(
                        "Short observations followed by one generous question.".to_owned(),
                        &lineage,
                    ),
                },
            )]),
        },
        extensions: extensions(&lineage),
        suggestions: BTreeMap::from([(
            "night_market_memory".to_owned(),
            CharacterSuggestion {
                id: "night_market_memory".to_owned(),
                target_path: "canon.inner_life.night_market_memory".to_owned(),
                proposal: suggested(
                    DomainValue::String(
                        "A remembered lantern exchange may create a useful tension.".to_owned(),
                    ),
                    &lineage,
                ),
            },
        )]),
        derived: CharacterDerivedViews {
            ocean: weave_character::derive_ocean(&HexacoProfile::default()),
        },
        provenance,
    };
    recompute_derived(&mut profile);
    profile
}

fn extensions(lineage: &[String]) -> BTreeMap<String, CharacterExtension> {
    let identity_namespace = "org.weave.character.identity_presentation";
    let expression_namespace = "org.weave.character.expression";
    let behavior_namespace = "org.weave.character.behavioral_signatures";
    let role_namespace = "org.weave.character.role_projections";
    let relationship_namespace = "org.weave.character.relationships";
    let alignment_namespace = "org.weave.character.alignment";
    let date_namespace = "org.weave.character.date_context";
    let tabletop_namespace = "org.weave.character.tabletop";
    BTreeMap::from([
        (
            alignment_namespace.to_owned(),
            CharacterExtension::AlignmentView(VersionedExtension {
                header: extension_header(alignment_namespace, 1, lineage),
                value: AlignmentView {
                    view_id: "org.weave.alignment.compass".to_owned(),
                    values: DomainValue::Object(BTreeMap::from([(
                        "orientation".to_owned(),
                        DomainValue::Symbol("steward".to_owned()),
                    )])),
                    input_paths: vec![
                        "canon.personality.agreeableness.factor".to_owned(),
                        "canon.personality.honesty_humility.factor".to_owned(),
                    ],
                },
            }),
        ),
        (
            behavior_namespace.to_owned(),
            CharacterExtension::BehavioralSignatures(VersionedExtension {
                header: extension_header(behavior_namespace, 1, lineage),
                value: BehavioralSignatures {
                    signatures: BTreeMap::from([(
                        "maps_before_moving".to_owned(),
                        BehavioralSignature {
                            id: "maps_before_moving".to_owned(),
                            cue: "Sketches a route before committing the group.".to_owned(),
                            strength: 0.8,
                        },
                    )]),
                },
            }),
        ),
        (
            date_namespace.to_owned(),
            CharacterExtension::DateContext(VersionedExtension {
                header: extension_header(date_namespace, 1, lineage),
                value: DateContext {
                    context_pack: "org.weave.context.synthetic_calendar".to_owned(),
                    context_version: "1.0.0".to_owned(),
                    context_hash: "a".repeat(64),
                    accepted_record_ids: vec!["early_rains".to_owned()],
                },
            }),
        ),
        (
            expression_namespace.to_owned(),
            CharacterExtension::Expression(VersionedExtension {
                header: extension_header(expression_namespace, 1, lineage),
                value: ExpressionData {
                    lexicon: BTreeMap::from([(
                        "waymark".to_owned(),
                        NormalizedExpressionTerm {
                            id: "waymark".to_owned(),
                            category: "org.weave.expression.navigation".to_owned(),
                            normalized: "waymark".to_owned(),
                            strength: 0.8,
                        },
                    )]),
                    preferences: BTreeMap::from([(
                        "clear_questions".to_owned(),
                        NormalizedPreference {
                            id: "clear_questions".to_owned(),
                            category: "org.weave.preference.communication".to_owned(),
                            target: "clear questions".to_owned(),
                            polarity: PreferencePolarity::Prefer,
                            strength: 0.9,
                        },
                    )]),
                    behavioral_signature_refs: vec![
                        "org.weave.signature.maps_before_moving".to_owned(),
                    ],
                    source_pack_refs: vec!["org.weave.expression.glasswind".to_owned()],
                },
            }),
        ),
        (
            identity_namespace.to_owned(),
            CharacterExtension::IdentityPresentation(VersionedExtension {
                header: extension_header(identity_namespace, 1, lineage),
                value: IdentityPresentation {
                    identity_refs: vec!["org.weave.identity.wayfinder".to_owned()],
                    presentation_refs: vec![
                        "assets/avatars/ari_vale.png".to_owned(),
                        "assets/palettes/cedar_snow.json".to_owned(),
                    ],
                },
            }),
        ),
        (
            relationship_namespace.to_owned(),
            CharacterExtension::Relationships(VersionedExtension {
                header: extension_header(relationship_namespace, 1, lineage),
                value: RelationshipEdges {
                    edges: BTreeMap::from([(
                        "mentor_sable".to_owned(),
                        RelationshipEdge {
                            id: "mentor_sable".to_owned(),
                            source_character_id: "org.weave.character.ari_vale".to_owned(),
                            target_character_id: "org.weave.character.sable_reed".to_owned(),
                            kind: "org.weave.relationship.mentor".to_owned(),
                            confidence: Confidence::High,
                        },
                    )]),
                },
            }),
        ),
        (
            role_namespace.to_owned(),
            CharacterExtension::RoleProjections(VersionedExtension {
                header: extension_header(role_namespace, 1, lineage),
                value: RoleProjections {
                    roles: BTreeMap::from([(
                        "route_steward".to_owned(),
                        RoleProjection {
                            id: "route_steward".to_owned(),
                            taxonomy: "org.weave.roles.glasswind".to_owned(),
                            role: "Route steward".to_owned(),
                            rationale: "Accepted as a narrative role, not a personality fact."
                                .to_owned(),
                            input_paths: vec![
                                "canon.personality.conscientiousness.factor".to_owned(),
                                "canon.personality.openness.factor".to_owned(),
                            ],
                        },
                    )]),
                },
            }),
        ),
        (
            tabletop_namespace.to_owned(),
            CharacterExtension::Tabletop(VersionedExtension {
                header: extension_header(tabletop_namespace, 1, lineage),
                value: OpaqueExtensionData {
                    interpretation: OpaqueInterpretation::PreservedInactive,
                    payload: DomainValue::Object(BTreeMap::from([(
                        "ruleset_ref".to_owned(),
                        DomainValue::String("org.weave.rules.synthetic@1".to_owned()),
                    )])),
                },
            }),
        ),
    ])
}

fn overlay(template: &CharacterTemplate) -> Result<CharacterOverlay, Box<dyn std::error::Error>> {
    let lineage = vec!["overlay_original".to_owned()];
    let display_hash = value_hash(&template.profile.canon.identity.display_name)?;
    let prudence_hash = value_hash(
        template
            .profile
            .canon
            .personality
            .conscientiousness
            .prudence
            .as_ref()
            .expect("prudence fixture"),
    )?;
    Ok(CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: "org.weave.character.overlay.ari_vale_revision".to_owned(),
        character_id: template.profile.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(template)?,
        }),
        operations: vec![
            CharacterOperation {
                id: "rename_for_glasswind".to_owned(),
                expected_prior_sha256: Some(display_hash),
                rationale: "Make the authored setting identity explicit.".to_owned(),
                action: CharacterOperationAction::SetDisplayName {
                    value: overridden("Ari Vale of Glasswind".to_owned(), &lineage),
                },
            },
            CharacterOperation {
                id: "confirm_prudence".to_owned(),
                expected_prior_sha256: Some(prudence_hash),
                rationale: "Explicitly replace a locked template trait after review.".to_owned(),
                action: CharacterOperationAction::SetHexacoTrait {
                    trait_id: weave_character::HexacoTrait::Prudence,
                    value: overridden(
                        TraitMeasurement::Band {
                            band: weave_character::TraitBand::VeryHigh,
                        },
                        &lineage,
                    ),
                },
            },
            CharacterOperation {
                id: "preserve_future_palette".to_owned(),
                expected_prior_sha256: None,
                rationale: "Retain an unknown optional extension without interpreting it."
                    .to_owned(),
                action: CharacterOperationAction::UpsertExtension {
                    namespace: "org.weave.character.future_palette".to_owned(),
                    extension: CharacterExtension::Opaque(VersionedExtension {
                        header: ExtensionHeader {
                            namespace: "org.weave.character.future_palette".to_owned(),
                            extension_version: 99,
                            authority: "org.weave.extension.future_palette".to_owned(),
                            rationale: "Preserved for a future compatible host.".to_owned(),
                            state: ValueState::Imported,
                            review: ReviewState::Accepted,
                            lock: LockState::Locked,
                            freshness: Freshness::Current,
                            lineage: lineage.clone(),
                            canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                        },
                        value: OpaqueExtensionData {
                            interpretation: OpaqueInterpretation::PreservedInactive,
                            payload: DomainValue::Object(BTreeMap::from([(
                                "accent".to_owned(),
                                DomainValue::Symbol("pine".to_owned()),
                            )])),
                        },
                    }),
                },
            },
        ],
        provenance: original_provenance("overlay_original", "overlay"),
    })
}

fn trait_value(score: f64, lineage: &[String]) -> Attributed<TraitMeasurement> {
    authored(TraitMeasurement::Score { score }, lineage)
}

fn authored<T>(value: T, lineage: &[String]) -> Attributed<T> {
    Attributed {
        value,
        state: ValueState::Authored,
        confidence: Confidence::High,
        review: ReviewState::NotRequired,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: None,
    }
}

fn overridden<T>(value: T, lineage: &[String]) -> Attributed<T> {
    Attributed {
        value,
        state: ValueState::Overridden,
        confidence: Confidence::High,
        review: ReviewState::Accepted,
        lock: LockState::Locked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: Some("Reviewed fictional author decision.".to_owned()),
    }
}

fn suggested(value: DomainValue, lineage: &[String]) -> Attributed<DomainValue> {
    Attributed {
        value,
        state: ValueState::Suggested,
        confidence: Confidence::Low,
        review: ReviewState::Pending,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        rationale: Some("Optional synthetic authoring prompt.".to_owned()),
    }
}

fn extension_header(namespace: &str, version: u32, lineage: &[String]) -> ExtensionHeader {
    ExtensionHeader {
        namespace: namespace.to_owned(),
        extension_version: version,
        authority: "org.weave.character.contract".to_owned(),
        rationale: "Explicit optional projection authored for the synthetic fixture.".to_owned(),
        state: ValueState::Reviewed,
        review: ReviewState::Accepted,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        canonical_personality_write_back: ExtensionWriteBack::Forbidden,
    }
}

fn original_provenance(source_id: &str, claim: &str) -> Provenance {
    Provenance {
        sources: vec![ProvenanceSource {
            id: source_id.to_owned(),
            kind: ProvenanceKind::Original,
            url: "https://github.com/chrisgliddon/weave".to_owned(),
            revision: "character-contract-v1".to_owned(),
            sha256: None,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            attribution: "Original synthetic Weave Character contract fixture.".to_owned(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: BTreeMap::from([(claim.to_owned(), vec![source_id.to_owned()])]),
    }
}

fn value_hash(value: &impl serde::Serialize) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn write_pair<T: serde::Serialize>(
    directory: &Path,
    stem: &str,
    value: &T,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = weave_domain::to_pretty_json(value)?;
    let ron = weave_domain::to_pretty_ron(value)?;
    write_or_check(
        &directory.join(format!("{stem}.json")),
        json.as_bytes(),
        write,
    )?;
    write_or_check(
        &directory.join(format!("{stem}.ron")),
        ron.as_bytes(),
        write,
    )
}

fn write_raw_json(
    path: &Path,
    value: &impl serde::Serialize,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = to_pretty_json(value)?;
    write_or_check(path, json.as_bytes(), write)
}

fn write_json_value(
    path: &Path,
    value: &serde_json::Value,
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = to_pretty_json(value)?;
    write_or_check(path, json.as_bytes(), write)
}

fn write_or_check(
    path: &Path,
    expected: &[u8],
    write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, expected)?;
        return Ok(());
    }
    let actual = fs::read(path)?;
    if actual != expected {
        return Err(format!("{} is stale", path.display()).into());
    }
    Ok(())
}
