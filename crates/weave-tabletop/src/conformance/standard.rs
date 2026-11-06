//! Registrations for every adapter shipped in this build.
//!
//! A contributor adds one descriptor and one row in [`standard_suite`]; the conformance checks
//! themselves are never edited.

use std::collections::BTreeMap;

use weave_domain::DomainValue;

use crate::conformance::policy::{
    DUNGEONPUNK_NOTICES, FREEHACK_NOTICES, PLUG_AND_PLAY_NOTICES, SPARK_NOTICES,
};
use crate::conformance::{
    ConformanceAdapter, ConformanceArtifact, ConformanceCreation, ConformanceNotice,
    ConformanceOperation, ConformanceSourceBundle, ConformanceSuite, standard_source_policy,
};
use crate::{
    DUNGEONPUNK_CREATION_FORMAT_VERSION, DungeonpunkAttributes, DungeonpunkClock,
    DungeonpunkClockTrigger, DungeonpunkCreationRequest, DungeonpunkGear, DungeonpunkGearKind,
    DungeonpunkMoveChoice, DungeonpunkResolver, DungeonpunkThreat, DungeonpunkThreatKind,
    FREEHACK_CAMPAIGN_SCHEMA_VERSION, FREEHACK_CREATION_FORMAT_VERSION, FreehackArchetypeOption,
    FreehackArchetypeProcedure, FreehackCampaignSchema, FreehackCreationRequest,
    FreehackDisclosure, FreehackFeatureOption, FreehackFeatureSelection, FreehackInventoryOption,
    FreehackInventorySelection, FreehackMemorySeed, FreehackModifierGeneration,
    FreehackModifierSpec, FreehackRarity, FreehackResolver, FreehackTrackOutcome,
    FreehackTrackTemplate, PLUG_AND_PLAY_CREATION_FORMAT_VERSION, PlugAndPlayAttributes,
    PlugAndPlayCreationRequest, PlugAndPlayMentalAttribute, PlugAndPlayModifier,
    PlugAndPlayPhysicalAttribute, PlugAndPlayResolver, PlugAndPlayRollAssignment,
    PlugAndPlayStatSource, TabletopCapability, TabletopError, TabletopResolver,
    WICKED_ONES_CREATION_FORMAT_VERSION, WickedOnesActionRatings, WickedOnesCalling,
    WickedOnesCreationRequest, WickedOnesGearItem, WickedOnesResolver, WickedOnesVileFriend,
    create_dungeonpunk_character, create_freehack_character, create_plug_and_play_character,
    create_wicked_ones_character, dungeonpunk_manifest, freehack_manifest, plug_and_play_manifest,
    wicked_ones_manifest,
};
use crate::{
    SPARK_ADAPTER_ID, SPARK_CREATION_FORMAT_VERSION, SparkAttribute, SparkAttributeSeed,
    SparkBeliefSeed, SparkCreationRequest, SparkRelationshipSeed, SparkResolver, SparkStep,
    create_spark_character, spark_manifest,
};

/// Plug-And-Play registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlugAndPlayConformance;

impl ConformanceAdapter for PlugAndPlayConformance {
    fn adapter_id(&self) -> &str {
        crate::PLUG_AND_PLAY_ADAPTER_ID
    }

    fn manifest(&self) -> crate::AdapterManifest {
        plug_and_play_manifest()
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(PlugAndPlayResolver)
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let preview = create_plug_and_play_character(&plug_and_play_creation_request(seed))?;
        let canonical = preview.to_json()?;
        Ok(ConformanceCreation {
            adapter: preview.adapter,
            definition: preview.definition,
            initial_state: preview.initial_state,
            canonical,
        })
    }

    fn scenario(
        &self,
        _creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        Ok(vec![ConformanceOperation {
            request_id: "conformance_check".to_owned(),
            operation: "check".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            input: object([
                ("attribute", DomainValue::Symbol("brains".to_owned())),
                ("difficulty", DomainValue::Number(12.0)),
                ("modifier", DomainValue::Number(0.0)),
                ("roll_mode", DomainValue::Symbol("normal".to_owned())),
                ("spend_fortune_on_failure", DomainValue::Bool(false)),
            ]),
        }])
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        ConformanceSourceBundle::new(
            ConformanceArtifact::digest(
                "Plug-And-Play Rules.pdf",
                crate::PLUG_AND_PLAY_RULES_SHA256,
            ),
            vec![ConformanceArtifact::digest(
                "Plug-And-Play Character Sheet.pdf",
                crate::PLUG_AND_PLAY_SHEET_SHA256,
            )],
            ConformanceArtifact::digest("LICENSE-CC0-1.0.txt", crate::CC0_1_0_LEGAL_CODE_SHA256)
                .bundled(),
            reviewed_notices(PLUG_AND_PLAY_NOTICES),
        )
    }
}

