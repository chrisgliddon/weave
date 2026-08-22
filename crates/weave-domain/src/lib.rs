//! Versioned, host-independent contracts for declarative Weave domain modules.
//!
//! This crate owns portable manifest, type, value, pack, compatibility, and provenance models.
//! It does not parse Weave source, execute stories, load native code, or depend on an editor or
//! game engine.

mod catalog;
mod model;
mod registry;
mod validation;

use std::fmt;

use schemars::JsonSchema;
use semver::Version;
use serde::Serialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};

pub use catalog::{DomainCatalog, ResolvedDomainGraph, ResolvedDomainModule};
pub use model::{
    CapabilityDeclaration, DOMAIN_CONTRACT_VERSION, DOMAIN_PACK_FORMAT_VERSION, DomainOverride,
    DomainPack, DomainValue, EntityCollectionDeclaration, ExportDeclaration, ExportSource,
    FieldDeclaration, ModuleAuthor, ModuleAuthoring, ModuleDependency, ModuleManifest,
    ModuleRequirement, PackDependency, Provenance, ProvenanceKind, ProvenanceSource,
    ProvenanceTransformation, TypeExpression,
};
pub use registry::{
    DOMAIN_LOCK_FILE_NAME, DOMAIN_LOCK_FORMAT_VERSION, DOMAIN_PROJECT_FILE_NAME,
    DOMAIN_PROJECT_FORMAT_VERSION, DOMAIN_REGISTRY_INDEX_VERSION, DomainArtifactCoordinate,
    DomainLock, DomainManifestSummary, DomainPackSummary, DomainPackageError, DomainProjectConfig,
    DomainRegistry, DomainRegistryIndex, DomainRegistrySnapshot, InstalledDomainArtifacts,
    InstalledDomainPack, LoadedDomainProject, LockedModule, LockedModuleDependency, LockedPack,
    LockedPackDependency, domain_lock_schema, domain_project_path, domain_project_schema,
    domain_registry_index_schema, load_adjacent_domain_project, load_domain_project,
};
pub use validation::{
    apply_authored_overrides, resolve_module_order, validate_effective_values, validate_manifest,
    validate_pack, validate_provenance,
};

const MANIFEST_SCHEMA_ID: &str = "urn:weave:schema:domain-module-manifest:1";
const PACK_SCHEMA_ID: &str = "urn:weave:schema:domain-pack:1";

