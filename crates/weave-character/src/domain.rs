use std::collections::BTreeMap;
use std::fmt;

use semver::Version;
use weave_domain::{
    CapabilityDeclaration, DomainError, DomainPack, DomainValue, ExportDeclaration, ExportSource,
    FieldDeclaration, ModuleAuthor, ModuleAuthoring, ModuleManifest, ModuleRequirement, Provenance,
    ProvenanceKind, ProvenanceSource, ProvenanceTransformation, ReadOnlyPathDeclaration,
    TypeExpression, validate_manifest, validate_pack,
};

use crate::{
    AcceptedDateContextCue, AlignmentPackRef, AlignmentPublicDecision, ApprovedAlignmentValue,
    Attributed, CharacterError, CharacterExtension, CharacterProfile, Confidence,
    DateContextCueKind, DateContextDecision, DateContextSensitivity, DateContextUncertainty,
    DerivedTrait, Freshness, HexacoProfile, LockState, OceanView, ReviewState, TraitBand,
    TraitMeasurement, ValueState, validate_profile,
};

/// Exact release of the declarative Weave Character domain module.
pub const CHARACTER_DOMAIN_MODULE_VERSION: &str = "1.2.0";

/// Failure while projecting a validated Character Profile through the shared domain boundary.
#[derive(Debug)]
pub enum CharacterDomainError {
    /// The source Character Profile is invalid.
    Character(CharacterError),
    /// The shared domain artifact is invalid.
    Domain(DomainError),
    /// The crate's embedded compatibility version is invalid.
    Configuration,
    /// Input provenance already reserves the projection transformation identifier.
    ProvenanceCollision,
}

impl fmt::Display for CharacterDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(error) => error.fmt(formatter),
            Self::Domain(error) => error.fmt(formatter),
            Self::Configuration => {
                formatter.write_str("invalid embedded Character domain compatibility version")
            }
            Self::ProvenanceCollision => formatter.write_str(
                "Character provenance reserves the domain projection transformation identifier",
            ),
        }
    }
}

impl std::error::Error for CharacterDomainError {}

impl From<CharacterError> for CharacterDomainError {
    fn from(error: CharacterError) -> Self {
        Self::Character(error)
    }
}

impl From<DomainError> for CharacterDomainError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

