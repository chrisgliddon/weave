use std::collections::BTreeMap;

use weave_character::*;

const PROFILE: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.json");
const RELATIONSHIP_COLLECTION: &str = include_str!(
    "../../../examples/domain-modules/weave-character/operations/collection.character-collection.json"
);
const RELATIONSHIP_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-character/relationships/reference.relationship-kind-pack.json"
);
const RELATIONSHIP_POLICY: &str = include_str!(
    "../../../examples/domain-modules/weave-character/relationships/project.relationship-policy.json"
);
const UNSAFE_EXPRESSION_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/invalid/restricted-placeholder.expression-pack.json"
);
const ASSISTANCE_COLLECTION: &str = include_str!(
    "../../../examples/domain-modules/weave-character/assistance/input.character-collection.json"
);
const ASSISTANCE_CANDIDATES: &str = include_str!(
    "../../../examples/domain-modules/weave-character/assistance/single.assistance-candidate-set.json"
);
const ASSISTANCE_REVIEW: &str = include_str!(
    "../../../examples/domain-modules/weave-character/assistance/single.assistance-decision-review.json"
);
const ASSISTANCE_RECEIPT: &str = include_str!(
    "../../../examples/domain-modules/weave-character/assistance/single.assistance-receipt.json"
);
const PROJECTION_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/glasswind.projection-pack.json"
);
const PROJECTION_PROPOSAL: &str = include_str!(
    "../../../examples/domain-modules/weave-character/projections/proposal.projection-proposal.json"
);

fn clean_profile() -> CharacterProfile {
    let mut profile = CharacterProfile::from_json(PROFILE).unwrap();
    profile.extensions.clear();
    profile.suggestions.clear();
    validate_profile(&profile).unwrap();
    profile
}

fn collection(profile: CharacterProfile) -> CharacterCollection {
    CharacterCollection {
        collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
        id: "org.weave.character.health.collection".to_owned(),
        revision: 1,
        characters: BTreeMap::from([(profile.id.clone(), profile)]),
    }
}

fn document(
    id: &str,
    kind: CharacterHealthDocumentKind,
    format: CharacterHealthDocumentFormat,
    path: &str,
    logical_id: &str,
    primary: bool,
    character_id: Option<&str>,
) -> CharacterHealthDocumentRef {
    CharacterHealthDocumentRef {
        id: id.to_owned(),
        kind,
        format,
        path: path.to_owned(),
        logical_id: logical_id.to_owned(),
        primary,
        character_id: character_id.map(ToOwned::to_owned),
    }
}

fn project(
    documents: Vec<CharacterHealthDocumentRef>,
    sources: BTreeMap<String, String>,
    policy: CharacterHealthPolicy,
    suppressions: Vec<CharacterHealthSuppression>,
    provenance: weave_domain::Provenance,
) -> CharacterHealthProject {
    CharacterHealthProject::new(
        CharacterHealthManifest {
            manifest_format_version: CHARACTER_HEALTH_MANIFEST_FORMAT_VERSION,
            id: "org.weave.character.health.synthetic".to_owned(),
            documents,
            policy,
            suppressions,
            provenance,
        },
        sources,
    )
    .unwrap()
}

fn one_document_project(
    collection: &CharacterCollection,
    policy: CharacterHealthPolicy,
    suppressions: Vec<CharacterHealthSuppression>,
) -> CharacterHealthProject {
    let id = "org.weave.health.collection.json";
    project(
        vec![document(
            id,
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.collection",
            true,
            None,
        )],
        BTreeMap::from([(
            id.to_owned(),
            weave_domain::to_pretty_json(collection).unwrap(),
        )]),
        policy,
        suppressions,
        collection
            .characters
            .values()
            .next()
            .unwrap()
            .provenance
            .clone(),
    )
}

