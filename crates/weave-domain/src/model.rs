use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Current serialized domain-module contract version.
pub const DOMAIN_CONTRACT_VERSION: u32 = 1;

/// Current serialized domain data-pack version.
pub const DOMAIN_PACK_FORMAT_VERSION: u32 = 1;

/// A portable, declarative domain-module manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleManifest {
    /// Module contract version.
    pub contract_version: u32,
    /// Data-pack format consumed by this module.
    pub pack_format_version: u32,
    /// Globally stable, dot-separated module identity.
    pub id: String,
    /// Semantic module version.
    pub version: String,
    /// Default source-language namespace.
    pub namespace: String,
    /// Human-readable module title.
    pub title: String,
    /// Concise module purpose.
    pub summary: String,
    /// Module authors or stewards.
    pub authors: Vec<ModuleAuthor>,
    /// SPDX license expression for the manifest and module-authored data.
    pub license: String,
    /// HTTPS location of the applicable license text.
    pub license_url: String,
    /// Compatible Weave releases.
    pub weave_version: String,
    /// Required host capabilities in canonical identifier order.
    pub capabilities: Vec<CapabilityDeclaration>,
    /// Required modules in canonical module-identifier order.
    pub dependencies: Vec<ModuleDependency>,
    /// Reusable named value types.
    pub types: BTreeMap<String, TypeExpression>,
    /// Values exposed to source, compiler, runtime, and host integrations.
    pub exports: BTreeMap<String, ExportDeclaration>,
    /// Machine-readable authorship and source lineage.
    pub provenance: Provenance,
}

/// One module author or steward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleAuthor {
    /// Display name.
    pub name: String,
    /// Optional public HTTPS profile or project page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// One versioned host capability required by a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDeclaration {
    /// Stable capability identifier.
    pub id: String,
    /// Capability contract version.
    pub version: u32,
}

/// One module dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleDependency {
    /// Globally stable module identity.
    pub id: String,
    /// Accepted semantic versions.
    pub version: String,
}

/// Closed, host-independent type language for domain values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum TypeExpression {
    /// Explicit null.
    Null,
    /// Boolean.
    Bool,
    /// Finite number with optional bounds.
    Number {
        /// Require an integral numeric value.
        integer: bool,
        /// Inclusive lower bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        /// Inclusive upper bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    /// UTF-8 text with scalar-length bounds.
    String {
        /// Minimum Unicode-scalar count.
        min_length: usize,
        /// Maximum Unicode-scalar count.
        max_length: usize,
    },
    /// Meaning-bearing atom from a closed, sorted set.
    Symbol {
        /// Allowed symbolic values.
        values: Vec<String>,
    },
    /// Homogeneous, bounded ordered values.
    List {
        /// Element type.
        items: Box<TypeExpression>,
        /// Minimum item count.
        min_items: usize,
        /// Maximum item count.
        max_items: usize,
    },
    /// Closed string-keyed value.
    Object {
        /// Declared fields, sorted by name in serialized output.
        fields: BTreeMap<String, FieldDeclaration>,
    },
    /// Reference to a manifest-local named type.
    Named {
        /// Name in [`ModuleManifest::types`].
        name: String,
    },
}

/// One field in a closed object type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldDeclaration {
    /// Field value type.
    #[serde(rename = "type")]
    pub value_type: TypeExpression,
    /// Whether the field must be present.
    pub required: bool,
    /// Author-facing field purpose.
    pub description: String,
}

/// One typed value exported by a module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportDeclaration {
    /// Export value type.
    #[serde(rename = "type")]
    pub value_type: TypeExpression,
    /// Whether a selected pack must provide an initial value.
    pub required: bool,
    /// Runtime mutability and ownership boundary.
    pub source: ExportSource,
    /// Author-facing export purpose.
    pub description: String,
}

/// Ownership of an exported value after compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportSource {
    /// Immutable compiled data supplied by packs.
    Pack,
    /// Versioned mutable state initialized from pack data and stored separately.
    State,
}

/// One portable domain data pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainPack {
    /// Data-pack format version.
    pub pack_format_version: u32,
    /// Stable pack identifier within its module.
    pub id: String,
    /// Semantic pack version.
    pub version: String,
    /// Human-readable pack title.
    pub title: String,
    /// Exact module identity plus an accepted semantic-version range.
    pub module: ModuleRequirement,
    /// Required packs in canonical identity order.
    pub dependencies: Vec<PackDependency>,
    /// Initial values keyed by module export name.
    pub values: BTreeMap<String, DomainValue>,
    /// Machine-readable source lineage for the data.
    pub provenance: Provenance,
}

/// Module requirement declared by a pack or project activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleRequirement {
    /// Globally stable module identity.
    pub id: String,
    /// Accepted semantic versions.
    pub version: String,
}

/// One pack dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackDependency {
    /// Owning module identity.
    pub module_id: String,
    /// Pack identifier within that module.
    pub pack_id: String,
    /// Accepted semantic versions.
    pub version: String,
}

/// Unambiguous portable value representation shared by RON and JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "value",
    deny_unknown_fields
)]
pub enum DomainValue {
    /// Explicit null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// UTF-8 text.
    String(String),
    /// Meaning-bearing atom.
    Symbol(String),
    /// Ordered values.
    List(Vec<DomainValue>),
    /// Closed string-keyed value.
    Object(BTreeMap<String, DomainValue>),
}

/// Machine-readable source lineage shared by manifests and packs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Public sources sorted by source identifier.
    pub sources: Vec<ProvenanceSource>,
    /// Documented transformations sorted by transformation identifier.
    pub transformations: Vec<ProvenanceTransformation>,
    /// Manifest or pack paths mapped to sorted source/transformation identifiers.
    pub claims: BTreeMap<String, Vec<String>>,
}

/// One public or original source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSource {
    /// Stable source identifier within this artifact.
    pub id: String,
    /// Source category.
    pub kind: ProvenanceKind,
    /// Public HTTPS source location.
    pub url: String,
    /// Immutable commit, tag, release, date, or authored revision.
    pub revision: String,
    /// SHA-256 for externally acquired source content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// SPDX license expression.
    pub license: String,
    /// HTTPS location of the applicable license text.
    pub license_url: String,
    /// Attribution retained with redistributed data.
    pub attribution: String,
    /// Whether the artifact changes or derives from this source.
    pub modified: bool,
}

/// Source category used by provenance validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    /// Original material authored for the artifact.
    Original,
    /// Unmodified compatibly licensed public source.
    PublicSource,
    /// Material derived from one or more declared sources.
    Derived,
}

/// One reviewable transformation from declared inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceTransformation {
    /// Stable transformation identifier.
    pub id: String,
    /// Sorted source or prior-transformation identifiers.
    pub inputs: Vec<String>,
    /// Human-readable, independently written transformation summary.
    pub description: String,
}