/// Build the public, declarative module manifest used by source, editor, compiler, and hosts.
pub fn character_module_manifest() -> Result<ModuleManifest, CharacterDomainError> {
    let types = BTreeMap::from([
        (
            "Agreeableness".to_owned(),
            factor_type(&["flexibility", "forgivingness", "gentleness", "patience"]),
        ),
        ("AttributedText".to_owned(), attributed_text_type()),
        (
            "CharacterDateContext".to_owned(),
            character_date_context_type(),
        ),
        ("CharacterDateCue".to_owned(), character_date_cue_type()),
        ("CharacterDatePack".to_owned(), character_date_pack_type()),
        (
            "CharacterAlignmentView".to_owned(),
            character_alignment_view_type(),
        ),
        (
            "CharacterAlignmentValue".to_owned(),
            character_alignment_value_type(),
        ),
        (
            "CharacterAlignmentPack".to_owned(),
            character_alignment_pack_type(),
        ),
        ("CharacterIdentity".to_owned(), character_identity_type()),
        ("CharacterProfile".to_owned(), runtime_profile_type()),
        (
            "CharacterProvenance".to_owned(),
            character_provenance_type(),
        ),
        (
            "Conscientiousness".to_owned(),
            factor_type(&["diligence", "organization", "perfectionism", "prudence"]),
        ),
        (
            "Emotionality".to_owned(),
            factor_type(&["anxiety", "dependence", "fearfulness", "sentimentality"]),
        ),
        (
            "Extraversion".to_owned(),
            factor_type(&[
                "liveliness",
                "social_boldness",
                "social_self_esteem",
                "sociability",
            ]),
        ),
        ("HexacoProfile".to_owned(), hexaco_type()),
        (
            "HonestyHumility".to_owned(),
            factor_type(&["fairness", "greed_avoidance", "modesty", "sincerity"]),
        ),
        ("OceanDimension".to_owned(), ocean_dimension_type()),
        ("OceanView".to_owned(), ocean_type()),
        (
            "Openness".to_owned(),
            factor_type(&[
                "aesthetic_appreciation",
                "creativity",
                "inquisitiveness",
                "unconventionality",
            ]),
        ),
        ("TraitEvidence".to_owned(), trait_evidence_type()),
    ]);
    let manifest = ModuleManifest {
        contract_version: 1,
        pack_format_version: 1,
        id: crate::CHARACTER_MODULE_ID.to_owned(),
        version: CHARACTER_DOMAIN_MODULE_VERSION.to_owned(),
        namespace: "character".to_owned(),
        title: "Weave Character".to_owned(),
        summary: "Typed, provenance-aware Character Profiles with canonical HEXACO evidence and a visibly lossy derived OCEAN view.".to_owned(),
        authors: vec![ModuleAuthor {
            name: "Weave Contributors".to_owned(),
            url: Some("https://github.com/chrisgliddon/weave".to_owned()),
        }],
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
        weave_version: ">=0.1.0, <0.2.0".to_owned(),
        capabilities: vec![
            CapabilityDeclaration {
                id: "data".to_owned(),
                version: 1,
            },
            CapabilityDeclaration {
                id: "editor_schema".to_owned(),
                version: 1,
            },
        ],
        dependencies: Vec::new(),
        types,
        exports: BTreeMap::from([(
            "profile".to_owned(),
            ExportDeclaration {
                value_type: named("CharacterProfile"),
                required: true,
                source: ExportSource::Pack,
                description: "Validated runtime projection of one Character Profile; canonical evidence and derived compatibility fields retain explicit state and lineage.".to_owned(),
            },
        )]),
        authoring: ModuleAuthoring {
            entity_collections: Vec::new(),
            read_only_paths: [
                (vec!["profile", "alignment"], "Reviewed alignment values retain exact pack and review fingerprints and remain non-diagnostic read-only shorthand; canonical authoring requires a separate reviewed operation."),
                (vec!["profile", "date_context"], "Reviewed temporal cues retain exact lineage and remain non-causal read-only context; canonical authoring requires a separate reviewed operation."),
                (vec!["profile", "hexaco"], "Canonical HEXACO evidence is revised in the profile artifact so dependent views can be recomputed and reviewed."),
                (vec!["profile", "identity", "id"], "The stable character identifier is not a display label and cannot be rewritten by a story override."),
                (vec!["profile", "ocean"], "OCEAN is a lossy derived compatibility view and cannot be authored as independent evidence."),
                (vec!["profile", "profile_format_version"], "The profile contract version is fixed by the selected pack."),
                (vec!["profile", "provenance"], "Pack provenance is immutable and cannot be replaced by story source."),
            ]
            .into_iter()
            .map(|(path, reason)| ReadOnlyPathDeclaration {
                path: path.into_iter().map(str::to_owned).collect(),
                reason: reason.to_owned(),
            })
            .collect(),
        },
        provenance: Provenance {
            sources: vec![ProvenanceSource {
                id: "weave_character_module".to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave/tree/main/crates/weave-character"
                    .to_owned(),
                revision: "character-domain-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
                attribution: "Original Weave Character domain projection authored by Weave Contributors."
                    .to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([
                (
                    "authoring.read_only_paths".to_owned(),
                    vec!["weave_character_module".to_owned()],
                ),
                (
                    "exports.profile".to_owned(),
                    vec!["weave_character_module".to_owned()],
                ),
                (
                    "types.CharacterProfile".to_owned(),
                    vec!["weave_character_module".to_owned()],
                ),
            ]),
        },
    };
    validate_manifest(&manifest, &current_weave()?)?;
    Ok(manifest)
}

/// Build one immutable domain pack from a complete, validated Character Profile.
pub fn character_domain_pack(
    profile: &CharacterProfile,
    id: impl Into<String>,
    version: impl Into<String>,
    title: impl Into<String>,
) -> Result<DomainPack, CharacterDomainError> {
    validate_profile(profile)?;
    let manifest = character_module_manifest()?;
    let pack = DomainPack {
        pack_format_version: 1,
        id: id.into(),
        version: version.into(),
        title: title.into(),
        module: ModuleRequirement {
            id: crate::CHARACTER_MODULE_ID.to_owned(),
            version: format!("={CHARACTER_DOMAIN_MODULE_VERSION}"),
        },
        dependencies: Vec::new(),
        values: BTreeMap::from([(
            "profile".to_owned(),
            character_profile_domain_value(profile),
        )]),
        provenance: projection_provenance(&profile.provenance)?,
    };
    validate_pack(&pack, &manifest, &current_weave()?)?;
    Ok(pack)
}

/// Project one validated profile into the finite value tree embedded in Story IR.
#[must_use]
pub fn character_profile_domain_value(profile: &CharacterProfile) -> DomainValue {
    let mut fields = BTreeMap::from(
        [
            ("hexaco", hexaco_value(&profile.canon.personality)),
            (
                "identity",
                object([
                    (
                        "display_name",
                        attributed_text_value(&profile.canon.identity.display_name),
                    ),
                    ("id", DomainValue::String(profile.id.clone())),
                    (
                        "aliases",
                        profile
                            .canon
                            .identity
                            .aliases
                            .as_ref()
                            .map(|aliases| {
                                DomainValue::List(
                                    aliases
                                        .value
                                        .iter()
                                        .cloned()
                                        .map(DomainValue::String)
                                        .collect(),
                                )
                            })
                            .unwrap_or_else(|| DomainValue::List(Vec::new())),
                    ),
                ]),
            ),
            ("ocean", ocean_value(&profile.derived.ocean)),
            (
                "profile_format_version",
                DomainValue::Number(f64::from(profile.profile_format_version)),
            ),
            (
                "provenance",
                object([
                    (
                        "sources",
                        DomainValue::List(
                            profile
                                .provenance
                                .sources
                                .iter()
                                .map(|source| DomainValue::String(source.id.clone()))
                                .collect(),
                        ),
                    ),
                    (
                        "transformations",
                        DomainValue::List(
                            profile
                                .provenance
                                .transformations
                                .iter()
                                .map(|transformation| {
                                    DomainValue::String(transformation.id.clone())
                                })
                                .collect(),
                        ),
                    ),
                ]),
            ),
        ]
        .map(|(key, value)| (key.to_owned(), value)),
    );
    if let Some(date_context) = date_context_value(profile) {
        fields.insert("date_context".to_owned(), date_context);
    }
    if let Some(alignment) = alignment_value(profile) {
        fields.insert("alignment".to_owned(), alignment);
    }
    DomainValue::Object(fields)
}

fn current_weave() -> Result<Version, CharacterDomainError> {
    Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| CharacterDomainError::Configuration)
}

fn projection_provenance(input: &Provenance) -> Result<Provenance, CharacterDomainError> {
    const PROJECTION_ID: &str = "weave_character_domain_projection_v3";
    if input
        .transformations
        .iter()
        .any(|transformation| transformation.id == PROJECTION_ID)
    {
        return Err(CharacterDomainError::ProvenanceCollision);
    }
    let mut transformations = input.transformations.clone();
    transformations.push(ProvenanceTransformation {
        id: PROJECTION_ID.to_owned(),
        inputs: input
            .sources
            .iter()
            .map(|source| source.id.clone())
            .collect(),
        description: "Validated, deterministic projection of Character Profile identity, HEXACO evidence, reviewed non-diagnostic alignment values, reviewed non-causal temporal context, lossy OCEAN compatibility fields, and lineage into the shared finite domain-value contract.".to_owned(),
    });
    transformations.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(Provenance {
        sources: input.sources.clone(),
        transformations,
        claims: BTreeMap::from([("values.profile".to_owned(), vec![PROJECTION_ID.to_owned()])]),
    })
}

fn attributed_text_type() -> TypeExpression {
    TypeExpression::Object {
        fields: evidence_metadata_fields(Some(field(
            text(1, 256),
            true,
            "Authored or reviewed text value.",
        ))),
    }
}

fn trait_evidence_type() -> TypeExpression {
    let mut fields = evidence_metadata_fields(None);
    fields.insert(
        "form".to_owned(),
        field(
            symbol(&["high", "low", "middle", "score", "very_high", "very_low"]),
            true,
            "Exact numeric input form or the retained five-band input form.",
        ),
    );
    fields.insert(
        "projection_score".to_owned(),
        field(
            number(false, 0.0, 1.0),
            true,
            "Exact normalized score, or the documented projection anchor for a band.",
        ),
    );
    TypeExpression::Object { fields }
}

fn evidence_metadata_fields(value: Option<FieldDeclaration>) -> BTreeMap<String, FieldDeclaration> {
    let mut fields = BTreeMap::from([
        (
            "confidence".to_owned(),
            field(
                symbol(&["high", "low", "moderate", "unknown"]),
                true,
                "Confidence in this representation.",
            ),
        ),
        (
            "freshness".to_owned(),
            field(
                symbol(&["current", "stale"]),
                true,
                "Whether exact upstream inputs remain current.",
            ),
        ),
        (
            "lineage".to_owned(),
            field(
                TypeExpression::List {
                    items: Box::new(text(1, 256)),
                    min_items: 1,
                    max_items: 256,
                },
                true,
                "Sorted profile provenance identifiers.",
            ),
        ),
        (
            "lock".to_owned(),
            field(symbol(&["locked", "unlocked"]), true, "Author lock state."),
        ),
        (
            "rationale".to_owned(),
            field(
                text(1, 2_048),
                false,
                "Explanation retained for non-trivial value states.",
            ),
        ),
        (
            "review".to_owned(),
            field(
                symbol(&["accepted", "not_required", "pending", "rejected"]),
                true,
                "Editorial review state.",
            ),
        ),
        (
            "state".to_owned(),
            field(
                symbol(&[
                    "authored",
                    "derived",
                    "imported",
                    "overridden",
                    "reviewed",
                    "suggested",
                ]),
                true,
                "How the value entered the record.",
            ),
        ),
    ]);
    if let Some(value) = value {
        fields.insert("value".to_owned(), value);
    }
    fields
}

fn factor_type(facets: &[&str]) -> TypeExpression {
    let mut fields = BTreeMap::from([(
        "summary".to_owned(),
        field(
            named("TraitEvidence"),
            false,
            "Optional canonical factor summary; absence is explicit missing data.",
        ),
    )]);
    for facet in facets {
        fields.insert(
            (*facet).to_owned(),
            field(
                named("TraitEvidence"),
                false,
                "Optional canonical HEXACO facet evidence; absence is never imputed.",
            ),
        );
    }
    TypeExpression::Object { fields }
}

fn hexaco_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "agreeableness".to_owned(),
                field(
                    named("Agreeableness"),
                    true,
                    "Agreeableness factor and four facets.",
                ),
            ),
            (
                "conscientiousness".to_owned(),
                field(
                    named("Conscientiousness"),
                    true,
                    "Conscientiousness factor and four facets.",
                ),
            ),
            (
                "emotionality".to_owned(),
                field(
                    named("Emotionality"),
                    true,
                    "Emotionality factor and four facets.",
                ),
            ),
            (
                "extraversion".to_owned(),
                field(
                    named("Extraversion"),
                    true,
                    "Extraversion factor and four facets.",
                ),
            ),
            (
                "honesty_humility".to_owned(),
                field(
                    named("HonestyHumility"),
                    true,
                    "Honesty-Humility factor and four facets.",
                ),
            ),
            (
                "openness".to_owned(),
                field(
                    named("Openness"),
                    true,
                    "Openness to Experience factor and four facets.",
                ),
            ),
        ]),
    }
}