fn raw_collection_project(source: String) -> CharacterHealthProject {
    let profile = clean_profile();
    let id = "org.weave.health.raw.collection";
    project(
        vec![document(
            id,
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.raw.collection",
            true,
            None,
        )],
        BTreeMap::from([(id.to_owned(), source)]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        profile.provenance,
    )
}

#[test]
fn healthy_portable_project_is_read_only_deterministic_and_text_json_ron_equivalent() {
    let profile = clean_profile();
    let collection = collection(profile.clone());
    let runtime = character_domain_pack(
        &profile,
        "ari_vale_health",
        "1.0.0",
        "Ari Vale Health Runtime",
    )
    .unwrap();
    let documents = vec![
        document(
            "org.weave.health.collection.json",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.collection",
            true,
            None,
        ),
        document(
            "org.weave.health.collection.ron",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Ron,
            "collection.character-collection.ron",
            "org.weave.health.collection",
            false,
            None,
        ),
        document(
            "org.weave.health.runtime.json",
            CharacterHealthDocumentKind::RuntimeDomainPack,
            CharacterHealthDocumentFormat::Json,
            "ari_vale.weave-domain.json",
            "org.weave.health.runtime",
            false,
            Some(&profile.id),
        ),
        document(
            "org.weave.health.runtime.ron",
            CharacterHealthDocumentKind::RuntimeDomainPack,
            CharacterHealthDocumentFormat::Ron,
            "ari_vale.weave-domain.ron",
            "org.weave.health.runtime",
            false,
            Some(&profile.id),
        ),
    ];
    let sources = BTreeMap::from([
        (documents[0].id.clone(), collection.to_json().unwrap()),
        (documents[1].id.clone(), collection.to_ron().unwrap()),
        (documents[2].id.clone(), runtime.to_json().unwrap()),
        (documents[3].id.clone(), runtime.to_ron().unwrap()),
    ]);
    let project = project(
        documents,
        sources,
        CharacterHealthPolicy::default(),
        Vec::new(),
        profile.provenance.clone(),
    );
    let before = project.clone();
    let first = audit_character_health(&project).unwrap();
    let second = audit_character_health(&project).unwrap();

    assert_eq!(project, before);
    assert_eq!(first, second);
    assert!(first.diagnostics.is_empty(), "{:#?}", first.diagnostics);
    assert_eq!(first.summary.ci_exit_code, 0);
    assert_eq!(first.coverage.factor_values_present, 6);
    assert_eq!(first.coverage.facet_values_present, 24);
    assert_eq!(
        CharacterHealthReport::from_json(&first.to_json().unwrap()).unwrap(),
        first
    );
    assert_eq!(
        CharacterHealthReport::from_ron(&first.to_ron().unwrap()).unwrap(),
        first
    );
    let text = render_character_health_text(&first).unwrap();
    assert!(text.contains(&first.input_sha256));
    assert!(text.contains("diagnostics_total=0"));
}

#[test]
fn incomplete_stale_malformed_and_reviewed_suppression_are_all_reported_without_abort() {
    let mut profile = clean_profile();
    profile
        .canon
        .personality
        .openness
        .creativity
        .as_mut()
        .unwrap()
        .confidence = Confidence::Unknown;
    profile.canon.identity.display_name.freshness = Freshness::Stale;
    profile.derived.ocean.algorithm_version += 1;
    let collection = collection(profile.clone());
    let document_id = "org.weave.health.collection.json";
    let creativity_path = format!(
        "characters.{}.canon.personality.openness.creativity.confidence",
        profile.id
    );
    let suppression = CharacterHealthSuppression {
        suppression_format_version: CHARACTER_HEALTH_SUPPRESSION_FORMAT_VERSION,
        id: "reviewed_confidence_gap".to_owned(),
        code: CharacterHealthDiagnosticCode::IncompleteFacetConfidence,
        document_id: Some(document_id.to_owned()),
        character_id: Some(profile.id.clone()),
        path_prefix: creativity_path,
        reviewed_by: "Casey Reviewer".to_owned(),
        rationale: "The current source cannot support a stronger confidence statement.".to_owned(),
        review_revision: 4,
    };
    let policy = CharacterHealthPolicy {
        failure_threshold: Some(CharacterHealthSeverity::Warning),
        require_portable_pairs: false,
        ..CharacterHealthPolicy::default()
    };
    let report = audit_character_health(&one_document_project(
        &collection,
        policy.clone(),
        vec![suppression],
    ))
    .unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::IncompleteFacetConfidence
            && diagnostic.suppressed_by.as_deref() == Some("reviewed_confidence_gap")
    }));
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == CharacterHealthDiagnosticCode::StaleFingerprint
        })
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::InconsistentDerivedView
    }));
    assert_eq!(report.summary.ci_exit_code, 2);

    let malformed_id = "org.weave.health.malformed.json";
    let malformed = project(
        vec![document(
            malformed_id,
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "malformed.character-collection.json",
            "org.weave.health.malformed",
            true,
            None,
        )],
        BTreeMap::from([(
            malformed_id.to_owned(),
            "{\n  \"collection_format_version\": 1,\n  \"id\": \"org.weave.health.malformed\",\n  \"revision\": 1\n}\n"
                .to_owned(),
        )]),
        policy,
        Vec::new(),
        profile.provenance,
    );
    let malformed_report = audit_character_health(&malformed).unwrap();
    assert!(malformed_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::MissingRequiredField
    }));
    assert!(
        malformed_report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == CharacterHealthDiagnosticCode::InvalidDocument
        })
    );

    let hidden =
        filter_character_health_report(&report, &CharacterHealthFilter::default()).unwrap();
    assert!(hidden.diagnostics.iter().all(|diagnostic| {
        diagnostic.suppressed_by.as_deref() != Some("reviewed_confidence_gap")
    }));
    assert!(hidden.suppressions[0].matched_diagnostic_ids.is_empty());
    let visible = filter_character_health_report(
        &report,
        &CharacterHealthFilter {
            include_suppressed: true,
            ..CharacterHealthFilter::default()
        },
    )
    .unwrap();
    assert!(!visible.suppressions[0].matched_diagnostic_ids.is_empty());
}

