use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{DomainCatalog, DomainValue};
use weave_tabletop::{
    ADAPTER_SELECTION_FORMAT_VERSION, AdapterSelection, CC0_1_0_LEGAL_CODE_SHA256,
    CHARACTER_PROJECTION_FORMAT_VERSION, PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
    PLUG_AND_PLAY_RULES_SHA256, PLUG_AND_PLAY_SHEET_SHA256, PlugAndPlayAttributes,
    PlugAndPlayCreationRequest, PlugAndPlayMentalAttribute, PlugAndPlayModifier,
    PlugAndPlayPhysicalAttribute, PlugAndPlayResolver, PlugAndPlayRollAssignment,
    PlugAndPlayStatSource, RESOLVER_FORMAT_VERSION, ResolutionRequest, ResolverRegistry,
    TabletopCapability, TabletopCharacterProjection, TabletopState, canonical_fingerprint,
    create_plug_and_play_character, plug_and_play_creation_preview_schema,
    plug_and_play_creation_request_schema, plug_and_play_manifest, project_receipt_for_audience,
    sha256_bytes, tabletop_domain_manifest, tabletop_domain_pack, validate_adapter_manifest,
    validate_adapter_selection, validate_character_projection,
    validate_plug_and_play_creation_preview, validate_resolution_receipt, validate_tabletop_state,
    verify_adapter_source_bundle,
};

const LICENSE_BYTES: &[u8] =
    include_bytes!("../../../examples/tabletop-adapters/plug-and-play/LICENSE-CC0-1.0.txt");

const STORY_SOURCE: &str = r#"module rules {
    id: "org.weave.tabletop.plug_and_play"
    version: "=1.0.0"
    pack: "ember_vale@=1.0.0"
}

VAR adapter_id = rules.adapter.id
VAR adapter_version = rules.adapter.version
VAR adapter_sha256 = rules.adapter.content_sha256
VAR checks_supported = rules.capabilities.checks_and_conflicts
VAR encounters_supported = rules.capabilities.encounters
VAR character_name = rules.definition.name
VAR agility = rules.definition.effective_attributes.agility
VAR brains = rules.definition.effective_attributes.brains
VAR brawn = rules.definition.effective_attributes.brawn
VAR wits = rules.definition.effective_attributes.wits
VAR fortune = rules.state.fortune_remaining
VAR survivability = rules.state.survivability_current
VAR wounded = rules.state.wounded

=== start ===
{checks_supported && encounters_supported:
    {character_name} enters play with Agility {agility}, Brains {brains}, Brawn {brawn}, Wits {wits}, Fortune {fortune}, and Survivability {survivability}.
    The active rules are {adapter_id}@{adapter_version} ({adapter_sha256}); wounded is {wounded}.
- else:
    This branch is unreachable because the selected adapter declares checks and encounters.
}
-> END
"#;

fn main() {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Err(error) = generate(&root, check) {
        eprintln!("Plug-And-Play fixture: {error}");
        std::process::exit(1);
    }
}