fn ocean_dimension_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "confidence".to_owned(),
                field(
                    symbol(&["high", "low", "moderate", "unknown"]),
                    true,
                    "Minimum confidence of the exact HEXACO inputs.",
                ),
            ),
            (
                "input_paths".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 512)),
                        min_items: 1,
                        max_items: 4,
                    },
                    true,
                    "Exact canonical HEXACO input paths.",
                ),
            ),
            (
                "score".to_owned(),
                field(
                    number(false, 0.0, 1.0),
                    true,
                    "Deterministically projected compatibility score.",
                ),
            ),
        ]),
    }
}

fn ocean_type() -> TypeExpression {
    let mut fields = BTreeMap::from([
        (
            "algorithm".to_owned(),
            field(
                text(1, 128),
                true,
                "Stable projection algorithm identifier.",
            ),
        ),
        (
            "algorithm_version".to_owned(),
            field(
                number(true, 1.0, 1.0),
                true,
                "Exact projection algorithm version.",
            ),
        ),
        (
            "independent_evidence".to_owned(),
            field(
                TypeExpression::Bool,
                true,
                "Always false; this view is not independent personality evidence.",
            ),
        ),
        (
            "lossy".to_owned(),
            field(
                TypeExpression::Bool,
                true,
                "Always true for the five-factor compatibility projection.",
            ),
        ),
        (
            "omitted_factor".to_owned(),
            field(
                symbol(&["honesty_humility"]),
                true,
                "Canonical HEXACO factor omitted by OCEAN.",
            ),
        ),
    ]);
    for dimension in [
        "agreeableness",
        "conscientiousness",
        "extraversion",
        "neuroticism",
        "openness",
    ] {
        fields.insert(
            dimension.to_owned(),
            field(
                named("OceanDimension"),
                false,
                "Optional derived dimension; absent when required canonical inputs are missing.",
            ),
        );
    }
    TypeExpression::Object { fields }
}

