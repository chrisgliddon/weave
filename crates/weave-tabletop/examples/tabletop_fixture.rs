use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{DomainCatalog, DomainValue, FieldDeclaration, TypeExpression};
use weave_tabletop::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_SELECTION_FORMAT_VERSION,
    ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition, AdapterExtensionSurface,
    AdapterManifest, AdapterProvenance, AdapterSelection, AdapterSourceClass,
    CHARACTER_PROJECTION_FORMAT_VERSION, CanonicalSuggestion, CreationField,
    CreationFieldAuthority, CreationStep, EntropyState, EventVisibility, ExcludedMaterial,
    HostAudience, HostVisibilityPolicy, LicenseTextReference, RESOLVER_CONTRACT_VERSION,
    RESOLVER_FORMAT_VERSION, ResolutionRequest, ResolvedAdapter, ResolverEvent,
    ResolverOperationDeclaration, ResolverOutput, ResolverRegistry, SuggestionDecision,
    TabletopCapability, TabletopCapabilityDeclaration, TabletopCharacterProjection, TabletopError,
    TabletopResolver, TabletopState, adapter_manifest_schema, adapter_selection_schema,
    canonical_fingerprint, character_projection_schema, project_receipt_for_audience,
    resolution_receipt_schema, resolution_request_schema, resolved_adapter, sha256_bytes,
    tabletop_domain_manifest, tabletop_domain_pack, tabletop_state_schema,
    validate_adapter_manifest, validate_adapter_selection, validate_character_projection,
    validate_resolution_receipt, validate_resolution_request, validate_tabletop_state,
    verify_adapter_source,
};

const SOURCE_BYTES: &[u8] =
    include_bytes!("../../../examples/tabletop-adapters/contract/source/synthetic-rules.txt");
const LICENSE_BYTES: &[u8] = include_bytes!("../../../examples/tabletop-adapters/contract/LICENSE");

fn main() {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Err(error) = generate(&root, check) {
        eprintln!("tabletop fixture: {error}");
        std::process::exit(1);
    }
}

