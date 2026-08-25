use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{
    CapabilityDeclaration, DOMAIN_CONTRACT_VERSION, DOMAIN_PACK_FORMAT_VERSION, DomainCatalog,
    DomainPack, DomainValue, ExportDeclaration, ExportSource, FieldDeclaration, ModuleAuthor,
    ModuleAuthoring, ModuleManifest, ModuleRequirement, Provenance, ProvenanceKind,
    ProvenanceSource, ProvenanceTransformation, ReadOnlyPathDeclaration, TypeExpression,
    validate_manifest, validate_pack,
};
use weave_tabletop::{
    ADAPTER_SELECTION_FORMAT_VERSION, AdapterSelection, CHARACTER_PROJECTION_FORMAT_VERSION,
    FREEHACK_CAMPAIGN_SCHEMA_VERSION, FREEHACK_CC0_1_0_LEGAL_CODE_SHA256,
    FREEHACK_CREATION_FORMAT_VERSION, FREEHACK_MARKDOWN_SHA256, FREEHACK_METADATA_SHA256,
    FREEHACK_PDF_SHA256, FREEHACK_PROBABILITY_FORMAT_VERSION, FREEHACK_RELEASE_URL,
    FREEHACK_ROLL_TOOL_SHA256, FREEHACK_SOURCE_REVISION, FREEHACK_SOURCE_URL,
    FreehackArchetypeOption, FreehackArchetypeProcedure, FreehackAuthorityReceipt,
    FreehackCampaignSchema, FreehackContribution, FreehackContributionSource,
    FreehackCreationPreview, FreehackCreationRequest, FreehackDisclosure, FreehackFeatureOption,
    FreehackFeatureSelection, FreehackInventoryOption, FreehackInventorySelection,
    FreehackMemorySeed, FreehackModifierGeneration, FreehackModifierSpec,
    FreehackProbabilityRequest, FreehackPublicReceipt, FreehackPublicState, FreehackRarity,
    FreehackResolver, FreehackSectionStatus, FreehackTrackOutcome, FreehackTrackTemplate,
    RESOLVER_FORMAT_VERSION, ResolutionRequest, ResolvedAdapter, ResolverRegistry,
    TabletopCapability, TabletopCharacterProjection, TabletopState, canonical_fingerprint,
    create_freehack_character, freehack_authority_receipt_schema, freehack_creation_preview_schema,
    freehack_creation_request_schema, freehack_manifest, freehack_probability_preview,
    freehack_probability_preview_schema, freehack_probability_request_schema,
    freehack_public_receipt_schema, freehack_public_state_schema,
    project_freehack_authority_receipt, project_freehack_public_receipt,
    project_freehack_public_state, resolved_adapter, sha256_bytes, validate_adapter_manifest,
    validate_adapter_selection, validate_character_projection, validate_freehack_authority_receipt,
    validate_freehack_creation_preview, validate_freehack_public_receipt,
    validate_freehack_public_state, validate_resolution_receipt, validate_tabletop_state,
    verify_adapter_source_bundle,
};

const LICENSE_BYTES: &[u8] =
    include_bytes!("../../../examples/tabletop-adapters/plug-and-play/LICENSE-CC0-1.0.txt");

const STORY_SOURCE: &str = r#"module freehack {
    id: "org.weave.tabletop.freehack.public"
    version: "=1.0.0"
    pack: "tavi_quill@=1.0.0"
}

VAR adapter_id = freehack.adapter.id
VAR adapter_version = freehack.adapter.version
VAR adapter_sha256 = freehack.adapter.content_sha256
VAR checks_supported = freehack.capabilities.checks_and_conflicts
VAR scenes_supported = freehack.capabilities.scenes
VAR tracks_supported = freehack.capabilities.resources_and_conditions
VAR character_name = freehack.character.name
VAR archetype = freehack.character.archetype_id
VAR focus = freehack.character.modifiers.focus
VAR fatigue = freehack.tracks.fatigue.progress
VAR section_status = freehack.snapshot.gantry_status
VAR public_memories = freehack.snapshot.memory_count

=== start ===
{checks_supported && scenes_supported && tracks_supported:
    {character_name}, a {archetype}, enters play with Focus {focus} and Fatigue {fatigue}.
    The gantry section is {section_status}; {public_memories} public memories are available.
    The active rules are {adapter_id}@{adapter_version} ({adapter_sha256}).
- else:
    This branch is unreachable because the public module declares checks, scenes, and tracks.
}
-> END
"#;

fn main() {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Err(error) = generate(&root, check) {
        eprintln!("Freehack fixture: {error}");
        std::process::exit(1);
    }
}