fn character_identity_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "aliases".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 256)),
                        min_items: 0,
                        max_items: 256,
                    },
                    true,
                    "Ordered aliases, empty when no aliases are asserted.",
                ),
            ),
            (
                "display_name".to_owned(),
                field(
                    named("AttributedText"),
                    true,
                    "Primary display name with authority and lineage.",
                ),
            ),
            (
                "id".to_owned(),
                field(
                    text(3, 256),
                    true,
                    "Stable namespaced character identifier.",
                ),
            ),
        ]),
    }
}

fn character_provenance_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "sources".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 256)),
                        min_items: 1,
                        max_items: 4_096,
                    },
                    true,
                    "Sorted source identifiers retained from the profile.",
                ),
            ),
            (
                "transformations".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 256)),
                        min_items: 0,
                        max_items: 4_096,
                    },
                    true,
                    "Sorted transformation identifiers retained from the profile.",
                ),
            ),
        ]),
    }
}

fn character_date_pack_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "id".to_owned(),
                field(text(3, 256), true, "Exact temporal context pack identity."),
            ),
            (
                "sha256".to_owned(),
                field(text(64, 64), true, "Exact lowercase pack content SHA-256."),
            ),
            (
                "version".to_owned(),
                field(text(1, 256), true, "Exact temporal context pack version."),
            ),
        ]),
    }
}

fn character_date_cue_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "content".to_owned(),
                field(text(1, 2_048), true, "Reviewed fictional authoring cue."),
            ),
            (
                "cue_source_ids".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 256)),
                        min_items: 1,
                        max_items: 4_096,
                    },
                    true,
                    "Lineage for the fictional cue and declared ranking vector.",
                ),
            ),
            (
                "decision".to_owned(),
                field(
                    symbol(&["accepted", "auto_approved", "edited", "overridden"]),
                    true,
                    "Exact review path by which the cue entered the public view.",
                ),
            ),
            (
                "fact_source_ids".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 256)),
                        min_items: 1,
                        max_items: 4_096,
                    },
                    true,
                    "Separate lineage for the matched dated fact.",
                ),
            ),
            (
                "id".to_owned(),
                field(text(1, 128), true, "Stable accepted cue identity."),
            ),
            (
                "kind".to_owned(),
                field(
                    symbol(&["affinity", "memory", "tension", "value", "voice"]),
                    true,
                    "Fictional authoring surface affected by this pending cue.",
                ),
            ),
            (
                "pack_id".to_owned(),
                field(text(3, 256), true, "Owning exact context pack identity."),
            ),
            (
                "record_id".to_owned(),
                field(text(1, 128), true, "Matched temporal record identity."),
            ),
            (
                "relevance".to_owned(),
                field(
                    number(false, 0.0, 1.0),
                    true,
                    "Deterministic fixed-point relevance exposed as a normalized number.",
                ),
            ),
            (
                "review_sha256".to_owned(),
                field(text(64, 64), true, "Exact complete review fingerprint."),
            ),
            (
                "sensitivity".to_owned(),
                field(
                    symbol(&["high", "low", "moderate"]),
                    true,
                    "Review sensitivity retained from the source record.",
                ),
            ),
            (
                "uncertainty".to_owned(),
                field(
                    symbol(&["bounded", "disputed", "exact"]),
                    true,
                    "Bounded fact uncertainty retained independently from relevance.",
                ),
            ),
        ]),
    }
}