/// Redaction-safe domain contract failure.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// An artifact semantic version is malformed. Its raw value is deliberately not echoed.
    #[error("invalid domain artifact version at `{path}`")]
    InvalidArtifactVersion {
        /// Stable artifact field path.
        path: &'static str,
    },
    /// A source activation semantic-version requirement is malformed.
    #[error("invalid domain activation requirement at `{path}`")]
    InvalidActivationRequirement {
        /// Stable activation field path.
        path: &'static str,
    },
    /// The selected module identity is absent from the explicit catalog.
    #[error("selected domain module is not installed")]
    ModuleNotInstalled,
    /// No installed release satisfies the source module requirement.
    #[error("no installed domain module version satisfies the activation")]
    ModuleVersionNotInstalled,
    /// The selected pack identity is absent from the explicit catalog.
    #[error("selected domain pack is not installed")]
    PackNotInstalled,
    /// No installed pack release satisfies the source pack requirement.
    #[error("no installed domain pack version satisfies the activation")]
    PackVersionNotInstalled,
    /// A catalog contains the same module identity and version twice.
    #[error("duplicate domain module artifact makes the selection ambiguous")]
    DuplicateManifestArtifact,
    /// A catalog contains the same module, pack, and version coordinate twice.
    #[error("duplicate domain pack artifact makes the selection ambiguous")]
    DuplicatePackArtifact,
    /// Module contract version is unsupported.
    #[error("unsupported domain contract version {found}; expected {expected}")]
    UnsupportedContract {
        /// Encountered version.
        found: u32,
        /// Supported version.
        expected: u32,
    },
    /// Pack format version is unsupported.
    #[error("unsupported domain pack format {found}; expected {expected}")]
    UnsupportedPackFormat {
        /// Encountered version.
        found: u32,
        /// Supported version.
        expected: u32,
    },
    /// One field failed closed validation. The value is deliberately not echoed.
    #[error("invalid domain field `{path}`: {reason}")]
    InvalidField {
        /// Stable field path.
        path: String,
        /// Redaction-safe explanation.
        reason: &'static str,
    },
    /// Module does not support the current Weave release.
    #[error("domain module `{module_id}` is incompatible with Weave {current}")]
    IncompatibleWeave {
        /// Validated public module identifier.
        module_id: String,
        /// Current Weave release.
        current: Version,
    },
    /// Host does not implement the exact required capability contract.
    #[error("unsupported domain capability `{id}` version {version}")]
    UnsupportedCapability {
        /// Validated public capability identifier.
        id: String,
        /// Requested capability version.
        version: u32,
    },
    /// Pack's module requirement excludes the selected module.
    #[error("domain pack `{pack_id}` is incompatible with module `{module_id}`")]
    IncompatibleModule {
        /// Validated public pack identifier.
        pack_id: String,
        /// Validated public module identifier.
        module_id: String,
    },
    /// A pack supplied a value the manifest does not export.
    #[error("domain pack provides unknown export `{name}`")]
    UnknownExport {
        /// Validated public export name.
        name: String,
    },
    /// A required export has no initial pack value.
    #[error("domain pack is missing required export `{name}`")]
    MissingExport {
        /// Validated public export name.
        name: String,
    },
    /// A named type reference is missing.
    #[error("domain manifest refers to unknown type `{name}`")]
    UnknownType {
        /// Validated public type name.
        name: String,
    },
    /// Named types form a recursive cycle, which v1 forbids.
    #[error("domain type `{name}` is recursive; contract v1 requires finite value trees")]
    RecursiveType {
        /// Validated public type name.
        name: String,
    },
    /// Value shape does not match its declaration.
    #[error("domain value `{path}` has type {found}; expected {expected}")]
    TypeMismatch {
        /// Stable value path.
        path: String,
        /// Redaction-safe expected category.
        expected: &'static str,
        /// Redaction-safe encountered category.
        found: &'static str,
    },
    /// Two selected module releases share one identity.
    #[error("multiple active releases of domain module `{module_id}`")]
    DuplicateModule {
        /// Validated public module identifier.
        module_id: String,
    },
    /// Two modules claim the same default namespace.
    #[error("domain namespace `{namespace}` is claimed by both `{first}` and `{second}`")]
    NamespaceCollision {
        /// Validated public namespace.
        namespace: String,
        /// First validated module identifier.
        first: String,
        /// Second validated module identifier.
        second: String,
    },
    /// A selected module dependency is absent.
    #[error("domain module `{module_id}` requires missing module `{dependency_id}`")]
    MissingDependency {
        /// Validated public module identifier.
        module_id: String,
        /// Validated public dependency identifier.
        dependency_id: String,
    },
    /// A selected dependency release does not satisfy the declared range.
    #[error("domain module `{module_id}` has incompatible dependency `{dependency_id}`")]
    IncompatibleDependency {
        /// Validated public module identifier.
        module_id: String,
        /// Validated public dependency identifier.
        dependency_id: String,
    },
    /// Module dependencies contain a cycle.
    #[error("domain module dependencies contain a cycle")]
    DependencyCycle,
    /// JSON input failed without echoing source content.
    #[error("invalid domain JSON at line {line}, column {column}")]
    InvalidJson {
        /// One-based line.
        line: usize,
        /// One-based column.
        column: usize,
    },
    /// RON input failed without echoing source content.
    #[error("invalid domain RON")]
    InvalidRon,
    /// Stable serialization failed.
    #[error("could not serialize domain contract data")]
    Serialize,
}

impl ModuleManifest {
    /// Parse strict JSON without duplicate keys.
    pub fn from_json(source: &str) -> Result<Self, DomainError> {
        parse_strict_json(source)
    }

    /// Parse strict RON.
    pub fn from_ron(source: &str) -> Result<Self, DomainError> {
        ron::from_str(source).map_err(|_| DomainError::InvalidRon)
    }

