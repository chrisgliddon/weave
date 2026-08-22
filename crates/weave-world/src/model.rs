use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, ProvenanceSource};

use crate::ResolvedWorld;

/// Current version of the deterministic World composition-plan and receipt contract.
pub const WORLD_COMPOSITION_FORMAT_VERSION: u32 = 1;

/// Current version shared by compact and full portable World exports.
pub const WORLD_EXPORT_FORMAT_VERSION: u32 = 1;

/// A reviewed plan that composes exact World packs into one ordinary output pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldCompositionPlan {
    /// Serialized plan version.
    pub format_version: u32,
    /// Identity and presentation of the generated pack.
    pub output: WorldOutputPack,
    /// Public deterministic entropy used only by layers with `seeded` selection.
    pub random_seed: u64,
    /// Canonically ordered broad, regional, then ecosystem layers.
    pub layers: Vec<WorldLayerSpec>,
    /// Original authorship and license record for the composition decisions.
    pub provenance: ProvenanceSource,
}

/// Coordinate and title assigned to a composed pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldOutputPack {
    /// Stable pack identifier.
    pub id: String,
    /// Semantic pack version.
    pub version: String,
    /// Human-readable title.
    pub title: String,
}

/// One deterministic layer in a World composition plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldLayerSpec {
    /// Stable plan-local layer identifier.
    pub id: String,
    /// Semantic scope used to enforce broad-to-specific order.
    pub role: WorldLayerRole,
    /// Exact candidate coordinates in canonical order.
    pub candidates: Vec<WorldPackSelector>,
    /// How one candidate is selected.
    pub selection: WorldLayerSelection,
    /// Sorted, non-overlapping export paths contributed by the selected candidate.
    pub include: Vec<String>,
    /// Explicit behavior when an included leaf already has a value.
    pub conflict: WorldConflictPolicy,
}

/// One exact World pack candidate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldPackSelector {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Exact semantic version without a range operator.
    pub version: String,
}

/// Semantic layer scopes, ordered from broadest to most specific.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorldLayerRole {
    /// Broad reference context.
    Broad,
    /// Regional refinement.
    Regional,
    /// Ecosystem or biome refinement.
    Ecosystem,
}

/// Candidate-selection behavior for one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorldLayerSelection {
    /// Require exactly one candidate.
    Exact,
    /// Select one sorted candidate with the stable SHA-256 chooser and plan seed.
    Seeded,
}

/// Conflict behavior applied independently to every selected leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorldConflictPolicy {
    /// Reject any overlap with an earlier layer.
    Reject,
    /// Preserve the earlier value and record the ignored incoming layer.
    Keep,
    /// Make the later, more specific layer authoritative.
    Replace,
}

/// Machine-readable proof of one deterministic composition run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldCompositionReceipt {
    /// Serialized receipt version.
    pub format_version: u32,
    /// Exact generated pack coordinate.
    pub output: WorldOutputPack,
    /// Public seed used by candidate selection.
    pub random_seed: u64,
    /// Selected layers in the exact application order.
    pub layers: Vec<AppliedWorldLayer>,
    /// Leaf-level additions, confirmations, replacements, and preserved conflicts.
    pub differences: Vec<WorldLayerDifference>,
}

/// One selected and applied layer retained in a composition receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppliedWorldLayer {
    /// Stable plan-local layer identifier.
    pub id: String,
    /// Semantic scope.
    pub role: WorldLayerRole,
    /// Candidate-selection behavior.
    pub selection: WorldLayerSelection,
    /// Exact sorted candidate set retained so seeded selection can be independently reproduced.
    pub candidates: Vec<WorldPackSelector>,
    /// Zero-based selected index in the plan's sorted candidate list.
    pub selected_candidate: usize,
    /// Exact selected pack coordinate.
    pub pack: WorldPackSelector,
    /// Sorted plan paths contributed by this layer.
    pub include: Vec<String>,
    /// Conflict policy used for this layer.
    pub conflict: WorldConflictPolicy,
}

/// One deterministic leaf decision made while composing layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldLayerDifference {
    /// Dot-separated export path.
    pub path: String,
    /// Incoming layer making this decision.
    pub incoming_layer_id: String,
    /// Earlier authoritative layer when the path overlapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_layer_id: Option<String>,
    /// Result of applying the explicit conflict policy.
    pub resolution: WorldLayerResolution,
}

/// Result of one leaf-level composition decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorldLayerResolution {
    /// First value supplied for this path.
    Added,
    /// Later layer supplied the same value and became the authoritative source.
    Confirmed,
    /// Later layer replaced a different earlier value.
    Replaced,
    /// Earlier value remained authoritative.
    Kept,
}

/// Full portable World projection with authoring and composition lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FullWorldExport {
    /// Serialized export version.
    pub format_version: u32,
    /// Exact module identity.
    pub module_id: String,
    /// Exact module version.
    pub module_version: String,
    /// Exact selected composed pack identity.
    pub pack_id: String,
    /// Exact selected composed pack version.
    pub pack_version: String,
    /// Deterministic composition proof and layer differences.
    pub composition: WorldCompositionReceipt,
    /// Canonically ordered source-authored paths applied after composition.
    pub authored_override_paths: Vec<Vec<String>>,
    /// Resolved rules, places, environment, climate, and value origins.
    pub world: ResolvedWorld,
}

/// Compact runtime World projection with all authoring provenance removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompactWorldExport {
    /// Serialized export version.
    pub format_version: u32,
    /// Exact module identity.
    pub module_id: String,
    /// Exact module version.
    pub module_version: String,
    /// Exact selected composed pack identity.
    pub pack_id: String,
    /// Exact selected composed pack version.
    pub pack_version: String,
    /// Optional typed world rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<DomainValue>,
    /// Root place identifiers in deterministic order.
    pub roots: Vec<String>,
    /// Compact resolved places keyed by stable identifier.
    pub places: BTreeMap<String, CompactWorldPlace>,
}

/// One resolved place without source or authoring lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompactWorldPlace {
    /// Stable source-facing identifier.
    pub id: String,
    /// Human-editable display name.
    pub name: String,
    /// Broad place kind.
    pub kind: String,
    /// Deterministic sibling order.
    pub order: i64,
    /// Optional parent identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Symmetric related-place identifiers.
    pub related_place_ids: Vec<String>,
    /// Effective environmental fields.
    pub environment: BTreeMap<String, DomainValue>,
    /// Effective climate fields.
    pub climate: BTreeMap<String, DomainValue>,
    /// Optional typed authored attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<DomainValue>,
}