fn character_date_context_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "accepted_record_ids".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 128)),
                        min_items: 0,
                        max_items: 65_536,
                    },
                    true,
                    "Sorted temporal records with accepted cues; empty means no reviewed cue was accepted.",
                ),
            ),
            (
                "canonical_personality_write_back".to_owned(),
                field(
                    TypeExpression::Bool,
                    true,
                    "Always false; temporal context is never personality evidence.",
                ),
            ),
            (
                "cues".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(named("CharacterDateCue")),
                        min_items: 0,
                        max_items: 16_384,
                    },
                    true,
                    "Reviewed fictional cues in stable candidate-id order.",
                ),
            ),
            (
                "packs".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(named("CharacterDatePack")),
                        min_items: 1,
                        max_items: 4_096,
                    },
                    true,
                    "Exact context pack coordinates retained by the review.",
                ),
            ),
        ]),
    }
}

fn character_alignment_pack_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "id".to_owned(),
                field(text(3, 256), true, "Exact alignment pack identity."),
            ),
            (
                "sha256".to_owned(),
                field(text(64, 64), true, "Exact lowercase pack content SHA-256."),
            ),
            (
                "version".to_owned(),
                field(text(1, 256), true, "Exact alignment pack version."),
            ),
        ]),
    }
}

fn character_alignment_value_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "coverage_micros".to_owned(),
                field(
                    number(true, 0.0, 1_000_000.0),
                    true,
                    "Exact evidence coverage in integer millionths.",
                ),
            ),
            (
                "decision".to_owned(),
                field(
                    symbol(&["edited", "overridden", "reviewed"]),
                    true,
                    "Review path by which this value entered the public view.",
                ),
            ),
            (
                "explanation".to_owned(),
                field(
                    text(1, 2_048),
                    true,
                    "Public non-diagnostic narrative explanation.",
                ),
            ),
            (
                "id".to_owned(),
                field(text(1, 128), true, "Stable alignment axis identifier."),
            ),
            (
                "input_paths".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 512)),
                        min_items: 0,
                        max_items: 4_096,
                    },
                    true,
                    "Sorted canonical evidence paths actually used by the reviewed value.",
                ),
            ),
            (
                "label".to_owned(),
                field(text(1, 256), true, "Pack-declared public narrative label."),
            ),
            (
                "label_id".to_owned(),
                field(text(1, 128), true, "Stable pack-declared label identifier."),
            ),
            (
                "score_micros".to_owned(),
                field(
                    number(true, -1_000_000.0, 1_000_000.0),
                    false,
                    "Optional exact signed score retained for the approved shorthand.",
                ),
            ),
        ]),
    }
}

fn character_alignment_view_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "applied_sha256".to_owned(),
                field(text(64, 64), true, "Exact atomic application fingerprint."),
            ),
            (
                "canonical_personality_write_back".to_owned(),
                field(
                    TypeExpression::Bool,
                    true,
                    "Always false; alignment is never canonical evidence.",
                ),
            ),
            (
                "input_paths".to_owned(),
                field(
                    TypeExpression::List {
                        items: Box::new(text(1, 512)),
                        min_items: 0,
                        max_items: 4_096,
                    },
                    true,
                    "Sorted union of canonical paths used by approved public values.",
                ),
            ),
            (
                "pack".to_owned(),
                field(
                    named("CharacterAlignmentPack"),
                    true,
                    "Exact declarative alignment pack coordinate.",
                ),
            ),
            (
                "review_sha256".to_owned(),
                field(text(64, 64), true, "Exact complete review fingerprint."),
            ),
            (
                "values".to_owned(),
                field(
                    TypeExpression::Map {
                        values: Box::new(named("CharacterAlignmentValue")),
                        min_entries: 0,
                        max_entries: 4_096,
                    },
                    true,
                    "Approved alignment values only; rejected and withheld values stay private to the authoring receipt.",
                ),
            ),
            (
                "view_id".to_owned(),
                field(text(3, 256), true, "Selected alignment view identity."),
            ),
        ]),
    }
}

