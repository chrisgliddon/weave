//! Verified CC0 Plug-And-Play adapter, creation workflow, and deterministic resolver.
//!
//! The implementation models the public rules independently as typed data and original Rust
//! procedures. It pins the official rules and character-sheet artifacts by exact hash but does not
//! redistribute their PDF layout, branding, or imagery.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, FieldDeclaration, TypeExpression};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterExtensionSurface, AdapterManifest, AdapterProvenance, AdapterSourceArtifact,
    AdapterSourceClass, CreationField, CreationFieldAuthority, CreationStep, EntropyState,
    EntropyStream, EventVisibility, ExcludedMaterial, HostVisibilityPolicy, LicenseTextReference,
    RESOLVER_CONTRACT_VERSION, ResolvedAdapter, ResolverEvent, ResolverOperationDeclaration,
    ResolverOutput, TabletopCapability, TabletopCapabilityDeclaration, TabletopError,
    TabletopResolver, TabletopState, canonical_fingerprint, resolved_adapter,
    validate_tabletop_state,
};

/// Stable adapter identity used by source, editor, runtime, and host integrations.
pub const PLUG_AND_PLAY_ADAPTER_ID: &str = "org.weave.tabletop.plug_and_play";
/// First independently modeled Plug-And-Play adapter release.
pub const PLUG_AND_PLAY_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned creation request and preview format.
pub const PLUG_AND_PLAY_CREATION_FORMAT_VERSION: u32 = 1;
/// Official public release page used to acquire both reviewed PDF inputs.
pub const PLUG_AND_PLAY_SOURCE_URL: &str = "https://distilledproductions.itch.io/plug-and-play";
/// SHA-256 of `Plug-And-Play Rules.pdf`, acquired from the official release page.
pub const PLUG_AND_PLAY_RULES_SHA256: &str =
    "4d91ab7cf489c7eef6a9ad3e74be47899fd6419ea3ab0776367d6f444115985e";
/// SHA-256 of `Plug-And-Play Character Sheet.pdf`, acquired from the official release page.
pub const PLUG_AND_PLAY_SHEET_SHA256: &str =
    "dfaf6f01a25773aa311f3662dff2c975096544b675c11dac5f4bf0ac42938d8b";
/// SHA-256 of the official plain-text CC0 1.0 legal code retained beside the adapter.
pub const CC0_1_0_LEGAL_CODE_SHA256: &str =
    "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499";

/// The four regular character attributes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PlugAndPlayAttribute {
    Agility,
    Brains,
    Brawn,
    Wits,
}

/// Attribute used for the whole contribution to Survivability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlugAndPlayPhysicalAttribute {
    Agility,
    Brawn,
}

/// Attribute used for the rounded-up half contribution to Survivability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlugAndPlayMentalAttribute {
    Brains,
    Wits,
}

/// Four base or effective regular attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayAttributes {
    pub agility: i32,
    pub brains: i32,
    pub brawn: i32,
    pub wits: i32,
}

impl PlugAndPlayAttributes {
    fn adjusted(&self, adjustment: &Self) -> Self {
        Self {
            agility: self.agility + adjustment.agility,
            brains: self.brains + adjustment.brains,
            brawn: self.brawn + adjustment.brawn,
            wits: self.wits + adjustment.wits,
        }
    }

    fn sum(&self) -> i32 {
        self.agility + self.brains + self.brawn + self.wits
    }
}

/// One balanced occupation, background, class, ancestry, or similarly scoped modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayModifier {
    pub id: String,
    pub label: String,
    pub attributes: PlugAndPlayAttributes,
    pub fortune: i32,
    pub explanation: String,
}

/// Assignment of the selected four d6 results to the four regular attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayRollAssignment {
    pub agility: u8,
    pub brains: u8,
    pub brawn: u8,
    pub wits: u8,
}

/// Source of the base ratings in a creation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PlugAndPlayStatSource {
    /// Generate two deterministic sets, then accept and assign one explicitly.
    Rolled {
        selected_set: u8,
        assignment: PlugAndPlayRollAssignment,
    },
    /// Supply the four d6-range ratings and d3-range Fortune value directly.
    Authored {
        attributes: PlugAndPlayAttributes,
        fortune: i32,
    },
}

/// Complete, versioned input to deterministic Plug-And-Play character creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    pub age: u16,
    pub seed: u64,
    pub stat_source: PlugAndPlayStatSource,
    pub survivability_body: PlugAndPlayPhysicalAttribute,
    pub survivability_mind: PlugAndPlayMentalAttribute,
    pub modifiers: Vec<PlugAndPlayModifier>,
    pub inventory: Vec<String>,
}

/// One of the two reproducible raw creation sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayRollSet {
    pub index: u8,
    pub d6: Vec<u8>,
    pub d3: u8,
    pub entropy_start: u64,
    pub entropy_end: u64,
}

/// Explainable result of creation before it is exported as a projection and mutable state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlugAndPlayCreationPreview {
    pub creation_format_version: u32,
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub roll_sets: Vec<PlugAndPlayRollSet>,
    pub selected_set: Option<u8>,
    pub base_attributes: PlugAndPlayAttributes,
    pub base_fortune: i32,
    pub effective_attributes: PlugAndPlayAttributes,
    pub effective_fortune: i32,
    pub survivability: i32,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
}

impl PlugAndPlayCreationRequest {
    /// Parse strict JSON and reject duplicate object keys.
    pub fn from_json(source: &str) -> Result<Self, TabletopError> {
        weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
    }

    /// Parse one strict RON creation request.
    pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
        ron::from_str(source).map_err(|_| TabletopError::Artifact)
    }

    /// Serialize canonical, pretty JSON.
    pub fn to_json(&self) -> Result<String, TabletopError> {
        weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
    }

    /// Serialize canonical, pretty RON.
    pub fn to_ron(&self) -> Result<String, TabletopError> {
        weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
    }
}

impl PlugAndPlayCreationPreview {
    /// Parse strict JSON and reject duplicate object keys.
    pub fn from_json(source: &str) -> Result<Self, TabletopError> {
        weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
    }

    /// Parse one strict RON creation preview.
    pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
        ron::from_str(source).map_err(|_| TabletopError::Artifact)
    }

    /// Serialize canonical, pretty JSON.
    pub fn to_json(&self) -> Result<String, TabletopError> {
        weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
    }

    /// Serialize canonical, pretty RON.
    pub fn to_ron(&self) -> Result<String, TabletopError> {
        weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
    }
}

/// Canonical JSON Schema for Plug-And-Play creation requests.
pub fn plug_and_play_creation_request_schema() -> Result<String, TabletopError> {
    creation_schema::<PlugAndPlayCreationRequest>(
        "urn:weave:schema:tabletop-plug-and-play-creation-request:1",
        "Weave Plug-And-Play Creation Request v1",
    )
}

/// Canonical JSON Schema for explainable Plug-And-Play creation previews.
pub fn plug_and_play_creation_preview_schema() -> Result<String, TabletopError> {
    creation_schema::<PlugAndPlayCreationPreview>(
        "urn:weave:schema:tabletop-plug-and-play-creation-preview:1",
        "Weave Plug-And-Play Creation Preview v1",
    )
}

fn creation_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, TabletopError> {
    let schema = schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| TabletopError::Artifact)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-tabletop-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut("creation_format_version"))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert(
                "const".to_owned(),
                serde_json::Value::from(PLUG_AND_PLAY_CREATION_FORMAT_VERSION),
            );
        }
    }
    sort_json_keys(&mut value);
    weave_domain::to_pretty_json(&value).map_err(|_| TabletopError::Artifact)
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                sort_json_keys(value);
            }
            values.sort_keys();
        }
        _ => {}
    }
}