fn generate(root: &Path, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = root.join("examples/tabletop-adapters/freehack");
    let schemas = root.join("schemas");
    let manifest = freehack_manifest();
    validate_adapter_manifest(&manifest)?;
    if sha256_bytes(LICENSE_BYTES) != FREEHACK_CC0_1_0_LEGAL_CODE_SHA256 {
        return Err("retained CC0 legal code hash changed".into());
    }
    verify_optional_source_audit(root, &manifest)?;

    let creation_request = creation_request();
    let preview = create_freehack_character(&creation_request)?;
    validate_freehack_creation_preview(&preview)?;
    let selection = AdapterSelection {
        selection_format_version: ADAPTER_SELECTION_FORMAT_VERSION,
        installed: vec![preview.adapter.clone()],
        primary: vec![preview.adapter.clone()],
    };
    validate_adapter_selection(&selection, std::slice::from_ref(&manifest))?;
    let projection = TabletopCharacterProjection {
        projection_format_version: CHARACTER_PROJECTION_FORMAT_VERSION,
        character_id: creation_request.character_id.clone(),
        canonical_profile_sha256: sha256_bytes(b"Tavi Quill original canonical profile v1"),
        canonical_character_write_back: false,
        active: Some(preview.definition.clone()),
        inactive: BTreeMap::new(),
        suggestions: Vec::new(),
    };
    validate_character_projection(&projection, std::slice::from_ref(&manifest))?;
    let initial_state = preview.initial_state.clone();
    validate_tabletop_state(&initial_state, &manifest)?;

    let probability_request = probability_request();
    let probability_preview = freehack_probability_preview(&probability_request)?;
    let mut registry = ResolverRegistry::new();
    registry.register(&manifest, FreehackResolver)?;
    let mut state = initial_state.clone();
    let mut playthrough = Vec::new();
    let mut mid_section_state = None;
    for (id, operation, capability, input) in operation_inputs() {
        let context = |error| format!("Freehack playthrough `{id}`: {error}");
        let request = resolution_request(id, operation, capability, &preview, &state, input)
            .map_err(context)?;
        let receipt = registry
            .execute(&manifest, &request, &state)
            .map_err(context)?;
        registry
            .replay(&manifest, &request, &state, &receipt)
            .map_err(context)?;
        validate_resolution_receipt(&receipt, &manifest).map_err(context)?;
        let authority = project_freehack_authority_receipt(&receipt).map_err(context)?;
        validate_freehack_authority_receipt(&authority).map_err(context)?;
        let public = project_freehack_public_receipt(&receipt).map_err(context)?;
        if let Some(public) = &public {
            validate_freehack_public_receipt(public).map_err(context)?;
        }
        state = receipt.after_state.clone();
        if id == "submit_ira" {
            mid_section_state = Some(state.clone());
        }
        playthrough.push((id, request, receipt, authority, public));
    }
    let final_state = state;
    let mid_section_state = mid_section_state.ok_or("mid-section save was not captured")?;
    let public_state = project_freehack_public_state(&final_state)?;
    validate_freehack_public_state(&public_state)?;

    let visible = playthrough
        .iter()
        .find(|(id, ..)| *id == "cross_wet_gantry")
        .ok_or("visible check is absent")?;
    let hidden = playthrough
        .iter()
        .find(|(id, ..)| *id == "notice_hidden_signal")
        .ok_or("hidden check is absent")?;
    if hidden.4.is_some() {
        return Err("hidden check unexpectedly produced a public projection".into());
    }
    let visible_public = visible
        .4
        .as_ref()
        .ok_or("visible check lacks a public projection")?;
    assert_public_boundary(visible_public, &public_state)?;

    let (public_module, public_pack) = public_domain_artifacts(&manifest, &preview, &public_state)?;
    let catalog = DomainCatalog::from_artifacts([public_module.clone()], [public_pack.clone()])?;
    let story = compile_with_modules(
        STORY_SOURCE,
        &CompileOptions {
            source_name: Some(
                "examples/tabletop-adapters/freehack/runtime/tavi-quill.weave".to_owned(),
            ),
        },
        &catalog,
    )?
    .story;

    let source_lock = source_lock();
    let mut artifacts = base_artifacts(
        &fixture,
        &schemas,
        &manifest,
        &selection,
        &creation_request,
        &preview,
        &projection,
        &initial_state,
        &mid_section_state,
        &final_state,
        &public_state,
        &probability_request,
        &probability_preview,
        &visible.1,
        &visible.3,
        visible_public,
        &hidden.3,
        &public_module,
        &public_pack,
        &story,
        &source_lock,
    )?;
    for (id, request, _receipt, authority, public) in playthrough {
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-request.json")),
            request.to_json()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-request.ron")),
            request.to_ron()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.authority-receipt.json")),
            authority.to_json()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.authority-receipt.ron")),
            authority.to_ron()?,
        ));
        if let Some(public) = public {
            artifacts.push((
                fixture.join(format!("playthrough/{id}.public-receipt.json")),
                public.to_json()?,
            ));
            artifacts.push((
                fixture.join(format!("playthrough/{id}.public-receipt.ron")),
                public.to_ron()?,
            ));
        }
    }
    for (path, contents) in artifacts {
        emit(&path, &contents, check)?;
    }
    Ok(())
}

fn verify_optional_source_audit(
    root: &Path,
    manifest: &weave_tabletop::AdapterManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let audit = root.join("target/source-audit/freehack");
    let pdf = audit.join("freehack.pdf");
    let markdown = audit.join("src/freehack.md");
    let metadata = audit.join("src/freehack.yml");
    let roll_tool = audit.join("tools/roll.py");
    if pdf.is_file() && markdown.is_file() && metadata.is_file() && roll_tool.is_file() {
        verify_adapter_source_bundle(
            manifest,
            &fs::read(pdf)?,
            &BTreeMap::from([
                ("src/freehack.md".to_owned(), fs::read(markdown)?),
                ("src/freehack.yml".to_owned(), fs::read(metadata)?),
                ("tools/roll.py".to_owned(), fs::read(roll_tool)?),
            ]),
            LICENSE_BYTES,
        )?;
    }
    Ok(())
}

