//! Self-tests for the registry-driven tabletop adapter conformance harness.
//!
//! The first test runs every shipped adapter through the standard suite. The remaining tests prove
//! the harness itself fails closed: deliberately broken fixtures must trip one check each with a
//! distinct, value-bearing diagnostic.

use std::sync::atomic::{AtomicU64, Ordering};

use weave_domain::DomainValue;
use weave_tabletop::conformance::policy::PLUG_AND_PLAY_SOURCE_POLICY;
use weave_tabletop::conformance::{
    AdapterSourcePolicy, CONFORMANCE_SEED, ConformanceAdapter, ConformanceCheck,
    ConformanceCreation, ConformanceOperation, ConformanceReport, ConformanceSourceBundle,
    ConformanceSuite, SourcePolicy, standard_source_policy, standard_suite,
};
use weave_tabletop::{
    AdapterManifest, EntropyStream, PlugAndPlayConformance, PlugAndPlayResolver, ResolverOutput,
    TabletopError, TabletopResolver, resolved_adapter,
};

const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const COLLISION_NAMESPACE: &str = "conformance_collision";

#[test]
fn every_registered_adapter_passes_every_check() {
    let suite = standard_suite();
    let report = suite.run();
    println!("{}", report.render());

    assert_eq!(
        suite.adapter_ids(),
        vec![
            "org.weave.tabletop.plug_and_play",
            "org.weave.tabletop.dungeonpunk",
            "org.weave.tabletop.freehack",
            "org.weave.tabletop.wicked_ones",
            "org.weave.tabletop.spark",
        ]
    );
    assert!(report.passed(), "{}", report.render());
    assert_eq!(report.adapters.len(), 5);
    for adapter in &report.adapters {
        assert!(adapter.failures.is_empty(), "{}", adapter.summary());
        assert_eq!(adapter.executed, ConformanceCheck::ALL.len());
        assert_eq!(adapter.skipped_checks(), 0);
        assert!(
            adapter.summary().starts_with("ok   "),
            "{}",
            adapter.summary()
        );
    }

    let summary = report.summary_lines();
    assert_eq!(summary.len(), report.adapters.len());
    assert_eq!(report.render(), summary.join("\n"));
    assert!(report.detail_lines().is_empty());
}

#[test]
fn a_new_adapter_joins_the_registry_without_touching_the_suite() {
    let fixture = Fixture::healthy("org.weave.tabletop.conformance_extra", "conformance_extra");
    let report = run_single(&fixture);
    println!("{}", report.render());

    assert!(report.passed(), "{}", report.render());
    assert_eq!(report.adapters[0].executed, ConformanceCheck::ALL.len());
    assert_eq!(
        report.adapters[0].summary(),
        format!(
            "ok   org.weave.tabletop.conformance_extra@1.0.0: {} of {} checks passed",
            ConformanceCheck::ALL.len(),
            ConformanceCheck::ALL.len()
        )
    );
}

#[test]
fn every_registered_adapter_has_a_source_policy_row() {
    let policy = standard_source_policy();
    for adapter_id in standard_suite().adapter_ids() {
        assert!(
            policy.entry(adapter_id).is_some(),
            "missing source policy row for `{adapter_id}`"
        );
    }
    let spark = policy
        .entry("org.weave.tabletop.spark")
        .expect("missing Spark source policy row");
    assert_eq!(spark.license, "CC0-1.0");
    assert_eq!(
        spark.source_sha256,
        weave_tabletop::SPARK_SOURCE_RECORD_SHA256
    );
}

#[test]
fn source_policy_is_complete_and_data_driven() {
    let policy = standard_source_policy();
    assert_eq!(policy.entries().count(), 5);
    for adapter_id in standard_suite().adapter_ids() {
        let entry = policy
            .entry(adapter_id)
            .unwrap_or_else(|| panic!("missing policy entry for `{adapter_id}`"));
        assert_eq!(entry.source_sha256.len(), 64);
        assert!(!entry.required_license_text.path.is_empty());
        assert!(!entry.required_license_text.sha256.is_empty());
    }
    assert!(policy.entry("org.weave.tabletop.unreviewed").is_none());
}

