use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use weave_core::ir::{
    PatternCollectionIr, PatternDrawMethodIr, PatternElementIr, PatternSystemIr, SpreadIr,
    ValueLiteral,
};

use crate::{
    DataPatternSystem, DrawMethod, PATTERN_MODEL_VERSION, PatternDefinition, PatternElement,
    PatternError, PatternValue, ReversalPolicy, SpreadDefinition,
};

/// Current schema version for a community pattern package.
pub const COMMUNITY_PACKAGE_FORMAT_VERSION: u32 = 1;

/// Current schema version for a generated community registry index.
pub const COMMUNITY_REGISTRY_INDEX_VERSION: u32 = 1;

const PACKAGE_SCHEMA_ID: &str = "urn:weave:schema:community-pattern-package:1";
const INSTALLED_PACKAGE_NAME: &str = "package.weave-pattern.json";
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ELEMENTS: usize = 4_096;
const MAX_FIELDS: usize = 64;
const MAX_SPREADS: usize = 64;
const MAX_POSITIONS: usize = 128;
const MAX_TEXT_BYTES: usize = 65_536;

/// One portable, data-only community pattern package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommunityPackage {
    /// Package-file schema version.
    pub schema_version: u32,
    /// Runtime pattern-model version used by the data.
    pub pattern_model_version: u32,
    /// Discovery, compatibility, licensing, and provenance metadata.
    pub metadata: PackageMetadata,
    /// Executable-through-data pattern definition. Packages cannot provide code.
    pub pattern: PackagePattern,
}

/// Human and machine-readable package metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    /// Stable Weave identifier used by stories, registries, and installed paths.
    pub id: String,
    /// Semantic package version.
    pub version: String,
    /// Human-readable title.
    pub title: String,
    /// Short discovery summary.
    pub summary: String,
    /// People or groups responsible for the package.
    pub authors: Vec<PackageAuthor>,
    /// SPDX license expression.
    pub license: String,
    /// HTTPS page containing the applicable license text.
    pub license_url: String,
    /// Attribution retained when the package is redistributed.
    pub attribution: String,
    /// Version requirement for compatible Weave releases.
    pub weave_version: String,
    /// Public source and revision from which the package was produced.
    pub source: PackageSource,
    /// Lowercase discovery tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Author-declared content notices used during discovery and moderation.
    #[serde(default)]
    pub content_warnings: Vec<String>,
}

/// One package author or steward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageAuthor {
    /// Display name.
    pub name: String,
    /// Optional public HTTPS profile or project page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Auditable public source metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageSource {
    /// Public HTTPS source location.
    pub url: String,
    /// Immutable tag, commit, release, or source revision.
    pub revision: String,
    /// Optional SHA-256 of the reviewed upstream source artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Whether the packaged data differs from that source.
    pub modified: bool,
}

/// One generic pattern definition carried by a package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackagePattern {
    /// Human-readable pattern-system name.
    pub name: String,
    /// Default selection algorithm. Community packages cannot add algorithms.
    pub draw_method: PackageDrawMethod,
    /// Whether one spread may repeat an element.
    pub allow_duplicates: bool,
    /// Whether elements with reversed fields may be reversed.
    pub reversals: bool,
    /// Source-ordered semantic elements.
    pub elements: Vec<PackageElement>,
    /// Named position sequences.
    #[serde(default)]
    pub spreads: Vec<PackageSpread>,
}

/// Data-only draw methods available to community packages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PackageDrawMethod {
    /// Select every eligible element with equal probability.
    Uniform,
    /// Use one positive numeric semantic field as a relative weight.
    Weighted {
        /// Weight field present on every element.
        field: String,
    },
}

/// One immutable semantic element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageElement {
    /// Stable identity, canonicalized from the element's `name` field.
    pub id: String,
    /// Upright scalar semantic fields.
    pub fields: BTreeMap<String, PackageValue>,
    /// Fields overlaid when the element is reversed.
    #[serde(default)]
    pub reversed_fields: BTreeMap<String, PackageValue>,
}

/// One named spread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageSpread {
    /// Stable spread identifier.
    pub id: String,
    /// Ordered semantic positions.
    pub positions: Vec<String>,
}

/// Ergonomic scalar value syntax for package JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PackageValue {
    /// Meaning-bearing atom encoded as `{ "symbol": "value" }`.
    Symbol(PackageSymbol),
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// Display string.
    String(String),
    /// Null.
    Null,
}

/// Explicit semantic symbol used by [`PackageValue`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageSymbol {
    /// Symbol value.
    pub symbol: String,
}

/// A parsed installed-package selector such as `ember_omens@^1.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRequirement {
    /// Stable package identifier.
    pub id: String,
    /// Accepted semantic versions.
    pub version: VersionReq,
}

impl FromStr for PackageRequirement {
    type Err = PackageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((id, requirement)) = value.split_once('@') else {
            return Err(PackageError::InvalidRequirement);
        };
        if !valid_identifier(id) || requirement.is_empty() || requirement.contains('@') {
            return Err(PackageError::InvalidRequirement);
        }
        let version =
            VersionReq::parse(requirement).map_err(|_| PackageError::InvalidRequirement)?;
        Ok(Self {
            id: id.to_owned(),
            version,
        })
    }
}