fn creation_request() -> FreehackCreationRequest {
    FreehackCreationRequest {
        creation_format_version: FREEHACK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.tavi_quill".to_owned(),
        name: "Tavi Quill".to_owned(),
        seed: 137,
        campaign: FreehackCampaignSchema {
            schema_version: FREEHACK_CAMPAIGN_SCHEMA_VERSION,
            id: "org.weave.freehack.gantry_fixture".to_owned(),
            title: "Gantry Fixture".to_owned(),
            archetype_procedure: FreehackArchetypeProcedure::RarityOffer,
            archetypes: vec![
                archetype(
                    "courier",
                    "Courier",
                    "Moves messages and small cargo between districts.",
                    FreehackRarity::Common,
                ),
                archetype(
                    "scribe",
                    "Scribe",
                    "Keeps exact public and private records.",
                    FreehackRarity::Common,
                ),
                archetype(
                    "diver",
                    "Diver",
                    "Works below the tide line.",
                    FreehackRarity::Uncommon,
                ),
                archetype(
                    "pilot",
                    "Pilot",
                    "Guides vessels through narrow channels.",
                    FreehackRarity::Uncommon,
                ),
                archetype(
                    "rigger",
                    "Rigger",
                    "Builds temporary lines and lifts.",
                    FreehackRarity::Uncommon,
                ),
                archetype(
                    "cartographer",
                    "Cartographer",
                    "Maintains charts of changing passages.",
                    FreehackRarity::Rare,
                ),
                archetype(
                    "signal_keeper",
                    "Signal Keeper",
                    "Coordinates long-range harbor signals.",
                    FreehackRarity::Rare,
                ),
            ],
            modifiers: vec![
                FreehackModifierSpec {
                    id: "focus".to_owned(),
                    label: "Focus".to_owned(),
                    description: "Sustained attention under pressure.".to_owned(),
                    minimum: -10,
                    maximum: 10,
                    generation: FreehackModifierGeneration::Authored,
                    player_visible: true,
                },
                FreehackModifierSpec {
                    id: "balance".to_owned(),
                    label: "Balance".to_owned(),
                    description: "Footing on moving or narrow surfaces.".to_owned(),
                    minimum: 1,
                    maximum: 10,
                    generation: FreehackModifierGeneration::RandomInclusive,
                    player_visible: true,
                },
                FreehackModifierSpec {
                    id: "clearance".to_owned(),
                    label: "Clearance".to_owned(),
                    description: "Campaign-specific access rating.".to_owned(),
                    minimum: 2,
                    maximum: 2,
                    generation: FreehackModifierGeneration::Fixed { value: 2 },
                    player_visible: false,
                },
            ],
            feature_count: 2,
            feature_options: vec![FreehackFeatureOption {
                id: "quiet_step".to_owned(),
                label: "Quiet Step".to_owned(),
                description: "Moves carefully across resonant decking.".to_owned(),
            }],
            allow_authored_features: true,
            inventory_budget: Some(10),
            inventory_options: vec![FreehackInventoryOption {
                id: "line_launcher".to_owned(),
                label: "Line Launcher".to_owned(),
                description: "Projects a light cord across a gap.".to_owned(),
                cost: Some(3),
            }],
            allow_authored_inventory: true,
            initial_tracks: vec![
                FreehackTrackTemplate {
                    id: "fatigue".to_owned(),
                    label: "Fatigue".to_owned(),
                    secret: false,
                    interval: "after strenuous movement".to_owned(),
                    target_count: 3,
                    advances_on: FreehackTrackOutcome::Failure,
                    consequence: "The character must pause and recover.".to_owned(),
                },
                FreehackTrackTemplate {
                    id: "hidden_watch".to_owned(),
                    label: "Hidden Watch".to_owned(),
                    secret: true,
                    interval: "after a loud disturbance".to_owned(),
                    target_count: 2,
                    advances_on: FreehackTrackOutcome::Success,
                    consequence: "An unseen observer changes position.".to_owned(),
                },
            ],
        },
        selected_archetype: Some("courier".to_owned()),
        authored_modifiers: BTreeMap::from([("focus".to_owned(), 4)]),
        features: vec![
            FreehackFeatureSelection::Catalog {
                id: "quiet_step".to_owned(),
            },
            FreehackFeatureSelection::Authored {
                id: "tide_memory".to_owned(),
                label: "Tide Memory".to_owned(),
                description: "Remembers recurring harbor currents.".to_owned(),
            },
        ],
        inventory: vec![
            FreehackInventorySelection::Catalog {
                id: "line_launcher".to_owned(),
                quantity: 2,
            },
            FreehackInventorySelection::Authored {
                id: "wax_tablet".to_owned(),
                label: "Wax Tablet".to_owned(),
                quantity: 2,
                cost_per_item: 1,
            },
        ],
        memories: vec![
            FreehackMemorySeed {
                id: "old_channel".to_owned(),
                topic: "the old freight channel".to_owned(),
                has_experience: true,
                rationale: "Established explicitly during character creation.".to_owned(),
                disclosure: FreehackDisclosure::Public,
            },
            FreehackMemorySeed {
                id: "sealed_route".to_owned(),
                topic: "a sealed maintenance route".to_owned(),
                has_experience: false,
                rationale: "Held for authority review until play establishes it.".to_owned(),
                disclosure: FreehackDisclosure::HostOnly,
            },
        ],
    }
}

fn archetype(
    id: &str,
    label: &str,
    description: &str,
    rarity: FreehackRarity,
) -> FreehackArchetypeOption {
    FreehackArchetypeOption {
        id: id.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        rarity,
    }
}

fn probability_request() -> FreehackProbabilityRequest {
    FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: vec![contribution(
            "focus",
            "Focus",
            4,
            FreehackContributionSource::Character,
        )],
        opposition: vec![contribution(
            "crosswind",
            "Crosswind",
            3,
            FreehackContributionSource::Situation,
        )],
    }
}

fn contribution(
    id: &str,
    label: &str,
    value: u32,
    source: FreehackContributionSource,
) -> FreehackContribution {
    FreehackContribution {
        id: id.to_owned(),
        label: label.to_owned(),
        value,
        source,
    }
}

fn value_contribution(id: &str, label: &str, value: u32, source: &str) -> DomainValue {
    object([
        ("id", DomainValue::String(id.to_owned())),
        ("label", DomainValue::String(label.to_owned())),
        ("source", DomainValue::Symbol(source.to_owned())),
        ("value", DomainValue::Number(f64::from(value))),
    ])
}

fn check_input(
    label: &str,
    opposition_id: &str,
    opposition_label: &str,
    disclosure: &str,
    reveal_opposition: bool,
    reveal_probability: bool,
) -> DomainValue {
    object([
        ("disclosure", DomainValue::Symbol(disclosure.to_owned())),
        ("label", DomainValue::String(label.to_owned())),
        (
            "opposition",
            DomainValue::List(vec![value_contribution(
                opposition_id,
                opposition_label,
                3,
                "situation",
            )]),
        ),
        ("reveal_opposition", DomainValue::Bool(reveal_opposition)),
        ("reveal_probability", DomainValue::Bool(reveal_probability)),
        ("reveal_support", DomainValue::Bool(disclosure == "public")),
        (
            "support",
            DomainValue::List(vec![value_contribution("focus", "Focus", 4, "character")]),
        ),
    ])
}