#[test]
fn a_changed_source_hash_fails_only_the_source_hash_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_hash",
        "conformance_hash",
        Break::HashMismatch,
    );
    let failure = single_failure(&fixture, ConformanceCheck::SourceHash);
    assert!(failure.detail.contains("source hash mismatch"), "{failure}");
    assert!(failure.detail.contains(ZERO_SHA256), "{failure}");
    assert!(
        failure
            .detail
            .contains(weave_tabletop::PLUG_AND_PLAY_RULES_SHA256),
        "{failure}"
    );
}

#[test]
fn a_missing_source_hash_is_rejected() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_missing_hash",
        "conformance_missing_hash",
        Break::MissingSourceHash,
    );
    let failure = single_failure(&fixture, ConformanceCheck::SourceHash);
    assert!(failure.detail.contains("missing source hash"), "{failure}");
}

#[test]
fn a_namespace_collision_fails_only_the_collision_check() {
    let first = Fixture::healthy("org.weave.conformance_collision.first", COLLISION_NAMESPACE);
    let second = Fixture::healthy(
        "org.weave.conformance_collision.second",
        COLLISION_NAMESPACE,
    );
    let policy = SourcePolicy::new(vec![fixture_policy(&first), fixture_policy(&second)]);
    let report = ConformanceSuite::new(vec![Box::new(first), Box::new(second)], policy).run();
    println!("{}", report.render());

    assert_eq!(report.adapters.len(), 2);
    assert!(!report.passed());
    for adapter in &report.adapters {
        assert_eq!(adapter.failures.len(), 1, "{}", adapter.summary());
        let failure = &adapter.failures[0];
        assert_eq!(failure.check, ConformanceCheck::NamespaceCollision);
        assert!(failure.detail.contains(COLLISION_NAMESPACE), "{failure}");
        assert!(
            failure.detail.contains("conformance_collision.first"),
            "{failure}"
        );
        assert!(
            failure.detail.contains("conformance_collision.second"),
            "{failure}"
        );
    }
}

#[test]
fn nondeterministic_creation_fails_only_the_creation_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_nondeterministic",
        "conformance_nondeterministic",
        Break::Nondeterministic,
    );
    let failure = single_failure(&fixture, ConformanceCheck::DeterministicCreation);
    assert!(failure.detail.contains("differs between runs"), "{failure}");
    assert!(
        failure.detail.contains(&CONFORMANCE_SEED.to_string()),
        "{failure}"
    );
}

#[test]
fn a_missing_notice_fails_only_the_notice_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_missing_notice",
        "conformance_missing_notice",
        Break::MissingNotice,
    );
    let failure = single_failure(&fixture, ConformanceCheck::LicenseNotice);
    assert!(
        failure.detail.contains("missing required notice"),
        "{failure}"
    );
    assert!(
        failure
            .detail
            .contains("The official PDFs are hash-pinned inputs"),
        "{failure}"
    );
}

#[test]
fn a_permissive_resolver_fails_only_the_unknown_capability_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_permissive",
        "conformance_permissive",
        Break::PermissiveResolver,
    );
    let failure = single_failure(&fixture, ConformanceCheck::UnknownCapability);
    assert!(
        failure.detail.contains("accepted undeclared operation"),
        "{failure}"
    );
    assert!(
        failure.detail.contains("conformance_undeclared_operation"),
        "{failure}"
    );
}

#[test]
fn a_policy_licence_mismatch_fails_only_the_licence_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_licence",
        "conformance_licence",
        Break::WrongPolicyLicence,
    );
    let failure = single_failure(&fixture, ConformanceCheck::LicensePolicy);
    assert!(
        failure
            .detail
            .contains("does not match reviewed policy `MIT`"),
        "{failure}"
    );
}