fn generate(root: &Path, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = root.join("examples/tabletop-adapters/contract");
    let schemas = root.join("schemas");
    let manifest = synthetic_manifest();
    validate_adapter_manifest(&manifest)?;
    verify_adapter_source(&manifest, SOURCE_BYTES, LICENSE_BYTES)?;
    let coordinate = resolved_adapter(&manifest)?;
    let selection = AdapterSelection {
        selection_format_version: ADAPTER_SELECTION_FORMAT_VERSION,
        installed: vec![coordinate.clone()],
        primary: vec![coordinate.clone()],
    };
    validate_adapter_selection(&selection, std::slice::from_ref(&manifest))?;
    let definition = definition();
    let projection = TabletopCharacterProjection {
        projection_format_version: CHARACTER_PROJECTION_FORMAT_VERSION,
        character_id: "org.weave.character.lumen_reed".to_owned(),
        canonical_profile_sha256: sha256_bytes(b"synthetic canonical character profile v1"),
        canonical_character_write_back: false,
        active: Some(AdapterCharacterDefinition {
            adapter: coordinate.clone(),
            definition_sha256: canonical_fingerprint(&definition)?,
            definition,
            completed_creation_steps: vec!["identity".to_owned(), "ratings".to_owned()],
        }),
        inactive: BTreeMap::new(),
        suggestions: vec![CanonicalSuggestion {
            id: "possible_goal".to_owned(),
            target_path: vec!["canon".to_owned(), "inner_life".to_owned(), "goals".to_owned()],
            proposed_value: DomainValue::String(
                "Keep the trail lanterns lit through the long crossing.".to_owned(),
            ),
            explanation: "An optional fictional goal entered during adapter creation; it remains a reviewed suggestion and is not inferred from personality or applied to canon.".to_owned(),
            source_paths: vec!["adapter.definition.approach".to_owned()],
            decision: SuggestionDecision::Proposed,
            rationale: None,
        }],
    };
    validate_character_projection(&projection, std::slice::from_ref(&manifest))?;
    let state = initial_state(
        coordinate.clone(),
        projection
            .active
            .as_ref()
            .expect("fixture has an active adapter")
            .definition_sha256
            .clone(),
    );
    validate_tabletop_state(&state, &manifest)?;
    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "crossing_attempt".to_owned(),
        adapter: coordinate,
        operation: "attempt".to_owned(),
        capability: TabletopCapability::ChecksAndConflicts,
        definition_sha256: projection
            .active
            .as_ref()
            .expect("fixture has an active adapter")
            .definition_sha256
            .clone(),
        definition: projection
            .active
            .as_ref()
            .expect("fixture has an active adapter")
            .definition
            .clone(),
        expected_state_sha256: canonical_fingerprint(&state)?,
        input: object([
            ("bonus", DomainValue::Number(2.0)),
            ("difficulty", DomainValue::Number(6.0)),
        ]),
    };
    validate_resolution_request(&request, &state, &manifest)?;
    let mut registry = ResolverRegistry::new();
    registry.register(&manifest, LanternTrailResolver)?;
    let receipt = registry.execute(&manifest, &request, &state)?;
    registry.replay(&manifest, &request, &state, &receipt)?;
    validate_resolution_receipt(&receipt, &manifest)?;
    let runtime_receipt = project_receipt_for_audience(&receipt, &manifest, HostAudience::Runtime)?;
    if runtime_receipt
        .events
        .iter()
        .filter(|event| event.payload.is_some())
        .count()
        != 1
    {
        return Err("runtime event projection exposed a hidden payload".into());
    }
    let domain_manifest = tabletop_domain_manifest(&manifest)?;
    let domain_pack = tabletop_domain_pack(
        &manifest,
        &projection,
        &state,
        "lumen_reed",
        "1.0.0",
        "Lumen Reed Lantern Trail Projection",
    )?;
    let domain_catalog =
        DomainCatalog::from_artifacts([domain_manifest.clone()], [domain_pack.clone()])?;
    let source =
        include_str!("../../../examples/tabletop-adapters/contract/runtime/lantern-trail.weave");
    let options = CompileOptions {
        source_name: Some(
            "examples/tabletop-adapters/contract/runtime/lantern-trail.weave".to_owned(),
        ),
    };
    let story = compile_with_modules(source, &options, &domain_catalog)?.story;

    let mut unsupported = manifest.clone();
    unsupported.manifest_format_version = 99;
    let conflicting = AdapterSelection {
        selection_format_version: ADAPTER_SELECTION_FORMAT_VERSION,
        installed: vec![resolved_adapter(&manifest)?],
        primary: vec![resolved_adapter(&manifest)?, resolved_adapter(&manifest)?],
    };
    let mut undeclared = request.clone();
    undeclared.operation = "advance".to_owned();
    undeclared.capability = TabletopCapability::Advancement;

    let artifacts = [
        (
            fixture.join("synthetic.tabletop-adapter.json"),
            manifest.to_json()?,
        ),
        (
            fixture.join("synthetic.tabletop-adapter.ron"),
            manifest.to_ron()?,
        ),
        (
            fixture.join("selection.tabletop-selection.json"),
            selection.to_json()?,
        ),
        (
            fixture.join("selection.tabletop-selection.ron"),
            selection.to_ron()?,
        ),
        (
            fixture.join("character.tabletop-projection.json"),
            projection.to_json()?,
        ),
        (
            fixture.join("character.tabletop-projection.ron"),
            projection.to_ron()?,
        ),
        (fixture.join("state.tabletop-state.json"), state.to_json()?),
        (fixture.join("state.tabletop-state.ron"), state.to_ron()?),
        (
            fixture.join("request.tabletop-request.json"),
            request.to_json()?,
        ),
        (
            fixture.join("request.tabletop-request.ron"),
            request.to_ron()?,
        ),
        (
            fixture.join("receipt.tabletop-receipt.json"),
            receipt.to_json()?,
        ),
        (
            fixture.join("receipt.tabletop-receipt.ron"),
            receipt.to_ron()?,
        ),
        (
            fixture.join("runtime.tabletop-receipt.json"),
            runtime_receipt.to_json()?,
        ),
        (
            fixture.join("runtime/module.weave-module.json"),
            domain_manifest.to_json()?,
        ),
        (
            fixture.join("runtime/module.weave-module.ron"),
            domain_manifest.to_ron()?,
        ),
        (
            fixture.join("runtime/lumen_reed.weave-domain.json"),
            domain_pack.to_json()?,
        ),
        (
            fixture.join("runtime/lumen_reed.weave-domain.ron"),
            domain_pack.to_ron()?,
        ),
        (
            fixture.join("runtime/lantern-trail.story.json"),
            to_json(&story)?,
        ),
        (
            fixture.join("runtime/lantern-trail.story.ron"),
            to_ron(&story)?,
        ),
        (
            fixture.join("invalid/unsupported-version.tabletop-adapter.json"),
            unsupported.to_json()?,
        ),
        (
            fixture.join("invalid/conflicting-primary.tabletop-selection.json"),
            conflicting.to_json()?,
        ),
        (
            fixture.join("invalid/undeclared-capability.tabletop-request.json"),
            undeclared.to_json()?,
        ),
        (
            schemas.join("weave-tabletop-adapter-manifest-v1.schema.json"),
            adapter_manifest_schema()?,
        ),
        (
            schemas.join("weave-tabletop-adapter-selection-v1.schema.json"),
            adapter_selection_schema()?,
        ),
        (
            schemas.join("weave-tabletop-character-projection-v1.schema.json"),
            character_projection_schema()?,
        ),
        (
            schemas.join("weave-tabletop-state-v1.schema.json"),
            tabletop_state_schema()?,
        ),
        (
            schemas.join("weave-tabletop-resolution-request-v1.schema.json"),
            resolution_request_schema()?,
        ),
        (
            schemas.join("weave-tabletop-resolution-receipt-v1.schema.json"),
            resolution_receipt_schema()?,
        ),
    ];
    for (path, contents) in artifacts {
        emit(&path, &contents, check)?;
    }
    Ok(())
}