fn operation_inputs() -> Vec<(&'static str, &'static str, TabletopCapability, DomainValue)> {
    vec![
        (
            "preview_crossing_probability",
            "preview_probability",
            TabletopCapability::ChecksAndConflicts,
            check_input(
                "Estimate the gantry crossing",
                "crosswind",
                "Crosswind",
                "public",
                true,
                true,
            ),
        ),
        (
            "cross_wet_gantry",
            "resolve_check",
            TabletopCapability::ChecksAndConflicts,
            check_input(
                "Cross the wet gantry",
                "sealed_current",
                "Sealed Current",
                "public",
                false,
                false,
            ),
        ),
        (
            "notice_hidden_signal",
            "resolve_check",
            TabletopCapability::ChecksAndConflicts,
            check_input(
                "Notice a hidden signal",
                "secret_opposition",
                "Secret Opposition Marker",
                "host_only",
                false,
                false,
            ),
        ),
        (
            "advance_hidden_watch",
            "advance_track",
            TabletopCapability::ResourcesAndConditions,
            object([
                ("outcome", DomainValue::Symbol("success".to_owned())),
                ("track_id", DomainValue::String("hidden_watch".to_owned())),
            ]),
        ),
        (
            "recall_tide_gate",
            "recall_memory",
            TabletopCapability::ResourcesAndConditions,
            object([
                ("connected", DomainValue::Bool(true)),
                ("disclosure", DomainValue::Symbol("public".to_owned())),
                ("id", DomainValue::String("tide_gate".to_owned())),
                ("obscurity", DomainValue::Number(6.0)),
                (
                    "topic",
                    DomainValue::String("the public tide-gate schedule".to_owned()),
                ),
            ]),
        ),
        (
            "open_gantry_turn",
            "open_section",
            TabletopCapability::Scenes,
            object([
                ("abstraction", DomainValue::Symbol("objective".to_owned())),
                ("id", DomainValue::String("gantry_turn".to_owned())),
                ("label", DomainValue::String("Gantry Turn".to_owned())),
                ("mapping", DomainValue::Symbol("mapped".to_owned())),
                (
                    "participants",
                    DomainValue::List(vec![
                        DomainValue::String("ira".to_owned()),
                        DomainValue::String("moss".to_owned()),
                    ]),
                ),
                ("reveal_after_resolution", DomainValue::Bool(false)),
                ("timing", DomainValue::Symbol("timed".to_owned())),
                ("turn_ticks", DomainValue::Number(30.0)),
            ]),
        ),
        (
            "submit_ira",
            "submit_action",
            TabletopCapability::Scenes,
            object([
                (
                    "action",
                    DomainValue::String("Private action marker: secure the west cable.".to_owned()),
                ),
                (
                    "map_intent",
                    DomainValue::String("Move to west anchor.".to_owned()),
                ),
                ("participant_id", DomainValue::String("ira".to_owned())),
                ("section_id", DomainValue::String("gantry_turn".to_owned())),
                ("submission_id", DomainValue::String("ira_first".to_owned())),
            ]),
        ),
        (
            "cancel_ira",
            "cancel_submission",
            TabletopCapability::Scenes,
            object([
                ("participant_id", DomainValue::String("ira".to_owned())),
                ("section_id", DomainValue::String("gantry_turn".to_owned())),
            ]),
        ),
        (
            "resubmit_ira",
            "submit_action",
            TabletopCapability::Scenes,
            object([
                (
                    "action",
                    DomainValue::String("Brace the east cable instead.".to_owned()),
                ),
                (
                    "map_intent",
                    DomainValue::String("Move to east anchor.".to_owned()),
                ),
                ("participant_id", DomainValue::String("ira".to_owned())),
                ("section_id", DomainValue::String("gantry_turn".to_owned())),
                (
                    "submission_id",
                    DomainValue::String("ira_second".to_owned()),
                ),
            ]),
        ),
        (
            "submit_moss",
            "submit_action",
            TabletopCapability::Scenes,
            object([
                (
                    "action",
                    DomainValue::String("Lower a counterweight.".to_owned()),
                ),
                (
                    "map_intent",
                    DomainValue::String("Remain by the central winch.".to_owned()),
                ),
                ("participant_id", DomainValue::String("moss".to_owned())),
                ("section_id", DomainValue::String("gantry_turn".to_owned())),
                (
                    "submission_id",
                    DomainValue::String("moss_first".to_owned()),
                ),
            ]),
        ),
        (
            "resolve_gantry_turn",
            "resolve_section",
            TabletopCapability::Scenes,
            object([
                (
                    "outcomes",
                    DomainValue::List(vec![
                        object([
                            (
                                "submission_id",
                                DomainValue::String("moss_first".to_owned()),
                            ),
                            (
                                "summary",
                                DomainValue::String("The counterweight settles.".to_owned()),
                            ),
                        ]),
                        object([
                            (
                                "submission_id",
                                DomainValue::String("ira_second".to_owned()),
                            ),
                            (
                                "summary",
                                DomainValue::String("The east cable holds.".to_owned()),
                            ),
                        ]),
                    ]),
                ),
                (
                    "public_summary",
                    DomainValue::String("The gantry stabilizes.".to_owned()),
                ),
                ("section_id", DomainValue::String("gantry_turn".to_owned())),
            ]),
        ),
        (
            "create_crossing_track",
            "create_track",
            TabletopCapability::ResourcesAndConditions,
            object([
                ("advances_on", DomainValue::Symbol("success".to_owned())),
                (
                    "consequence",
                    DomainValue::String("The crossing route becomes dependable.".to_owned()),
                ),
                ("id", DomainValue::String("crossing_ready".to_owned())),
                (
                    "interval",
                    DomainValue::String("after a stable crossing".to_owned()),
                ),
                ("label", DomainValue::String("Crossing Ready".to_owned())),
                ("secret", DomainValue::Bool(false)),
                ("target_count", DomainValue::Number(1.0)),
            ]),
        ),
        (
            "advance_crossing_track",
            "advance_track",
            TabletopCapability::ResourcesAndConditions,
            object([
                ("outcome", DomainValue::Symbol("success".to_owned())),
                ("track_id", DomainValue::String("crossing_ready".to_owned())),
            ]),
        ),
        (
            "open_route_turn",
            "open_section",
            TabletopCapability::Scenes,
            object([
                ("abstraction", DomainValue::Symbol("strategic".to_owned())),
                ("id", DomainValue::String("route_turn".to_owned())),
                ("label", DomainValue::String("Route Turn".to_owned())),
                ("mapping", DomainValue::Symbol("unmapped".to_owned())),
                (
                    "participants",
                    DomainValue::List(vec![
                        DomainValue::String("ira".to_owned()),
                        DomainValue::String("moss".to_owned()),
                    ]),
                ),
                ("reveal_after_resolution", DomainValue::Bool(false)),
                ("timing", DomainValue::Symbol("timed".to_owned())),
                ("turn_ticks", DomainValue::Number(5.0)),
            ]),
        ),
        (
            "submit_ira_route",
            "submit_action",
            TabletopCapability::Scenes,
            object([
                (
                    "action",
                    DomainValue::String("Mark a fallback route.".to_owned()),
                ),
                (
                    "map_intent",
                    DomainValue::String("No map position is required.".to_owned()),
                ),
                ("participant_id", DomainValue::String("ira".to_owned())),
                ("section_id", DomainValue::String("route_turn".to_owned())),
                ("submission_id", DomainValue::String("ira_route".to_owned())),
            ]),
        ),
        (
            "timeout_route",
            "timeout_section",
            TabletopCapability::Scenes,
            object([
                ("elapsed_ticks", DomainValue::Number(5.0)),
                ("section_id", DomainValue::String("route_turn".to_owned())),
            ]),
        ),
    ]
}