fn runtime_profile_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "alignment".to_owned(),
                field(
                    named("CharacterAlignmentView"),
                    false,
                    "Optional reviewed, non-diagnostic narrative alignment shorthand.",
                ),
            ),
            (
                "date_context".to_owned(),
                field(
                    named("CharacterDateContext"),
                    false,
                    "Optional reviewed, non-causal temporal authoring context.",
                ),
            ),
            (
                "hexaco".to_owned(),
                field(
                    named("HexacoProfile"),
                    true,
                    "Canonical six-factor, 24-facet HEXACO evidence.",
                ),
            ),
            (
                "identity".to_owned(),
                field(
                    named("CharacterIdentity"),
                    true,
                    "Stable identity separated from optional presentation.",
                ),
            ),
            (
                "ocean".to_owned(),
                field(
                    named("OceanView"),
                    true,
                    "Lossy and read-only OCEAN compatibility view.",
                ),
            ),
            (
                "profile_format_version".to_owned(),
                field(
                    number(true, 1.0, 1.0),
                    true,
                    "Exact Character Profile contract version.",
                ),
            ),
            (
                "provenance".to_owned(),
                field(
                    named("CharacterProvenance"),
                    true,
                    "Profile source and transformation identifiers.",
                ),
            ),
        ]),
    }
}

fn field(value_type: TypeExpression, required: bool, description: &str) -> FieldDeclaration {
    FieldDeclaration {
        value_type,
        required,
        description: description.to_owned(),
    }
}

fn named(name: &str) -> TypeExpression {
    TypeExpression::Named {
        name: name.to_owned(),
    }
}

fn text(min_length: usize, max_length: usize) -> TypeExpression {
    TypeExpression::String {
        min_length,
        max_length,
    }
}

fn number(integer: bool, minimum: f64, maximum: f64) -> TypeExpression {
    TypeExpression::Number {
        integer,
        minimum: Some(minimum),
        maximum: Some(maximum),
    }
}