impl fmt::Display for PackageRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.id, self.version)
    }
}

/// One locally installed package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    /// Package identity.
    pub id: String,
    /// Installed semantic version.
    pub version: Version,
    /// Canonical package file.
    pub path: PathBuf,
    /// SHA-256 of the canonical package file.
    pub sha256: String,
}

/// One publish-ready artifact and its integrity sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedPackage {
    /// Canonical package artifact.
    pub artifact: PathBuf,
    /// Adjacent SHA-256 sidecar.
    pub checksum: PathBuf,
    /// Artifact SHA-256.
    pub sha256: String,
}

/// Portable local or hosted registry listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegistryIndex {
    /// Registry index schema version.
    pub schema_version: u32,
    /// Deterministically sorted packages.
    pub packages: Vec<PackageSummary>,
}

impl RegistryIndex {
    /// Serialize a stable, human-readable registry index.
    pub fn to_json(&self) -> Result<String, PackageError> {
        pretty_json(self)
    }
}

/// Discovery-safe package metadata without executable hooks or local paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageSummary {
    /// Stable package identity.
    pub id: String,
    /// Semantic package version.
    pub version: String,
    /// Human-readable title.
    pub title: String,
    /// Short description.
    pub summary: String,
    /// Author display names.
    pub authors: Vec<String>,
    /// SPDX license expression.
    pub license: String,
    /// Discovery tags.
    pub tags: Vec<String>,
    /// SHA-256 of the canonical artifact.
    pub sha256: String,
    /// Slash-separated path relative to the registry root.
    pub artifact: String,
}

/// Structured package, registry, or integrity failure.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// File operation failed.
    #[error("could not access `{path}`: {source}")]
    Io {
        /// Target path.
        path: PathBuf,
        /// Operating-system failure.
        #[source]
        source: io::Error,
    },
    /// Input exceeded the bounded parser size.
    #[error("community pattern package exceeds the {max_bytes}-byte size limit")]
    TooLarge {
        /// Maximum accepted bytes.
        max_bytes: u64,
    },
    /// JSON did not match the closed package schema.
    #[error("invalid community pattern package JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// JSON serialization failed.
    #[error("could not serialize community pattern data: {0}")]
    Serialize(#[source] serde_json::Error),
    /// Package schema version is unsupported.
    #[error("unsupported community package format {found}; expected {expected}")]
    UnsupportedFormat {
        /// Encountered version.
        found: u32,
        /// Supported version.
        expected: u32,
    },
    /// Runtime pattern data version is unsupported.
    #[error("unsupported package pattern model {found}; expected {expected}")]
    UnsupportedPatternModel {
        /// Encountered version.
        found: u32,
        /// Supported version.
        expected: u32,
    },
    /// One metadata or pattern field is invalid. Values are deliberately not echoed.
    #[error("invalid community package field `{field}`: {reason}")]
    InvalidField {
        /// Stable field path.
        field: String,
        /// Redaction-safe explanation.
        reason: &'static str,
    },
    /// Package semantic version is malformed.
    #[error("invalid semantic version in community package metadata")]
    InvalidVersion,
    /// Package Weave requirement is malformed.
    #[error("invalid Weave version requirement in community package metadata")]
    InvalidWeaveRequirement,
    /// Package does not support the running Weave release.
    #[error("community package is incompatible with Weave {current}")]
    IncompatibleWeave {
        /// Running release.
        current: Version,
    },
    /// Story/registry selector is malformed.
    #[error("invalid package requirement; expected `package_id@version_requirement`")]
    InvalidRequirement,
    /// Runtime definition validation failed.
    #[error("invalid community pattern data: {0}")]
    Pattern(#[from] PatternError),
    /// An integrity sidecar was malformed or did not match.
    #[error("community package checksum verification failed")]
    Checksum,
    /// A different artifact already occupies the versioned destination.
    #[error("refusing to overwrite a different artifact at `{0}`")]
    Conflict(PathBuf),
    /// No installed release satisfies the selector.
    #[error("no installed community package satisfies `{0}`")]
    NoMatchingPackage(PackageRequirement),
    /// Registry layout and package metadata disagree.
    #[error("community pattern registry entry is inconsistent")]
    InvalidRegistryEntry,
}

/// Validate package metadata, compatibility, limits, and executable data.
pub fn validate_package(package: &CommunityPackage) -> Result<(), PackageError> {
    if package.schema_version != COMMUNITY_PACKAGE_FORMAT_VERSION {
        return Err(PackageError::UnsupportedFormat {
            found: package.schema_version,
            expected: COMMUNITY_PACKAGE_FORMAT_VERSION,
        });
    }
    if package.pattern_model_version != PATTERN_MODEL_VERSION {
        return Err(PackageError::UnsupportedPatternModel {
            found: package.pattern_model_version,
            expected: PATTERN_MODEL_VERSION,
        });
    }
    validate_metadata(&package.metadata)?;
    validate_pattern(&package.metadata.id, &package.pattern)?;
    DataPatternSystem::new(build_definition(package))?;
    Ok(())
}

/// Read, integrity-check, parse, and validate one package file.
pub fn load_package(path: impl AsRef<Path>) -> Result<CommunityPackage, PackageError> {
    load_package_file(path.as_ref(), false)
}

fn load_package_file(
    path: &Path,
    require_checksum: bool,
) -> Result<CommunityPackage, PackageError> {
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    if metadata.len() > MAX_PACKAGE_BYTES {
        return Err(PackageError::TooLarge {
            max_bytes: MAX_PACKAGE_BYTES,
        });
    }
    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    if require_checksum && !checksum_path(path).is_file() {
        return Err(PackageError::Checksum);
    }
    verify_checksum_sidecar(path, &bytes)?;
    serde_json::from_slice::<UniqueJson>(&bytes).map_err(PackageError::InvalidJson)?;
    let package =
        serde_json::from_slice::<CommunityPackage>(&bytes).map_err(PackageError::InvalidJson)?;
    validate_package(&package)?;
    Ok(package)
}

struct UniqueJson;

impl<'de> Deserialize<'de> for UniqueJson {
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
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate object key"));
            }
            let _ = map.next_value::<UniqueJson>()?;
        }
        Ok(UniqueJson)
    }
}