/// Dungeonpunk registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct DungeonpunkConformance;

impl ConformanceAdapter for DungeonpunkConformance {
    fn adapter_id(&self) -> &str {
        crate::DUNGEONPUNK_ADAPTER_ID
    }

    fn manifest(&self) -> crate::AdapterManifest {
        dungeonpunk_manifest()
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(DungeonpunkResolver)
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let preview = create_dungeonpunk_character(&dungeonpunk_creation_request(seed))?;
        let canonical = preview.to_json()?;
        Ok(ConformanceCreation {
            adapter: preview.adapter,
            definition: preview.definition,
            initial_state: preview.initial_state,
            canonical,
        })
    }

    fn scenario(
        &self,
        _creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        Ok(vec![ConformanceOperation {
            request_id: "conformance_struggle".to_owned(),
            operation: "struggle".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            input: object([
                ("attribute", DomainValue::Symbol("strength".to_owned())),
                ("edge", DomainValue::Symbol("neutral".to_owned())),
                ("push", DomainValue::Bool(false)),
            ]),
        }])
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        ConformanceSourceBundle::new(
            ConformanceArtifact::digest(
                "dungeonpunk-google-doc.pdf",
                crate::DUNGEONPUNK_PDF_SHA256,
            ),
            vec![ConformanceArtifact::digest(
                "dungeonpunk-google-doc.txt",
                crate::DUNGEONPUNK_TEXT_SHA256,
            )],
            ConformanceArtifact::digest("LICENSE-CC0-1.0.txt", crate::CC0_1_0_LEGAL_CODE_SHA256)
                .bundled(),
            reviewed_notices(DUNGEONPUNK_NOTICES),
        )
    }
}

/// Freehack registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct FreehackConformance;

impl ConformanceAdapter for FreehackConformance {
    fn adapter_id(&self) -> &str {
        crate::FREEHACK_ADAPTER_ID
    }

    fn manifest(&self) -> crate::AdapterManifest {
        freehack_manifest()
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(FreehackResolver)
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let preview = create_freehack_character(&freehack_creation_request(seed))?;
        let canonical = preview.to_json()?;
        Ok(ConformanceCreation {
            adapter: preview.adapter,
            definition: preview.definition,
            initial_state: preview.initial_state,
            canonical,
        })
    }

    fn scenario(
        &self,
        _creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        Ok(vec![ConformanceOperation {
            request_id: "conformance_resolve_check".to_owned(),
            operation: "resolve_check".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            input: object([
                ("disclosure", DomainValue::Symbol("public".to_owned())),
                (
                    "label",
                    DomainValue::String("Cross the conformance gantry".to_owned()),
                ),
                (
                    "opposition",
                    DomainValue::List(vec![value_contribution(
                        "conformance_opposition",
                        "Conformance Opposition",
                        3,
                        "situation",
                    )]),
                ),
                ("reveal_opposition", DomainValue::Bool(true)),
                ("reveal_probability", DomainValue::Bool(true)),
                ("reveal_support", DomainValue::Bool(true)),
                (
                    "support",
                    DomainValue::List(vec![value_contribution("focus", "Focus", 4, "character")]),
                ),
            ]),
        }])
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        ConformanceSourceBundle::new(
            ConformanceArtifact::digest("freehack.pdf", crate::FREEHACK_PDF_SHA256),
            vec![
                ConformanceArtifact::digest("src/freehack.md", crate::FREEHACK_MARKDOWN_SHA256),
                ConformanceArtifact::digest("src/freehack.yml", crate::FREEHACK_METADATA_SHA256),
                ConformanceArtifact::digest("tools/roll.py", crate::FREEHACK_ROLL_TOOL_SHA256),
            ],
            ConformanceArtifact::digest(
                "LICENSE-CC0-1.0.txt",
                crate::FREEHACK_CC0_1_0_LEGAL_CODE_SHA256,
            )
            .bundled(),
            reviewed_notices(FREEHACK_NOTICES),
        )
    }
}

/// Wicked Ones registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct WickedOnesConformance;

impl ConformanceAdapter for WickedOnesConformance {
    fn adapter_id(&self) -> &str {
        crate::WICKED_ONES_ADAPTER_ID
    }