/// Validate and deterministically create a complete playable character.
///
/// Rolled creation consumes two complete candidate sets from the supplied seed before the chosen
/// set is accepted. The resulting entropy cursor is retained in mutable state, so play continues
/// from the exact point at which creation stopped. Authored ratings consume no entropy.
pub fn create_plug_and_play_character(
    request: &PlugAndPlayCreationRequest,
) -> Result<PlugAndPlayCreationPreview, TabletopError> {
    validate_creation_request(request)?;

    let manifest = plug_and_play_manifest();
    crate::validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let mut entropy = EntropyStream::from_state(EntropyState {
        algorithm: "sha256_counter_v1".to_owned(),
        seed: request.seed,
        cursor: 0,
    })?;

    let (roll_sets, selected_set, assignment, base_attributes, base_fortune, creation_mode) =
        match &request.stat_source {
            PlugAndPlayStatSource::Rolled {
                selected_set,
                assignment,
            } => {
                let mut roll_sets = Vec::with_capacity(2);
                for index in 0..2 {
                    let entropy_start = entropy.state().cursor;
                    let mut d6 = Vec::with_capacity(4);
                    for _ in 0..4 {
                        d6.push(draw_creation_die(&mut entropy, 6)?);
                    }
                    let d3 = draw_creation_die(&mut entropy, 3)?;
                    roll_sets.push(PlugAndPlayRollSet {
                        index,
                        d6,
                        d3,
                        entropy_start,
                        entropy_end: entropy.state().cursor,
                    });
                }
                let chosen = &roll_sets[usize::from(*selected_set)];
                let attributes = PlugAndPlayAttributes {
                    agility: i32::from(chosen.d6[usize::from(assignment.agility)]),
                    brains: i32::from(chosen.d6[usize::from(assignment.brains)]),
                    brawn: i32::from(chosen.d6[usize::from(assignment.brawn)]),
                    wits: i32::from(chosen.d6[usize::from(assignment.wits)]),
                };
                let fortune = i32::from(chosen.d3);
                (
                    roll_sets,
                    Some(*selected_set),
                    Some(assignment.clone()),
                    attributes,
                    fortune,
                    "rolled",
                )
            }
            PlugAndPlayStatSource::Authored {
                attributes,
                fortune,
            } => (
                Vec::new(),
                None,
                None,
                attributes.clone(),
                *fortune,
                "authored",
            ),
        };

    let total_adjustment = request.modifiers.iter().fold(
        PlugAndPlayAttributes {
            agility: 0,
            brains: 0,
            brawn: 0,
            wits: 0,
        },
        |total, modifier| total.adjusted(&modifier.attributes),
    );
    let effective_attributes = base_attributes.adjusted(&total_adjustment);
    let effective_fortune = base_fortune
        + request
            .modifiers
            .iter()
            .map(|modifier| modifier.fortune)
            .sum::<i32>();
    validate_effective_ratings(&effective_attributes, effective_fortune)?;

    let body = match request.survivability_body {
        PlugAndPlayPhysicalAttribute::Agility => effective_attributes.agility,
        PlugAndPlayPhysicalAttribute::Brawn => effective_attributes.brawn,
    };
    let mind = match request.survivability_mind {
        PlugAndPlayMentalAttribute::Brains => effective_attributes.brains,
        PlugAndPlayMentalAttribute::Wits => effective_attributes.wits,
    };
    let survivability = body + (mind + 1) / 2;

    let definition_value = creation_definition_value(
        request,
        &roll_sets,
        selected_set,
        assignment.as_ref(),
        &base_attributes,
        base_fortune,
        &effective_attributes,
        effective_fortune,
        survivability,
        creation_mode,
    );
    weave_domain::validate_typed_value(
        "definition",
        &definition_value,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec![
            "details".to_owned(),
            "identity".to_owned(),
            "ratings".to_owned(),
            "survivability".to_owned(),
        ],
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: adapter.clone(),
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy: entropy.state(),
        value: object([
            ("dead", DomainValue::Bool(false)),
            (
                "fortune_remaining",
                DomainValue::Number(f64::from(effective_fortune)),
            ),
            (
                "inventory",
                DomainValue::List(
                    request
                        .inventory
                        .iter()
                        .cloned()
                        .map(DomainValue::String)
                        .collect(),
                ),
            ),
            ("jammed_items", DomainValue::List(Vec::new())),
            ("last_operation", DomainValue::String(String::new())),
            (
                "survivability_current",
                DomainValue::Number(f64::from(survivability)),
            ),
            ("wounded", DomainValue::Bool(false)),
        ]),
    };
    validate_tabletop_state(&initial_state, &manifest)?;

    let explanations = vec![
        format!(
            "Creation seed {} produced {} visible candidate set(s); play begins at entropy cursor {}.",
            request.seed,
            roll_sets.len(),
            initial_state.entropy.cursor
        ),
        format!(
            "Survivability {} uses {:?} in full plus half of {:?}, rounded up.",
            survivability, request.survivability_body, request.survivability_mind
        ),
        "Each of the two modifiers is independently zero-sum across the four attributes and Fortune."
            .to_owned(),
    ];

    Ok(PlugAndPlayCreationPreview {
        creation_format_version: PLUG_AND_PLAY_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter,
        roll_sets,
        selected_set,
        base_attributes,
        base_fortune,
        effective_attributes,
        effective_fortune,
        survivability,
        definition,
        initial_state,
        explanations,
    })
}