#[test]
fn credential_shaped_source_values_are_whole_value_redacted_from_reports() {
    let sensitive = ["ghp", "_", "synthetic_fixture_value_1234567890"].concat();
    let source =
        format!("{{\n  \"api_key\": \"{sensitive}\",\n  \"collection_format_version\": 1\n}}\n");
    let report = audit_character_health(&raw_collection_project(source.clone())).unwrap();
    let sensitive_diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == CharacterHealthDiagnosticCode::SensitiveValue)
        .unwrap();
    assert!(sensitive_diagnostic.remediation.contains("[REDACTED]"));
    for rendered in [
        report.to_json().unwrap(),
        report.to_ron().unwrap(),
        render_character_health_text(&report).unwrap(),
    ] {
        assert!(!rendered.contains(&sensitive));
        assert!(rendered.contains("[REDACTED]"));
    }

    let mut attempted_suppression = raw_collection_project(source.clone());
    attempted_suppression
        .manifest
        .suppressions
        .push(CharacterHealthSuppression {
            suppression_format_version: CHARACTER_HEALTH_SUPPRESSION_FORMAT_VERSION,
            id: "hide_sensitive_value".to_owned(),
            code: CharacterHealthDiagnosticCode::SensitiveValue,
            document_id: Some("org.weave.health.raw.collection".to_owned()),
            character_id: None,
            path_prefix: "document".to_owned(),
            reviewed_by: "Casey Reviewer".to_owned(),
            rationale: "Exercise the fail-closed suppression boundary.".to_owned(),
            review_revision: 1,
        });
    let attempted_suppression = audit_character_health(&attempted_suppression).unwrap();
    assert!(attempted_suppression.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::SensitiveValue
            && diagnostic.suppressed_by.is_none()
    }));
    assert!(attempted_suppression.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::InvalidSuppression
    }));

    let redacted = source.replace(&sensitive, "[REDACTED]");
    let redacted_report = audit_character_health(&raw_collection_project(redacted)).unwrap();
    assert!(
        redacted_report
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code != CharacterHealthDiagnosticCode::SensitiveValue })
    );
}