/// Validate and stage a deterministic artifact plus SHA-256 sidecar for publication.
pub fn publish_package(
    source: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
) -> Result<PublishedPackage, PackageError> {
    let package = load_package(source)?;
    let version = package.version()?;
    let bytes = package.canonical_json()?.into_bytes();
    let output_directory = output_directory.as_ref();
    fs::create_dir_all(output_directory).map_err(|source| io_error(output_directory, source))?;
    let artifact = output_directory.join(format!(
        "{}-{}.weave-pattern.json",
        package.metadata.id, version
    ));
    write_new_or_same(&artifact, &bytes)?;
    let sha256 = sha256_hex(&bytes);
    let checksum = checksum_path(&artifact);
    let checksum_body = format!(
        "{}  {}\n",
        sha256,
        artifact
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid_field("artifact", "file name is not UTF-8"))?
    );
    write_new_or_same(&checksum, checksum_body.as_bytes())?;
    Ok(PublishedPackage {
        artifact,
        checksum,
        sha256,
    })
}

impl CommunityPackage {
    /// Parsed semantic package version.
    pub fn version(&self) -> Result<Version, PackageError> {
        Version::parse(&self.metadata.version).map_err(|_| PackageError::InvalidVersion)
    }

    /// Convert validated package data to the host-independent runtime definition.
    pub fn pattern_definition(&self) -> Result<PatternDefinition, PackageError> {
        validate_package(self)?;
        Ok(build_definition(self))
    }

    /// Lower validated package data into ordinary story IR.
    pub fn pattern_ir(&self) -> Result<PatternSystemIr, PackageError> {
        validate_package(self)?;
        let elements = self
            .pattern
            .elements
            .iter()
            .map(|element| {
                let mut fields = element
                    .fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.to_ir()))
                    .collect::<BTreeMap<_, _>>();
                if let Some(value) = element.reversed_fields.get("meaning") {
                    fields.insert("reversed_meaning".to_owned(), value.to_ir());
                }
                PatternElementIr { fields }
            })
            .collect();
        let spreads = self
            .pattern
            .spreads
            .iter()
            .map(|spread| {
                (
                    spread.id.clone(),
                    SpreadIr {
                        positions: spread.positions.clone(),
                    },
                )
            })
            .collect();
        Ok(PatternSystemIr {
            builtin: None,
            collections: BTreeMap::from([(
                "elements".to_owned(),
                PatternCollectionIr { elements },
            )]),
            spreads,
            draw_method: self.pattern.draw_method.to_ir(),
            allow_duplicates: self.pattern.allow_duplicates,
            reversals: self.pattern.reversals,
        })
    }

    /// Stable, pretty JSON used by publication and installation.
    pub fn canonical_json(&self) -> Result<String, PackageError> {
        validate_package(self)?;
        pretty_json(self)
    }
}

impl PackageValue {
    fn to_pattern(&self) -> PatternValue {
        match self {
            Self::Symbol(value) => PatternValue::Symbol(value.symbol.clone()),
            Self::Bool(value) => PatternValue::Bool(*value),
            Self::Number(value) => PatternValue::Number(*value),
            Self::String(value) => PatternValue::String(value.clone()),
            Self::Null => PatternValue::Null,
        }
    }

    fn to_ir(&self) -> ValueLiteral {
        match self {
            Self::Symbol(value) => ValueLiteral::Symbol(value.symbol.clone()),
            Self::Bool(value) => ValueLiteral::Bool(*value),
            Self::Number(value) => ValueLiteral::Number(*value),
            Self::String(value) => ValueLiteral::String(value.clone()),
            Self::Null => ValueLiteral::Null,
        }
    }
}

impl PackageDrawMethod {
    fn to_pattern(&self) -> DrawMethod {
        match self {
            Self::Uniform => DrawMethod::Uniform,
            Self::Weighted { field } => DrawMethod::Weighted {
                field: field.clone(),
            },
        }
    }