/// Validate an exported creation preview without trusting its redundant explanation fields.
pub fn validate_plug_and_play_creation_preview(
    preview: &PlugAndPlayCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != PLUG_AND_PLAY_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    if preview.request_sha256.len() != 64
        || !preview
            .request_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "request_sha256",
            "expected a lowercase SHA-256 fingerprint",
        ));
    }
    let manifest = plug_and_play_manifest();
    let adapter = resolved_adapter(&manifest)?;
    if preview.adapter != adapter || preview.definition.adapter != adapter {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    if preview.definition.definition_sha256
        != canonical_fingerprint(&preview.definition.definition)?
    {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    if preview.definition.completed_creation_steps
        != ["details", "identity", "ratings", "survivability"]
    {
        return Err(invalid(
            "completed_creation_steps",
            "creation preview is incomplete",
        ));
    }
    weave_domain::validate_typed_value(
        "definition",
        &preview.definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    validate_tabletop_state(&preview.initial_state, &manifest)?;
    if preview.initial_state.definition_sha256 != preview.definition.definition_sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    validate_effective_ratings(&preview.effective_attributes, preview.effective_fortune)?;

    let definition = value_object("definition", &preview.definition.definition)?;
    if attributes_from_value(
        "definition.base_attributes",
        object_field("definition", definition, "base_attributes")?,
    )? != preview.base_attributes
        || object_integer("definition", definition, "base_fortune")? != preview.base_fortune
        || attributes_from_value(
            "definition.effective_attributes",
            object_field("definition", definition, "effective_attributes")?,
        )? != preview.effective_attributes
        || object_integer("definition", definition, "fortune_initial")? != preview.effective_fortune
        || object_integer("definition", definition, "survivability")? != preview.survivability
        || object_integer(
            "state",
            value_object("state", &preview.initial_state.value)?,
            "fortune_remaining",
        )? != preview.effective_fortune
        || object_integer(
            "state",
            value_object("state", &preview.initial_state.value)?,
            "survivability_current",
        )? != preview.survivability
    {
        return Err(invalid(
            "creation_preview",
            "redundant creation values do not match the exported definition and state",
        ));
    }
    match preview.selected_set {
        Some(selected) if selected <= 1 && preview.roll_sets.len() == 2 => {
            if preview.roll_sets.iter().enumerate().any(|(index, set)| {
                usize::from(set.index) != index
                    || set.d6.len() != 4
                    || set.entropy_start > set.entropy_end
            }) || preview.roll_sets[1].entropy_end != preview.initial_state.entropy.cursor
            {
                return Err(invalid(
                    "roll_sets",
                    "rolled creation entropy lineage is inconsistent",
                ));
            }
        }
        None if preview.roll_sets.is_empty() && preview.initial_state.entropy.cursor == 0 => {}
        _ => {
            return Err(invalid(
                "roll_sets",
                "creation mode and roll-set lineage disagree",
            ));
        }
    }
    for explanation in &preview.explanations {
        validate_bounded_text("explanations", explanation, 1, 2_048)?;
    }
    Ok(())
}

fn attributes_from_value(
    path: &str,
    value: &DomainValue,
) -> Result<PlugAndPlayAttributes, TabletopError> {
    let fields = value_object(path, value)?;
    Ok(PlugAndPlayAttributes {
        agility: object_integer(path, fields, "agility")?,
        brains: object_integer(path, fields, "brains")?,
        brawn: object_integer(path, fields, "brawn")?,
        wits: object_integer(path, fields, "wits")?,
    })
}

fn draw_creation_die(entropy: &mut EntropyStream, sides: u64) -> Result<u8, TabletopError> {
    let value = entropy.draw_bounded(sides)? + 1;
    u8::try_from(value).map_err(|_| invalid("creation.roll", "die result is out of range"))
}

fn validate_creation_request(request: &PlugAndPlayCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != PLUG_AND_PLAY_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    if !(1..=300).contains(&request.age) {
        return Err(invalid("age", "expected an age from 1 through 300"));
    }
    match &request.stat_source {
        PlugAndPlayStatSource::Rolled {
            selected_set,
            assignment,
        } => {
            if *selected_set > 1 {
                return Err(invalid(
                    "stat_source.selected_set",
                    "expected set zero or one",
                ));
            }
            let mut indexes = [
                assignment.agility,
                assignment.brains,
                assignment.brawn,
                assignment.wits,
            ];
            indexes.sort_unstable();
            if indexes != [0, 1, 2, 3] {
                return Err(invalid(
                    "stat_source.assignment",
                    "expected each roll index exactly once",
                ));
            }
        }
        PlugAndPlayStatSource::Authored {
            attributes,
            fortune,
        } => {
            validate_attributes("stat_source.attributes", attributes, 1, 6)?;
            if !(1..=3).contains(fortune) {
                return Err(invalid(
                    "stat_source.fortune",
                    "expected an authored Fortune rating from 1 through 3",
                ));
            }
        }
    }
    if request.modifiers.len() != 2 {
        return Err(invalid("modifiers", "expected exactly two modifiers"));
    }
    let mut previous_id: Option<&str> = None;
    for modifier in &request.modifiers {
        validate_local_identifier("modifiers.id", &modifier.id)?;
        if previous_id.is_some_and(|previous| previous >= modifier.id.as_str()) {
            return Err(invalid(
                "modifiers.id",
                "expected unique modifiers ordered by identifier",
            ));
        }
        previous_id = Some(&modifier.id);
        validate_bounded_text("modifiers.label", &modifier.label, 1, 160)?;
        validate_bounded_text("modifiers.explanation", &modifier.explanation, 1, 512)?;
        validate_attributes("modifiers.attributes", &modifier.attributes, -6, 6)?;
        if !(-6..=6).contains(&modifier.fortune) {
            return Err(invalid(
                "modifiers.fortune",
                "expected an adjustment from -6 through 6",
            ));
        }
        let adjustments = [
            modifier.attributes.agility,
            modifier.attributes.brains,
            modifier.attributes.brawn,
            modifier.attributes.wits,
            modifier.fortune,
        ];
        if modifier.attributes.sum() + modifier.fortune != 0
            || !adjustments.iter().any(|value| *value > 0)
            || !adjustments.iter().any(|value| *value < 0)
        {
            return Err(invalid(
                "modifiers",
                "each modifier must contain equal positive and negative adjustments",
            ));
        }
    }
    if request.inventory.len() > 64 {
        return Err(invalid("inventory", "expected no more than 64 items"));
    }
    let mut inventory = BTreeSet::new();
    for item in &request.inventory {
        validate_bounded_text("inventory", item, 1, 120)?;
        if !inventory.insert(item) {
            return Err(invalid("inventory", "expected unique item names"));
        }
    }
    Ok(())
}

fn validate_effective_ratings(
    attributes: &PlugAndPlayAttributes,
    fortune: i32,
) -> Result<(), TabletopError> {
    validate_attributes("effective_attributes", attributes, 0, 12)?;
    if !(0..=6).contains(&fortune) {
        return Err(invalid(
            "effective_fortune",
            "balanced modifiers produced an out-of-range Fortune rating",
        ));
    }
    Ok(())
}

fn validate_attributes(
    path: &str,
    attributes: &PlugAndPlayAttributes,
    minimum: i32,
    maximum: i32,
) -> Result<(), TabletopError> {
    if [
        attributes.agility,
        attributes.brains,
        attributes.brawn,
        attributes.wits,
    ]
    .into_iter()
    .any(|value| !(minimum..=maximum).contains(&value))
    {
        return Err(invalid(path, "one or more attributes are out of range"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn creation_definition_value(
    request: &PlugAndPlayCreationRequest,
    roll_sets: &[PlugAndPlayRollSet],
    selected_set: Option<u8>,
    assignment: Option<&PlugAndPlayRollAssignment>,
    base_attributes: &PlugAndPlayAttributes,
    base_fortune: i32,
    effective_attributes: &PlugAndPlayAttributes,
    effective_fortune: i32,
    survivability: i32,
    creation_mode: &str,
) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "age".to_owned(),
            DomainValue::Number(f64::from(request.age)),
        ),
        (
            "base_attributes".to_owned(),
            attributes_value(base_attributes),
        ),
        (
            "base_fortune".to_owned(),
            DomainValue::Number(f64::from(base_fortune)),
        ),
        (
            "creation_mode".to_owned(),
            DomainValue::Symbol(creation_mode.to_owned()),
        ),
        (
            "creation_seed".to_owned(),
            DomainValue::String(request.seed.to_string()),
        ),
        (
            "effective_attributes".to_owned(),
            attributes_value(effective_attributes),
        ),
        (
            "fortune_initial".to_owned(),
            DomainValue::Number(f64::from(effective_fortune)),
        ),
        (
            "inventory".to_owned(),
            DomainValue::List(
                request
                    .inventory
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        (
            "modifiers".to_owned(),
            DomainValue::List(request.modifiers.iter().map(modifier_value).collect()),
        ),
        ("name".to_owned(), DomainValue::String(request.name.clone())),
        (
            "roll_sets".to_owned(),
            DomainValue::List(roll_sets.iter().map(roll_set_value).collect()),
        ),
        (
            "survivability".to_owned(),
            DomainValue::Number(f64::from(survivability)),
        ),
        (
            "survivability_body".to_owned(),
            DomainValue::Symbol(physical_symbol(request.survivability_body).to_owned()),
        ),
        (
            "survivability_mind".to_owned(),
            DomainValue::Symbol(mental_symbol(request.survivability_mind).to_owned()),
        ),
    ]);
    if let Some(assignment) = assignment {
        fields.insert("assignment".to_owned(), assignment_value(assignment));
    }
    if let Some(selected_set) = selected_set {
        fields.insert(
            "selected_set".to_owned(),
            DomainValue::Number(f64::from(selected_set)),
        );
    }
    DomainValue::Object(fields)
}

fn attributes_value(attributes: &PlugAndPlayAttributes) -> DomainValue {
    object([
        (
            "agility",
            DomainValue::Number(f64::from(attributes.agility)),
        ),
        ("brains", DomainValue::Number(f64::from(attributes.brains))),
        ("brawn", DomainValue::Number(f64::from(attributes.brawn))),
        ("wits", DomainValue::Number(f64::from(attributes.wits))),
    ])
}

fn modifier_value(modifier: &PlugAndPlayModifier) -> DomainValue {
    object([
        ("attributes", attributes_value(&modifier.attributes)),
        (
            "explanation",
            DomainValue::String(modifier.explanation.clone()),
        ),
        ("fortune", DomainValue::Number(f64::from(modifier.fortune))),
        ("id", DomainValue::String(modifier.id.clone())),
        ("label", DomainValue::String(modifier.label.clone())),
    ])
}

fn roll_set_value(roll_set: &PlugAndPlayRollSet) -> DomainValue {
    object([
        ("d3", DomainValue::Number(f64::from(roll_set.d3))),
        (
            "d6",
            DomainValue::List(
                roll_set
                    .d6
                    .iter()
                    .map(|value| DomainValue::Number(f64::from(*value)))
                    .collect(),
            ),
        ),
        (
            "entropy_end",
            DomainValue::Number(roll_set.entropy_end as f64),
        ),
        (
            "entropy_start",
            DomainValue::Number(roll_set.entropy_start as f64),
        ),
        ("index", DomainValue::Number(f64::from(roll_set.index))),
    ])
}

fn assignment_value(assignment: &PlugAndPlayRollAssignment) -> DomainValue {
    object([
        (
            "agility",
            DomainValue::Number(f64::from(assignment.agility)),
        ),
        ("brains", DomainValue::Number(f64::from(assignment.brains))),
        ("brawn", DomainValue::Number(f64::from(assignment.brawn))),
        ("wits", DomainValue::Number(f64::from(assignment.wits))),
    ])
}

const fn physical_symbol(attribute: PlugAndPlayPhysicalAttribute) -> &'static str {
    match attribute {
        PlugAndPlayPhysicalAttribute::Agility => "agility",
        PlugAndPlayPhysicalAttribute::Brawn => "brawn",
    }
}

const fn mental_symbol(attribute: PlugAndPlayMentalAttribute) -> &'static str {
    match attribute {
        PlugAndPlayMentalAttribute::Brains => "brains",
        PlugAndPlayMentalAttribute::Wits => "wits",
    }
}

/// Trusted resolver compiled into Weave for the exact Plug-And-Play adapter release.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlugAndPlayResolver;

impl TabletopResolver for PlugAndPlayResolver {
    fn adapter_id(&self) -> &str {
        PLUG_AND_PLAY_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        PLUG_AND_PLAY_ADAPTER_VERSION
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        let definition = value_object("definition", definition)?;
        let request = value_object("request", request)?;
        let mut state = value_object("state", state)?.clone();
        if object_bool("state", &state, "dead")? && operation != "take_damage" {
            return Err(invalid(
                "state.dead",
                "a dead character cannot resolve this operation",
            ));
        }

        let cursor_before = entropy.state().cursor;
        let mut draws = Vec::new();
        let resolved = match operation {
            "attack" => resolve_attack(definition, request, &mut state, entropy, &mut draws)?,
            "chase" => resolve_chase(definition, request, &state, entropy, &mut draws)?,
            "check" => resolve_check(definition, request, &mut state, entropy, &mut draws)?,
            "fortune_test" => resolve_fortune_test(request, &state, entropy, &mut draws)?,
            "group_check" => resolve_group_check(request, entropy, &mut draws)?,
            "initiative" => resolve_initiative(request, entropy, &mut draws)?,
            "take_damage" => resolve_take_damage(request, &mut state)?,
            _ => {
                return Err(invalid(
                    "operation",
                    "operation is not implemented by this resolver",
                ));
            }
        };
        state.insert(
            "last_operation".to_owned(),
            DomainValue::String(operation.to_owned()),
        );

        let mut events = vec![resolved.public_event];
        if operation != "take_damage" {
            events.push(entropy_trace_event(
                operation,
                cursor_before,
                entropy.state().cursor,
                draws,
            ));
        }
        if resolved.resource_changed {
            events.push(resource_changed_event(&state)?);
        }
        Ok(ResolverOutput {
            state: DomainValue::Object(state),
            events,
        })
    }
}

struct OperationResolution {
    public_event: ResolverEvent,
    resource_changed: bool,
}

#[derive(Debug, Clone, Copy)]
enum RollMode {
    Advantage,
    Disadvantage,
    Normal,
}

fn resolve_check(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let attribute = request_attribute(request, "attribute")?;
    let rating = definition_attribute(definition, attribute)?;
    let difficulty = object_integer("request", request, "difficulty")?;
    let modifier = object_integer("request", request, "modifier")?;
    let mode = object_roll_mode("request", request, "roll_mode")?;
    let spend_on_failure = object_bool("request", request, "spend_fortune_on_failure")?;
    let wounded_penalty = physical_wound_penalty(state, attribute)?;

    let mut die = roll_d6(entropy, mode, draws)?;
    let mut total = die + rating + modifier + wounded_penalty;
    let mut outcome = ordinary_check_outcome(die, total, difficulty);
    let mut rerolled = false;
    let mut resource_changed = false;
    if !outcome.success && spend_on_failure {
        let fortune = object_integer("state", state, "fortune_remaining")?;
        if fortune > 0 {
            state.insert(
                "fortune_remaining".to_owned(),
                DomainValue::Number(f64::from(fortune - 1)),
            );
            die = roll_d6(entropy, mode, draws)?;
            total = die + rating + modifier + wounded_penalty;
            outcome = ordinary_check_outcome(die, total, difficulty);
            rerolled = true;
            resource_changed = true;
        }
    }

    Ok(OperationResolution {
        public_event: public_event(
            "check_resolved",
            object([
                (
                    "attribute",
                    DomainValue::Symbol(attribute_symbol_value(attribute).to_owned()),
                ),
                ("critical", DomainValue::Bool(outcome.critical)),
                ("die", DomainValue::Number(f64::from(die))),
                ("fumble", DomainValue::Bool(outcome.fumble)),
                ("outcome", DomainValue::Symbol(outcome.label.to_owned())),
                ("rerolled", DomainValue::Bool(rerolled)),
                ("total", DomainValue::Number(f64::from(total))),
            ]),
        ),
        resource_changed,
    })
}

fn resolve_fortune_test(
    request: &BTreeMap<String, DomainValue>,
    state: &BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let mode = object_roll_mode("request", request, "roll_mode")?;
    let fortune = object_integer("state", state, "fortune_remaining")?;
    let die = roll_d6(entropy, mode, draws)?;
    Ok(OperationResolution {
        public_event: public_event(
            "fortune_test_resolved",
            object([
                ("die", DomainValue::Number(f64::from(die))),
                ("fortune", DomainValue::Number(f64::from(fortune))),
                ("success", DomainValue::Bool(die < fortune)),
            ]),
        ),
        resource_changed: false,
    })
}

fn resolve_attack(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let attribute = request_attribute(request, "attribute")?;
    let rating = definition_attribute(definition, attribute)?;
    let damage_die = object_integer("request", request, "damage_die")?;
    let modifier = object_integer("request", request, "modifier")?;
    let mode = object_roll_mode("request", request, "roll_mode")?;
    let target = object_integer("request", request, "target_survivability")?;
    let weapon_id = object_string("request", request, "weapon_id")?;
    let weapon_kind = object_symbol("request", request, "weapon_kind")?;
    validate_weapon(state, weapon_id, weapon_kind, damage_die)?;

    let die = roll_d6(entropy, mode, draws)?;
    let total = die + rating + rating / 2 + modifier + physical_wound_penalty(state, attribute)?;
    let critical = die == 6;
    let fumble = die == 1;
    let hit = !fumble && (critical || total >= target);
    let mut damage = 0;
    let mut self_damage = 0;
    let mut jammed = false;
    let mut resource_changed = false;

    if hit {
        damage = if weapon_kind == "unarmed" {
            1
        } else if critical {
            damage_die
        } else {
            roll_damage(entropy, damage_die, draws)?
        };
    } else if fumble && weapon_kind == "melee" {
        // The half-damage result is rounded up so odd maximum-damage ratings stay explicit and
        // portable across hosts.
        self_damage = (damage_die + 1) / 2;
        apply_damage(state, self_damage)?;
        resource_changed = true;
    } else if fumble && weapon_kind == "ranged" {
        add_jammed_item(state, weapon_id)?;
        jammed = true;
        resource_changed = true;
    }

    Ok(OperationResolution {
        public_event: public_event(
            "attack_resolved",
            object([
                ("critical", DomainValue::Bool(critical)),
                ("damage", DomainValue::Number(f64::from(damage))),
                ("fumble", DomainValue::Bool(fumble)),
                ("hit", DomainValue::Bool(hit)),
                ("jammed", DomainValue::Bool(jammed)),
                ("self_damage", DomainValue::Number(f64::from(self_damage))),
                ("total", DomainValue::Number(f64::from(total))),
                ("weapon_id", DomainValue::String(weapon_id.to_owned())),
            ]),
        ),
        resource_changed,
    })
}

fn resolve_take_damage(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<OperationResolution, TabletopError> {
    let amount = object_integer("request", request, "amount")?;
    apply_damage(state, amount)?;
    Ok(OperationResolution {
        public_event: public_event(
            "damage_applied",
            object([
                ("amount", DomainValue::Number(f64::from(amount))),
                (
                    "dead",
                    DomainValue::Bool(object_bool("state", state, "dead")?),
                ),
                (
                    "remaining",
                    DomainValue::Number(f64::from(object_integer(
                        "state",
                        state,
                        "survivability_current",
                    )?)),
                ),
                (
                    "wounded",
                    DomainValue::Bool(object_bool("state", state, "wounded")?),
                ),
            ]),
        ),
        resource_changed: true,
    })
}

fn resolve_group_check(
    request: &BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let difficulty = object_integer("request", request, "difficulty")?;
    let participants = object_list("request", request, "participants")?;
    let mut ids = BTreeSet::new();
    let mut results = Vec::with_capacity(participants.len());
    let mut successes = 0;
    for participant in participants {
        let participant = value_object("request.participants", participant)?;
        let id = object_string("request.participants", participant, "id")?;
        validate_bounded_text("request.participants.id", id, 1, 128)?;
        if !ids.insert(id) {
            return Err(invalid(
                "request.participants.id",
                "participant identifiers must be unique",
            ));
        }
        let modifier = object_integer("request.participants", participant, "modifier")?;
        let rating = object_integer("request.participants", participant, "rating")?;
        let mode = object_roll_mode("request.participants", participant, "roll_mode")?;
        let die = roll_d6(entropy, mode, draws)?;
        let total = die + rating + modifier;
        let outcome = ordinary_check_outcome(die, total, difficulty);
        if outcome.success {
            successes += 1;
        }
        results.push(object([
            ("critical", DomainValue::Bool(outcome.critical)),
            ("die", DomainValue::Number(f64::from(die))),
            ("fumble", DomainValue::Bool(outcome.fumble)),
            ("id", DomainValue::String(id.to_owned())),
            ("success", DomainValue::Bool(outcome.success)),
            ("total", DomainValue::Number(f64::from(total))),
        ]));
    }
    let failures = participants.len() as i32 - successes;
    Ok(OperationResolution {
        public_event: public_event(
            "group_check_resolved",
            object([
                ("failures", DomainValue::Number(f64::from(failures))),
                ("results", DomainValue::List(results)),
                ("success", DomainValue::Bool(successes > failures)),
                ("successes", DomainValue::Number(f64::from(successes))),
            ]),
        ),
        resource_changed: false,
    })
}

fn resolve_initiative(
    request: &BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let participants = object_list("request", request, "participants")?;
    let mut participant_ids = BTreeSet::new();
    let mut deck = initiative_deck();
    let mut entries = Vec::with_capacity(participants.len());
    for participant in participants {
        let participant = value_string("request.participants", participant)?;
        validate_bounded_text("request.participants", participant, 1, 128)?;
        if !participant_ids.insert(participant) {
            return Err(invalid(
                "request.participants",
                "participant identifiers must be unique",
            ));
        }
        let index = entropy.draw_bounded(deck.len() as u64)?;
        draws.push(index);
        let index = usize::try_from(index)
            .map_err(|_| invalid("initiative", "card index is out of range"))?;
        let card = deck.remove(index);
        entries.push(InitiativeResult {
            participant: participant.to_owned(),
            card,
        });
    }
    entries.sort_by(|left, right| {
        right
            .card
            .rank
            .cmp(&left.card.rank)
            .then_with(|| suit_strength(right.card.suit).cmp(&suit_strength(left.card.suit)))
            .then_with(|| left.participant.cmp(&right.participant))
    });
    let entries = entries
        .into_iter()
        .map(|entry| {
            object([
                ("label", DomainValue::String(card_label(entry.card))),
                ("participant", DomainValue::String(entry.participant)),
                ("rank", DomainValue::Number(f64::from(entry.card.rank))),
                ("suit", DomainValue::Symbol(entry.card.suit.to_owned())),
            ])
        })
        .collect();
    Ok(OperationResolution {
        public_event: public_event(
            "initiative_ordered",
            object([("entries", DomainValue::List(entries))]),
        ),
        resource_changed: false,
    })
}

fn resolve_chase(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<OperationResolution, TabletopError> {
    let attribute = request_attribute(request, "attribute")?;
    let player_rating = definition_attribute(definition, attribute)?;
    let max_checks = object_integer("request", request, "max_checks")?;
    let opponent_modifier = object_integer("request", request, "opponent_modifier")?;
    let opponent_rating = object_integer("request", request, "opponent_rating")?;
    let opponent_mode = object_roll_mode("request", request, "opponent_roll_mode")?;
    let player_modifier = object_integer("request", request, "player_modifier")?;
    let player_mode = object_roll_mode("request", request, "player_roll_mode")?;
    let player_penalty = physical_wound_penalty(state, attribute)?;
    let mut player_wins = 0;
    let mut opponent_wins = 0;
    let mut rounds = Vec::new();
    let mut decisive_outcome = None;

    for round in 1..=max_checks {
        let player_die = roll_d6(entropy, player_mode, draws)?;
        let opponent_die = roll_d6(entropy, opponent_mode, draws)?;
        let player_total = player_die + player_rating + player_modifier + player_penalty;
        let opponent_total = opponent_die + opponent_rating + opponent_modifier;
        let winner = chase_winner(player_die, player_total, opponent_die, opponent_total);
        match winner {
            "player" => player_wins += 1,
            "opponent" => opponent_wins += 1,
            _ => {}
        }
        rounds.push(object([
            ("opponent_die", DomainValue::Number(f64::from(opponent_die))),
            (
                "opponent_total",
                DomainValue::Number(f64::from(opponent_total)),
            ),
            ("player_die", DomainValue::Number(f64::from(player_die))),
            ("player_total", DomainValue::Number(f64::from(player_total))),
            ("round", DomainValue::Number(f64::from(round))),
            ("winner", DomainValue::Symbol(winner.to_owned())),
        ]));
        if matches!(player_die, 1 | 6) || matches!(opponent_die, 1 | 6) {
            decisive_outcome = Some(match winner {
                "player" => "escaped",
                "opponent" => "caught",
                _ => "stalemate",
            });
            break;
        }
    }
    let outcome = decisive_outcome.unwrap_or_else(|| match player_wins.cmp(&opponent_wins) {
        std::cmp::Ordering::Greater => "escaped",
        std::cmp::Ordering::Less => "caught",
        std::cmp::Ordering::Equal => "stalemate",
    });
    Ok(OperationResolution {
        public_event: public_event(
            "chase_resolved",
            object([
                ("outcome", DomainValue::Symbol(outcome.to_owned())),
                ("rounds", DomainValue::List(rounds)),
            ]),
        ),
        resource_changed: false,
    })
}

struct CheckOutcome {
    success: bool,
    critical: bool,
    fumble: bool,
    label: &'static str,
}

fn ordinary_check_outcome(die: i32, total: i32, difficulty: i32) -> CheckOutcome {
    if die == 6 {
        CheckOutcome {
            success: true,
            critical: true,
            fumble: false,
            label: "critical_success",
        }
    } else if die == 1 {
        CheckOutcome {
            success: false,
            critical: false,
            fumble: true,
            label: "fumble",
        }
    } else if total > difficulty {
        CheckOutcome {
            success: true,
            critical: false,
            fumble: false,
            label: "success",
        }
    } else {
        CheckOutcome {
            success: false,
            critical: false,
            fumble: false,
            label: "failure",
        }
    }
}

fn chase_winner(
    player_die: i32,
    player_total: i32,
    opponent_die: i32,
    opponent_total: i32,
) -> &'static str {
    let player_edge = i32::from(player_die == 6) - i32::from(player_die == 1);
    let opponent_edge = i32::from(opponent_die == 6) - i32::from(opponent_die == 1);
    match player_edge
        .cmp(&opponent_edge)
        .then_with(|| player_total.cmp(&opponent_total))
    {
        std::cmp::Ordering::Greater => "player",
        std::cmp::Ordering::Less => "opponent",
        std::cmp::Ordering::Equal => "tie",
    }
}

fn roll_d6(
    entropy: &mut EntropyStream,
    mode: RollMode,
    draws: &mut Vec<u64>,
) -> Result<i32, TabletopError> {
    let first = entropy.draw_bounded(6)? + 1;
    draws.push(first);
    let selected = match mode {
        RollMode::Normal => first,
        RollMode::Advantage | RollMode::Disadvantage => {
            let second = entropy.draw_bounded(6)? + 1;
            draws.push(second);
            match mode {
                RollMode::Advantage => first.max(second),
                RollMode::Disadvantage => first.min(second),
                RollMode::Normal => first,
            }
        }
    };
    i32::try_from(selected).map_err(|_| invalid("entropy", "die result is out of range"))
}

fn roll_damage(
    entropy: &mut EntropyStream,
    damage_die: i32,
    draws: &mut Vec<u64>,
) -> Result<i32, TabletopError> {
    let upper = u64::try_from(damage_die)
        .map_err(|_| invalid("request.damage_die", "damage die must be positive"))?;
    let result = entropy.draw_bounded(upper)? + 1;
    draws.push(result);
    i32::try_from(result).map_err(|_| invalid("entropy", "damage result is out of range"))
}

fn apply_damage(
    state: &mut BTreeMap<String, DomainValue>,
    amount: i32,
) -> Result<(), TabletopError> {
    if amount < 0 {
        return Err(invalid("damage", "damage cannot be negative"));
    }
    let current = object_integer("state", state, "survivability_current")?;
    let wounded = object_bool("state", state, "wounded")?;
    let mut dead = object_bool("state", state, "dead")?;
    let remaining;
    let next_wounded;
    if amount > 0 && (wounded || current == 0) {
        dead = true;
        remaining = 0;
        next_wounded = true;
    } else {
        remaining = current.saturating_sub(amount).max(0);
        next_wounded = remaining == 0;
    }
    state.insert("dead".to_owned(), DomainValue::Bool(dead));
    state.insert(
        "survivability_current".to_owned(),
        DomainValue::Number(f64::from(remaining)),
    );
    state.insert("wounded".to_owned(), DomainValue::Bool(next_wounded));
    Ok(())
}

fn validate_weapon(
    state: &BTreeMap<String, DomainValue>,
    weapon_id: &str,
    weapon_kind: &str,
    damage_die: i32,
) -> Result<(), TabletopError> {
    match weapon_kind {
        "unarmed" if weapon_id == "unarmed" && damage_die == 1 => Ok(()),
        "unarmed" => Err(invalid(
            "request.weapon_id",
            "unarmed attacks require weapon id `unarmed` and damage one",
        )),
        "melee" | "ranged" => {
            let inventory = object_string_list("state", state, "inventory")?;
            if !inventory.iter().any(|item| item == weapon_id) {
                return Err(invalid(
                    "request.weapon_id",
                    "weapon must be present in inventory",
                ));
            }
            if weapon_kind == "ranged"
                && object_string_list("state", state, "jammed_items")?
                    .iter()
                    .any(|item| item == weapon_id)
            {
                return Err(invalid(
                    "request.weapon_id",
                    "a jammed ranged weapon cannot be used",
                ));
            }
            Ok(())
        }
        _ => Err(invalid(
            "request.weapon_kind",
            "expected melee, ranged, or unarmed",
        )),
    }
}

fn add_jammed_item(
    state: &mut BTreeMap<String, DomainValue>,
    weapon_id: &str,
) -> Result<(), TabletopError> {
    let mut items = object_string_list("state", state, "jammed_items")?;
    if !items.iter().any(|item| item == weapon_id) {
        items.push(weapon_id.to_owned());
        items.sort();
    }
    state.insert(
        "jammed_items".to_owned(),
        DomainValue::List(items.into_iter().map(DomainValue::String).collect()),
    );
    Ok(())
}

fn physical_wound_penalty(
    state: &BTreeMap<String, DomainValue>,
    attribute: PlugAndPlayAttribute,
) -> Result<i32, TabletopError> {
    let physical = matches!(
        attribute,
        PlugAndPlayAttribute::Agility | PlugAndPlayAttribute::Brawn
    );
    Ok(if physical && object_bool("state", state, "wounded")? {
        -5
    } else {
        0
    })
}

fn definition_attribute(
    definition: &BTreeMap<String, DomainValue>,
    attribute: PlugAndPlayAttribute,
) -> Result<i32, TabletopError> {
    let attributes = value_object(
        "definition.effective_attributes",
        object_field("definition", definition, "effective_attributes")?,
    )?;
    object_integer(
        "definition.effective_attributes",
        attributes,
        attribute_symbol_value(attribute),
    )
}

fn request_attribute(
    request: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<PlugAndPlayAttribute, TabletopError> {
    match object_symbol("request", request, field)? {
        "agility" => Ok(PlugAndPlayAttribute::Agility),
        "brains" => Ok(PlugAndPlayAttribute::Brains),
        "brawn" => Ok(PlugAndPlayAttribute::Brawn),
        "wits" => Ok(PlugAndPlayAttribute::Wits),
        _ => Err(invalid(
            "request.attribute",
            "expected a regular character attribute",
        )),
    }
}

const fn attribute_symbol_value(attribute: PlugAndPlayAttribute) -> &'static str {
    match attribute {
        PlugAndPlayAttribute::Agility => "agility",
        PlugAndPlayAttribute::Brains => "brains",
        PlugAndPlayAttribute::Brawn => "brawn",
        PlugAndPlayAttribute::Wits => "wits",
    }
}

fn object_roll_mode(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<RollMode, TabletopError> {
    match object_symbol(path, fields, field)? {
        "advantage" => Ok(RollMode::Advantage),
        "disadvantage" => Ok(RollMode::Disadvantage),
        "normal" => Ok(RollMode::Normal),
        _ => Err(invalid(
            format!("{path}.{field}"),
            "expected normal, advantage, or disadvantage",
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct InitiativeCard {
    rank: u8,
    suit: &'static str,
}

struct InitiativeResult {
    participant: String,
    card: InitiativeCard,
}

fn initiative_deck() -> Vec<InitiativeCard> {
    let mut deck = Vec::with_capacity(54);
    for rank in 2..=14 {
        for suit in ["diamonds", "clubs", "hearts", "spades"] {
            deck.push(InitiativeCard { rank, suit });
        }
    }
    deck.push(InitiativeCard {
        rank: 0,
        suit: "joker",
    });
    deck.push(InitiativeCard {
        rank: 0,
        suit: "joker",
    });
    deck
}

const fn suit_strength(suit: &str) -> u8 {
    match suit.as_bytes() {
        b"spades" => 4,
        b"hearts" => 3,
        b"clubs" => 2,
        b"diamonds" => 1,
        _ => 0,
    }
}

fn card_label(card: InitiativeCard) -> String {
    if card.rank == 0 {
        return "Joker".to_owned();
    }
    let rank = match card.rank {
        11 => "J".to_owned(),
        12 => "Q".to_owned(),
        13 => "K".to_owned(),
        14 => "A".to_owned(),
        value => value.to_string(),
    };
    format!("{rank} {}", card.suit)
}

fn public_event(kind: &str, payload: DomainValue) -> ResolverEvent {
    ResolverEvent {
        kind: kind.to_owned(),
        visibility: EventVisibility::Public,
        payload,
    }
}

fn entropy_trace_event(
    operation: &str,
    cursor_before: u64,
    cursor_after: u64,
    draws: Vec<u64>,
) -> ResolverEvent {
    ResolverEvent {
        kind: "entropy_trace".to_owned(),
        visibility: EventVisibility::HostOnly,
        payload: object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            (
                "draws",
                DomainValue::List(
                    draws
                        .into_iter()
                        .map(|draw| DomainValue::Number(draw as f64))
                        .collect(),
                ),
            ),
            ("operation", DomainValue::String(operation.to_owned())),
        ]),
    }
}

fn resource_changed_event(
    state: &BTreeMap<String, DomainValue>,
) -> Result<ResolverEvent, TabletopError> {
    Ok(ResolverEvent {
        kind: "resource_changed".to_owned(),
        visibility: EventVisibility::Authoring,
        payload: object([
            (
                "dead",
                DomainValue::Bool(object_bool("state", state, "dead")?),
            ),
            (
                "fortune_remaining",
                DomainValue::Number(f64::from(object_integer(
                    "state",
                    state,
                    "fortune_remaining",
                )?)),
            ),
            (
                "survivability_current",
                DomainValue::Number(f64::from(object_integer(
                    "state",
                    state,
                    "survivability_current",
                )?)),
            ),
            (
                "wounded",
                DomainValue::Bool(object_bool("state", state, "wounded")?),
            ),
        ]),
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

fn value_object(
    path: impl Into<String>,
    value: &DomainValue,
) -> Result<&BTreeMap<String, DomainValue>, TabletopError> {
    match value {
        DomainValue::Object(fields) => Ok(fields),
        _ => Err(invalid(path, "expected an object")),
    }
}

fn object_field<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a DomainValue, TabletopError> {
    fields
        .get(field)
        .ok_or_else(|| invalid(format!("{path}.{field}"), "required field is missing"))
}

fn object_integer(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<i32, TabletopError> {
    value_integer(
        &format!("{path}.{field}"),
        object_field(path, fields, field)?,
    )
}

fn value_integer(path: &str, value: &DomainValue) -> Result<i32, TabletopError> {
    let DomainValue::Number(number) = value else {
        return Err(invalid(path, "expected an integer"));
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || *number < f64::from(i32::MIN)
        || *number > f64::from(i32::MAX)
    {
        return Err(invalid(path, "expected a finite 32-bit integer"));
    }
    Ok(*number as i32)
}

fn object_bool(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<bool, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::Bool(value) => Ok(*value),
        _ => Err(invalid(format!("{path}.{field}"), "expected a boolean")),
    }
}

fn object_string<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    value_string(
        format!("{path}.{field}"),
        object_field(path, fields, field)?,
    )
}

fn value_string(path: impl Into<String>, value: &DomainValue) -> Result<&str, TabletopError> {
    match value {
        DomainValue::String(value) => Ok(value),
        _ => Err(invalid(path, "expected a string")),
    }
}

fn object_symbol<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::Symbol(value) => Ok(value),
        _ => Err(invalid(format!("{path}.{field}"), "expected a symbol")),
    }
}

fn object_list<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a [DomainValue], TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::List(values) => Ok(values),
        _ => Err(invalid(format!("{path}.{field}"), "expected a list")),
    }
}

fn object_string_list(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Vec<String>, TabletopError> {
    object_list(path, fields, field)?
        .iter()
        .map(|value| value_string(format!("{path}.{field}"), value).map(str::to_owned))
        .collect()
}

fn validate_bounded_text(
    path: impl Into<String>,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), TabletopError> {
    if !(minimum..=maximum).contains(&value.chars().count()) {
        return Err(invalid(path, "text length is outside allowed bounds"));
    }
    Ok(())
}

fn validate_global_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() > 255
        || value.split('.').count() < 3
        || value
            .split('.')
            .any(|segment| !valid_local_identifier(segment))
    {
        return Err(invalid(path, "expected a namespaced lowercase identifier"));
    }
    Ok(())
}

fn validate_local_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    if !valid_local_identifier(value) {
        return Err(invalid(path, "expected a lowercase identifier"));
    }
    Ok(())
}

fn valid_local_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && value.len() <= 128
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn invalid(path: impl Into<String>, reason: &'static str) -> TabletopError {
    TabletopError::InvalidField {
        path: path.into(),
        reason,
    }
}

/// Build the exact verified manifest used by every Plug-And-Play surface.
#[must_use]
pub fn plug_and_play_manifest() -> AdapterManifest {
    let attributes_type = object_type([
        (
            "agility",
            required(integer(0.0, 12.0), "Effective Agility rating."),
        ),
        (
            "brains",
            required(integer(0.0, 12.0), "Effective Brains rating."),
        ),
        (
            "brawn",
            required(integer(0.0, 12.0), "Effective Brawn rating."),
        ),
        (
            "wits",
            required(integer(0.0, 12.0), "Effective Wits rating."),
        ),
    ]);
    let attribute_adjustments_type = object_type([
        (
            "agility",
            required(integer(-6.0, 6.0), "Agility adjustment."),
        ),
        ("brains", required(integer(-6.0, 6.0), "Brains adjustment.")),
        ("brawn", required(integer(-6.0, 6.0), "Brawn adjustment.")),
        ("wits", required(integer(-6.0, 6.0), "Wits adjustment.")),
    ]);
    let modifier_type = object_type([
        (
            "attributes",
            required(
                named("AttributeAdjustments"),
                "Balanced regular-attribute adjustments.",
            ),
        ),
        (
            "explanation",
            required(
                string(1, 512),
                "Original author-facing adjustment explanation.",
            ),
        ),
        (
            "fortune",
            required(integer(-6.0, 6.0), "Balanced Fortune adjustment."),
        ),
        (
            "id",
            required(string(1, 128), "Stable modifier identifier."),
        ),
        (
            "label",
            required(string(1, 160), "Author-facing modifier label."),
        ),
    ]);
    let roll_set_type = object_type([
        ("d3", required(integer(1.0, 3.0), "Raw Fortune die result.")),
        (
            "d6",
            required(
                list(integer(1.0, 6.0), 4, 4),
                "Four assignable regular-attribute results.",
            ),
        ),
        (
            "entropy_end",
            required(integer(0.0, u32::MAX as f64), "Cursor after this set."),
        ),
        (
            "entropy_start",
            required(integer(0.0, u32::MAX as f64), "Cursor before this set."),
        ),
        ("index", required(integer(0.0, 1.0), "Creation set index.")),
    ]);
    let assignment_type = object_type([
        (
            "agility",
            required(integer(0.0, 3.0), "Agility roll index."),
        ),
        ("brains", required(integer(0.0, 3.0), "Brains roll index.")),
        ("brawn", required(integer(0.0, 3.0), "Brawn roll index.")),
        ("wits", required(integer(0.0, 3.0), "Wits roll index.")),
    ]);
    let group_result_type = object_type([
        (
            "critical",
            required(TypeExpression::Bool, "Natural critical flag."),
        ),
        ("die", required(integer(1.0, 6.0), "Selected d6 result.")),
        (
            "fumble",
            required(TypeExpression::Bool, "Natural fumble flag."),
        ),
        ("id", required(string(1, 128), "Participant identifier.")),
        (
            "success",
            required(TypeExpression::Bool, "Individual outcome."),
        ),
        (
            "total",
            required(integer(-32.0, 64.0), "Resolved individual total."),
        ),
    ]);
    let initiative_entry_type = object_type([
        ("label", required(string(1, 32), "Portable card label.")),
        (
            "participant",
            required(string(1, 128), "Participant identifier."),
        ),
        (
            "rank",
            required(integer(0.0, 14.0), "Numeric rank; zero is a joker."),
        ),
        (
            "suit",
            required(
                symbol(["clubs", "diamonds", "hearts", "joker", "spades"]),
                "Portable suit identity.",
            ),
        ),
    ]);
    let chase_round_type = object_type([
        (
            "opponent_die",
            required(integer(1.0, 6.0), "Opponent selected die."),
        ),
        (
            "opponent_total",
            required(integer(-32.0, 64.0), "Opponent total."),
        ),
        (
            "player_die",
            required(integer(1.0, 6.0), "Player selected die."),
        ),
        (
            "player_total",
            required(integer(-32.0, 64.0), "Player total."),
        ),
        (
            "round",
            required(integer(1.0, 3.0), "One-based chase round."),
        ),
        (
            "winner",
            required(symbol(["opponent", "player", "tie"]), "Round winner."),
        ),
    ]);

    let types = BTreeMap::from([
        ("Assignment".to_owned(), assignment_type),
        (
            "AttributeAdjustments".to_owned(),
            attribute_adjustments_type,
        ),
        ("Attributes".to_owned(), attributes_type),
        ("ChaseRound".to_owned(), chase_round_type),
        ("GroupResult".to_owned(), group_result_type),
        ("InitiativeEntry".to_owned(), initiative_entry_type),
        ("Modifier".to_owned(), modifier_type),
        ("RollSet".to_owned(), roll_set_type),
    ]);

    let definition_type = object_type([
        (
            "age",
            required(integer(1.0, 300.0), "Explicit fictional age."),
        ),
        (
            "assignment",
            optional(
                named("Assignment"),
                "Explicit assignment for rolled creation.",
            ),
        ),
        (
            "base_attributes",
            required(named("Attributes"), "Unmodified accepted ratings."),
        ),
        (
            "base_fortune",
            required(integer(1.0, 3.0), "Unmodified accepted Fortune rating."),
        ),
        (
            "creation_mode",
            required(symbol(["authored", "rolled"]), "Explicit creation mode."),
        ),
        (
            "creation_seed",
            required(string(1, 20), "Exact decimal deterministic seed."),
        ),
        (
            "effective_attributes",
            required(
                named("Attributes"),
                "Ratings after both balanced modifiers.",
            ),
        ),
        (
            "fortune_initial",
            required(integer(0.0, 6.0), "Fortune after balanced modifiers."),
        ),
        (
            "inventory",
            required(list(string(1, 120), 0, 64), "Initial carried item names."),
        ),
        (
            "modifiers",
            required(
                list(named("Modifier"), 2, 2),
                "Exactly two balanced modifiers.",
            ),
        ),
        (
            "name",
            required(string(1, 160), "Explicit fictional display name."),
        ),
        (
            "roll_sets",
            required(
                list(named("RollSet"), 0, 2),
                "Deterministic raw creation options.",
            ),
        ),
        (
            "selected_set",
            optional(integer(0.0, 1.0), "Explicitly accepted rolled set."),
        ),
        (
            "survivability",
            required(integer(0.0, 18.0), "Derived starting Survivability."),
        ),
        (
            "survivability_body",
            required(symbol(["agility", "brawn"]), "Whole physical contribution."),
        ),
        (
            "survivability_mind",
            required(
                symbol(["brains", "wits"]),
                "Rounded-up half mental contribution.",
            ),
        ),
    ]);

    let state_type = object_type([
        (
            "dead",
            required(TypeExpression::Bool, "Whether further play is unavailable."),
        ),
        (
            "fortune_remaining",
            required(integer(0.0, 6.0), "Unspent Fortune points."),
        ),
        (
            "inventory",
            required(list(string(1, 120), 0, 64), "Current carried items."),
        ),
        (
            "jammed_items",
            required(
                list(string(1, 120), 0, 64),
                "Ranged items jammed by a fumble.",
            ),
        ),
        (
            "last_operation",
            required(string(0, 128), "Most recently resolved operation."),
        ),
        (
            "survivability_current",
            required(integer(0.0, 18.0), "Current Survivability."),
        ),
        (
            "wounded",
            required(TypeExpression::Bool, "Wounded state at zero Survivability."),
        ),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: PLUG_AND_PLAY_ADAPTER_ID.to_owned(),
        version: PLUG_AND_PLAY_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "plug_and_play".to_owned(),
        title: "Plug-And-Play compatible adapter".to_owned(),
        compatibility_label: "Compatible with the CC0 Plug-And-Play rules; not endorsed by its creator"
            .to_owned(),
        summary: "An independently implemented, deterministic adapter for quick d6 character creation, checks, Fortune, combat, group action, card initiative, and bounded chases."
            .to_owned(),
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
            capability(TabletopCapability::EquipmentAndAbilities),
            capability(TabletopCapability::Encounters),
        ],
        types,
        definition_type,
        state_type,
        creation_steps: creation_steps(),
        operations: operations(),
        event_types: event_types(),
        visibility: HostVisibilityPolicy::default(),
        migrations: Vec::new(),
        provenance: AdapterProvenance {
            source_class: AdapterSourceClass::VerifiedCc0,
            source_url: PLUG_AND_PLAY_SOURCE_URL.to_owned(),
            exact_artifact: "Plug-And-Play Rules.pdf".to_owned(),
            revision: "PDF metadata 2024-11-27; official release files retrieved 2026-08-24"
                .to_owned(),
            retrieved_on: "2026-08-24".to_owned(),
            sha256: PLUG_AND_PLAY_RULES_SHA256.to_owned(),
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "d6, d3, and d2 terminology".to_owned(),
                "checks, difficulty, criticals, fumbles, advantage, and disadvantage".to_owned(),
                "combat hit, damage, healing boundary, and wounds".to_owned(),
                "character ratings, Fortune, Survivability, balanced modifiers, and inventory"
                    .to_owned(),
                "group checks, card initiative, and bounded chases".to_owned(),
            ],
            exclusions: vec![
                ExcludedMaterial::CommunitySupplements,
                ExcludedMaterial::Logos,
                ExcludedMaterial::TradeDress,
                ExcludedMaterial::Artwork,
                ExcludedMaterial::Layout,
                ExcludedMaterial::UnverifiedAssets,
            ],
            attribution: "Compatibility implementation for Plug-And-Play by Olav Jakobson Digranes, distributed under CC0-1.0 by Distilled Productions Limited. Adapter code, fixtures, prose, and scenarios are independently authored for Weave."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: CC0_1_0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: vec![AdapterSourceArtifact {
                source_url: PLUG_AND_PLAY_SOURCE_URL.to_owned(),
                exact_artifact: "Plug-And-Play Character Sheet.pdf".to_owned(),
                revision: "PDF metadata 2024-11-27; official release file retrieved 2026-08-24"
                    .to_owned(),
                retrieved_on: "2026-08-24".to_owned(),
                sha256: PLUG_AND_PLAY_SHEET_SHA256.to_owned(),
                media_type: "application/pdf".to_owned(),
                purpose: "Verify the official field inventory: name, age, four regular attributes, Fortune, Survivability, two modifiers, inventory, and notes."
                    .to_owned(),
                redistributed: false,
            }],
            notices: vec![
                "The official PDFs are hash-pinned inputs and are not redistributed with Weave."
                    .to_owned(),
                "Factual compatibility naming does not imply sponsorship or endorsement."
                    .to_owned(),
            ],
            compatibility_statement: "This adapter is an independent compatibility implementation for the CC0 Plug-And-Play rules. It does not bundle the official PDFs, branding, artwork, or layout and is not endorsed by the original creator."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "identity".to_owned(),
            title: "Character identity".to_owned(),
            description: "Enter an original fictional name and age; these values are never inferred from the canonical Character profile."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("name", "Name", string(1, 160), true),
                creation_field("age", "Age", integer(1.0, 300.0), true),
            ],
        },
        CreationStep {
            id: "ratings".to_owned(),
            title: "Ratings".to_owned(),
            description: "Choose deterministic rolled creation or explicit ratings, then inspect both roll sets, their seed lineage, and the selected assignment before accepting."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "creation_mode",
                    "Creation mode",
                    symbol(["authored", "rolled"]),
                    true,
                ),
                creation_field("seed", "Seed", string(1, 20), true),
                creation_field("agility", "Agility", integer(1.0, 6.0), false),
                creation_field("brains", "Brains", integer(1.0, 6.0), false),
                creation_field("brawn", "Brawn", integer(1.0, 6.0), false),
                creation_field("wits", "Wits", integer(1.0, 6.0), false),
                creation_field("fortune", "Fortune", integer(1.0, 3.0), false),
            ],
        },
        CreationStep {
            id: "survivability".to_owned(),
            title: "Survivability".to_owned(),
            description: "Choose one physical rating plus one rounded-up half mental rating; the derived value is recomputed and cannot be authored directly."
                .to_owned(),
            required_capability: TabletopCapability::DerivedValues,
            fields: vec![
                creation_field(
                    "survivability_body",
                    "Physical basis",
                    symbol(["agility", "brawn"]),
                    true,
                ),
                creation_field(
                    "survivability_mind",
                    "Mental basis",
                    symbol(["brains", "wits"]),
                    true,
                ),
            ],
        },
        CreationStep {
            id: "details".to_owned(),
            title: "Balanced modifiers and inventory".to_owned(),
            description: "Add exactly two independently described zero-sum modifiers and an explicit bounded inventory."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("modifiers", "Two modifiers", list(named("Modifier"), 2, 2), true),
                creation_field("inventory", "Inventory", list(string(1, 120), 0, 64), true),
            ],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "attack".to_owned(),
            operation(
                "attack",
                "Resolve an attack",
                "Resolve a weapon or unarmed attack against explicit target Survivability, including critical damage, melee self-harm, ranged jams, and wound state.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "attribute",
                        required(attribute_symbol(), "Attack attribute."),
                    ),
                    (
                        "damage_die",
                        required(integer(1.0, 20.0), "Maximum weapon damage."),
                    ),
                    (
                        "modifier",
                        required(integer(-12.0, 12.0), "Situational modifier."),
                    ),
                    (
                        "roll_mode",
                        required(roll_mode_symbol(), "Normal, advantage, or disadvantage."),
                    ),
                    (
                        "target_survivability",
                        required(integer(0.0, 18.0), "Target number met to hit."),
                    ),
                    (
                        "weapon_id",
                        required(string(1, 120), "Inventory item or `unarmed`."),
                    ),
                    (
                        "weapon_kind",
                        required(symbol(["melee", "ranged", "unarmed"]), "Weapon behavior."),
                    ),
                ]),
                ["attack_resolved", "entropy_trace", "resource_changed"],
            ),
        ),
        (
            "chase".to_owned(),
            operation(
                "chase",
                "Resolve a bounded chase",
                "Resolve no more than three opposed checks and stop immediately on a natural critical or fumble.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "attribute",
                        required(attribute_symbol(), "Player chase attribute."),
                    ),
                    (
                        "max_checks",
                        required(integer(1.0, 3.0), "Explicit chase bound."),
                    ),
                    (
                        "opponent_modifier",
                        required(integer(-12.0, 12.0), "Opponent situational modifier."),
                    ),
                    (
                        "opponent_rating",
                        required(integer(0.0, 12.0), "Opponent rating."),
                    ),
                    (
                        "opponent_roll_mode",
                        required(roll_mode_symbol(), "Opponent roll mode."),
                    ),
                    (
                        "player_modifier",
                        required(integer(-12.0, 12.0), "Player situational modifier."),
                    ),
                    (
                        "player_roll_mode",
                        required(roll_mode_symbol(), "Player roll mode."),
                    ),
                ]),
                ["chase_resolved", "entropy_trace"],
            ),
        ),
        (
            "check".to_owned(),
            operation(
                "check",
                "Resolve a check",
                "Roll one d6 or two for advantage/disadvantage, apply the selected rating and explicit difficulty modifier, and optionally spend Fortune after failure.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "attribute",
                        required(attribute_symbol(), "Checked attribute."),
                    ),
                    (
                        "difficulty",
                        required(integer(0.0, 18.0), "Total that must be exceeded."),
                    ),
                    (
                        "modifier",
                        required(integer(-12.0, 12.0), "Situational modifier."),
                    ),
                    (
                        "roll_mode",
                        required(roll_mode_symbol(), "Normal, advantage, or disadvantage."),
                    ),
                    (
                        "spend_fortune_on_failure",
                        required(TypeExpression::Bool, "Explicit reroll policy."),
                    ),
                ]),
                ["check_resolved", "entropy_trace", "resource_changed"],
            ),
        ),
        (
            "fortune_test".to_owned(),
            operation(
                "fortune_test",
                "Resolve a Fortune test",
                "Roll below current Fortune without critical or fumble semantics.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "roll_mode",
                    required(roll_mode_symbol(), "Normal, advantage, or disadvantage."),
                )]),
                ["entropy_trace", "fortune_test_resolved"],
            ),
        ),
        (
            "group_check".to_owned(),
            operation(
                "group_check",
                "Resolve a group check",
                "Resolve each explicit participant independently; the group succeeds only with more successes than failures.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "difficulty",
                        required(integer(0.0, 18.0), "Total each participant must exceed."),
                    ),
                    (
                        "participants",
                        required(
                            list(
                                object_type([
                                    ("id", required(string(1, 128), "Participant identifier.")),
                                    (
                                        "modifier",
                                        required(integer(-12.0, 12.0), "Situational modifier."),
                                    ),
                                    (
                                        "rating",
                                        required(
                                            integer(0.0, 12.0),
                                            "Relevant participant rating.",
                                        ),
                                    ),
                                    (
                                        "roll_mode",
                                        required(roll_mode_symbol(), "Participant roll mode."),
                                    ),
                                ]),
                                1,
                                64,
                            ),
                            "Explicit group participants.",
                        ),
                    ),
                ]),
                ["entropy_trace", "group_check_resolved"],
            ),
        ),
        (
            "initiative".to_owned(),
            operation(
                "initiative",
                "Draw card initiative",
                "Draw without replacement from a portable 54-card representation and order by rank, then Spades, Hearts, Clubs, and Diamonds; jokers have no value.",
                TabletopCapability::Encounters,
                object_type([(
                    "participants",
                    required(
                        list(string(1, 128), 1, 54),
                        "Unique participant identifiers.",
                    ),
                )]),
                ["entropy_trace", "initiative_ordered"],
            ),
        ),
        (
            "take_damage".to_owned(),
            operation(
                "take_damage",
                "Apply damage",
                "Reduce current Survivability, mark the character wounded at zero, and mark further positive damage as fatal.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "amount",
                    required(integer(0.0, 100.0), "Explicit incoming damage."),
                )]),
                ["damage_applied", "resource_changed"],
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    BTreeMap::from([
        (
            "attack_resolved".to_owned(),
            object_type([
                (
                    "critical",
                    required(TypeExpression::Bool, "Natural critical flag."),
                ),
                ("damage", required(integer(0.0, 20.0), "Outgoing damage.")),
                (
                    "fumble",
                    required(TypeExpression::Bool, "Natural fumble flag."),
                ),
                (
                    "hit",
                    required(TypeExpression::Bool, "Whether the target was hit."),
                ),
                (
                    "jammed",
                    required(TypeExpression::Bool, "Whether the ranged item jammed."),
                ),
                (
                    "self_damage",
                    required(integer(0.0, 20.0), "Damage applied to the attacker."),
                ),
                ("total", required(integer(-32.0, 64.0), "Attack total.")),
                (
                    "weapon_id",
                    required(string(1, 120), "Explicit weapon identifier."),
                ),
            ]),
        ),
        (
            "chase_resolved".to_owned(),
            object_type([
                (
                    "outcome",
                    required(symbol(["caught", "escaped", "stalemate"]), "Chase outcome."),
                ),
                (
                    "rounds",
                    required(list(named("ChaseRound"), 1, 3), "Resolved rounds."),
                ),
            ]),
        ),
        (
            "check_resolved".to_owned(),
            object_type([
                (
                    "attribute",
                    required(attribute_symbol(), "Checked attribute."),
                ),
                (
                    "critical",
                    required(TypeExpression::Bool, "Natural critical flag."),
                ),
                ("die", required(integer(1.0, 6.0), "Final selected die.")),
                (
                    "fumble",
                    required(TypeExpression::Bool, "Natural fumble flag."),
                ),
                (
                    "outcome",
                    required(
                        symbol(["critical_success", "failure", "fumble", "success"]),
                        "Final check outcome.",
                    ),
                ),
                (
                    "rerolled",
                    required(
                        TypeExpression::Bool,
                        "Whether Fortune replaced the first result.",
                    ),
                ),
                (
                    "total",
                    required(integer(-32.0, 64.0), "Final check total."),
                ),
            ]),
        ),
        (
            "damage_applied".to_owned(),
            object_type([
                ("amount", required(integer(0.0, 100.0), "Incoming damage.")),
                ("dead", required(TypeExpression::Bool, "Fatal state.")),
                (
                    "remaining",
                    required(integer(0.0, 18.0), "Remaining Survivability."),
                ),
                ("wounded", required(TypeExpression::Bool, "Wounded state.")),
            ]),
        ),
        (
            "entropy_trace".to_owned(),
            object_type([
                (
                    "cursor_after",
                    required(integer(0.0, u32::MAX as f64), "Entropy cursor after."),
                ),
                (
                    "cursor_before",
                    required(integer(0.0, u32::MAX as f64), "Entropy cursor before."),
                ),
                (
                    "draws",
                    required(
                        list(integer(0.0, u32::MAX as f64), 0, 256),
                        "Raw bounded draws.",
                    ),
                ),
                ("operation", required(string(1, 128), "Resolver operation.")),
            ]),
        ),
        (
            "fortune_test_resolved".to_owned(),
            object_type([
                ("die", required(integer(1.0, 6.0), "Selected d6 result.")),
                (
                    "fortune",
                    required(integer(0.0, 6.0), "Current Fortune threshold."),
                ),
                (
                    "success",
                    required(TypeExpression::Bool, "Whether die is below Fortune."),
                ),
            ]),
        ),
        (
            "group_check_resolved".to_owned(),
            object_type([
                (
                    "failures",
                    required(integer(0.0, 64.0), "Individual failures."),
                ),
                (
                    "results",
                    required(
                        list(named("GroupResult"), 1, 64),
                        "Ordered participant results.",
                    ),
                ),
                (
                    "success",
                    required(
                        TypeExpression::Bool,
                        "Whether successes outnumber failures.",
                    ),
                ),
                (
                    "successes",
                    required(integer(0.0, 64.0), "Individual successes."),
                ),
            ]),
        ),
        (
            "initiative_ordered".to_owned(),
            object_type([(
                "entries",
                required(
                    list(named("InitiativeEntry"), 1, 54),
                    "Highest-first initiative order.",
                ),
            )]),
        ),
        (
            "resource_changed".to_owned(),
            object_type([
                ("dead", required(TypeExpression::Bool, "Fatal state.")),
                (
                    "fortune_remaining",
                    required(integer(0.0, 6.0), "Remaining Fortune."),
                ),
                (
                    "survivability_current",
                    required(integer(0.0, 18.0), "Remaining Survivability."),
                ),
                ("wounded", required(TypeExpression::Bool, "Wounded state.")),
            ]),
        ),
    ])
}