#[test]
fn a_forged_redistributed_flag_fails_only_the_forbidden_file_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_forbidden",
        "conformance_forbidden",
        Break::ForgedRedistributedFlag,
    );
    let failure = single_failure(&fixture, ConformanceCheck::ForbiddenFile);
    assert!(failure.detail.contains("forbidden file"), "{failure}");
    assert!(
        failure.detail.contains("Plug-And-Play Character Sheet.pdf"),
        "{failure}"
    );
    assert!(
        !fixture.manifest().provenance.additional_artifacts[0].redistributed,
        "the forged manifest flag must stay false for this fixture"
    );
}

#[test]
fn a_changed_bundle_digest_fails_only_the_source_bundle_check() {
    let fixture = Fixture::broken(
        "org.weave.tabletop.conformance_bundle_digest",
        "conformance_bundle_digest",
        Break::ChangedBundleDigest,
    );
    let failure = single_failure(&fixture, ConformanceCheck::SourceBundle);
    assert!(
        failure
            .detail
            .contains("companion artifact digest mismatch"),
        "{failure}"
    );
    assert!(
        failure.detail.contains("Plug-And-Play Character Sheet.pdf"),
        "{failure}"
    );
    assert!(failure.detail.contains(ZERO_SHA256), "{failure}");
}

#[test]
fn every_category_reports_a_distinct_adapter_named_diagnostic() {
    let fixtures = [
        Fixture::broken(
            "org.weave.tabletop.conformance_hash",
            "conformance_hash",
            Break::HashMismatch,
        ),
        Fixture::broken(
            "org.weave.tabletop.conformance_nondeterministic",
            "conformance_nondeterministic",
            Break::Nondeterministic,
        ),
        Fixture::broken(
            "org.weave.tabletop.conformance_missing_notice",
            "conformance_missing_notice",
            Break::MissingNotice,
        ),
        Fixture::broken(
            "org.weave.tabletop.conformance_permissive",
            "conformance_permissive",
            Break::PermissiveResolver,
        ),
    ];

    let mut messages = Vec::new();
    for fixture in &fixtures {
        let report = run_single(fixture);
        assert_eq!(report.failure_count(), 1, "{}", report.render());
        let failure = &report.adapters[0].failures[0];
        let rendered = failure.to_string();
        assert!(
            rendered.starts_with(&format!("adapter `{}` failed", fixture.id)),
            "{rendered}"
        );
        assert!(rendered.contains(failure.check.id()), "{rendered}");
        assert!(
            report.summary_lines()[0].contains(fixture.id),
            "{}",
            report.summary_lines()[0]
        );
        messages.push(rendered);
    }
    let mut unique = messages.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), messages.len(), "{messages:#?}");
}

fn single_failure(
    fixture: &Fixture,
    expected: ConformanceCheck,
) -> weave_tabletop::conformance::ConformanceFailure {
    let report = run_single(fixture);
    println!("{}", report.render());
    assert_eq!(report.adapters.len(), 1);
    let adapter = &report.adapters[0];
    assert_eq!(
        adapter.failures.len(),
        1,
        "expected exactly one failure for `{}`; got {}: {}",
        fixture.id,
        adapter.failures.len(),
        report.render()
    );
    let failure = adapter.failures[0].clone();
    assert_eq!(
        failure.check,
        expected,
        "expected `{}` for `{}`; got `{}`",
        expected.id(),
        fixture.id,
        failure.check.id()
    );
    assert_eq!(failure.adapter, fixture.id);
    failure
}