    fn to_ir(&self) -> PatternDrawMethodIr {
        match self {
            Self::Uniform => PatternDrawMethodIr::Uniform,
            Self::Weighted { field } => PatternDrawMethodIr::WeightedBy {
                field: field.clone(),
            },
        }
    }
}

/// A deterministic, version-resolving directory registry.
#[derive(Debug, Clone)]
pub struct PackageRegistry {
    root: PathBuf,
}

impl PackageRegistry {
    /// Open a registry root. It is created only by a write operation.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Registry root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Install a validated package under `id/version` without overwriting other data.
    pub fn install(&self, source: impl AsRef<Path>) -> Result<InstalledPackage, PackageError> {
        let package = load_package(source)?;
        let version = package.version()?;
        let directory = self
            .root
            .join(&package.metadata.id)
            .join(version.to_string());
        fs::create_dir_all(&directory).map_err(|source| io_error(&directory, source))?;
        let path = directory.join(INSTALLED_PACKAGE_NAME);
        let bytes = package.canonical_json()?.into_bytes();
        write_new_or_same(&path, &bytes)?;
        let sha256 = sha256_hex(&bytes);
        let checksum = checksum_path(&path);
        let checksum_body = format!("{sha256}  {INSTALLED_PACKAGE_NAME}\n");
        write_new_or_same(&checksum, checksum_body.as_bytes())?;
        Ok(InstalledPackage {
            id: package.metadata.id,
            version,
            path,
            sha256,
        })
    }

    /// Resolve the greatest installed semantic version satisfying a selector.
    pub fn resolve(
        &self,
        requirement: &PackageRequirement,
    ) -> Result<CommunityPackage, PackageError> {
        let directory = self.root.join(&requirement.id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PackageError::NoMatchingPackage(requirement.clone()));
            }
            Err(source) => return Err(io_error(&directory, source)),
        };
        let mut versions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| io_error(&directory, source))?;
            if !entry
                .file_type()
                .map_err(|source| io_error(entry.path(), source))?
                .is_dir()
            {
                continue;
            }
            let Some(value) = entry.file_name().to_str().map(str::to_owned) else {
                return Err(PackageError::InvalidRegistryEntry);
            };
            if let Ok(version) = Version::parse(&value)
                && requirement.version.matches(&version)
            {
                versions.push(version);
            }
        }
        versions.sort_by(|left, right| right.cmp(left));
        if let Some(version) = versions.into_iter().next() {
            let path = directory
                .join(version.to_string())
                .join(INSTALLED_PACKAGE_NAME);
            let package = load_package_file(&path, true)?;
            if package.metadata.id != requirement.id || package.version()? != version {
                return Err(PackageError::InvalidRegistryEntry);
            }
            return Ok(package);
        }
        Err(PackageError::NoMatchingPackage(requirement.clone()))
    }

    /// Validate and list every installed package in deterministic order.
    pub fn discover(&self) -> Result<Vec<PackageSummary>, PackageError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut summaries = Vec::new();
        for id_entry in fs::read_dir(&self.root).map_err(|source| io_error(&self.root, source))? {
            let id_entry = id_entry.map_err(|source| io_error(&self.root, source))?;
            if !id_entry
                .file_type()
                .map_err(|source| io_error(id_entry.path(), source))?
                .is_dir()
            {
                continue;
            }
            let Some(id) = id_entry.file_name().to_str().map(str::to_owned) else {
                return Err(PackageError::InvalidRegistryEntry);
            };
            if !valid_identifier(&id) {
                return Err(PackageError::InvalidRegistryEntry);
            }
            for version_entry in
                fs::read_dir(id_entry.path()).map_err(|source| io_error(id_entry.path(), source))?
            {
                let version_entry =
                    version_entry.map_err(|source| io_error(id_entry.path(), source))?;
                if !version_entry
                    .file_type()
                    .map_err(|source| io_error(version_entry.path(), source))?
                    .is_dir()
                {
                    continue;
                }
                let Some(version_text) = version_entry.file_name().to_str().map(str::to_owned)
                else {
                    return Err(PackageError::InvalidRegistryEntry);
                };
                let version = Version::parse(&version_text)
                    .map_err(|_| PackageError::InvalidRegistryEntry)?;
                let path = version_entry.path().join(INSTALLED_PACKAGE_NAME);
                let package = load_package_file(&path, true)?;
                if package.metadata.id != id || package.version()? != version {
                    return Err(PackageError::InvalidRegistryEntry);
                }
                let bytes = package.canonical_json()?.into_bytes();
                summaries.push(package.summary(
                    sha256_hex(&bytes),
                    format!("{id}/{version}/{INSTALLED_PACKAGE_NAME}"),
                ));
            }
        }
        summaries.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.version.cmp(&right.version))
        });
        Ok(summaries)
    }

    /// Build the portable discovery index for this registry.
    pub fn index(&self) -> Result<RegistryIndex, PackageError> {
        Ok(RegistryIndex {
            schema_version: COMMUNITY_REGISTRY_INDEX_VERSION,
            packages: self.discover()?,
        })
    }
}