#[test]
fn portability_and_distribution_policy_are_explicit_and_never_implicit_quotas() {
    let profile = clean_profile();
    let collection = collection(profile.clone());
    let mut runtime = character_domain_pack(
        &profile,
        "ari_vale_health_policy",
        "1.0.0",
        "Ari Vale Health Policy Runtime",
    )
    .unwrap();
    runtime
        .values
        .insert("profile".to_owned(), weave_domain::DomainValue::Null);
    let documents = vec![
        document(
            "org.weave.health.policy.collection",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.policy.collection",
            true,
            None,
        ),
        document(
            "org.weave.health.policy.runtime",
            CharacterHealthDocumentKind::RuntimeDomainPack,
            CharacterHealthDocumentFormat::Json,
            "ari_vale.weave-domain.json",
            "org.weave.health.policy.runtime",
            false,
            Some(&profile.id),
        ),
    ];
    let report = audit_character_health(&project(
        documents.clone(),
        BTreeMap::from([
            (documents[0].id.clone(), collection.to_json().unwrap()),
            (
                documents[1].id.clone(),
                weave_domain::to_pretty_json(&runtime).unwrap(),
            ),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            csv_loss_diagnostics: true,
            distribution_constraints: vec![CharacterHealthDistributionConstraint {
                id: "documented_minimum_corpus".to_owned(),
                metric: CharacterHealthDistributionMetric::CharacterCount,
                minimum: Some(2),
                maximum: None,
                rationale: "This synthetic project explicitly exercises a documented constraint."
                    .to_owned(),
                policy_url: "https://github.com/chrisgliddon/weave".to_owned(),
            }],
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        profile.provenance.clone(),
    ))
    .unwrap();
    for code in [
        CharacterHealthDiagnosticCode::RuntimeAuthorityBoundary,
        CharacterHealthDiagnosticCode::CsvLoss,
        CharacterHealthDiagnosticCode::DistributionPolicy,
    ] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "missing {code:?}"
        );
    }
    assert_eq!(report.distributions.constraints_configured, 1);

    let unconstrained = audit_character_health(&one_document_project(
        &collection,
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
    ))
    .unwrap();
    assert!(unconstrained.diagnostics.iter().all(|diagnostic| {
        diagnostic.code != CharacterHealthDiagnosticCode::DistributionPolicy
    }));
}

#[test]
fn projection_provenance_version_explanation_review_and_freshness_are_aggregated() {
    let mut profile = CharacterProfile::from_json(PROFILE).unwrap();
    let CharacterExtension::RoleProjections(projections) = profile
        .extensions
        .get_mut(ROLE_PROJECTION_EXTENSION_NAMESPACE)
        .unwrap()
    else {
        panic!("fixture uses typed role projections")
    };
    let scored = projections
        .value
        .roles
        .values_mut()
        .find(|role| role.score_micros.is_some())
        .unwrap();
    scored.explanation.clear();
    scored.input_paths.clear();
    let invalid_collection = collection(profile.clone());
    let invalid_report = audit_character_health(&raw_collection_project(
        weave_domain::to_pretty_json(&invalid_collection).unwrap(),
    ))
    .unwrap();
    for code in [
        CharacterHealthDiagnosticCode::MissingPackProvenance,
        CharacterHealthDiagnosticCode::UnexplainedProjectionScore,
    ] {
        assert!(
            invalid_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "missing {code:?}"
        );
    }

    let clean = clean_profile();
    let current_collection = collection(clean.clone());
    let mut unsupported_pack = ProjectionPack::from_json(PROJECTION_PACK).unwrap();
    unsupported_pack.pack_format_version = 0;
    let proposal = ProjectionProposal::from_json(PROJECTION_PROPOSAL).unwrap();
    let documents = vec![
        document(
            "org.weave.health.projection.collection",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.projection.collection",
            true,
            None,
        ),
        document(
            "org.weave.health.projection.pack",
            CharacterHealthDocumentKind::ProjectionPack,
            CharacterHealthDocumentFormat::Json,
            "glasswind.projection-pack.json",
            "org.weave.health.projection.pack",
            false,
            None,
        ),
        document(
            "org.weave.health.projection.proposal",
            CharacterHealthDocumentKind::ProjectionProposal,
            CharacterHealthDocumentFormat::Json,
            "proposal.projection-proposal.json",
            "org.weave.health.projection.proposal",
            false,
            None,
        ),
    ];
    let report = audit_character_health(&project(
        documents.clone(),
        BTreeMap::from([
            (
                documents[0].id.clone(),
                current_collection.to_json().unwrap(),
            ),
            (
                documents[1].id.clone(),
                weave_domain::to_pretty_json(&unsupported_pack).unwrap(),
            ),
            (documents[2].id.clone(), proposal.to_json().unwrap()),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        clean.provenance,
    ))
    .unwrap();
    for code in [
        CharacterHealthDiagnosticCode::UnsupportedProjectionVersion,
        CharacterHealthDiagnosticCode::UnreviewedProjection,
        CharacterHealthDiagnosticCode::StaleProjection,
    ] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "missing {code:?}"
        );
    }
}