fn synthetic_manifest() -> AdapterManifest {
    let attempt_input = object_type([
        (
            "bonus",
            field(integer(-3.0, 3.0), true, "Explicit attempt bonus."),
        ),
        (
            "difficulty",
            field(integer(2.0, 12.0), true, "Explicit target difficulty."),
        ),
    ]);
    let attempt_event = object_type([
        (
            "outcome",
            field(symbol(["failure", "success"]), true, "Public outcome."),
        ),
        ("total", field(integer(-2.0, 9.0), true, "Resolved total.")),
    ]);
    let private_event = object_type([(
        "die",
        field(integer(1.0, 6.0), true, "Authority-host entropy result."),
    )]);
    let resource_event = object_type([(
        "momentum",
        field(integer(0.0, 6.0), true, "Remaining fictional momentum."),
    )]);
    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: "org.weave.tabletop.lantern_trail".to_owned(),
        version: "1.0.0".to_owned(),
        schema_version: 1,
        namespace: "lantern_trail".to_owned(),
        title: "Lantern Trail Synthetic Adapter".to_owned(),
        compatibility_label: "Original Weave contract fixture; no external system compatibility"
            .to_owned(),
        summary: "A deliberately tiny original rules procedure that tests portable creation, state, entropy, events, visibility, and replay without representing a published game.".to_owned(),
        weave_version: ">=0.1.0, <0.2.0".to_owned(),
        character_contract_version: ">=1.0.0, <2.0.0".to_owned(),
        extension_surface: AdapterExtensionSurface::DeclarativeDataWithRegisteredResolver {
            resolver_contract_version: RESOLVER_CONTRACT_VERSION,
        },
        capabilities: vec![
            capability(TabletopCapability::CharacterCreation),
            capability(TabletopCapability::DerivedValues),
            capability(TabletopCapability::ChecksAndConflicts),
            capability(TabletopCapability::ResourcesAndConditions),
        ],
        types: BTreeMap::new(),
        definition_type: object_type([
            (
                "approach",
                field(
                    symbol(["bold", "careful", "clever"]),
                    true,
                    "Explicit fictional approach.",
                ),
            ),
            (
                "call_sign",
                field(string(1, 80), true, "Adapter-owned display call sign."),
            ),
            (
                "focus",
                field(integer(0.0, 4.0), true, "Explicit Focus rating."),
            ),
            (
                "resilience",
                field(integer(0.0, 4.0), true, "Explicit Resilience rating."),
            ),
        ]),
        state_type: object_type([
            (
                "conditions",
                field(
                    TypeExpression::List {
                        items: Box::new(symbol(["fatigued", "rattled"])),
                        min_items: 0,
                        max_items: 2,
                    },
                    true,
                    "Current adapter-owned conditions.",
                ),
            ),
            (
                "last_total",
                field(integer(-2.0, 9.0), true, "Most recent resolved total."),
            ),
            (
                "momentum",
                field(integer(0.0, 6.0), true, "Mutable fictional momentum."),
            ),
        ]),
        creation_steps: vec![
            CreationStep {
                id: "identity".to_owned(),
                title: "Trail identity".to_owned(),
                description: "Choose adapter-owned identity details; do not infer them from Character personality.".to_owned(),
                required_capability: TabletopCapability::CharacterCreation,
                fields: vec![
                    creation_field("call_sign", "Call sign", string(1, 80)),
                    creation_field("approach", "Approach", symbol(["bold", "careful", "clever"])),
                ],
            },
            CreationStep {
                id: "ratings".to_owned(),
                title: "Trail ratings".to_owned(),
                description: "Assign the two adapter ratings explicitly without personality-derived defaults.".to_owned(),
                required_capability: TabletopCapability::CharacterCreation,
                fields: vec![
                    creation_field("focus", "Focus", integer(0.0, 4.0)),
                    creation_field("resilience", "Resilience", integer(0.0, 4.0)),
                ],
            },
        ],
        operations: BTreeMap::from([(
            "attempt".to_owned(),
            ResolverOperationDeclaration {
                id: "attempt".to_owned(),
                title: "Resolve an attempt".to_owned(),
                description: "Consume one deterministic draw, compare an explicit total, and emit visibility-scoped typed events.".to_owned(),
                required_capability: TabletopCapability::ChecksAndConflicts,
                request_type: attempt_input,
                event_kinds: vec![
                    "attempt_resolved".to_owned(),
                    "private_roll".to_owned(),
                    "resource_changed".to_owned(),
                ],
                consumes_entropy: true,
            },
        )]),
        event_types: BTreeMap::from([
            ("attempt_resolved".to_owned(), attempt_event),
            ("private_roll".to_owned(), private_event),
            ("resource_changed".to_owned(), resource_event),
        ]),
        visibility: HostVisibilityPolicy::default(),
        migrations: Vec::new(),
        provenance: AdapterProvenance {
            source_class: AdapterSourceClass::Original,
            source_url: "https://github.com/chrisgliddon/weave".to_owned(),
            exact_artifact: "source/synthetic-rules.txt".to_owned(),
            revision: "weave-tabletop-contract-v1".to_owned(),
            retrieved_on: "2026-08-22".to_owned(),
            sha256: sha256_bytes(SOURCE_BYTES),
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            covered_files_or_sections: vec![
                "Complete independently authored Lantern Trail synthetic fixture".to_owned(),
            ],
            exclusions: vec![
                ExcludedMaterial::CommunitySupplements,
                ExcludedMaterial::Logos,
                ExcludedMaterial::TradeDress,
                ExcludedMaterial::Artwork,
                ExcludedMaterial::Layout,
                ExcludedMaterial::UnverifiedAssets,
            ],
            attribution: "Lantern Trail synthetic contract fixture, authored for Weave under MIT."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE".to_owned(),
                sha256: sha256_bytes(LICENSE_BYTES),
            },
            additional_artifacts: Vec::new(),
            notices: vec![
                "Factual compatibility labels must not imply endorsement.".to_owned(),
            ],
            compatibility_statement: "This original fixture has no external ruleset compatibility and names no third-party product.".to_owned(),
        },
    }
}