    fn manifest(&self) -> crate::AdapterManifest {
        wicked_ones_manifest()
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(WickedOnesResolver)
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let preview = create_wicked_ones_character(&wicked_ones_creation_request(seed))?;
        let canonical = preview.to_json()?;
        Ok(ConformanceCreation {
            adapter: preview.adapter,
            definition: preview.definition,
            initial_state: preview.initial_state,
            canonical,
        })
    }

    fn scenario(
        &self,
        _creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        Ok(vec![ConformanceOperation {
            request_id: "conformance_resolve_action".to_owned(),
            operation: "resolve_action".to_owned(),
            capability: TabletopCapability::ChecksAndConflicts,
            input: object([
                ("action", DomainValue::Symbol("sneak".to_owned())),
                ("approach", DomainValue::Symbol("guile".to_owned())),
                ("bonus", DomainValue::Number(0.0)),
                ("effect", DomainValue::Symbol("standard".to_owned())),
                ("going_hard", DomainValue::Bool(false)),
                (
                    "intention",
                    DomainValue::String("Slip past the conformance ledger".to_owned()),
                ),
                ("position", DomainValue::Symbol("risky".to_owned())),
                ("teamwork", DomainValue::List(Vec::new())),
            ]),
        }])
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        ConformanceSourceBundle::new(
            ConformanceArtifact::checked_bytes(
                "SOURCE_RECORD.txt",
                crate::wicked_ones::WICKED_ONES_SOURCE_RECORD_SHA256,
                crate::wicked_ones::SOURCE_RECORD.as_bytes(),
            )
            .bundled(),
            Vec::new(),
            ConformanceArtifact::digest(
                "LICENSE-CC0-1.0.txt",
                crate::WICKED_ONES_CC0_LEGAL_CODE_SHA256,
            )
            .bundled(),
            reviewed_notices(crate::WICKED_ONES_NOTICES),
        )
    }
}

/// Spark registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct SparkConformance;

impl ConformanceAdapter for SparkConformance {
    fn adapter_id(&self) -> &str {
        SPARK_ADAPTER_ID
    }

    fn manifest(&self) -> crate::AdapterManifest {
        spark_manifest()
    }

    fn resolver(&self) -> Box<dyn TabletopResolver> {
        Box::new(SparkResolver)
    }

    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError> {
        let preview = create_spark_character(&spark_creation_request(seed))?;
        let canonical = preview.to_json()?;
        Ok(ConformanceCreation {
            adapter: preview.adapter,
            definition: preview.definition,
            initial_state: preview.initial_state,
            canonical,
        })
    }