fn generate(root: &Path, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = root.join("examples/tabletop-adapters/plug-and-play");
    let schemas = root.join("schemas");
    let manifest = plug_and_play_manifest();
    validate_adapter_manifest(&manifest)?;
    if sha256_bytes(LICENSE_BYTES) != CC0_1_0_LEGAL_CODE_SHA256 {
        return Err("retained CC0 legal code hash changed".into());
    }
    let audit_root = root.join("target/source-audit/plug-and-play");
    let audited_rules = audit_root.join("rules.pdf");
    let audited_sheet = audit_root.join("character-sheet.pdf");
    if audited_rules.is_file() && audited_sheet.is_file() {
        verify_adapter_source_bundle(
            &manifest,
            &fs::read(audited_rules)?,
            &BTreeMap::from([(
                "Plug-And-Play Character Sheet.pdf".to_owned(),
                fs::read(audited_sheet)?,
            )]),
            LICENSE_BYTES,
        )?;
    }

    let creation_request = creation_request();
    let preview = create_plug_and_play_character(&creation_request)?;
    validate_plug_and_play_creation_preview(&preview)?;
    let selection = AdapterSelection {
        selection_format_version: ADAPTER_SELECTION_FORMAT_VERSION,
        installed: vec![preview.adapter.clone()],
        primary: vec![preview.adapter.clone()],
    };
    validate_adapter_selection(&selection, std::slice::from_ref(&manifest))?;
    let projection = TabletopCharacterProjection {
        projection_format_version: CHARACTER_PROJECTION_FORMAT_VERSION,
        character_id: creation_request.character_id.clone(),
        canonical_profile_sha256: sha256_bytes(b"Ember Vale original canonical profile v1"),
        canonical_character_write_back: false,
        active: Some(preview.definition.clone()),
        inactive: BTreeMap::new(),
        suggestions: Vec::new(),
    };
    validate_character_projection(&projection, std::slice::from_ref(&manifest))?;
    let initial_state = preview.initial_state.clone();
    validate_tabletop_state(&initial_state, &manifest)?;

    let mut registry = ResolverRegistry::new();
    registry.register(&manifest, PlugAndPlayResolver)?;
    let mut state = initial_state.clone();
    let mut playthrough = Vec::new();
    for (id, operation, capability, input) in operation_inputs() {
        let request = resolution_request(id, operation, capability, &preview, &state, input)?;
        let receipt = registry.execute(&manifest, &request, &state)?;
        registry.replay(&manifest, &request, &state, &receipt)?;
        validate_resolution_receipt(&receipt, &manifest)?;
        state = receipt.after_state.clone();
        playthrough.push((id, request, receipt));
    }
    let final_state = state;
    let (first_id, first_request, first_receipt) = playthrough
        .first()
        .ok_or("playthrough must contain a resolver transition")?;
    if *first_id != "cross_rooftop" {
        return Err("unexpected canonical playthrough order".into());
    }
    let runtime_receipt = project_receipt_for_audience(
        first_receipt,
        &manifest,
        weave_tabletop::HostAudience::Runtime,
    )?;

    let domain_manifest = tabletop_domain_manifest(&manifest)?;
    let domain_pack = tabletop_domain_pack(
        &manifest,
        &projection,
        &final_state,
        "ember_vale",
        "1.0.0",
        "Ember Vale Plug-And-Play Projection",
    )?;
    let catalog = DomainCatalog::from_artifacts([domain_manifest.clone()], [domain_pack.clone()])?;
    let story = compile_with_modules(
        STORY_SOURCE,
        &CompileOptions {
            source_name: Some(
                "examples/tabletop-adapters/plug-and-play/runtime/ember-vale.weave".to_owned(),
            ),
        },
        &catalog,
    )?
    .story;

    let source_lock = serde_json::json!({
        "format_version": 1,
        "official_release_page": "https://distilledproductions.itch.io/plug-and-play",
        "retrieved_on": "2026-08-24",
        "artifacts": [
            {
                "exact_artifact": "Plug-And-Play Character Sheet.pdf",
                "media_type": "application/pdf",
                "redistributed": false,
                "sha256": PLUG_AND_PLAY_SHEET_SHA256,
                "upload_id": 12106934
            },
            {
                "exact_artifact": "Plug-And-Play Rules.pdf",
                "media_type": "application/pdf",
                "redistributed": false,
                "sha256": PLUG_AND_PLAY_RULES_SHA256,
                "upload_id": 12106935
            },
            {
                "exact_artifact": "LICENSE-CC0-1.0.txt",
                "media_type": "text/plain",
                "redistributed": true,
                "sha256": CC0_1_0_LEGAL_CODE_SHA256,
                "source_url": "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
            }
        ],
        "review_boundary": {
            "included": [
                "character field inventory",
                "creation and derived-value procedures",
                "checks, Fortune, combat, group action, initiative, and chase procedures"
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
        }
    });

    let mut artifacts = vec![
        (
            fixture.join("plug-and-play.tabletop-adapter.json"),
            manifest.to_json()?,
        ),
        (
            fixture.join("plug-and-play.tabletop-adapter.ron"),
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
            fixture.join("state.tabletop-state.json"),
            initial_state.to_json()?,
        ),
        (
            fixture.join("state.tabletop-state.ron"),
            initial_state.to_ron()?,
        ),
        (
            fixture.join("final-state.tabletop-state.json"),
            final_state.to_json()?,
        ),
        (
            fixture.join("final-state.tabletop-state.ron"),
            final_state.to_ron()?,
        ),
        (
            fixture.join("request.tabletop-request.json"),
            first_request.to_json()?,
        ),
        (
            fixture.join("request.tabletop-request.ron"),
            first_request.to_ron()?,
        ),
        (
            fixture.join("receipt.tabletop-receipt.json"),
            first_receipt.to_json()?,
        ),
        (
            fixture.join("receipt.tabletop-receipt.ron"),
            first_receipt.to_ron()?,
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
            fixture.join("runtime/ember_vale.weave-domain.json"),
            domain_pack.to_json()?,
        ),
        (
            fixture.join("runtime/ember_vale.weave-domain.ron"),
            domain_pack.to_ron()?,
        ),
        (
            fixture.join("runtime/ember-vale.weave"),
            STORY_SOURCE.to_owned(),
        ),
        (
            fixture.join("runtime/ember-vale.story.json"),
            to_json(&story)?,
        ),
        (
            fixture.join("runtime/ember-vale.story.ron"),
            to_ron(&story)?,
        ),
        (
            fixture.join("SOURCE.lock.json"),
            weave_domain::to_pretty_json(&source_lock)?,
        ),
        (
            schemas.join("weave-tabletop-plug-and-play-creation-request-v1.schema.json"),
            plug_and_play_creation_request_schema()?,
        ),
        (
            schemas.join("weave-tabletop-plug-and-play-creation-preview-v1.schema.json"),
            plug_and_play_creation_preview_schema()?,
        ),
    ];
    for (id, request, receipt) in playthrough {
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-request.json")),
            request.to_json()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-request.ron")),
            request.to_ron()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-receipt.json")),
            receipt.to_json()?,
        ));
        artifacts.push((
            fixture.join(format!("playthrough/{id}.tabletop-receipt.ron")),
            receipt.to_ron()?,
        ));
    }
    for (path, contents) in artifacts {
        emit(&path, &contents, check)?;
    }
    Ok(())
}