fn run_single(fixture: &Fixture) -> ConformanceReport {
    ConformanceSuite::new(
        vec![Box::new(*fixture)],
        SourcePolicy::new(vec![fixture_policy(fixture)]),
    )
    .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Break {
    None,
    HashMismatch,
    MissingSourceHash,
    Nondeterministic,
    MissingNotice,
    PermissiveResolver,
    WrongPolicyLicence,
    ChangedBundleDigest,
    ForgedRedistributedFlag,
}

/// Test-only adapter derived from the Plug-And-Play fixture.
///
/// Each variant changes exactly one public property so a failure can be attributed to one check.
#[derive(Debug, Clone, Copy)]
struct Fixture {
    id: &'static str,
    namespace: &'static str,
    behavior: Break,
}

impl Fixture {
    const fn healthy(id: &'static str, namespace: &'static str) -> Self {
        Self {
            id,
            namespace,
            behavior: Break::None,
        }
    }

    const fn broken(id: &'static str, namespace: &'static str, behavior: Break) -> Self {
        Self {
            id,
            namespace,
            behavior,
        }
    }
}

impl ConformanceAdapter for Fixture {
    fn adapter_id(&self) -> &str {
        self.id
    }

    fn manifest(&self) -> AdapterManifest {
        let mut manifest = PlugAndPlayConformance.manifest();
        manifest.id = self.id.to_owned();
        manifest.namespace = self.namespace.to_owned();
        if self.behavior == Break::MissingNotice {
            manifest.provenance.notices.clear();
        }
        manifest
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(FixtureResolver {
            id: self.id.to_owned(),
            version: "1.0.0".to_owned(),
            permissive: self.behavior == Break::PermissiveResolver,
        })
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let mut creation = PlugAndPlayConformance.create(seed)?;
        let resolved = resolved_adapter(&self.manifest())?;
        creation.adapter = resolved.clone();
        creation.definition.adapter = resolved.clone();
        creation.initial_state.adapter = resolved;
        if self.behavior == Break::Nondeterministic {
            static NONCE: AtomicU64 = AtomicU64::new(0);
            let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
            creation
                .canonical
                .push_str(&format!("\n// creation nonce {nonce}"));
        }
        Ok(creation)
    }

    fn scenario(
        &self,
        creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        PlugAndPlayConformance.scenario(creation)
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        let mut bundle = PlugAndPlayConformance.source_bundle();
        match self.behavior {
            Break::ChangedBundleDigest => {
                if let Some(sheet) = bundle.companions.first_mut() {
                    sheet.sha256 = ZERO_SHA256.to_owned();
                }
            }
            Break::ForgedRedistributedFlag => {
                if let Some(sheet) = bundle.companions.first_mut() {
                    sheet.bundled = true;
                }
            }
            _ => {}
        }
        bundle
    }
}

struct FixtureResolver {
    id: String,
    version: String,
    permissive: bool,
}

impl TabletopResolver for FixtureResolver {
    fn adapter_id(&self) -> &str {
        &self.id
    }

    fn adapter_version(&self) -> &str {
        &self.version
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        const DECLARED: [&str; 7] = [
            "attack",
            "chase",
            "check",
            "fortune_test",
            "group_check",
            "initiative",
            "take_damage",
        ];
        if self.permissive && !DECLARED.contains(&operation) {
            return Ok(ResolverOutput {
                state: state.clone(),
                events: Vec::new(),
            });
        }
        PlugAndPlayResolver.resolve(operation, definition, request, state, entropy)
    }
}

fn fixture_policy(fixture: &Fixture) -> AdapterSourcePolicy {
    let mut policy = AdapterSourcePolicy {
        adapter_id: fixture.id,
        namespace: fixture.namespace,
        ..PLUG_AND_PLAY_SOURCE_POLICY
    };
    match fixture.behavior {
        Break::HashMismatch => policy.source_sha256 = ZERO_SHA256,
        Break::MissingSourceHash => policy.source_sha256 = "",
        Break::WrongPolicyLicence => policy.license = "MIT",
        _ => {}
    }
    policy
}

#[test]
#[ignore = "prints the full conformance report; run with --ignored --nocapture"]
fn conformance_report_lines() {
    let report = standard_suite().run();
    println!("{}", report.render());
    assert!(report.passed(), "{}", report.render());
    assert_eq!(report.summary_lines().len(), report.adapters.len());
}