    fn scenario(
        &self,
        _creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError> {
        Ok(vec![
            ConformanceOperation {
                request_id: "conformance_open".to_owned(),
                operation: "open_conflict".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([
                    (
                        "conflict_id",
                        DomainValue::String("conformance_gate".to_owned()),
                    ),
                    ("max_escalation", DomainValue::Number(0.0)),
                    (
                        "stakes",
                        DomainValue::String("Control of the conformance gate.".to_owned()),
                    ),
                    ("victory_cost_amount", DomainValue::Number(0.0)),
                    ("victory_cost_attribute", DomainValue::String(String::new())),
                    (
                        "victory_cost_kind",
                        DomainValue::Symbol("influence".to_owned()),
                    ),
                ]),
            },
            ConformanceOperation {
                request_id: "conformance_declare_owner".to_owned(),
                operation: "declare_action".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([
                    (
                        "actor_id",
                        DomainValue::String(SPARK_CONFORMANCE_CHARACTER_ID.to_owned()),
                    ),
                    ("attribute", DomainValue::Symbol("mind".to_owned())),
                    (
                        "conflict_id",
                        DomainValue::String("conformance_gate".to_owned()),
                    ),
                    (
                        "declaration_id",
                        DomainValue::String("conformance_owner".to_owned()),
                    ),
                    (
                        "description",
                        DomainValue::String("The owner reads the tide table aloud.".to_owned()),
                    ),
                ]),
            },
            ConformanceOperation {
                request_id: "conformance_declare_opponent".to_owned(),
                operation: "declare_action".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([
                    (
                        "actor_id",
                        DomainValue::String("conformance_opponent".to_owned()),
                    ),
                    ("attribute", DomainValue::Symbol("body".to_owned())),
                    (
                        "conflict_id",
                        DomainValue::String("conformance_gate".to_owned()),
                    ),
                    (
                        "declaration_id",
                        DomainValue::String("conformance_opponent".to_owned()),
                    ),
                    (
                        "description",
                        DomainValue::String(
                            "The opponent contests the conformance gate.".to_owned(),
                        ),
                    ),
                ]),
            },
            ConformanceOperation {
                request_id: "conformance_support".to_owned(),
                operation: "add_supporter".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([
                    (
                        "conflict_id",
                        DomainValue::String("conformance_gate".to_owned()),
                    ),
                    (
                        "declaration_id",
                        DomainValue::String("conformance_owner".to_owned()),
                    ),
                    (
                        "relationship_id",
                        DomainValue::String("conformance_sable".to_owned()),
                    ),
                ]),
            },
            ConformanceOperation {
                request_id: "conformance_roll".to_owned(),
                operation: "roll_conflict".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([(
                    "conflict_id",
                    DomainValue::String("conformance_gate".to_owned()),
                )]),
            },
            ConformanceOperation {
                request_id: "conformance_resolve".to_owned(),
                operation: "resolve_conflict".to_owned(),
                capability: TabletopCapability::ChecksAndConflicts,
                input: object([(
                    "conflict_id",
                    DomainValue::String("conformance_gate".to_owned()),
                )]),
            },
        ])
    }

    fn source_bundle(&self) -> ConformanceSourceBundle {
        ConformanceSourceBundle::new(
            ConformanceArtifact::checked_bytes(
                "spark-source-record.txt",
                crate::spark::SPARK_SOURCE_RECORD_SHA256,
                crate::spark::SPARK_SOURCE_RECORD.as_bytes(),
            )
            .bundled(),
            Vec::new(),
            ConformanceArtifact::digest(
                "LICENSE-CC0-1.0.txt",
                crate::SPARK_CC0_1_0_LEGAL_CODE_SHA256,
            )
            .bundled(),
            reviewed_notices(SPARK_NOTICES),
        )
    }
}

/// Suite covering every adapter shipped in this build.
#[must_use]
pub fn standard_suite() -> ConformanceSuite {
    ConformanceSuite::new(
        vec![
            Box::new(PlugAndPlayConformance),
            Box::new(DungeonpunkConformance),
            Box::new(FreehackConformance),
            Box::new(WickedOnesConformance),
            Box::new(SparkConformance),
        ],
        standard_source_policy(),
    )
}

fn reviewed_notices(notices: &[&str]) -> Vec<ConformanceNotice> {
    notices
        .iter()
        .map(|text| ConformanceNotice::new(*text))
        .collect()
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn value_contribution(id: &str, label: &str, value: u32, source: &str) -> DomainValue {
    object([
        ("id", DomainValue::String(id.to_owned())),
        ("label", DomainValue::String(label.to_owned())),
        ("source", DomainValue::Symbol(source.to_owned())),
        ("value", DomainValue::Number(f64::from(value))),
    ])
}

fn plug_and_play_creation_request(seed: u64) -> PlugAndPlayCreationRequest {
    PlugAndPlayCreationRequest {
        creation_format_version: PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.conformance_ember".to_owned(),
        name: "Conformance Ember".to_owned(),
        age: 30,
        seed,
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
                id: "fleet".to_owned(),
                label: "Fleet courier".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 1,
                    brains: 0,
                    brawn: -1,
                    wits: 0,
                },
                fortune: 0,
                explanation: "Quick footwork is offset by a lighter frame.".to_owned(),
            },
            PlugAndPlayModifier {
                id: "studious".to_owned(),
                label: "Studious observer".to_owned(),
                attributes: PlugAndPlayAttributes {
                    agility: 0,
                    brains: 1,
                    brawn: 0,
                    wits: -1,
                },
                fortune: 0,
                explanation: "Careful study is offset by slower improvisation.".to_owned(),
            },
        ],
        inventory: vec!["field knife".to_owned(), "signal pistol".to_owned()],
    }
}