    /// Serialize canonical human-readable JSON.
    pub fn to_json(&self) -> Result<String, DomainError> {
        pretty_json(self)
    }

    /// Serialize canonical human-readable RON.
    pub fn to_ron(&self) -> Result<String, DomainError> {
        pretty_ron(self)
    }
}

impl DomainPack {
    /// Parse strict JSON without duplicate keys.
    pub fn from_json(source: &str) -> Result<Self, DomainError> {
        parse_strict_json(source)
    }

    /// Parse strict RON.
    pub fn from_ron(source: &str) -> Result<Self, DomainError> {
        ron::from_str(source).map_err(|_| DomainError::InvalidRon)
    }

    /// Serialize canonical human-readable JSON.
    pub fn to_json(&self) -> Result<String, DomainError> {
        pretty_json(self)
    }

    /// Serialize canonical human-readable RON.
    pub fn to_ron(&self) -> Result<String, DomainError> {
        pretty_ron(self)
    }
}

/// Generate the canonical module-manifest JSON Schema.
pub fn module_manifest_schema() -> Result<String, DomainError> {
    schema::<ModuleManifest>(
        MANIFEST_SCHEMA_ID,
        "Weave Domain Module Manifest v1",
        "contract_version",
        DOMAIN_CONTRACT_VERSION,
    )
}

/// Generate the canonical domain-pack JSON Schema.
pub fn domain_pack_schema() -> Result<String, DomainError> {
    schema::<DomainPack>(
        PACK_SCHEMA_ID,
        "Weave Domain Pack v1",
        "pack_format_version",
        DOMAIN_PACK_FORMAT_VERSION,
    )
}

/// Parse a strict JSON value without accepting duplicate object keys.
///
/// Domain-specific companion crates use this boundary so every versioned artifact has the same
/// fail-closed JSON behavior as manifests and packs.
pub fn parse_strict_json<T>(source: &str) -> Result<T, DomainError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str::<UniqueJson>(source).map_err(json_error)?;
    serde_json::from_str(source).map_err(json_error)
}

/// Serialize one portable artifact as stable, human-readable JSON.
pub fn to_pretty_json(value: &impl Serialize) -> Result<String, DomainError> {
    pretty_json(value)
}

/// Serialize one portable artifact as stable, human-readable RON.
pub fn to_pretty_ron(value: &impl Serialize) -> Result<String, DomainError> {
    pretty_ron(value)
}

fn json_error(error: serde_json::Error) -> DomainError {
    DomainError::InvalidJson {
        line: error.line(),
        column: error.column(),
    }
}

fn pretty_json(value: &impl Serialize) -> Result<String, DomainError> {
    let mut output = serde_json::to_string_pretty(value).map_err(|_| DomainError::Serialize)?;
    output.push('\n');
    Ok(output)
}

fn pretty_ron(value: &impl Serialize) -> Result<String, DomainError> {
    let config = ron::ser::PrettyConfig::new()
        .new_line("\n")
        .indentor("    ")
        .struct_names(false)
        .enumerate_arrays(false)
        .compact_arrays(false)
        .compact_maps(false);
    let mut output =
        ron::ser::to_string_pretty(value, config).map_err(|_| DomainError::Serialize)?;
    output.push('\n');
    Ok(output)
}

fn schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version_property: &str,
    version: u32,
) -> Result<String, DomainError> {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| DomainError::Serialize)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".into(), serde_json::Value::String(id.into()));
        root.insert("title".into(), serde_json::Value::String(title.into()));
        root.insert(
            "x-weave-domain-contract-version".into(),
            serde_json::Value::from(DOMAIN_CONTRACT_VERSION),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut(version_property))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".into(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    pretty_json(&value)
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                sort_json_keys(value);
            }
            object.sort_keys();
        }
        _ => {}
    }
}

struct UniqueJson;

impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = UniqueJson;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_string<E>(self, _: String) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<UniqueJson>()?.is_some() {}
        Ok(UniqueJson)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = std::collections::BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate object key"));
            }
            let _ = map.next_value::<UniqueJson>()?;
        }
        Ok(UniqueJson)
    }
}