impl CommunityPackage {
    fn summary(&self, sha256: String, artifact: String) -> PackageSummary {
        PackageSummary {
            id: self.metadata.id.clone(),
            version: self.metadata.version.clone(),
            title: self.metadata.title.clone(),
            summary: self.metadata.summary.clone(),
            authors: self
                .metadata
                .authors
                .iter()
                .map(|author| author.name.clone())
                .collect(),
            license: self.metadata.license.clone(),
            tags: self.metadata.tags.clone(),
            sha256,
            artifact,
        }
    }
}

/// Generate the canonical JSON Schema for community package format 1.
pub fn community_package_schema() -> Result<String, PackageError> {
    let schema = schemars::schema_for!(CommunityPackage);
    let mut value = serde_json::to_value(schema).map_err(PackageError::Serialize)?;
    if let Some(root) = value.as_object_mut() {
        root.insert(
            "$id".to_owned(),
            serde_json::Value::String(PACKAGE_SCHEMA_ID.to_owned()),
        );
        root.insert(
            "title".to_owned(),
            serde_json::Value::String("Weave Community Pattern Package v1".to_owned()),
        );
        root.insert(
            "x-weave-community-package-version".to_owned(),
            serde_json::Value::from(COMMUNITY_PACKAGE_FORMAT_VERSION),
        );
        set_property_const(root, "schema_version", COMMUNITY_PACKAGE_FORMAT_VERSION);
        set_property_const(root, "pattern_model_version", PATTERN_MODEL_VERSION);
    }
    sort_json_keys(&mut value);
    let mut output = serde_json::to_string_pretty(&value).map_err(PackageError::Serialize)?;
    output.push('\n');
    Ok(output)
}

fn validate_metadata(metadata: &PackageMetadata) -> Result<(), PackageError> {
    if !valid_identifier(&metadata.id) {
        return Err(invalid_field(
            "metadata.id",
            "expected an ASCII Weave identifier",
        ));
    }
    let version = Version::parse(&metadata.version).map_err(|_| PackageError::InvalidVersion)?;
    if version.build.as_str().contains('/') {
        return Err(PackageError::InvalidVersion);
    }
    let requirement = VersionReq::parse(&metadata.weave_version)
        .map_err(|_| PackageError::InvalidWeaveRequirement)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| PackageError::InvalidWeaveRequirement)?;
    if !requirement.matches(&current) {
        return Err(PackageError::IncompatibleWeave { current });
    }
    validate_text("metadata.title", &metadata.title, 160)?;
    validate_text("metadata.summary", &metadata.summary, 1_024)?;
    validate_text("metadata.attribution", &metadata.attribution, 4_096)?;
    if metadata.authors.is_empty() || metadata.authors.len() > 32 {
        return Err(invalid_field(
            "metadata.authors",
            "expected between 1 and 32 authors",
        ));
    }
    for (index, author) in metadata.authors.iter().enumerate() {
        validate_text(
            &format!("metadata.authors[{index}].name"),
            &author.name,
            160,
        )?;
        if let Some(url) = &author.url {
            validate_https_url(&format!("metadata.authors[{index}].url"), url)?;
        }
    }
    validate_spdx_expression(&metadata.license)?;
    validate_https_url("metadata.license_url", &metadata.license_url)?;
    validate_https_url("metadata.source.url", &metadata.source.url)?;
    validate_text("metadata.source.revision", &metadata.source.revision, 256)?;
    if let Some(checksum) = &metadata.source.sha256
        && !valid_sha256(checksum)
    {
        return Err(invalid_field(
            "metadata.source.sha256",
            "expected 64 hexadecimal characters",
        ));
    }
    if metadata.tags.len() > 32 {
        return Err(invalid_field(
            "metadata.tags",
            "at most 32 tags are allowed",
        ));
    }
    let mut tags = BTreeSet::new();
    for tag in &metadata.tags {
        if tag.is_empty()
            || tag.len() > 48
            || !tag.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
            || !tags.insert(tag)
        {
            return Err(invalid_field(
                "metadata.tags",
                "tags must be unique lowercase words separated by hyphens",
            ));
        }
    }
    if metadata.content_warnings.len() > 32 {
        return Err(invalid_field(
            "metadata.content_warnings",
            "at most 32 content warnings are allowed",
        ));
    }
    for warning in &metadata.content_warnings {
        validate_text("metadata.content_warnings", warning, 256)?;
    }
    Ok(())
}