fn symbol(values: &[&str]) -> TypeExpression {
    TypeExpression::Symbol {
        values: values.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn alignment_value(profile: &CharacterProfile) -> Option<DomainValue> {
    let CharacterExtension::AlignmentView(extension) = profile
        .extensions
        .get(crate::ALIGNMENT_EXTENSION_NAMESPACE)?
    else {
        return None;
    };
    let value = &extension.value;
    Some(object([
        (
            "applied_sha256",
            DomainValue::String(value.applied_sha256.clone()),
        ),
        ("canonical_personality_write_back", DomainValue::Bool(false)),
        (
            "input_paths",
            DomainValue::List(
                value
                    .input_paths
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("pack", alignment_pack_value(&value.pack)),
        (
            "review_sha256",
            DomainValue::String(value.review_sha256.clone()),
        ),
        (
            "values",
            DomainValue::Object(
                value
                    .values
                    .iter()
                    .map(|(id, value)| (id.clone(), alignment_public_value(value)))
                    .collect(),
            ),
        ),
        ("view_id", DomainValue::String(value.view_id.clone())),
    ]))
}

fn alignment_pack_value(pack: &AlignmentPackRef) -> DomainValue {
    object([
        ("id", DomainValue::String(pack.id.clone())),
        ("sha256", DomainValue::String(pack.sha256.clone())),
        ("version", DomainValue::String(pack.version.clone())),
    ])
}

fn alignment_public_value(value: &ApprovedAlignmentValue) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "coverage_micros".to_owned(),
            DomainValue::Number(f64::from(value.coverage_micros)),
        ),
        (
            "decision".to_owned(),
            DomainValue::Symbol(alignment_decision(value.decision).to_owned()),
        ),
        (
            "explanation".to_owned(),
            DomainValue::String(value.explanation.clone()),
        ),
        ("id".to_owned(), DomainValue::String(value.id.clone())),
        (
            "input_paths".to_owned(),
            DomainValue::List(
                value
                    .input_paths
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("label".to_owned(), DomainValue::String(value.label.clone())),
        (
            "label_id".to_owned(),
            DomainValue::String(value.label_id.clone()),
        ),
    ]);
    if let Some(score) = value.score_micros {
        fields.insert(
            "score_micros".to_owned(),
            DomainValue::Number(f64::from(score)),
        );
    }
    DomainValue::Object(fields)
}

const fn alignment_decision(value: AlignmentPublicDecision) -> &'static str {
    match value {
        AlignmentPublicDecision::Reviewed => "reviewed",
        AlignmentPublicDecision::Edited => "edited",
        AlignmentPublicDecision::Overridden => "overridden",
    }
}

fn date_context_value(profile: &CharacterProfile) -> Option<DomainValue> {
    let CharacterExtension::DateContext(extension) = profile
        .extensions
        .get(crate::DATE_CONTEXT_EXTENSION_NAMESPACE)?
    else {
        return None;
    };
    let value = &extension.value;
    let packs = std::iter::once(crate::DateContextPackRef {
        id: value.context_pack.clone(),
        version: value.context_version.clone(),
        sha256: value.context_hash.clone(),
    })
    .chain(value.additional_context_packs.iter().cloned())
    .map(|pack| {
        object([
            ("id", DomainValue::String(pack.id)),
            ("sha256", DomainValue::String(pack.sha256)),
            ("version", DomainValue::String(pack.version)),
        ])
    })
    .collect();
    Some(object([
        (
            "accepted_record_ids",
            DomainValue::List(
                value
                    .accepted_record_ids
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("canonical_personality_write_back", DomainValue::Bool(false)),
        (
            "cues",
            DomainValue::List(value.accepted_cues.values().map(date_cue_value).collect()),
        ),
        ("packs", DomainValue::List(packs)),
    ]))
}

fn date_cue_value(cue: &AcceptedDateContextCue) -> DomainValue {
    object([
        ("content", DomainValue::String(cue.content.clone())),
        (
            "cue_source_ids",
            DomainValue::List(
                cue.cue_source_ids
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        (
            "decision",
            DomainValue::Symbol(date_decision(cue.decision).to_owned()),
        ),
        (
            "fact_source_ids",
            DomainValue::List(
                cue.fact_source_ids
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("id", DomainValue::String(cue.id.clone())),
        (
            "kind",
            DomainValue::Symbol(date_cue_kind(cue.kind).to_owned()),
        ),
        ("pack_id", DomainValue::String(cue.pack.id.clone())),
        ("record_id", DomainValue::String(cue.record_id.clone())),
        ("relevance", DomainValue::Number(cue.relevance)),
        (
            "review_sha256",
            DomainValue::String(cue.review_sha256.clone()),
        ),
        (
            "sensitivity",
            DomainValue::Symbol(date_sensitivity(cue.sensitivity).to_owned()),
        ),
        (
            "uncertainty",
            DomainValue::Symbol(date_uncertainty(cue.uncertainty).to_owned()),
        ),
    ])
}

const fn date_cue_kind(value: DateContextCueKind) -> &'static str {
    match value {
        DateContextCueKind::Affinity => "affinity",
        DateContextCueKind::Tension => "tension",
        DateContextCueKind::Value => "value",
        DateContextCueKind::Memory => "memory",
        DateContextCueKind::Voice => "voice",
    }
}

const fn date_decision(value: DateContextDecision) -> &'static str {
    match value {
        DateContextDecision::Accepted => "accepted",
        DateContextDecision::AutoApproved => "auto_approved",
        DateContextDecision::Edited => "edited",
        DateContextDecision::Overridden => "overridden",
    }
}

const fn date_uncertainty(value: DateContextUncertainty) -> &'static str {
    match value {
        DateContextUncertainty::Exact => "exact",
        DateContextUncertainty::Bounded => "bounded",
        DateContextUncertainty::Disputed => "disputed",
    }
}

const fn date_sensitivity(value: DateContextSensitivity) -> &'static str {
    match value {
        DateContextSensitivity::Low => "low",
        DateContextSensitivity::Moderate => "moderate",
        DateContextSensitivity::High => "high",
    }
}

fn hexaco_value(profile: &HexacoProfile) -> DomainValue {
    object([
        (
            "agreeableness",
            factor_value([
                ("flexibility", profile.agreeableness.flexibility.as_ref()),
                (
                    "forgivingness",
                    profile.agreeableness.forgivingness.as_ref(),
                ),
                ("gentleness", profile.agreeableness.gentleness.as_ref()),
                ("patience", profile.agreeableness.patience.as_ref()),
                ("summary", profile.agreeableness.factor.as_ref()),
            ]),
        ),
        (
            "conscientiousness",
            factor_value([
                ("diligence", profile.conscientiousness.diligence.as_ref()),
                (
                    "organization",
                    profile.conscientiousness.organization.as_ref(),
                ),
                (
                    "perfectionism",
                    profile.conscientiousness.perfectionism.as_ref(),
                ),
                ("prudence", profile.conscientiousness.prudence.as_ref()),
                ("summary", profile.conscientiousness.factor.as_ref()),
            ]),
        ),
        (
            "emotionality",
            factor_value([
                ("anxiety", profile.emotionality.anxiety.as_ref()),
                ("dependence", profile.emotionality.dependence.as_ref()),
                ("fearfulness", profile.emotionality.fearfulness.as_ref()),
                (
                    "sentimentality",
                    profile.emotionality.sentimentality.as_ref(),
                ),
                ("summary", profile.emotionality.factor.as_ref()),
            ]),
        ),
        (
            "extraversion",
            factor_value([
                ("liveliness", profile.extraversion.liveliness.as_ref()),
                (
                    "social_boldness",
                    profile.extraversion.social_boldness.as_ref(),
                ),
                (
                    "social_self_esteem",
                    profile.extraversion.social_self_esteem.as_ref(),
                ),
                ("sociability", profile.extraversion.sociability.as_ref()),
                ("summary", profile.extraversion.factor.as_ref()),
            ]),
        ),
        (
            "honesty_humility",
            factor_value([
                ("fairness", profile.honesty_humility.fairness.as_ref()),
                (
                    "greed_avoidance",
                    profile.honesty_humility.greed_avoidance.as_ref(),
                ),
                ("modesty", profile.honesty_humility.modesty.as_ref()),
                ("sincerity", profile.honesty_humility.sincerity.as_ref()),
                ("summary", profile.honesty_humility.factor.as_ref()),
            ]),
        ),
        (
            "openness",
            factor_value([
                (
                    "aesthetic_appreciation",
                    profile.openness.aesthetic_appreciation.as_ref(),
                ),
                ("creativity", profile.openness.creativity.as_ref()),
                ("inquisitiveness", profile.openness.inquisitiveness.as_ref()),
                ("summary", profile.openness.factor.as_ref()),
                (
                    "unconventionality",
                    profile.openness.unconventionality.as_ref(),
                ),
            ]),
        ),
    ])
}

fn factor_value<'a>(
    values: impl IntoIterator<Item = (&'a str, Option<&'a Attributed<TraitMeasurement>>)>,
) -> DomainValue {
    DomainValue::Object(
        values
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name.to_owned(), trait_value(value))))
            .collect(),
    )
}