#[test]
fn assistance_review_provider_policy_and_accepted_source_lineage_are_aggregated() {
    let assistance_collection = CharacterCollection::from_json(ASSISTANCE_COLLECTION).unwrap();
    let mut candidates = AssistanceCandidateSet::from_json(ASSISTANCE_CANDIDATES).unwrap();
    let documents = vec![
        document(
            "org.weave.health.assistance.candidates",
            CharacterHealthDocumentKind::AssistanceCandidateSet,
            CharacterHealthDocumentFormat::Json,
            "candidate-set.json",
            "org.weave.health.assistance.candidates",
            false,
            Some(&candidates.input_profile.id),
        ),
        document(
            "org.weave.health.assistance.collection",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.assistance.collection",
            true,
            None,
        ),
    ];
    let provenance = assistance_collection
        .characters
        .values()
        .next()
        .unwrap()
        .provenance
        .clone();
    let unreviewed = audit_character_health(&project(
        documents.clone(),
        BTreeMap::from([
            (documents[0].id.clone(), candidates.to_json().unwrap()),
            (
                documents[1].id.clone(),
                assistance_collection.to_json().unwrap(),
            ),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        provenance.clone(),
    ))
    .unwrap();
    assert!(unreviewed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::UnreviewedAssistanceCandidate
    }));

    candidates.preview.request.provider.credential_required = true;
    let invalid_provider = audit_character_health(&project(
        documents.clone(),
        BTreeMap::from([
            (
                documents[0].id.clone(),
                weave_domain::to_pretty_json(&candidates).unwrap(),
            ),
            (
                documents[1].id.clone(),
                assistance_collection.to_json().unwrap(),
            ),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        provenance,
    ))
    .unwrap();
    assert!(invalid_provider.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::ProviderMetadataPolicy
    }));

    let receipt = AssistanceReceipt::from_json(ASSISTANCE_RECEIPT).unwrap();
    let output = collection(receipt.output_profile);
    let missing_source = audit_character_health(&one_document_project(
        &output,
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
    ))
    .unwrap();
    assert!(missing_source.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::MissingAcceptedSource
    }));
}