fn resolution_request(
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    preview: &FreehackCreationPreview,
    state: &TabletopState,
    input: DomainValue,
) -> Result<ResolutionRequest, weave_tabletop::TabletopError> {
    Ok(ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: request_id.to_owned(),
        adapter: preview.adapter.clone(),
        operation: operation.to_owned(),
        capability,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(state)?,
        input,
    })
}

fn assert_public_boundary(
    receipt: &FreehackPublicReceipt,
    state: &FreehackPublicState,
) -> Result<(), Box<dyn std::error::Error>> {
    let public = format!("{}\n{}", receipt.to_json()?, state.to_json()?);
    let forbidden = [
        "Secret Opposition Marker",
        "secret_opposition",
        "Notice a hidden signal",
        "hidden_watch",
        "unseen observer",
        "sealed_route",
        "a sealed maintenance route",
        "Private action marker",
        "ira_first",
        "west anchor",
        "entropy",
        "request_sha256",
        "revision",
        "opposition_total",
        "draw_index",
        "signed_result",
        "_authority",
    ];
    if let Some(marker) = forbidden.iter().find(|marker| public.contains(**marker)) {
        return Err(
            format!("public Freehack artifact contains forbidden marker `{marker}`").into(),
        );
    }
    Ok(())
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn public_domain_artifacts(
    manifest: &weave_tabletop::AdapterManifest,
    preview: &FreehackCreationPreview,
    public_state: &FreehackPublicState,
) -> Result<(ModuleManifest, DomainPack), Box<dyn std::error::Error>> {
    if preview.adapter != resolved_adapter(manifest)? || public_state.adapter != preview.adapter {
        return Err("public module inputs do not share the reviewed adapter coordinate".into());
    }

    let public_modifiers = preview
        .campaign
        .modifiers
        .iter()
        .filter(|modifier| modifier.player_visible)
        .map(|modifier| {
            let value = preview
                .modifiers
                .get(&modifier.id)
                .copied()
                .ok_or("player-visible modifier is absent from the creation preview")?;
            Ok((
                modifier.id.clone(),
                required(
                    integer(f64::from(modifier.minimum), f64::from(modifier.maximum)),
                    "Player-visible campaign modifier.",
                ),
                (modifier.id.clone(), DomainValue::Number(f64::from(value))),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let modifier_type = TypeExpression::Object {
        fields: public_modifiers
            .iter()
            .map(|(id, field, _)| (id.clone(), field.clone()))
            .collect(),
    };
    let modifier_value = DomainValue::Object(
        public_modifiers
            .into_iter()
            .map(|(_, _, value)| value)
            .collect(),
    );

    let adapter_type = object_type([
        (
            "content_sha256",
            required(string(64, 64), "Exact reviewed adapter manifest hash."),
        ),
        (
            "id",
            required(string(1, 255), "Stable reviewed adapter identity."),
        ),
        (
            "schema_version",
            required(integer(1.0, f64::from(u32::MAX)), "State schema version."),
        ),
        (
            "version",
            required(string(1, 64), "Exact adapter semantic version."),
        ),
    ]);
    let capability_type = object_type([
        (
            "checks_and_conflicts",
            required(TypeExpression::Bool, "Public check presentation support."),
        ),
        (
            "resources_and_conditions",
            required(TypeExpression::Bool, "Public track presentation support."),
        ),
        (
            "scenes",
            required(TypeExpression::Bool, "Public section presentation support."),
        ),
    ]);
    let character_type = object_type([
        (
            "archetype_id",
            required(string(0, 128), "Selected setting-authored archetype."),
        ),
        (
            "feature_ids",
            required(
                list(string(1, 128), 0, 256),
                "Public selected feature identities.",
            ),
        ),
        (
            "inventory_ids",
            required(
                list(string(1, 128), 0, 256),
                "Public selected inventory identities.",
            ),
        ),
        (
            "modifiers",
            required(modifier_type, "Only player-visible campaign modifiers."),
        ),
        ("name", required(string(1, 160), "Fictional display name.")),
    ]);
    let snapshot_type = object_type([
        (
            "gantry_status",
            required(
                TypeExpression::Symbol {
                    values: vec![
                        "open".to_owned(),
                        "resolved".to_owned(),
                        "timed_out".to_owned(),
                    ],
                },
                "Public gantry section lifecycle.",
            ),
        ),
        (
            "memory_count",
            required(integer(0.0, 256.0), "Number of public memories."),
        ),
        (
            "section_count",
            required(integer(0.0, 256.0), "Number of public sections."),
        ),
        (
            "track_count",
            required(integer(0.0, 256.0), "Number of public tracks."),
        ),
    ]);
    let track_type = object_type([
        (
            "advances_on",
            required(
                TypeExpression::Symbol {
                    values: vec!["failure".to_owned(), "success".to_owned()],
                },
                "Outcome that advances this public track.",
            ),
        ),
        (
            "completed",
            required(TypeExpression::Bool, "Public completion state."),
        ),
        (
            "consequence",
            required(string(1, 512), "Public completion consequence."),
        ),
        (
            "id",
            required(string(1, 128), "Stable public track identity."),
        ),
        (
            "interval",
            required(string(1, 240), "Public interval or event trigger."),
        ),
        ("label", required(string(1, 160), "Public track label.")),
        (
            "progress",
            required(integer(0.0, 1_000_000.0), "Current public progress."),
        ),
        (
            "target_count",
            required(integer(1.0, 1_000_000.0), "Public completion target."),
        ),
    ]);

    let module = ModuleManifest {
        contract_version: DOMAIN_CONTRACT_VERSION,
        pack_format_version: DOMAIN_PACK_FORMAT_VERSION,
        id: "org.weave.tabletop.freehack.public".to_owned(),
        version: "1.0.0".to_owned(),
        namespace: "freehack".to_owned(),
        title: "Freehack public projection".to_owned(),
        summary: "Leak-free, immutable values for player-facing Freehack story consumers."
            .to_owned(),
        authors: vec![ModuleAuthor {
            name: "Weave contributors".to_owned(),
            url: Some("https://github.com/chrisgliddon/weave".to_owned()),
        }],
        license: "MIT".to_owned(),
        license_url: "https://github.com/chrisgliddon/weave/blob/dev/LICENSE".to_owned(),
        weave_version: manifest.weave_version.clone(),
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
        types: BTreeMap::new(),
        exports: BTreeMap::from([
            (
                "adapter".to_owned(),
                export(adapter_type, "Exact safe adapter coordinate."),
            ),
            (
                "capabilities".to_owned(),
                export(capability_type, "Publicly usable adapter capabilities."),
            ),
            (
                "character".to_owned(),
                export(character_type, "Player-visible character projection."),
            ),
            (
                "snapshot".to_owned(),
                export(snapshot_type, "Player-visible final-state summary."),
            ),
            (
                "tracks".to_owned(),
                export(
                    TypeExpression::Map {
                        values: Box::new(track_type),
                        min_entries: 0,
                        max_entries: 256,
                    },
                    "All and only public generic tracks, addressable by stable id.",
                ),
            ),
        ]),
        authoring: ModuleAuthoring {
            entity_collections: Vec::new(),
            read_only_paths: ["adapter", "capabilities", "character", "snapshot", "tracks"]
                .into_iter()
                .map(|name| ReadOnlyPathDeclaration {
                    path: vec![name.to_owned()],
                    reason: "Generated only by the Freehack public projection boundary.".to_owned(),
                })
                .collect(),
        },
        provenance: public_provenance(["manifest"]),
    };

    if !public_state
        .tracks
        .iter()
        .any(|track| track.id == "fatigue")
    {
        return Err("public fatigue track is absent".into());
    }
    let gantry = public_state
        .sections
        .iter()
        .find(|section| section.id == "gantry_turn")
        .ok_or("public gantry section is absent")?;
    let pack = DomainPack {
        pack_format_version: DOMAIN_PACK_FORMAT_VERSION,
        id: "tavi_quill".to_owned(),
        version: "1.0.0".to_owned(),
        title: "Tavi Quill Freehack public projection".to_owned(),
        module: ModuleRequirement {
            id: module.id.clone(),
            version: format!("={}", module.version),
        },
        dependencies: Vec::new(),
        values: BTreeMap::from([
            ("adapter".to_owned(), coordinate_value(&preview.adapter)),
            (
                "capabilities".to_owned(),
                object([
                    ("checks_and_conflicts", DomainValue::Bool(true)),
                    ("resources_and_conditions", DomainValue::Bool(true)),
                    ("scenes", DomainValue::Bool(true)),
                ]),
            ),
            (
                "character".to_owned(),
                object([
                    (
                        "archetype_id",
                        DomainValue::String(preview.selected_archetype.clone().unwrap_or_default()),
                    ),
                    ("feature_ids", string_list_value(&preview.feature_ids)),
                    ("inventory_ids", string_list_value(&preview.inventory_ids)),
                    ("modifiers", modifier_value),
                    ("name", DomainValue::String(creation_name(preview)?)),
                ]),
            ),
            (
                "snapshot".to_owned(),
                object([
                    (
                        "gantry_status",
                        DomainValue::Symbol(section_status(gantry.status).to_owned()),
                    ),
                    (
                        "memory_count",
                        DomainValue::Number(public_state.memories.len() as f64),
                    ),
                    (
                        "section_count",
                        DomainValue::Number(public_state.sections.len() as f64),
                    ),
                    (
                        "track_count",
                        DomainValue::Number(public_state.tracks.len() as f64),
                    ),
                ]),
            ),
            (
                "tracks".to_owned(),
                DomainValue::Object(
                    public_state
                        .tracks
                        .iter()
                        .map(|track| {
                            (
                                track.id.clone(),
                                object([
                                    (
                                        "advances_on",
                                        DomainValue::Symbol(
                                            track_outcome(track.advances_on).to_owned(),
                                        ),
                                    ),
                                    ("completed", DomainValue::Bool(track.completed)),
                                    (
                                        "consequence",
                                        DomainValue::String(track.consequence.clone()),
                                    ),
                                    ("id", DomainValue::String(track.id.clone())),
                                    ("interval", DomainValue::String(track.interval.clone())),
                                    ("label", DomainValue::String(track.label.clone())),
                                    ("progress", DomainValue::Number(f64::from(track.progress))),
                                    (
                                        "target_count",
                                        DomainValue::Number(f64::from(track.target_count)),
                                    ),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ),
        ]),
        provenance: public_provenance([
            "values.adapter",
            "values.capabilities",
            "values.character",
            "values.snapshot",
            "values.tracks",
        ]),
    };

    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    validate_manifest(&module, &current)?;
    validate_pack(&pack, &module, &current)?;
    Ok((module, pack))
}

fn creation_name(preview: &FreehackCreationPreview) -> Result<String, Box<dyn std::error::Error>> {
    let DomainValue::Object(definition) = &preview.definition.definition else {
        return Err("Freehack character definition is not an object".into());
    };
    match definition.get("name") {
        Some(DomainValue::String(name)) => Ok(name.clone()),
        _ => Err("Freehack character definition lacks a name".into()),
    }
}

const fn section_status(status: FreehackSectionStatus) -> &'static str {
    match status {
        FreehackSectionStatus::Open => "open",
        FreehackSectionStatus::Resolved => "resolved",
        FreehackSectionStatus::TimedOut => "timed_out",
    }
}

const fn track_outcome(outcome: FreehackTrackOutcome) -> &'static str {
    match outcome {
        FreehackTrackOutcome::Failure => "failure",
        FreehackTrackOutcome::Success => "success",
    }
}

fn coordinate_value(adapter: &ResolvedAdapter) -> DomainValue {
    object([
        (
            "content_sha256",
            DomainValue::String(adapter.content_sha256.clone()),
        ),
        ("id", DomainValue::String(adapter.id.clone())),
        (
            "schema_version",
            DomainValue::Number(f64::from(adapter.schema_version)),
        ),
        ("version", DomainValue::String(adapter.version.clone())),
    ])
}

fn string_list_value(values: &[String]) -> DomainValue {
    DomainValue::List(
        values
            .iter()
            .map(|value| DomainValue::String(value.clone()))
            .collect(),
    )
}

fn export(value_type: TypeExpression, description: &str) -> ExportDeclaration {
    ExportDeclaration {
        value_type,
        required: true,
        source: ExportSource::Pack,
        description: description.to_owned(),
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

fn required(value_type: TypeExpression, description: &str) -> FieldDeclaration {
    FieldDeclaration {
        value_type,
        required: true,
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

const fn string(min_length: usize, max_length: usize) -> TypeExpression {
    TypeExpression::String {
        min_length,
        max_length,
    }
}

fn list(items: TypeExpression, min_items: usize, max_items: usize) -> TypeExpression {
    TypeExpression::List {
        items: Box::new(items),
        min_items,
        max_items,
    }
}

fn public_provenance<const N: usize>(claims: [&str; N]) -> Provenance {
    Provenance {
        sources: vec![
            ProvenanceSource {
                id: "freehack_source".to_owned(),
                kind: ProvenanceKind::PublicSource,
                url: format!(
                    "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/src/freehack.md"
                ),
                revision: FREEHACK_SOURCE_REVISION.to_owned(),
                sha256: Some(FREEHACK_MARKDOWN_SHA256.to_owned()),
                license: "CC0-1.0".to_owned(),
                license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                    .to_owned(),
                attribution: "Freehack 2.1 by Amini Allight.".to_owned(),
                modified: true,
            },
            ProvenanceSource {
                id: "weave_fixture".to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "freehack-public-fixture-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/dev/LICENSE".to_owned(),
                attribution: "Original Freehack fixture scenario and projection authored by Weave contributors."
                    .to_owned(),
                modified: false,
            },
        ],
        transformations: vec![ProvenanceTransformation {
            id: "public_projection".to_owned(),
            inputs: vec!["freehack_source".to_owned(), "weave_fixture".to_owned()],
            description: "Projects only explicitly disclosed adapter, character, track, section, and memory facts into an immutable story module; no private resolution data is carried."
                .to_owned(),
        }],
        claims: claims
            .into_iter()
            .map(|claim| (claim.to_owned(), vec!["public_projection".to_owned()]))
            .collect(),
    }
}

fn source_lock() -> serde_json::Value {
    serde_json::json!({
        "format_version": 1,
        "official_release_page": FREEHACK_RELEASE_URL,
        "official_source_repository": FREEHACK_SOURCE_URL,
        "release": "2.1",
        "release_date": "2026-07-13",
        "revision": FREEHACK_SOURCE_REVISION,
        "retrieved_on": "2026-08-25",
        "artifacts": [
            {
                "exact_artifact": "freehack.pdf",
                "media_type": "application/pdf",
                "redistributed": false,
                "sha256": FREEHACK_PDF_SHA256,
                "source_url": FREEHACK_RELEASE_URL
            },
            {
                "exact_artifact": "src/freehack.md",
                "media_type": "text/markdown",
                "redistributed": false,
                "revision": FREEHACK_SOURCE_REVISION,
                "sha256": FREEHACK_MARKDOWN_SHA256,
                "source_url": format!(
                    "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/src/freehack.md"
                )
            },
            {
                "exact_artifact": "src/freehack.yml",
                "media_type": "application/yaml",
                "redistributed": false,
                "revision": FREEHACK_SOURCE_REVISION,
                "sha256": FREEHACK_METADATA_SHA256,
                "source_url": format!(
                    "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/src/freehack.yml"
                )
            },
            {
                "exact_artifact": "tools/roll.py",
                "media_type": "text/x-python",
                "redistributed": false,
                "revision": FREEHACK_SOURCE_REVISION,
                "sha256": FREEHACK_ROLL_TOOL_SHA256,
                "source_url": format!(
                    "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/tools/roll.py"
                )
            },
            {
                "exact_artifact": "LICENSE-CC0-1.0.txt",
                "media_type": "text/plain",
                "redistributed": true,
                "sha256": FREEHACK_CC0_1_0_LEGAL_CODE_SHA256,
                "source_url": "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
            }
        ],
        "review_boundary": {
            "included": [
                "campaign-configured character fields, archetype procedures, features, inventory, and memories",
                "exact support/opposition probability, signed result, and magnitude behavior",
                "generic tracks and simultaneous sections",
                "official executable roll reference cross-check"
            ],
            "excluded": [
                "artwork",
                "branding",
                "community supplements",
                "layout",
                "logos",
                "trade dress",
                "unverified assets"
            ]
        },
        "implementation": {
            "copy_policy": "independently authored, setting-neutral Rust behavior",
            "public_projection": "authority inputs, entropy traces, hidden opposition, secret tracks, private memories, and pending submissions are excluded"
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn base_artifacts(
    fixture: &Path,
    schemas: &Path,
    manifest: &weave_tabletop::AdapterManifest,
    selection: &AdapterSelection,
    creation_request: &FreehackCreationRequest,
    preview: &FreehackCreationPreview,
    projection: &TabletopCharacterProjection,
    initial_state: &TabletopState,
    mid_section_state: &TabletopState,
    final_state: &TabletopState,
    public_state: &FreehackPublicState,
    probability_request: &FreehackProbabilityRequest,
    probability_preview: &weave_tabletop::FreehackProbabilityPreview,
    visible_request: &ResolutionRequest,
    visible_authority: &FreehackAuthorityReceipt,
    visible_public: &FreehackPublicReceipt,
    hidden_authority: &FreehackAuthorityReceipt,
    public_module: &ModuleManifest,
    public_pack: &DomainPack,
    story: &weave_core::ir::StoryIr,
    source_lock: &serde_json::Value,
) -> Result<Vec<(PathBuf, String)>, Box<dyn std::error::Error>> {
    Ok(vec![
        (
            fixture.join("freehack.tabletop-adapter.json"),
            manifest.to_json()?,
        ),
        (
            fixture.join("freehack.tabletop-adapter.ron"),
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
            fixture.join("creation.tabletop-creation.json"),
            creation_request.to_json()?,
        ),
        (
            fixture.join("creation.tabletop-creation.ron"),
            creation_request.to_ron()?,
        ),
        (
            fixture.join("preview.tabletop-creation-preview.json"),
            preview.to_json()?,
        ),
        (
            fixture.join("preview.tabletop-creation-preview.ron"),
            preview.to_ron()?,
        ),
        (
            fixture.join("character.tabletop-projection.json"),
            projection.to_json()?,
        ),
        (
            fixture.join("character.tabletop-projection.ron"),
            projection.to_ron()?,
        ),
        (
            fixture.join("authority-state.tabletop-state.json"),
            initial_state.to_json()?,
        ),
        (
            fixture.join("authority-state.tabletop-state.ron"),
            initial_state.to_ron()?,
        ),
        (
            fixture.join("mid-section-authority-state.tabletop-state.json"),
            mid_section_state.to_json()?,
        ),
        (
            fixture.join("mid-section-authority-state.tabletop-state.ron"),
            mid_section_state.to_ron()?,
        ),
        (
            fixture.join("final-authority-state.tabletop-state.json"),
            final_state.to_json()?,
        ),
        (
            fixture.join("final-authority-state.tabletop-state.ron"),
            final_state.to_ron()?,
        ),
        (
            fixture.join("public-state.freehack-public-state.json"),
            public_state.to_json()?,
        ),
        (
            fixture.join("public-state.freehack-public-state.ron"),
            public_state.to_ron()?,
        ),
        (
            fixture.join("probability.freehack-probability.json"),
            probability_request.to_json()?,
        ),
        (
            fixture.join("probability.freehack-probability.ron"),
            probability_request.to_ron()?,
        ),
        (
            fixture.join("probability-preview.freehack-probability-preview.json"),
            probability_preview.to_json()?,
        ),
        (
            fixture.join("probability-preview.freehack-probability-preview.ron"),
            probability_preview.to_ron()?,
        ),
        (
            fixture.join("request.tabletop-request.json"),
            visible_request.to_json()?,
        ),
        (
            fixture.join("request.tabletop-request.ron"),
            visible_request.to_ron()?,
        ),
        (
            fixture.join("authority-receipt.freehack-authority-receipt.json"),
            visible_authority.to_json()?,
        ),
        (
            fixture.join("authority-receipt.freehack-authority-receipt.ron"),
            visible_authority.to_ron()?,
        ),
        (
            fixture.join("public-receipt.freehack-public-receipt.json"),
            visible_public.to_json()?,
        ),
        (
            fixture.join("public-receipt.freehack-public-receipt.ron"),
            visible_public.to_ron()?,
        ),
        (
            fixture.join("hidden-authority-receipt.freehack-authority-receipt.json"),
            hidden_authority.to_json()?,
        ),
        (
            fixture.join("hidden-authority-receipt.freehack-authority-receipt.ron"),
            hidden_authority.to_ron()?,
        ),
        (
            fixture.join("runtime/module.weave-module.json"),
            public_module.to_json()?,
        ),
        (
            fixture.join("runtime/module.weave-module.ron"),
            public_module.to_ron()?,
        ),
        (
            fixture.join("runtime/tavi_quill.weave-domain.json"),
            public_pack.to_json()?,
        ),
        (
            fixture.join("runtime/tavi_quill.weave-domain.ron"),
            public_pack.to_ron()?,
        ),
        (
            fixture.join("runtime/tavi-quill.weave"),
            STORY_SOURCE.to_owned(),
        ),
        (
            fixture.join("runtime/tavi-quill.story.json"),
            to_json(story)?,
        ),
        (fixture.join("runtime/tavi-quill.story.ron"), to_ron(story)?),
        (
            fixture.join("SOURCE.lock.json"),
            weave_domain::to_pretty_json(source_lock)?,
        ),
        (
            fixture.join("LICENSE-CC0-1.0.txt"),
            String::from_utf8(LICENSE_BYTES.to_vec())?,
        ),
        (
            schemas.join("weave-tabletop-freehack-creation-request-v1.schema.json"),
            freehack_creation_request_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-creation-preview-v1.schema.json"),
            freehack_creation_preview_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-probability-request-v1.schema.json"),
            freehack_probability_request_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-probability-preview-v1.schema.json"),
            freehack_probability_preview_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-public-state-v1.schema.json"),
            freehack_public_state_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-public-receipt-v1.schema.json"),
            freehack_public_receipt_schema()?,
        ),
        (
            schemas.join("weave-tabletop-freehack-authority-receipt-v1.schema.json"),
            freehack_authority_receipt_schema()?,
        ),
    ])
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