fn validate_pattern(id: &str, pattern: &PackagePattern) -> Result<(), PackageError> {
    validate_text("pattern.name", &pattern.name, 160)?;
    if pattern.elements.is_empty() || pattern.elements.len() > MAX_ELEMENTS {
        return Err(invalid_field(
            "pattern.elements",
            "expected between 1 and 4096 elements",
        ));
    }
    if pattern.spreads.len() > MAX_SPREADS {
        return Err(invalid_field(
            "pattern.spreads",
            "at most 64 spreads are allowed",
        ));
    }
    let mut elements = BTreeSet::new();
    for (index, element) in pattern.elements.iter().enumerate() {
        if !valid_identifier(&element.id) || !elements.insert(&element.id) {
            return Err(invalid_field(
                "pattern.elements[].id",
                "element identifiers must be unique Weave identifiers",
            ));
        }
        if element.fields.is_empty() || element.fields.len() > MAX_FIELDS {
            return Err(invalid_field(
                "pattern.elements[].fields",
                "expected between 1 and 64 fields",
            ));
        }
        if element.reversed_fields.len() > MAX_FIELDS {
            return Err(invalid_field(
                "pattern.elements[].reversed_fields",
                "at most 64 reversed fields are allowed",
            ));
        }
        for (field, value) in element.fields.iter().chain(&element.reversed_fields) {
            if !valid_identifier(field) {
                return Err(invalid_field(
                    "pattern.elements[].fields",
                    "field names must be Weave identifiers",
                ));
            }
            validate_value("pattern.elements[].fields", value)?;
        }
        if element.fields.contains_key("reversed_meaning") {
            return Err(invalid_field(
                "pattern.elements[].fields.reversed_meaning",
                "use `reversed_fields.meaning` instead",
            ));
        }
        if element
            .reversed_fields
            .keys()
            .any(|field| field != "meaning")
        {
            return Err(invalid_field(
                "pattern.elements[].reversed_fields",
                "format 1 supports only a reversed `meaning` field",
            ));
        }
        let Some(PackageValue::String(name) | PackageValue::Symbol(PackageSymbol { symbol: name })) =
            element.fields.get("name")
        else {
            return Err(invalid_field(
                "pattern.elements[].fields.name",
                "every element requires a string or symbol name",
            ));
        };
        if !element.fields.contains_key("meaning") {
            return Err(invalid_field(
                "pattern.elements[].fields.meaning",
                "every element requires a meaning",
            ));
        }
        if element.id != canonical_element_id(name, index) {
            return Err(invalid_field(
                "pattern.elements[].id",
                "identifier must be the canonical lowercase form of the name",
            ));
        }
    }
    if pattern.reversals
        && !pattern
            .elements
            .iter()
            .any(|element| !element.reversed_fields.is_empty())
    {
        return Err(invalid_field(
            "pattern.reversals",
            "reversals require at least one reversed meaning",
        ));
    }
    let mut spreads = BTreeSet::new();
    for spread in &pattern.spreads {
        if !valid_identifier(&spread.id) || !spreads.insert(&spread.id) {
            return Err(invalid_field(
                "pattern.spreads[].id",
                "spread identifiers must be unique Weave identifiers",
            ));
        }
        if spread.positions.is_empty() || spread.positions.len() > MAX_POSITIONS {
            return Err(invalid_field(
                "pattern.spreads[].positions",
                "expected between 1 and 128 positions",
            ));
        }
        let mut positions = BTreeSet::new();
        for position in &spread.positions {
            if !valid_identifier(position) || !positions.insert(position) {
                return Err(invalid_field(
                    "pattern.spreads[].positions",
                    "positions must be unique Weave identifiers",
                ));
            }
        }
    }
    if let PackageDrawMethod::Weighted { field } = &pattern.draw_method
        && !valid_identifier(field)
    {
        return Err(invalid_field(
            "pattern.draw_method.field",
            "weight field must be a Weave identifier",
        ));
    }
    if contains_sensitive(id) {
        return Err(invalid_field(
            "metadata.id",
            "sensitive values are forbidden",
        ));
    }
    Ok(())
}

fn validate_value(field: &str, value: &PackageValue) -> Result<(), PackageError> {
    match value {
        PackageValue::Symbol(symbol) => validate_text(field, &symbol.symbol, MAX_TEXT_BYTES),
        PackageValue::String(value) => validate_text(field, value, MAX_TEXT_BYTES),
        PackageValue::Number(value) if !value.is_finite() => {
            Err(invalid_field(field, "numbers must be finite"))
        }
        PackageValue::Bool(_) | PackageValue::Number(_) | PackageValue::Null => Ok(()),
    }
}

fn validate_text(field: &str, value: &str, maximum: usize) -> Result<(), PackageError> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(invalid_field(
            field,
            "text is empty or exceeds its size limit",
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(invalid_field(field, "control characters are forbidden"));
    }
    if contains_sensitive(value) {
        return Err(invalid_field(field, "sensitive values are forbidden"));
    }
    Ok(())
}

fn validate_https_url(field: &str, value: &str) -> Result<(), PackageError> {
    validate_text(field, value, 2_048)?;
    let url = Url::parse(value).map_err(|_| invalid_field(field, "expected a public HTTPS URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid_field(
            field,
            "expected a public HTTPS URL without credentials",
        ));
    }
    Ok(())
}

