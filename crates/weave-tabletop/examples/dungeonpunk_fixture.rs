use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use weave_compiler::{CompileOptions, compile_with_modules, to_json, to_ron};
use weave_domain::{DomainCatalog, DomainValue};
use weave_tabletop::{
    ADAPTER_SELECTION_FORMAT_VERSION, AdapterSelection, CC0_1_0_LEGAL_CODE_SHA256,
    CHARACTER_PROJECTION_FORMAT_VERSION, DUNGEONPUNK_CREATION_FORMAT_VERSION,
    DUNGEONPUNK_PDF_SHA256, DUNGEONPUNK_RELEASE_URL, DUNGEONPUNK_SOURCE_REVISION,
    DUNGEONPUNK_SOURCE_URL, DUNGEONPUNK_TEXT_SHA256, DungeonpunkAttributes, DungeonpunkClock,
    DungeonpunkClockTrigger, DungeonpunkCreationPreview, DungeonpunkCreationRequest,
    DungeonpunkGear, DungeonpunkGearKind, DungeonpunkMoveChoice, DungeonpunkResolver,
    DungeonpunkThreat, DungeonpunkThreatKind, RESOLVER_FORMAT_VERSION, ResolutionRequest,
    ResolverRegistry, TabletopCapability, TabletopCharacterProjection, TabletopState,
    canonical_fingerprint, create_dungeonpunk_character, dungeonpunk_creation_preview_schema,
    dungeonpunk_creation_request_schema, dungeonpunk_manifest, project_receipt_for_audience,
    sha256_bytes, tabletop_domain_manifest, tabletop_domain_pack, validate_adapter_manifest,
    validate_adapter_selection, validate_character_projection,
    validate_dungeonpunk_creation_preview, validate_resolution_receipt, validate_tabletop_state,
    verify_adapter_source_bundle,
};

const LICENSE_BYTES: &[u8] =
    include_bytes!("../../../examples/tabletop-adapters/plug-and-play/LICENSE-CC0-1.0.txt");

const STORY_SOURCE: &str = r#"module rules {
    id: "org.weave.tabletop.dungeonpunk"
    version: "=1.0.0"
    pack: "vesper_ash@=1.0.0"
}

VAR adapter_id = rules.adapter.id
VAR adapter_version = rules.adapter.version
VAR adapter_sha256 = rules.adapter.content_sha256
VAR checks_supported = rules.capabilities.checks_and_conflicts
VAR clocks_supported = rules.capabilities.clocks
VAR character_name = rules.definition.name
VAR strength = rules.definition.attributes.strength
VAR fate = rules.definition.constants.fate
VAR hp = rules.state.hp_current
VAR stress = rules.state.stress
VAR xp = rules.state.xp
VAR encumbered = rules.state.encumbered

=== start ===
{checks_supported && clocks_supported:
    {character_name} enters play with Strength {strength}, FATE {fate}, HP {hp}, Stress {stress}, XP {xp}, and encumbered set to {encumbered}.
    The active rules are {adapter_id}@{adapter_version} ({adapter_sha256}).
- else:
    This branch is unreachable because the selected adapter declares checks and clocks.
}
-> END
"#;

fn main() {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Err(error) = generate(&root, check) {
        eprintln!("Dungeonpunk fixture: {error}");
        std::process::exit(1);
    }
}