fn creation_request() -> PlugAndPlayCreationRequest {
    PlugAndPlayCreationRequest {
        creation_format_version: PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.ember_vale".to_owned(),
        name: "Ember Vale".to_owned(),
        age: 31,
        seed: 2_026_082_401,
        stat_source: PlugAndPlayStatSource::Rolled {
            selected_set: 1,
            assignment: PlugAndPlayRollAssignment {
                agility: 3,
                brains: 1,
                brawn: 0,
                wits: 2,
            },
        },
        survivability_body: PlugAndPlayPhysicalAttribute::Brawn,
        survivability_mind: PlugAndPlayMentalAttribute::Brains,
        modifiers: vec![
            PlugAndPlayModifier {
                id: "field_medic".to_owned(),
                label: "Field medic".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 0,
                    brains: 1,
                    brawn: 0,
                    wits: -1,
                },
                fortune: 0,
                explanation: "Practiced analysis is offset by slower improvisation.".to_owned(),
            },
            PlugAndPlayModifier {
                id: "rooftop_runner".to_owned(),
                label: "Rooftop runner".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 1,
                    brains: 0,
                    brawn: -1,
                    wits: 0,
                },
                fortune: 0,
                explanation: "Fast movement is offset by a lighter frame.".to_owned(),
            },
        ],
        inventory: vec![
            "field knife".to_owned(),
            "signal pistol".to_owned(),
            "weather cloak".to_owned(),
        ],
    }
}

fn operation_inputs() -> Vec<(&'static str, &'static str, TabletopCapability, DomainValue)> {
    vec![
        (
            "cross_rooftop",
            "check",
            TabletopCapability::ChecksAndConflicts,
            object([
                ("attribute", DomainValue::Symbol("agility".to_owned())),
                ("difficulty", DomainValue::Number(18.0)),
                ("modifier", DomainValue::Number(0.0)),
                ("roll_mode", DomainValue::Symbol("advantage".to_owned())),
                ("spend_fortune_on_failure", DomainValue::Bool(true)),
            ]),
        ),
        (
            "test_fortune",
            "fortune_test",
            TabletopCapability::ResourcesAndConditions,
            object([("roll_mode", DomainValue::Symbol("normal".to_owned()))]),
        ),
        (
            "fire_signal_pistol",
            "attack",
            TabletopCapability::ChecksAndConflicts,
            object([
                ("attribute", DomainValue::Symbol("agility".to_owned())),
                ("damage_die", DomainValue::Number(6.0)),
                ("modifier", DomainValue::Number(0.0)),
                ("roll_mode", DomainValue::Symbol("normal".to_owned())),
                ("target_survivability", DomainValue::Number(7.0)),
                ("weapon_id", DomainValue::String("signal pistol".to_owned())),
                ("weapon_kind", DomainValue::Symbol("ranged".to_owned())),
            ]),
        ),
        (
            "lift_gate_together",
            "group_check",
            TabletopCapability::ChecksAndConflicts,
            object([
                ("difficulty", DomainValue::Number(7.0)),
                (
                    "participants",
                    DomainValue::List(vec![
                        participant("ember", 4, 0, "normal"),
                        participant("rook", 3, 1, "normal"),
                        participant("tamsin", 5, -1, "advantage"),
                    ]),
                ),
            ]),
        ),
        (
            "draw_initiative",
            "initiative",
            TabletopCapability::Encounters,
            object([(
                "participants",
                DomainValue::List(
                    ["ember", "rook", "warden"]
                        .into_iter()
                        .map(|value| DomainValue::String(value.to_owned()))
                        .collect(),
                ),
            )]),
        ),
        (
            "canal_chase",
            "chase",
            TabletopCapability::Encounters,
            object([
                ("attribute", DomainValue::Symbol("agility".to_owned())),
                ("max_checks", DomainValue::Number(3.0)),
                ("opponent_modifier", DomainValue::Number(0.0)),
                ("opponent_rating", DomainValue::Number(4.0)),
                (
                    "opponent_roll_mode",
                    DomainValue::Symbol("normal".to_owned()),
                ),
                ("player_modifier", DomainValue::Number(0.0)),
                ("player_roll_mode", DomainValue::Symbol("normal".to_owned())),
            ]),
        ),
        (
            "take_fall_damage",
            "take_damage",
            TabletopCapability::ResourcesAndConditions,
            object([("amount", DomainValue::Number(99.0))]),
        ),
    ]
}

fn resolution_request(
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    preview: &weave_tabletop::PlugAndPlayCreationPreview,
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

fn participant(id: &str, rating: i32, modifier: i32, roll_mode: &str) -> DomainValue {
    object([
        ("id", DomainValue::String(id.to_owned())),
        ("modifier", DomainValue::Number(f64::from(modifier))),
        ("rating", DomainValue::Number(f64::from(rating))),
        ("roll_mode", DomainValue::Symbol(roll_mode.to_owned())),
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