fn validate_spdx_expression(value: &str) -> Result<(), PackageError> {
    validate_text("metadata.license", value, 256)?;
    let mut tokens = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'(' => {
                tokens.push(SpdxToken::Left);
                index += 1;
            }
            b')' => {
                tokens.push(SpdxToken::Right);
                index += 1;
            }
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'(' | b')')
                {
                    index += 1;
                }
                let token = &value[start..index];
                let parsed = match token {
                    "AND" => SpdxToken::And,
                    "OR" => SpdxToken::Or,
                    "WITH" => SpdxToken::With,
                    _ if token.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
                    }) =>
                    {
                        SpdxToken::Atom
                    }
                    _ => {
                        return Err(invalid_field(
                            "metadata.license",
                            "expected SPDX expression syntax",
                        ));
                    }
                };
                tokens.push(parsed);
            }
        }
    }
    let mut parser = SpdxParser {
        tokens: &tokens,
        at: 0,
    };
    if !parser.parse_or() || parser.at != tokens.len() {
        return Err(invalid_field(
            "metadata.license",
            "expected SPDX expression syntax",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpdxToken {
    Atom,
    And,
    Or,
    With,
    Left,
    Right,
}

struct SpdxParser<'a> {
    tokens: &'a [SpdxToken],
    at: usize,
}

impl SpdxParser<'_> {
    fn parse_or(&mut self) -> bool {
        if !self.parse_and() {
            return false;
        }
        while self.take(SpdxToken::Or) {
            if !self.parse_and() {
                return false;
            }
        }
        true
    }

    fn parse_and(&mut self) -> bool {
        if !self.parse_primary() {
            return false;
        }
        while self.take(SpdxToken::And) {
            if !self.parse_primary() {
                return false;
            }
        }
        true
    }

    fn parse_primary(&mut self) -> bool {
        if self.take(SpdxToken::Left) {
            return self.parse_or() && self.take(SpdxToken::Right);
        }
        if !self.take(SpdxToken::Atom) {
            return false;
        }
        !self.take(SpdxToken::With) || self.take(SpdxToken::Atom)
    }

    fn take(&mut self, token: SpdxToken) -> bool {
        if self.tokens.get(self.at) == Some(&token) {
            self.at += 1;
            true
        } else {
            false
        }
    }
}

fn build_definition(package: &CommunityPackage) -> PatternDefinition {
    PatternDefinition {
        version: PATTERN_MODEL_VERSION,
        id: package.metadata.id.clone(),
        name: package.pattern.name.clone(),
        elements: package
            .pattern
            .elements
            .iter()
            .map(|element| PatternElement {
                id: element.id.clone(),
                fields: element
                    .fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.to_pattern()))
                    .collect(),
                reversed_fields: element
                    .reversed_fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.to_pattern()))
                    .collect(),
                reversible: !element.reversed_fields.is_empty(),
            })
            .collect(),
        spreads: package
            .pattern
            .spreads
            .iter()
            .map(|spread| {
                (
                    spread.id.clone(),
                    SpreadDefinition {
                        name: spread.id.clone(),
                        positions: spread.positions.clone(),
                    },
                )
            })
            .collect(),
        default_method: package.pattern.draw_method.to_pattern(),
        allow_duplicates: package.pattern.allow_duplicates,
        reversal_policy: if package.pattern.reversals {
            ReversalPolicy::Half
        } else {
            ReversalPolicy::Never
        },
    }
}

fn canonical_element_id(name: &str, index: usize) -> String {
    let mut id = String::new();
    let mut underscore = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
            underscore = false;
        } else if !id.is_empty() && !underscore {
            id.push('_');
            underscore = true;
        }
    }
    while id.ends_with('_') {
        id.pop();
    }
    if id.is_empty() || id.starts_with(|character: char| character.is_ascii_digit()) {
        format!("element_{index}_{id}")
    } else {
        id
    }
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn contains_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    if lower.contains("github_pat_") || lower.contains("ghp_") {
        return true;
    }
    value
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
        .any(|token| {
            (token.starts_with("sk-") && token.len() >= 19)
                || (token.starts_with("AKIA")
                    && token.len() == 20
                    && token
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric()))
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn checksum_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".sha256");
    PathBuf::from(value)
}

fn verify_checksum_sidecar(path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
    let checksum = checksum_path(path);
    let text = match fs::read_to_string(&checksum) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&checksum, source)),
    };
    if text.len() > 512 {
        return Err(PackageError::Checksum);
    }
    let expected = text
        .split_ascii_whitespace()
        .next()
        .filter(|value| valid_sha256(value))
        .ok_or(PackageError::Checksum)?;
    if !expected.eq_ignore_ascii_case(&sha256_hex(bytes)) {
        return Err(PackageError::Checksum);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn write_new_or_same(path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
    if path.exists() {
        let existing = fs::read(path).map_err(|source| io_error(path, source))?;
        return if existing == bytes {
            Ok(())
        } else {
            Err(PackageError::Conflict(path.to_path_buf()))
        };
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| io_error(parent, source))?;
    temporary
        .write_all(bytes)
        .map_err(|source| io_error(temporary.path(), source))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| io_error(temporary.path(), source))?;
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path).map_err(|source| io_error(path, source))?;
            if existing == bytes {
                Ok(())
            } else {
                Err(PackageError::Conflict(path.to_path_buf()))
            }
        }
        Err(error) => Err(io_error(path, error.error)),
    }
}