fn capability(capability: TabletopCapability) -> TabletopCapabilityDeclaration {
    TabletopCapabilityDeclaration {
        capability,
        version: 1,
    }
}

fn operation<const N: usize>(
    id: &str,
    title: &str,
    description: &str,
    required_capability: TabletopCapability,
    request_type: TypeExpression,
    event_kinds: [&str; N],
) -> ResolverOperationDeclaration {
    ResolverOperationDeclaration {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        required_capability,
        request_type,
        event_kinds: event_kinds.into_iter().map(str::to_owned).collect(),
        consumes_entropy: id != "take_damage",
    }
}

fn creation_field(
    id: &str,
    label: &str,
    value_type: TypeExpression,
    required: bool,
) -> CreationField {
    CreationField {
        id: id.to_owned(),
        label: label.to_owned(),
        description: format!("Explicit adapter-owned {label} input."),
        value_type,
        required,
        authority: CreationFieldAuthority::AdapterOwned,
    }
}

fn attribute_symbol() -> TypeExpression {
    symbol(["agility", "brains", "brawn", "wits"])
}

fn roll_mode_symbol() -> TypeExpression {
    symbol(["advantage", "disadvantage", "normal"])
}

fn required(value_type: TypeExpression, description: &str) -> FieldDeclaration {
    field(value_type, true, description)
}

fn optional(value_type: TypeExpression, description: &str) -> FieldDeclaration {
    field(value_type, false, description)
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

fn list(items: TypeExpression, min_items: usize, max_items: usize) -> TypeExpression {
    TypeExpression::List {
        items: Box::new(items),
        min_items,
        max_items,
    }
}

fn named(name: &str) -> TypeExpression {
    TypeExpression::Named {
        name: name.to_owned(),
    }
}

fn object_type<const N: usize>(fields: [(&str, FieldDeclaration); N]) -> TypeExpression {
    TypeExpression::Object {
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}