fn definition() -> DomainValue {
    object([
        ("approach", DomainValue::Symbol("careful".to_owned())),
        ("call_sign", DomainValue::String("Lumen Reed".to_owned())),
        ("focus", DomainValue::Number(3.0)),
        ("resilience", DomainValue::Number(2.0)),
    ])
}

fn initial_state(adapter: ResolvedAdapter, definition_sha256: String) -> TabletopState {
    TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter,
        owner_id: "org.weave.character.lumen_reed".to_owned(),
        definition_sha256,
        revision: 0,
        entropy: EntropyState {
            algorithm: "sha256_counter_v1".to_owned(),
            seed: 20260822,
            cursor: 0,
        },
        value: object([
            ("conditions", DomainValue::List(Vec::new())),
            ("last_total", DomainValue::Number(0.0)),
            ("momentum", DomainValue::Number(2.0)),
        ]),
    }
}

struct LanternTrailResolver;

impl TabletopResolver for LanternTrailResolver {
    fn adapter_id(&self) -> &str {
        "org.weave.tabletop.lantern_trail"
    }

    fn adapter_version(&self) -> &str {
        "1.0.0"
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut weave_tabletop::EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        if operation != "attempt" {
            return Err(TabletopError::UndeclaredEvent);
        }
        let request = as_object(request)?;
        let definition = as_object(definition)?;
        let mut state = as_object(state)?.clone();
        let difficulty = integer_value(&request["difficulty"])?;
        let bonus = integer_value(&request["bonus"])?;
        let focus = integer_value(&definition["focus"])?;
        let die = entropy.draw_bounded(6)? as i64 + 1;
        let total = die + bonus + focus - 3;
        let success = total >= difficulty;
        let momentum = integer_value(&state["momentum"])?;
        let next_momentum = if success {
            (momentum + 1).min(6)
        } else {
            momentum.saturating_sub(1)
        };
        state.insert("last_total".to_owned(), DomainValue::Number(total as f64));
        state.insert(
            "momentum".to_owned(),
            DomainValue::Number(next_momentum as f64),
        );
        Ok(ResolverOutput {
            state: DomainValue::Object(state),
            events: vec![
                ResolverEvent {
                    kind: "attempt_resolved".to_owned(),
                    visibility: EventVisibility::Public,
                    payload: object([
                        (
                            "outcome",
                            DomainValue::Symbol(
                                if success { "success" } else { "failure" }.to_owned(),
                            ),
                        ),
                        ("total", DomainValue::Number(total as f64)),
                    ]),
                },
                ResolverEvent {
                    kind: "private_roll".to_owned(),
                    visibility: EventVisibility::HostOnly,
                    payload: object([("die", DomainValue::Number(die as f64))]),
                },
                ResolverEvent {
                    kind: "resource_changed".to_owned(),
                    visibility: EventVisibility::Authoring,
                    payload: object([("momentum", DomainValue::Number(next_momentum as f64))]),
                },
            ],
        })
    }
}