fn pretty_json(value: &impl Serialize) -> Result<String, PackageError> {
    let mut output = serde_json::to_string_pretty(value).map_err(PackageError::Serialize)?;
    output.push('\n');
    Ok(output)
}

fn set_property_const(
    root: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: u32,
) {
    if let Some(property) = root
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|properties| properties.get_mut(name))
        .and_then(serde_json::Value::as_object_mut)
    {
        property.insert("const".to_owned(), serde_json::Value::from(value));
    }
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

fn invalid_field(field: impl Into<String>, reason: &'static str) -> PackageError {
    PackageError::InvalidField {
        field: field.into(),
        reason,
    }
}

fn io_error(path: impl Into<PathBuf>, source: io::Error) -> PackageError {
    PackageError::Io {
        path: path.into(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    const SAMPLE: &str =
        include_str!("../../../patterns/community/ember-omens/package.weave-pattern.json");

    fn sample() -> CommunityPackage {
        serde_json::from_str(SAMPLE).expect("sample package JSON")
    }

    #[test]
    fn sample_is_valid_and_lowers_without_executable_extensions() {
        let package = sample();
        validate_package(&package).expect("valid sample");
        let definition = package.pattern_definition().expect("definition");
        let ir = package.pattern_ir().expect("pattern IR");

        assert_eq!(definition.id, "ember_omens");
        assert_eq!(definition.elements.len(), 4);
        assert_eq!(ir.collections["elements"].elements.len(), 4);
        assert!(ir.builtin.is_none());
        assert_eq!(package.canonical_json().expect("canonical JSON"), SAMPLE);
    }

    #[test]
    fn invalid_versions_and_sensitive_values_fail_closed() {
        let mut package = sample();
        package.schema_version += 1;
        assert!(matches!(
            validate_package(&package),
            Err(PackageError::UnsupportedFormat { .. })
        ));

        let mut package = sample();
        package.metadata.weave_version = ">=99".to_owned();
        assert!(matches!(
            validate_package(&package),
            Err(PackageError::IncompatibleWeave { .. })
        ));

        let mut package = sample();
        package.metadata.attribution = ["github", "_pat_", "synthetic_value"].concat();
        assert!(matches!(
            validate_package(&package),
            Err(PackageError::InvalidField { .. })
        ));
    }

    #[test]
    fn duplicate_json_keys_are_rejected_before_deserialization() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("duplicate.json");
        let duplicate = SAMPLE.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1,\n  \"schema_version\": 1,",
            1,
        );
        fs::write(&path, duplicate).expect("write duplicate JSON");

        assert!(matches!(
            load_package(path),
            Err(PackageError::InvalidJson(_))
        ));
    }

    #[test]
    fn registry_install_resolve_discover_and_publish_are_deterministic() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source.json");
        fs::write(&source, SAMPLE).expect("write sample");
        let registry = PackageRegistry::new(directory.path().join("registry"));

        let installed = registry.install(&source).expect("install package");
        let requirement = "ember_omens@^1.0"
            .parse::<PackageRequirement>()
            .expect("requirement");
        let resolved = registry.resolve(&requirement).expect("resolve package");
        let index = registry.index().expect("build index");
        let published =
            publish_package(&source, directory.path().join("publish")).expect("publish package");

        assert_eq!(installed.sha256.len(), 64);
        assert_eq!(resolved.metadata.version, "1.0.0");
        assert_eq!(index.packages.len(), 1);
        assert_eq!(index.packages[0].sha256, installed.sha256);
        assert!(published.artifact.is_file());
        assert!(published.checksum.is_file());
        registry.install(&source).expect("idempotent install");

        fs::remove_file(checksum_path(&installed.path)).expect("remove registry checksum");
        assert!(matches!(
            registry.resolve(&requirement),
            Err(PackageError::Checksum)
        ));
    }

    #[test]
    fn tampered_published_artifact_is_rejected() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source.json");
        fs::write(&source, SAMPLE).expect("write sample");
        let published =
            publish_package(&source, directory.path().join("publish")).expect("publish package");
        fs::write(&published.artifact, "{}\n").expect("tamper artifact");

        assert!(matches!(
            load_package(&published.artifact),
            Err(PackageError::Checksum)
        ));
    }

    #[test]
    fn generated_schema_is_stable_and_versioned() {
        let schema = community_package_schema().expect("schema");
        let expected = include_str!("../../../schemas/community-pattern-package-v1.schema.json");
        assert_eq!(schema, expected);
        let value: serde_json::Value = serde_json::from_str(&schema).expect("schema JSON");
        assert_eq!(value["$id"], PACKAGE_SCHEMA_ID);
        assert_eq!(value["properties"]["schema_version"]["const"], 1);
    }

    #[test]
    fn selectors_and_spdx_syntax_are_strict() {
        assert!("ember_omens@^1.0".parse::<PackageRequirement>().is_ok());
        assert!("ember-omens@^1.0".parse::<PackageRequirement>().is_err());
        assert!(validate_spdx_expression("MIT OR Apache-2.0").is_ok());
        assert!(validate_spdx_expression("MIT Apache-2.0").is_err());
        assert!(validate_spdx_expression("(MIT OR Apache-2.0").is_err());
    }
}