fn dungeonpunk_creation_request(seed: u64) -> DungeonpunkCreationRequest {
    DungeonpunkCreationRequest {
        creation_format_version: DUNGEONPUNK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.conformance_vesper".to_owned(),
        name: "Conformance Vesper".to_owned(),
        seed,
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

fn wicked_ones_creation_request(seed: u64) -> WickedOnesCreationRequest {
    WickedOnesCreationRequest {
        creation_format_version: WICKED_ONES_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.conformance_marrow".to_owned(),
        name: "Conformance Marrow".to_owned(),
        race: "Emberkin".to_owned(),
        look: "Ash-grey horns and a patched coat".to_owned(),
        calling: WickedOnesCalling::Whisperer,
        actions: WickedOnesActionRatings {
            analyze: 1,
            invent: 0,
            perceive: 1,
            brawl: 1,
            demolish: 0,
            sneak: 2,
            command: 0,
            persuade: 1,
            endure: 0,
        },
        dark_impulse: "Collect every hidden name".to_owned(),
        revelry: "Counting stolen coins by candlelight".to_owned(),
        gear: vec![
            WickedOnesGearItem {
                id: "chalk_knife".to_owned(),
                label: "Chalk Knife".to_owned(),
                quantity: 1,
            },
            WickedOnesGearItem {
                id: "coil_of_rope".to_owned(),
                label: "Coil of Rope".to_owned(),
                quantity: 2,
            },
        ],
        abilities: vec!["silent_step".to_owned(), "silver_tongue".to_owned()],
        vile_friend: WickedOnesVileFriend {
            name: "Grist".to_owned(),
            detail: "A patient tunnel rat that remembers grudges".to_owned(),
        },
        starting_resources: 3,
        seed,
    }
}

fn freehack_creation_request(seed: u64) -> FreehackCreationRequest {
    FreehackCreationRequest {
        creation_format_version: FREEHACK_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.conformance_tavi".to_owned(),
        name: "Conformance Tavi".to_owned(),
        seed,
        campaign: freehack_campaign(),
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

fn freehack_campaign() -> FreehackCampaignSchema {
    FreehackCampaignSchema {
        schema_version: FREEHACK_CAMPAIGN_SCHEMA_VERSION,
        id: "org.weave.freehack.conformance_harbor".to_owned(),
        title: "Conformance Harbor".to_owned(),
        archetype_procedure: FreehackArchetypeProcedure::RarityOffer,
        archetypes: vec![
            FreehackArchetypeOption {
                id: "courier".to_owned(),
                label: "Courier".to_owned(),
                description: "Moves messages and small cargo between districts.".to_owned(),
                rarity: FreehackRarity::Common,
            },
            FreehackArchetypeOption {
                id: "scribe".to_owned(),
                label: "Scribe".to_owned(),
                description: "Keeps exact public and private records.".to_owned(),
                rarity: FreehackRarity::Common,
            },
            FreehackArchetypeOption {
                id: "diver".to_owned(),
                label: "Diver".to_owned(),
                description: "Works below the tide line.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "pilot".to_owned(),
                label: "Pilot".to_owned(),
                description: "Guides vessels through narrow channels.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "rigger".to_owned(),
                label: "Rigger".to_owned(),
                description: "Builds temporary lines and lifts.".to_owned(),
                rarity: FreehackRarity::Uncommon,
            },
            FreehackArchetypeOption {
                id: "cartographer".to_owned(),
                label: "Cartographer".to_owned(),
                description: "Maintains charts of changing passages.".to_owned(),
                rarity: FreehackRarity::Rare,
            },
            FreehackArchetypeOption {
                id: "signal_keeper".to_owned(),
                label: "Signal Keeper".to_owned(),
                description: "Coordinates long-range harbor signals.".to_owned(),
                rarity: FreehackRarity::Rare,
            },
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
    }
}

const SPARK_CONFORMANCE_CHARACTER_ID: &str = "org.weave.character.conformance_lumen";

fn spark_creation_request(seed: u64) -> SparkCreationRequest {
    SparkCreationRequest {
        creation_format_version: SPARK_CREATION_FORMAT_VERSION,
        character_id: SPARK_CONFORMANCE_CHARACTER_ID.to_owned(),
        name: "Conformance Lumen".to_owned(),
        concept: "A lantern-keeper who reads the conformance tide.".to_owned(),
        beliefs: vec![
            SparkBeliefSeed {
                id: "steady_light".to_owned(),
                statement: "A steady light protects the people behind it.".to_owned(),
            },
            SparkBeliefSeed {
                id: "honest_warning".to_owned(),
                statement: "A warning is only useful when it is honest.".to_owned(),
            },
            SparkBeliefSeed {
                id: "shared_route".to_owned(),
                statement: "Every safe route belongs to the people who use it.".to_owned(),
            },
        ],
        relationships: vec![SparkRelationshipSeed {
            id: "conformance_sable".to_owned(),
            name: "Conformance Sable".to_owned(),
            description: "A signal runner who knows the conformance gate.".to_owned(),
        }],
        attributes: vec![
            SparkAttributeSeed {
                attribute: SparkAttribute::Body,
                step: SparkStep::D4,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Heart,
                step: SparkStep::D6,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Mind,
                step: SparkStep::D12,
            },
            SparkAttributeSeed {
                attribute: SparkAttribute::Spark,
                step: SparkStep::D8,
            },
        ],
        talents: Vec::new(),
        starting_influence: 10 + u32::try_from(seed % 7).unwrap_or(0),
    }
}