fn capability(capability: TabletopCapability) -> TabletopCapabilityDeclaration {
    TabletopCapabilityDeclaration {
        capability,
        version: 1,
    }
}

fn creation_field(id: &str, label: &str, value_type: TypeExpression) -> CreationField {
    CreationField {
        id: id.to_owned(),
        label: label.to_owned(),
        description: format!("Explicit adapter-owned {label} value."),
        value_type,
        required: true,
        authority: CreationFieldAuthority::AdapterOwned,
    }
}

fn field(value_type: TypeExpression, required: bool, description: &str) -> FieldDeclaration {
    FieldDeclaration {
        value_type,
        required,
        description: description.to_owned(),
    }
}

fn integer(minimum: f64, maximum: f64) -> TypeExpression {
    TypeExpression::Number {
        integer: true,
        minimum: Some(minimum),
        maximum: Some(maximum),
    }
}

fn string(min_length: usize, max_length: usize) -> TypeExpression {
    TypeExpression::String {
        min_length,
        max_length,
    }
}

fn symbol<const N: usize>(values: [&str; N]) -> TypeExpression {
    TypeExpression::Symbol {
        values: values.into_iter().map(str::to_owned).collect(),
    }
}

fn object_type<const N: usize>(fields: [(&str, FieldDeclaration); N]) -> TypeExpression {
    TypeExpression::Object {
        fields: fields
            .into_iter()
            .map(|(name, field)| (name.to_owned(), field))
            .collect(),
    }
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn as_object(value: &DomainValue) -> Result<&BTreeMap<String, DomainValue>, TabletopError> {
    match value {
        DomainValue::Object(value) => Ok(value),
        _ => Err(TabletopError::SchemaMismatch { path: "resolver" }),
    }
}

fn integer_value(value: &DomainValue) -> Result<i64, TabletopError> {
    match value {
        DomainValue::Number(value) if value.fract() == 0.0 => Ok(*value as i64),
        _ => Err(TabletopError::SchemaMismatch { path: "resolver" }),
    }
}

fn emit(path: &Path, contents: &str, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    if check {
        if fs::read_to_string(path).ok().as_deref() != Some(contents) {
            return Err(format!("{} is stale", path.display()).into());
        }
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
    }
    Ok(())
}