fn trait_value(value: &Attributed<TraitMeasurement>) -> DomainValue {
    let (form, projection_score) = match value.value {
        TraitMeasurement::Score { score } => ("score", score),
        TraitMeasurement::Band { band } => (trait_band(band), band.projection_anchor()),
    };
    let mut fields = evidence_metadata_value(value);
    fields.insert("form".to_owned(), DomainValue::Symbol(form.to_owned()));
    fields.insert(
        "projection_score".to_owned(),
        DomainValue::Number(projection_score),
    );
    DomainValue::Object(fields)
}

fn attributed_text_value(value: &Attributed<String>) -> DomainValue {
    let mut fields = evidence_metadata_value(value);
    fields.insert("value".to_owned(), DomainValue::String(value.value.clone()));
    DomainValue::Object(fields)
}

fn evidence_metadata_value<T>(value: &Attributed<T>) -> BTreeMap<String, DomainValue> {
    let mut fields = BTreeMap::from([
        (
            "confidence".to_owned(),
            DomainValue::Symbol(confidence(value.confidence).to_owned()),
        ),
        (
            "freshness".to_owned(),
            DomainValue::Symbol(freshness(value.freshness).to_owned()),
        ),
        (
            "lineage".to_owned(),
            DomainValue::List(
                value
                    .lineage
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        (
            "lock".to_owned(),
            DomainValue::Symbol(lock(value.lock).to_owned()),
        ),
        (
            "review".to_owned(),
            DomainValue::Symbol(review(value.review).to_owned()),
        ),
        (
            "state".to_owned(),
            DomainValue::Symbol(state(value.state).to_owned()),
        ),
    ]);
    if let Some(rationale) = &value.rationale {
        fields.insert(
            "rationale".to_owned(),
            DomainValue::String(rationale.clone()),
        );
    }
    fields
}

fn ocean_value(ocean: &OceanView) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "algorithm".to_owned(),
            DomainValue::String(ocean.algorithm.clone()),
        ),
        (
            "algorithm_version".to_owned(),
            DomainValue::Number(f64::from(ocean.algorithm_version)),
        ),
        (
            "independent_evidence".to_owned(),
            DomainValue::Bool(ocean.independent_evidence),
        ),
        ("lossy".to_owned(), DomainValue::Bool(ocean.lossy)),
        (
            "omitted_factor".to_owned(),
            DomainValue::Symbol("honesty_humility".to_owned()),
        ),
    ]);
    for (name, dimension) in [
        ("agreeableness", ocean.agreeableness.as_ref()),
        ("conscientiousness", ocean.conscientiousness.as_ref()),
        ("extraversion", ocean.extraversion.as_ref()),
        ("neuroticism", ocean.neuroticism.as_ref()),
        ("openness", ocean.openness.as_ref()),
    ] {
        if let Some(dimension) = dimension {
            fields.insert(name.to_owned(), ocean_dimension_value(dimension));
        }
    }
    DomainValue::Object(fields)
}

fn ocean_dimension_value(value: &DerivedTrait) -> DomainValue {
    object([
        (
            "confidence",
            DomainValue::Symbol(confidence(value.confidence).to_owned()),
        ),
        (
            "input_paths",
            DomainValue::List(
                value
                    .input_paths
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("score", DomainValue::Number(value.score)),
    ])
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

const fn trait_band(value: TraitBand) -> &'static str {
    match value {
        TraitBand::VeryLow => "very_low",
        TraitBand::Low => "low",
        TraitBand::Middle => "middle",
        TraitBand::High => "high",
        TraitBand::VeryHigh => "very_high",
    }
}

const fn confidence(value: Confidence) -> &'static str {
    match value {
        Confidence::Unknown => "unknown",
        Confidence::Low => "low",
        Confidence::Moderate => "moderate",
        Confidence::High => "high",
    }
}

const fn state(value: ValueState) -> &'static str {
    match value {
        ValueState::Suggested => "suggested",
        ValueState::Derived => "derived",
        ValueState::Imported => "imported",
        ValueState::Reviewed => "reviewed",
        ValueState::Authored => "authored",
        ValueState::Overridden => "overridden",
    }
}

const fn review(value: ReviewState) -> &'static str {
    match value {
        ReviewState::NotRequired => "not_required",
        ReviewState::Pending => "pending",
        ReviewState::Accepted => "accepted",
        ReviewState::Rejected => "rejected",
    }
}

const fn lock(value: LockState) -> &'static str {
    match value {
        LockState::Unlocked => "unlocked",
        LockState::Locked => "locked",
    }
}

const fn freshness(value: Freshness) -> &'static str {
    match value {
        Freshness::Current => "current",
        Freshness::Stale => "stale",
    }
}