fn generate(root: &Path, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = root.join("examples/tabletop-adapters/dungeonpunk");
    let schemas = root.join("schemas");
    let manifest = dungeonpunk_manifest();
    validate_adapter_manifest(&manifest)?;
    if sha256_bytes(LICENSE_BYTES) != CC0_1_0_LEGAL_CODE_SHA256 {
        return Err("retained CC0 legal code hash changed".into());
    }
    let audit_root = root.join("target/source-audit/dungeonpunk");
    let audited_pdf = audit_root.join("dungeonpunk-google-doc.pdf");
    let audited_text = audit_root.join("dungeonpunk-google-doc.txt");
    if audited_pdf.is_file() && audited_text.is_file() {
        verify_adapter_source_bundle(
            &manifest,
            &fs::read(audited_pdf)?,
            &BTreeMap::from([(
                "dungeonpunk-google-doc.txt".to_owned(),
                fs::read(audited_text)?,
            )]),
            LICENSE_BYTES,
        )?;
    }

    let creation_request = creation_request();
    let preview = create_dungeonpunk_character(&creation_request)?;
    validate_dungeonpunk_creation_preview(&preview)?;
    let selection = AdapterSelection {
        selection_format_version: ADAPTER_SELECTION_FORMAT_VERSION,
        installed: vec![preview.adapter.clone()],
        primary: vec![preview.adapter.clone()],
    };
    validate_adapter_selection(&selection, std::slice::from_ref(&manifest))?;
    let projection = TabletopCharacterProjection {
        projection_format_version: CHARACTER_PROJECTION_FORMAT_VERSION,
        character_id: creation_request.character_id.clone(),
        canonical_profile_sha256: sha256_bytes(b"Vesper Ash original canonical profile v1"),
        canonical_character_write_back: false,
        active: Some(preview.definition.clone()),
        inactive: BTreeMap::new(),
        suggestions: Vec::new(),
    };
    validate_character_projection(&projection, std::slice::from_ref(&manifest))?;
    let initial_state = preview.initial_state.clone();
    validate_tabletop_state(&initial_state, &manifest)?;

    let mut registry = ResolverRegistry::new();
    registry.register(&manifest, DungeonpunkResolver)?;
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
    if *first_id != "raise_gate_together" {
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
        "vesper_ash",
        "1.0.0",
        "Vesper Ash Dungeonpunk Projection",
    )?;
    let catalog = DomainCatalog::from_artifacts([domain_manifest.clone()], [domain_pack.clone()])?;
    let story = compile_with_modules(
        STORY_SOURCE,
        &CompileOptions {
            source_name: Some(
                "examples/tabletop-adapters/dungeonpunk/runtime/vesper-ash.weave".to_owned(),
            ),
        },
        &catalog,
    )?
    .story;

    let source_lock = serde_json::json!({
        "format_version": 1,
        "official_release_page": DUNGEONPUNK_RELEASE_URL,
        "official_source": DUNGEONPUNK_SOURCE_URL,
        "revision": DUNGEONPUNK_SOURCE_REVISION,
        "retrieved_on": "2026-08-25",
        "artifacts": [
            {
                "exact_artifact": "dungeonpunk-google-doc.pdf",
                "media_type": "application/pdf",
                "redistributed": false,
                "sha256": DUNGEONPUNK_PDF_SHA256
            },
            {
                "exact_artifact": "dungeonpunk-google-doc.txt",
                "media_type": "text/plain",
                "redistributed": false,
                "sha256": DUNGEONPUNK_TEXT_SHA256
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
                "character creation, stats, HP, Stress, XP, gear, load, and bonds",
                "Struggle pools and outcomes, harm, rest, Brace, death, and growth",
                "core move inventory, clocks, threats, armor, and structured GM moves"
            ],
            "excluded": [
                "artwork",
                "branding",
                "community supplements",
                "layout",
                "logos",
                "trade dress",
                "unverified assets",
                "wiki content"
            ]
        }
    });

    let mut artifacts = vec![
        (
            fixture.join("dungeonpunk.tabletop-adapter.json"),
            manifest.to_json()?,
        ),
        (
            fixture.join("dungeonpunk.tabletop-adapter.ron"),
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
            fixture.join("runtime/vesper_ash.weave-domain.json"),
            domain_pack.to_json()?,
        ),
        (
            fixture.join("runtime/vesper_ash.weave-domain.ron"),
            domain_pack.to_ron()?,
        ),
        (
            fixture.join("runtime/vesper-ash.weave"),
            STORY_SOURCE.to_owned(),
        ),
        (
            fixture.join("runtime/vesper-ash.story.json"),
            to_json(&story)?,
        ),
        (
            fixture.join("runtime/vesper-ash.story.ron"),
            to_ron(&story)?,
        ),
        (
            fixture.join("SOURCE.lock.json"),
            weave_domain::to_pretty_json(&source_lock)?,
        ),
        (
            fixture.join("LICENSE-CC0-1.0.txt"),
            String::from_utf8(LICENSE_BYTES.to_vec())?,
        ),
        (
            schemas.join("weave-tabletop-dungeonpunk-creation-request-v1.schema.json"),
            dungeonpunk_creation_request_schema()?,
        ),
        (
            schemas.join("weave-tabletop-dungeonpunk-creation-preview-v1.schema.json"),
            dungeonpunk_creation_preview_schema()?,
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

fn creation_request() -> DungeonpunkCreationRequest {
    DungeonpunkCreationRequest {
        creation_format_version: DUNGEONPUNK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.vesper_ash".to_owned(),
        name: "Vesper Ash".to_owned(),
        seed: 83,
        attributes: DungeonpunkAttributes {
            strength: 2,
            dexterity: 1,
            constitution: 1,
            intelligence: 1,
            charisma: 0,
            wisdom: 0,
        },
        special_moves: ["hale_and_hearty", "loaded_for_bear", "warriors_strike"]
            .into_iter()
            .map(|id| DungeonpunkMoveChoice::Core { id: id.to_owned() })
            .collect(),
        gear: vec![
            gear(
                "armor",
                "Patched plate",
                DungeonpunkGearKind::Armor,
                4,
                true,
            ),
            gear(
                "field_pack",
                "Field pack",
                DungeonpunkGearKind::Pack,
                4,
                false,
            ),
            gear(
                "rations",
                "Road rations",
                DungeonpunkGearKind::Rations,
                2,
                false,
            ),
            gear(
                "shield",
                "Scrap shield",
                DungeonpunkGearKind::Shield,
                2,
                true,
            ),
            gear(
                "sword",
                "Notched sword",
                DungeonpunkGearKind::Weapon,
                4,
                true,
            ),
            gear("tools", "Repair tools", DungeonpunkGearKind::Tool, 1, false),
        ],
        bonds: vec![
            "I owe Rook a road home.".to_owned(),
            "Mara knows what woke below the bridge.".to_owned(),
        ],
        clocks: vec![DungeonpunkClock {
            id: "bridge_wakes".to_owned(),
            label: "The bridge wakes".to_owned(),
            segments: 4,
            filled: 2,
            trigger: DungeonpunkClockTrigger::Full,
        }],
        threats: vec![DungeonpunkThreat {
            id: "ash_hound".to_owned(),
            label: "Ash hound".to_owned(),
            kind: DungeonpunkThreatKind::Foe,
            hit_points: 8,
            armor: 1,
            clock_id: Some("bridge_wakes".to_owned()),
        }],
    }
}

fn gear(
    id: &str,
    label: &str,
    kind: DungeonpunkGearKind,
    weight: u8,
    equipped: bool,
) -> DungeonpunkGear {
    DungeonpunkGear {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
        weight,
        uses: None,
        equipped,
    }
}

fn operation_inputs() -> Vec<(&'static str, &'static str, TabletopCapability, DomainValue)> {
    vec![
        (
            "raise_gate_together",
            "struggle",
            TabletopCapability::ChecksAndConflicts,
            object([
                ("attribute", DomainValue::Symbol("strength".to_owned())),
                ("edge", DomainValue::Symbol("neutral".to_owned())),
                ("helper_encumbered", DomainValue::Bool(false)),
                ("helper_id", DomainValue::String("mara".to_owned())),
                ("push", DomainValue::Bool(true)),
            ]),
        ),
        (
            "rest_after_push",
            "rest",
            TabletopCapability::ResourcesAndConditions,
            object([("hours", DomainValue::Number(4.0))]),
        ),
        (
            "read_omens",
            "struggle",
            TabletopCapability::ChecksAndConflicts,
            fallback_struggle(),
        ),
        (
            "bargain_gate",
            "struggle",
            TabletopCapability::ChecksAndConflicts,
            fallback_struggle(),
        ),
        (
            "learn_bridgecraft",
            "grow",
            TabletopCapability::Advancement,
            object([
                ("kind", DomainValue::Symbol("add_move".to_owned())),
                ("move_id", DomainValue::String("bridgecraft".to_owned())),
                ("move_label", DomainValue::String("Bridgecraft".to_owned())),
            ]),
        ),
        (
            "take_minor_harm",
            "minor_damage",
            TabletopCapability::ResourcesAndConditions,
            object([]),
        ),
        (
            "rest_after_harm",
            "rest",
            TabletopCapability::ResourcesAndConditions,
            object([("hours", DomainValue::Number(2.0))]),
        ),
        (
            "wake_bridge",
            "tick_clock",
            TabletopCapability::Clocks,
            object([
                ("clock_id", DomainValue::String("bridge_wakes".to_owned())),
                ("delta", DomainValue::Number(2.0)),
            ]),
        ),
        (
            "strike_ash_hound",
            "harm_threat",
            TabletopCapability::Encounters,
            object([
                ("threat_id", DomainValue::String("ash_hound".to_owned())),
                ("tier", DomainValue::Symbol("standard".to_owned())),
            ]),
        ),
        (
            "brace_falling_stone",
            "damage",
            TabletopCapability::ResourcesAndConditions,
            object([
                ("brace_item_id", DomainValue::String("armor".to_owned())),
                ("tier", DomainValue::Symbol("standard".to_owned())),
            ]),
        ),
        (
            "warn_of_trouble",
            "gm_move",
            TabletopCapability::CampaignState,
            object([
                ("category", DomainValue::Symbol("warn".to_owned())),
                (
                    "move",
                    DomainValue::Symbol("show_distant_trouble".to_owned()),
                ),
            ]),
        ),
    ]
}

fn fallback_struggle() -> DomainValue {
    object([
        ("attribute", DomainValue::Symbol("charisma".to_owned())),
        ("edge", DomainValue::Symbol("disadvantage".to_owned())),
        ("push", DomainValue::Bool(false)),
    ])
}

fn resolution_request(
    request_id: &str,
    operation: &str,
    capability: TabletopCapability,
    preview: &DungeonpunkCreationPreview,
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