#[test]
fn relationship_expression_projection_assistance_and_portability_diagnostics_compose() {
    let mut relationship_collection =
        CharacterCollection::from_json(RELATIONSHIP_COLLECTION).unwrap();
    let (owner_id, mut duplicate) = relationship_collection
        .characters
        .iter()
        .find_map(|(owner_id, profile)| {
            match profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE) {
                Some(CharacterExtension::Relationships(record)) => record
                    .value
                    .edges
                    .values()
                    .next()
                    .map(|edge| (owner_id.clone(), edge.clone())),
                _ => None,
            }
        })
        .unwrap();
    duplicate.id = "health_duplicate".to_owned();
    let owner = relationship_collection
        .characters
        .get_mut(&owner_id)
        .unwrap();
    let CharacterExtension::Relationships(record) = owner
        .extensions
        .get_mut(RELATIONSHIP_EXTENSION_NAMESPACE)
        .unwrap()
    else {
        panic!("relationship fixture uses the typed graph extension")
    };
    record.value.edges.insert(duplicate.id.clone(), duplicate);

    let primary_id = "org.weave.health.composed.collection";
    let expression_id = "org.weave.health.composed.expression";
    let pack_id = "org.weave.health.composed.relationship_pack";
    let policy_id = "org.weave.health.composed.relationship_policy";
    let documents = vec![
        document(
            primary_id,
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.composed.collection",
            true,
            None,
        ),
        document(
            expression_id,
            CharacterHealthDocumentKind::ExpressionPack,
            CharacterHealthDocumentFormat::Json,
            "unsafe.expression-pack.json",
            "org.weave.health.composed.expression",
            false,
            None,
        ),
        document(
            pack_id,
            CharacterHealthDocumentKind::RelationshipKindPack,
            CharacterHealthDocumentFormat::Json,
            "relationship-kind-pack.json",
            "org.weave.health.composed.relationship_pack",
            false,
            None,
        ),
        document(
            policy_id,
            CharacterHealthDocumentKind::RelationshipPolicy,
            CharacterHealthDocumentFormat::Json,
            "relationship-policy.json",
            "org.weave.health.composed.relationship_policy",
            false,
            None,
        ),
    ];
    let report = audit_character_health(&project(
        documents,
        BTreeMap::from([
            (
                primary_id.to_owned(),
                relationship_collection.to_json().unwrap(),
            ),
            (expression_id.to_owned(), UNSAFE_EXPRESSION_PACK.to_owned()),
            (pack_id.to_owned(), RELATIONSHIP_PACK.to_owned()),
            (policy_id.to_owned(), RELATIONSHIP_POLICY.to_owned()),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        relationship_collection
            .characters
            .values()
            .next()
            .unwrap()
            .provenance
            .clone(),
    ))
    .unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::DuplicateRelationship
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::UnsafePersonalization
    }));

    let mut assistance_collection = CharacterCollection::from_json(ASSISTANCE_COLLECTION).unwrap();
    let candidates = AssistanceCandidateSet::from_json(ASSISTANCE_CANDIDATES).unwrap();
    assistance_collection
        .characters
        .get_mut(&candidates.input_profile.id)
        .unwrap()
        .canon
        .identity
        .display_name
        .value
        .push_str(" Revised");
    let assistance_documents = vec![
        document(
            "org.weave.health.assistance.candidates",
            CharacterHealthDocumentKind::AssistanceCandidateSet,
            CharacterHealthDocumentFormat::Json,
            "candidate-set.json",
            "org.weave.health.assistance.candidates",
            false,
            Some(&candidates.input_profile.id),
        ),
        document(
            "org.weave.health.assistance.collection",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.json",
            "org.weave.health.assistance.collection",
            true,
            None,
        ),
        document(
            "org.weave.health.assistance.review",
            CharacterHealthDocumentKind::AssistanceDecisionReview,
            CharacterHealthDocumentFormat::Json,
            "decision-review.json",
            "org.weave.health.assistance.review",
            false,
            Some(&candidates.input_profile.id),
        ),
    ];
    let assistance_report = audit_character_health(&project(
        assistance_documents,
        BTreeMap::from([
            (
                "org.weave.health.assistance.candidates".to_owned(),
                ASSISTANCE_CANDIDATES.to_owned(),
            ),
            (
                "org.weave.health.assistance.collection".to_owned(),
                assistance_collection.to_json().unwrap(),
            ),
            (
                "org.weave.health.assistance.review".to_owned(),
                ASSISTANCE_REVIEW.to_owned(),
            ),
        ]),
        CharacterHealthPolicy {
            require_portable_pairs: false,
            ..CharacterHealthPolicy::default()
        },
        Vec::new(),
        assistance_collection
            .characters
            .values()
            .next()
            .unwrap()
            .provenance
            .clone(),
    ))
    .unwrap();
    assert!(assistance_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::StaleAssistanceCandidate
    }));
    assert!(!assistance_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == CharacterHealthDiagnosticCode::UnreviewedAssistanceCandidate
    }));

    let clean = collection(clean_profile());
    let mut changed = clean.clone();
    changed.revision += 1;
    let mismatch = project(
        vec![
            document(
                "org.weave.health.mismatch.json",
                CharacterHealthDocumentKind::CharacterCollection,
                CharacterHealthDocumentFormat::Json,
                "collection.json",
                "org.weave.health.mismatch",
                true,
                None,
            ),
            document(
                "org.weave.health.mismatch.ron",
                CharacterHealthDocumentKind::CharacterCollection,
                CharacterHealthDocumentFormat::Ron,
                "collection.ron",
                "org.weave.health.mismatch",
                false,
                None,
            ),
        ],
        BTreeMap::from([
            (
                "org.weave.health.mismatch.json".to_owned(),
                clean.to_json().unwrap(),
            ),
            (
                "org.weave.health.mismatch.ron".to_owned(),
                changed.to_ron().unwrap(),
            ),
        ]),
        CharacterHealthPolicy::default(),
        Vec::new(),
        clean.characters.values().next().unwrap().provenance.clone(),
    );
    let mismatch = audit_character_health(&mismatch).unwrap();
    assert!(
        mismatch.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == CharacterHealthDiagnosticCode::RonJsonMismatch
        })
    );
}
