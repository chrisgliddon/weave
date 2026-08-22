use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    DomainCatalog, DomainError, DomainPack, ModuleManifest, ResolvedDomainGraph, validate_manifest,
    validate_pack,
};

/// Current project-file schema for domain-module discovery.
pub const DOMAIN_PROJECT_FORMAT_VERSION: u32 = 1;
/// Current exact-selection lock-file schema.
pub const DOMAIN_LOCK_FORMAT_VERSION: u32 = 1;
/// Current portable registry-index schema.
pub const DOMAIN_REGISTRY_INDEX_VERSION: u32 = 1;
/// Project-relative configuration file discovered beside a `.weave` source file.
pub const DOMAIN_PROJECT_FILE_NAME: &str = "weave.modules.json";
/// Exact-selection lock written beside [`DOMAIN_PROJECT_FILE_NAME`].
pub const DOMAIN_LOCK_FILE_NAME: &str = "weave.lock";

const MODULE_FILE_NAME: &str = "module.weave-module.json";
const PACK_FILE_NAME: &str = "pack.weave-domain.json";
const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024;

/// Closed project configuration for explicit local artifact discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainProjectConfig {
    /// Project-file schema version.
    pub schema_version: u32,
    /// Project-relative registry roots in sorted order.
    #[serde(default)]
    pub registries: Vec<String>,
    /// Project-relative manifest references in sorted order.
    #[serde(default)]
    pub manifests: Vec<String>,
    /// Project-relative pack references in sorted order.
    #[serde(default)]
    pub packs: Vec<String>,
}

impl Default for DomainProjectConfig {
    fn default() -> Self {
        Self {
            schema_version: DOMAIN_PROJECT_FORMAT_VERSION,
            registries: Vec::new(),
            manifests: Vec::new(),
            packs: Vec::new(),
        }
    }
}

impl DomainProjectConfig {
    /// Parse a strict project JSON document without duplicate keys.
    pub fn from_json(source: &str) -> Result<Self, DomainPackageError> {
        let config = crate::parse_json(source)?;
        validate_project_config(&config)?;
        Ok(config)
    }

    /// Serialize stable project JSON.
    pub fn to_json(&self) -> Result<String, DomainPackageError> {
        validate_project_config(self)?;
        Ok(crate::pretty_json(self)?)
    }
}

/// One exact manifest dependency recorded in a lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedModuleDependency {
    /// Stable dependency identity.
    pub id: String,
    /// Exact selected version.
    pub version: String,
}

/// One exact selected manifest and its integrity digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedModule {
    /// Stable module identity.
    pub id: String,
    /// Exact selected version.
    pub version: String,
    /// SHA-256 of the canonical manifest artifact.
    pub sha256: String,
    /// Exact dependencies in deterministic identity order.
    pub dependencies: Vec<LockedModuleDependency>,
}

/// One exact pack dependency recorded in a lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedPackDependency {
    /// Stable owning module identity.
    pub module_id: String,
    /// Stable pack identity within the module.
    pub pack_id: String,
    /// Exact selected pack version.
    pub version: String,
}

/// One exact selected data pack and its integrity digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedPack {
    /// Stable owning module identity.
    pub module_id: String,
    /// Exact selected module version used to validate this pack.
    pub module_version: String,
    /// Stable pack identity.
    pub pack_id: String,
    /// Exact selected pack version.
    pub version: String,
    /// SHA-256 of the canonical pack artifact.
    pub sha256: String,
    /// Exact dependencies in deterministic coordinate order.
    pub dependencies: Vec<LockedPackDependency>,
}

/// Reproducible exact domain selection for one project build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainLock {
    /// Lock-file schema version.
    pub lock_version: u32,
    /// Selected modules in dependency order.
    pub modules: Vec<LockedModule>,
    /// Selected packs in dependency order.
    pub packs: Vec<LockedPack>,
}

impl DomainLock {
    /// Parse and validate a strict lock JSON document.
    pub fn from_json(source: &str) -> Result<Self, DomainPackageError> {
        let lock: Self = crate::parse_json(source)?;
        if lock.lock_version != DOMAIN_LOCK_FORMAT_VERSION {
            return Err(DomainPackageError::UnsupportedLockFormat {
                found: lock.lock_version,
                expected: DOMAIN_LOCK_FORMAT_VERSION,
            });
        }
        validate_lock(&lock)?;
        Ok(lock)
    }

    /// Serialize stable lock JSON.
    pub fn to_json(&self) -> Result<String, DomainPackageError> {
        if self.lock_version != DOMAIN_LOCK_FORMAT_VERSION {
            return Err(DomainPackageError::UnsupportedLockFormat {
                found: self.lock_version,
                expected: DOMAIN_LOCK_FORMAT_VERSION,
            });
        }
        validate_lock(self)?;
        Ok(crate::pretty_json(self)?)
    }
}

/// Discovery-safe manifest metadata for a local or hosted registry index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainManifestSummary {
    /// Stable module identity.
    pub id: String,
    /// Exact release.
    pub version: String,
    /// Human-readable title.
    pub title: String,
    /// Concise purpose.
    pub summary: String,
    /// Author display names.
    pub authors: Vec<String>,
    /// SPDX license expression.
    pub license: String,
    /// SHA-256 of the canonical artifact.
    pub sha256: String,
    /// Slash-separated artifact path relative to the registry root.
    pub artifact: String,
}

/// Discovery-safe data-pack metadata for a local or hosted registry index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainPackSummary {
    /// Stable owning module identity.
    pub module_id: String,
    /// Stable pack identity.
    pub id: String,
    /// Exact release.
    pub version: String,
    /// Human-readable title.
    pub title: String,
    /// SPDX expressions declared by the pack's provenance sources.
    pub licenses: Vec<String>,
    /// SHA-256 of the canonical artifact.
    pub sha256: String,
    /// Slash-separated artifact path relative to the registry root.
    pub artifact: String,
}

/// Portable deterministic registry listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainRegistryIndex {
    /// Registry-index schema version.
    pub schema_version: u32,
    /// Installed manifests in identity/version order.
    pub modules: Vec<DomainManifestSummary>,
    /// Installed packs in module/id/version order.
    pub packs: Vec<DomainPackSummary>,
}

impl DomainRegistryIndex {
    /// Serialize stable registry-index JSON.
    pub fn to_json(&self) -> Result<String, DomainPackageError> {
        Ok(crate::pretty_json(self)?)
    }
}

/// Coordinate used to associate exact selections with integrity digests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DomainArtifactCoordinate {
    /// One exact manifest.
    Manifest { id: String, version: String },
    /// One exact data pack.
    Pack {
        module_id: String,
        id: String,
        version: String,
    },
}

/// Validated snapshot of one deterministic registry.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainRegistrySnapshot {
    /// All installed artifacts as an explicit compiler catalog.
    pub catalog: DomainCatalog,
    /// Portable discovery metadata.
    pub index: DomainRegistryIndex,
    /// Artifact and sidecar files relevant to file watching.
    pub files: Vec<PathBuf>,
    digests: BTreeMap<DomainArtifactCoordinate, String>,
}

/// Result of installing or staging one module release and its packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledDomainArtifacts {
    /// Stable module identity.
    pub module_id: String,
    /// Exact module version.
    pub module_version: String,
    /// Canonical manifest path.
    pub manifest: PathBuf,
    /// Manifest digest.
    pub manifest_sha256: String,
    /// Canonical pack paths and digests in pack identity/version order.
    pub packs: Vec<InstalledDomainPack>,
}

/// One installed pack result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledDomainPack {
    /// Stable pack identity.
    pub id: String,
    /// Exact pack version.
    pub version: String,
    /// Canonical pack path.
    pub path: PathBuf,
    /// Pack digest.
    pub sha256: String,
}

/// Loaded project configuration, catalog, hashes, and watch targets.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedDomainProject {
    config_path: PathBuf,
    lock_path: PathBuf,
    config: DomainProjectConfig,
    catalog: DomainCatalog,
    files: Vec<PathBuf>,
    digests: BTreeMap<DomainArtifactCoordinate, String>,
}

impl LoadedDomainProject {
    /// Create an empty adjacent-project context for a source path.
    #[must_use]
    pub fn empty_for_source(source_path: impl AsRef<Path>) -> Self {
        let config_path = domain_project_path(source_path);
        let root = config_path.parent().unwrap_or_else(|| Path::new("."));
        Self {
            lock_path: root.join(DOMAIN_LOCK_FILE_NAME),
            config_path: config_path.clone(),
            config: DomainProjectConfig::default(),
            catalog: DomainCatalog::new(),
            files: vec![config_path],
            digests: BTreeMap::new(),
        }
    }

    /// Parsed project configuration.
    #[must_use]
    pub const fn config(&self) -> &DomainProjectConfig {
        &self.config
    }

    /// Explicit compiler/editor artifact catalog.
    #[must_use]
    pub const fn catalog(&self) -> &DomainCatalog {
        &self.catalog
    }

    /// Project configuration path, including when the optional adjacent file is absent.
    #[must_use]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Default exact-selection lock path.
    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    /// Files whose changes can alter module compilation.
    #[must_use]
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Whether the adjacent project configuration currently exists.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.config_path.exists()
    }

    /// Build a deterministic lock for a resolved compiler dependency graph.
    pub fn lock_for(&self, graph: &ResolvedDomainGraph) -> Result<DomainLock, DomainPackageError> {
        let mut modules = Vec::with_capacity(graph.modules.len());
        for id in &graph.module_order {
            let manifest = graph
                .modules
                .get(id)
                .ok_or(DomainPackageError::InvalidRegistryEntry)?;
            let sha256 = self
                .digests
                .get(&DomainArtifactCoordinate::Manifest {
                    id: id.clone(),
                    version: manifest.version.clone(),
                })
                .cloned()
                .ok_or(DomainPackageError::MissingDigest)?;
            let dependencies = manifest
                .dependencies
                .iter()
                .map(|dependency| {
                    let selected = graph
                        .modules
                        .get(&dependency.id)
                        .ok_or(DomainPackageError::InvalidRegistryEntry)?;
                    Ok(LockedModuleDependency {
                        id: dependency.id.clone(),
                        version: selected.version.clone(),
                    })
                })
                .collect::<Result<Vec<_>, DomainPackageError>>()?;
            modules.push(LockedModule {
                id: id.clone(),
                version: manifest.version.clone(),
                sha256,
                dependencies,
            });
        }

        let mut packs = Vec::with_capacity(graph.packs.len());
        for (module_id, pack_id) in &graph.pack_order {
            let pack = graph
                .packs
                .get(&(module_id.clone(), pack_id.clone()))
                .ok_or(DomainPackageError::InvalidRegistryEntry)?;
            let manifest = graph
                .modules
                .get(module_id)
                .ok_or(DomainPackageError::InvalidRegistryEntry)?;
            let sha256 = self
                .digests
                .get(&DomainArtifactCoordinate::Pack {
                    module_id: module_id.clone(),
                    id: pack_id.clone(),
                    version: pack.version.clone(),
                })
                .cloned()
                .ok_or(DomainPackageError::MissingDigest)?;
            let dependencies = pack
                .dependencies
                .iter()
                .map(|dependency| {
                    let selected = graph
                        .packs
                        .get(&(dependency.module_id.clone(), dependency.pack_id.clone()))
                        .ok_or(DomainPackageError::InvalidRegistryEntry)?;
                    Ok(LockedPackDependency {
                        module_id: dependency.module_id.clone(),
                        pack_id: dependency.pack_id.clone(),
                        version: selected.version.clone(),
                    })
                })
                .collect::<Result<Vec<_>, DomainPackageError>>()?;
            packs.push(LockedPack {
                module_id: module_id.clone(),
                module_version: manifest.version.clone(),
                pack_id: pack_id.clone(),
                version: pack.version.clone(),
                sha256,
                dependencies,
            });
        }
        let lock = DomainLock {
            lock_version: DOMAIN_LOCK_FORMAT_VERSION,
            modules,
            packs,
        };
        validate_lock(&lock)?;
        Ok(lock)
    }

    /// Atomically write the default project lock.
    pub fn write_lock(&self, lock: &DomainLock) -> Result<(), DomainPackageError> {
        atomic_write(&self.lock_path, lock.to_json()?.as_bytes())
    }

    /// Require the default project lock to match exactly.
    pub fn verify_lock(&self, expected: &DomainLock) -> Result<(), DomainPackageError> {
        let source = fs::read_to_string(&self.lock_path).map_err(io_error)?;
        let actual = DomainLock::from_json(&source)?;
        if &actual == expected {
            Ok(())
        } else {
            Err(DomainPackageError::LockMismatch)
        }
    }
}

/// Deterministic on-disk registry rooted at `modules/<id>/<version>`.
#[derive(Debug, Clone)]
pub struct DomainRegistry {
    root: PathBuf,
}

impl DomainRegistry {
    /// Open a registry root. Read operations do not create it.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Registry root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate and install one manifest with its packs without replacing different bytes.
    pub fn install(
        &self,
        manifest_source: impl AsRef<Path>,
        pack_sources: &[PathBuf],
        current_weave: &Version,
    ) -> Result<InstalledDomainArtifacts, DomainPackageError> {
        let manifest = load_manifest_file(manifest_source.as_ref(), false)?;
        validate_manifest(&manifest, current_weave)?;
        let mut packs = pack_sources
            .iter()
            .map(|path| load_pack_file(path, false))
            .collect::<Result<Vec<_>, _>>()?;
        for pack in &packs {
            validate_pack(pack, &manifest, current_weave)?;
        }
        packs.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| semantic_version_order(&left.version, &right.version))
        });
        let mut pack_coordinates = BTreeSet::new();
        for pack in &packs {
            if !pack_coordinates.insert((&pack.id, &pack.version)) {
                return Err(DomainError::DuplicatePackArtifact.into());
            }
        }

        fs::create_dir_all(&self.root).map_err(io_error)?;
        let canonical_root = self.root.canonicalize().map_err(io_error)?;

        let module_directory = self
            .root
            .join("modules")
            .join(&manifest.id)
            .join(&manifest.version);
        let manifest_path = module_directory.join(MODULE_FILE_NAME);
        let manifest_bytes = manifest.to_json()?.into_bytes();
        let mut writes = vec![ArtifactWrite::new(manifest_path.clone(), manifest_bytes)];
        let mut installed_packs = Vec::with_capacity(packs.len());
        for pack in packs {
            let path = module_directory
                .join("packs")
                .join(&pack.id)
                .join(&pack.version)
                .join(PACK_FILE_NAME);
            let bytes = pack.to_json()?.into_bytes();
            let sha256 = sha256_hex(&bytes);
            installed_packs.push(InstalledDomainPack {
                id: pack.id,
                version: pack.version,
                path: path.clone(),
                sha256,
            });
            writes.push(ArtifactWrite::new(path, bytes));
        }
        for write in &writes {
            ensure_write_confined(&canonical_root, &write.path)?;
            preflight_write(&write.path, &write.bytes)?;
            ensure_write_confined(&canonical_root, &checksum_path(&write.path))?;
            preflight_write(
                &checksum_path(&write.path),
                write.checksum_body().as_bytes(),
            )?;
        }
        for write in &writes {
            write_new_or_same(&write.path, &write.bytes)?;
            write_new_or_same(
                &checksum_path(&write.path),
                write.checksum_body().as_bytes(),
            )?;
        }
        Ok(InstalledDomainArtifacts {
            module_id: manifest.id,
            module_version: manifest.version,
            manifest: manifest_path,
            manifest_sha256: writes[0].sha256.clone(),
            packs: installed_packs,
        })
    }

    /// Validate every installed artifact and return a deterministic snapshot.
    pub fn discover(
        &self,
        current_weave: &Version,
    ) -> Result<DomainRegistrySnapshot, DomainPackageError> {
        let modules_root = self.root.join("modules");
        if !modules_root.exists() {
            return Ok(DomainRegistrySnapshot {
                catalog: DomainCatalog::new(),
                index: DomainRegistryIndex {
                    schema_version: DOMAIN_REGISTRY_INDEX_VERSION,
                    modules: Vec::new(),
                    packs: Vec::new(),
                },
                files: Vec::new(),
                digests: BTreeMap::new(),
            });
        }
        let mut catalog = DomainCatalog::new();
        let mut module_summaries = Vec::new();
        let mut pack_summaries = Vec::new();
        let mut files = Vec::new();
        let mut digests = BTreeMap::new();
        let canonical_root = self.root.canonicalize().map_err(io_error)?;

        for id_directory in sorted_directories(&modules_root)? {
            let id = file_name(&id_directory)?;
            for version_directory in sorted_directories(&id_directory)? {
                let version_text = file_name(&version_directory)?;
                let version = Version::parse(&version_text)
                    .map_err(|_| DomainPackageError::InvalidRegistryEntry)?;
                let manifest_path = version_directory.join(MODULE_FILE_NAME);
                ensure_registry_file(&canonical_root, &manifest_path)?;
                ensure_registry_file(&canonical_root, &checksum_path(&manifest_path))?;
                let manifest = load_manifest_file(&manifest_path, true)?;
                if manifest.id != id || manifest.version != version.to_string() {
                    return Err(DomainPackageError::InvalidRegistryEntry);
                }
                validate_manifest(&manifest, current_weave)?;
                let bytes = fs::read(&manifest_path).map_err(io_error)?;
                if bytes != manifest.to_json()?.as_bytes() {
                    return Err(DomainPackageError::InvalidRegistryEntry);
                }
                let sha256 = sha256_hex(&bytes);
                let coordinate = DomainArtifactCoordinate::Manifest {
                    id: manifest.id.clone(),
                    version: manifest.version.clone(),
                };
                digests.insert(coordinate, sha256.clone());
                module_summaries.push(DomainManifestSummary {
                    id: manifest.id.clone(),
                    version: manifest.version.clone(),
                    title: manifest.title.clone(),
                    summary: manifest.summary.clone(),
                    authors: manifest
                        .authors
                        .iter()
                        .map(|author| author.name.clone())
                        .collect(),
                    license: manifest.license.clone(),
                    sha256,
                    artifact: format!(
                        "modules/{}/{}/{}",
                        manifest.id, manifest.version, MODULE_FILE_NAME
                    ),
                });
                catalog.insert_manifest(manifest)?;
                files.extend([manifest_path.clone(), checksum_path(&manifest_path)]);

                let packs_root = version_directory.join("packs");
                if !packs_root.exists() {
                    continue;
                }
                for pack_id_directory in sorted_directories(&packs_root)? {
                    let pack_id = file_name(&pack_id_directory)?;
                    for pack_version_directory in sorted_directories(&pack_id_directory)? {
                        let pack_version_text = file_name(&pack_version_directory)?;
                        let pack_version = Version::parse(&pack_version_text)
                            .map_err(|_| DomainPackageError::InvalidRegistryEntry)?;
                        let pack_path = pack_version_directory.join(PACK_FILE_NAME);
                        ensure_registry_file(&canonical_root, &pack_path)?;
                        ensure_registry_file(&canonical_root, &checksum_path(&pack_path))?;
                        let pack = load_pack_file(&pack_path, true)?;
                        if pack.module.id != id
                            || pack.id != pack_id
                            || pack.version != pack_version.to_string()
                        {
                            return Err(DomainPackageError::InvalidRegistryEntry);
                        }
                        let bytes = fs::read(&pack_path).map_err(io_error)?;
                        if bytes != pack.to_json()?.as_bytes() {
                            return Err(DomainPackageError::InvalidRegistryEntry);
                        }
                        let sha256 = sha256_hex(&bytes);
                        let mut licenses = pack
                            .provenance
                            .sources
                            .iter()
                            .map(|source| source.license.clone())
                            .collect::<Vec<_>>();
                        licenses.sort();
                        licenses.dedup();
                        digests.insert(
                            DomainArtifactCoordinate::Pack {
                                module_id: pack.module.id.clone(),
                                id: pack.id.clone(),
                                version: pack.version.clone(),
                            },
                            sha256.clone(),
                        );
                        pack_summaries.push(DomainPackSummary {
                            module_id: pack.module.id.clone(),
                            id: pack.id.clone(),
                            version: pack.version.clone(),
                            title: pack.title.clone(),
                            licenses,
                            sha256,
                            artifact: format!(
                                "modules/{}/{}/packs/{}/{}/{}",
                                pack.module.id, version, pack.id, pack.version, PACK_FILE_NAME
                            ),
                        });
                        catalog.insert_pack(pack)?;
                        files.extend([pack_path.clone(), checksum_path(&pack_path)]);
                    }
                }
            }
        }
        validate_catalog(&catalog, current_weave)?;
        module_summaries.sort_by(summary_module_order);
        pack_summaries.sort_by(summary_pack_order);
        files.sort();
        files.dedup();
        Ok(DomainRegistrySnapshot {
            catalog,
            index: DomainRegistryIndex {
                schema_version: DOMAIN_REGISTRY_INDEX_VERSION,
                modules: module_summaries,
                packs: pack_summaries,
            },
            files,
            digests,
        })
    }
}

/// Load one explicit project configuration and every referenced artifact.
pub fn load_domain_project(
    config_path: impl AsRef<Path>,
    current_weave: &Version,
) -> Result<LoadedDomainProject, DomainPackageError> {
    let config_path = config_path.as_ref().to_path_buf();
    let source = fs::read_to_string(&config_path).map_err(io_error)?;
    let config = DomainProjectConfig::from_json(&source)?;
    let project_root = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(io_error)?;
    let mut catalog = DomainCatalog::new();
    let mut files = vec![config_path.clone()];
    let mut digests = BTreeMap::new();

    for (index, registry) in config.registries.iter().enumerate() {
        let path = project_path(&project_root, registry, format!("registries[{index}]"))?;
        let snapshot = DomainRegistry::new(path).discover(current_weave)?;
        catalog.extend(snapshot.catalog)?;
        files.extend(snapshot.files);
        merge_digests(&mut digests, snapshot.digests)?;
    }
    for (index, manifest_path) in config.manifests.iter().enumerate() {
        let path = project_path(&project_root, manifest_path, format!("manifests[{index}]"))?;
        let manifest = load_manifest_file(&path, false)?;
        validate_manifest(&manifest, current_weave)?;
        let bytes = fs::read(&path).map_err(io_error)?;
        let digest = sha256_hex(&bytes);
        digests.insert(
            DomainArtifactCoordinate::Manifest {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
            },
            digest,
        );
        catalog.insert_manifest(manifest)?;
        files.push(path.clone());
        if checksum_path(&path).exists() {
            ensure_project_sidecar(&project_root, &path, format!("manifests[{index}]"))?;
            files.push(checksum_path(&path));
            verify_checksum_sidecar(&path, &bytes, false)?;
        }
    }
    for (index, pack_path) in config.packs.iter().enumerate() {
        let path = project_path(&project_root, pack_path, format!("packs[{index}]"))?;
        let pack = load_pack_file(&path, false)?;
        let bytes = fs::read(&path).map_err(io_error)?;
        let digest = sha256_hex(&bytes);
        digests.insert(
            DomainArtifactCoordinate::Pack {
                module_id: pack.module.id.clone(),
                id: pack.id.clone(),
                version: pack.version.clone(),
            },
            digest,
        );
        catalog.insert_pack(pack)?;
        files.push(path.clone());
        if checksum_path(&path).exists() {
            ensure_project_sidecar(&project_root, &path, format!("packs[{index}]"))?;
            files.push(checksum_path(&path));
            verify_checksum_sidecar(&path, &bytes, false)?;
        }
    }
    validate_catalog(&catalog, current_weave)?;
    files.sort();
    files.dedup();
    Ok(LoadedDomainProject {
        lock_path: project_root.join(DOMAIN_LOCK_FILE_NAME),
        config_path,
        config,
        catalog,
        files,
        digests,
    })
}

/// Load the one documented adjacent project file, or an empty catalog when it is absent.
pub fn load_adjacent_domain_project(
    source_path: impl AsRef<Path>,
    current_weave: &Version,
) -> Result<LoadedDomainProject, DomainPackageError> {
    let config_path = domain_project_path(source_path.as_ref());
    if config_path.exists() {
        return load_domain_project(config_path, current_weave);
    }
    Ok(LoadedDomainProject::empty_for_source(source_path))
}

/// Adjacent project configuration path for one source file.
#[must_use]
pub fn domain_project_path(source_path: impl AsRef<Path>) -> PathBuf {
    source_path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(DOMAIN_PROJECT_FILE_NAME)
}

/// Generate the project configuration JSON Schema.
pub fn domain_project_schema() -> Result<String, DomainPackageError> {
    schema::<DomainProjectConfig>(
        "urn:weave:schema:domain-project:1",
        "Weave Domain Project v1",
        "schema_version",
        DOMAIN_PROJECT_FORMAT_VERSION,
    )
}

/// Generate the exact-selection lock JSON Schema.
pub fn domain_lock_schema() -> Result<String, DomainPackageError> {
    schema::<DomainLock>(
        "urn:weave:schema:domain-lock:1",
        "Weave Domain Lock v1",
        "lock_version",
        DOMAIN_LOCK_FORMAT_VERSION,
    )
}

/// Generate the portable registry-index JSON Schema.
pub fn domain_registry_index_schema() -> Result<String, DomainPackageError> {
    schema::<DomainRegistryIndex>(
        "urn:weave:schema:domain-registry-index:1",
        "Weave Domain Registry Index v1",
        "schema_version",
        DOMAIN_REGISTRY_INDEX_VERSION,
    )
}

/// Structured package, project, integrity, or discovery failure.
#[derive(Debug, thiserror::Error)]
pub enum DomainPackageError {
    /// File operation failed. The path is deliberately omitted from rendered output.
    #[error("domain artifact file access failed")]
    Io(#[source] io::Error),
    /// One artifact exceeded the bounded parser size.
    #[error("domain artifact exceeds the {max_bytes}-byte size limit")]
    TooLarge { max_bytes: u64 },
    /// Artifact bytes are not UTF-8.
    #[error("domain artifact is not valid UTF-8")]
    InvalidUtf8,
    /// Portable contract validation failed.
    #[error("{0}")]
    Contract(#[from] DomainError),
    /// A checksum is absent, malformed, or does not match.
    #[error("domain artifact checksum verification failed")]
    Checksum,
    /// A different artifact already occupies an immutable coordinate.
    #[error("refusing to overwrite a different domain artifact")]
    Conflict,
    /// Registry paths and decoded identities disagree.
    #[error("domain registry entry is inconsistent")]
    InvalidRegistryEntry,
    /// Project path is absolute, escapes the project, is sensitive, or is not canonical UTF-8.
    #[error("invalid domain project path at `{field}`")]
    InvalidProjectPath { field: String },
    /// Project configuration schema is unsupported.
    #[error("unsupported domain project format {found}; expected {expected}")]
    UnsupportedProjectFormat { found: u32, expected: u32 },
    /// Lock schema is unsupported.
    #[error("unsupported domain lock format {found}; expected {expected}")]
    UnsupportedLockFormat { found: u32, expected: u32 },
    /// Exact lock differs from current artifact selection or integrity.
    #[error("domain lock does not match the resolved artifacts")]
    LockMismatch,
    /// A selected artifact has no project integrity record.
    #[error("resolved domain artifact has no integrity record")]
    MissingDigest,
}

struct ArtifactWrite {
    path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
}

impl ArtifactWrite {
    fn new(path: PathBuf, bytes: Vec<u8>) -> Self {
        let sha256 = sha256_hex(&bytes);
        Self {
            path,
            bytes,
            sha256,
        }
    }

    fn checksum_body(&self) -> String {
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact");
        format!("{}  {name}\n", self.sha256)
    }
}

fn load_manifest_file(
    path: &Path,
    require_checksum: bool,
) -> Result<ModuleManifest, DomainPackageError> {
    let bytes = read_artifact(path)?;
    verify_checksum_sidecar(path, &bytes, require_checksum)?;
    let source = std::str::from_utf8(&bytes).map_err(|_| DomainPackageError::InvalidUtf8)?;
    if is_ron(path) {
        Ok(ModuleManifest::from_ron(source)?)
    } else {
        Ok(ModuleManifest::from_json(source)?)
    }
}

fn load_pack_file(path: &Path, require_checksum: bool) -> Result<DomainPack, DomainPackageError> {
    let bytes = read_artifact(path)?;
    verify_checksum_sidecar(path, &bytes, require_checksum)?;
    let source = std::str::from_utf8(&bytes).map_err(|_| DomainPackageError::InvalidUtf8)?;
    if is_ron(path) {
        Ok(DomainPack::from_ron(source)?)
    } else {
        Ok(DomainPack::from_json(source)?)
    }
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, DomainPackageError> {
    let metadata = fs::metadata(path).map_err(io_error)?;
    if metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(DomainPackageError::TooLarge {
            max_bytes: MAX_ARTIFACT_BYTES,
        });
    }
    fs::read(path).map_err(io_error)
}

fn validate_catalog(
    catalog: &DomainCatalog,
    current_weave: &Version,
) -> Result<(), DomainPackageError> {
    let manifests = catalog.manifests().collect::<Vec<_>>();
    let packs = catalog.packs().collect::<Vec<_>>();
    for manifest in &manifests {
        validate_manifest(manifest, current_weave)?;
        for dependency in &manifest.dependencies {
            let requirement = VersionReq::parse(&dependency.version).map_err(|_| {
                DomainPackageError::Contract(DomainError::InvalidArtifactVersion {
                    path: "manifest.dependencies.version",
                })
            })?;
            let available = manifests.iter().any(|candidate| {
                candidate.id == dependency.id
                    && Version::parse(&candidate.version)
                        .is_ok_and(|version| requirement.matches(&version))
            });
            if !available {
                return Err(DomainError::MissingDependency {
                    module_id: manifest.id.clone(),
                    dependency_id: dependency.id.clone(),
                }
                .into());
            }
        }
    }
    for pack in &packs {
        let requirement = VersionReq::parse(&pack.module.version).map_err(|_| {
            DomainPackageError::Contract(DomainError::InvalidArtifactVersion {
                path: "pack.module.version",
            })
        })?;
        let compatible = manifests
            .iter()
            .filter(|manifest| {
                manifest.id == pack.module.id
                    && Version::parse(&manifest.version)
                        .is_ok_and(|version| requirement.matches(&version))
            })
            .copied()
            .collect::<Vec<_>>();
        if compatible.is_empty() {
            return Err(DomainError::MissingDependency {
                module_id: pack.module.id.clone(),
                dependency_id: pack.module.id.clone(),
            }
            .into());
        }
        for manifest in compatible {
            validate_pack(pack, manifest, current_weave)?;
        }
        for dependency in &pack.dependencies {
            let requirement = VersionReq::parse(&dependency.version).map_err(|_| {
                DomainPackageError::Contract(DomainError::InvalidArtifactVersion {
                    path: "pack.dependencies.version",
                })
            })?;
            let available = packs.iter().any(|candidate| {
                candidate.module.id == dependency.module_id
                    && candidate.id == dependency.pack_id
                    && Version::parse(&candidate.version)
                        .is_ok_and(|version| requirement.matches(&version))
            });
            if !available {
                return Err(DomainError::MissingDependency {
                    module_id: pack.module.id.clone(),
                    dependency_id: dependency.module_id.clone(),
                }
                .into());
            }
        }
    }
    Ok(())
}

fn validate_project_config(config: &DomainProjectConfig) -> Result<(), DomainPackageError> {
    if config.schema_version != DOMAIN_PROJECT_FORMAT_VERSION {
        return Err(DomainPackageError::UnsupportedProjectFormat {
            found: config.schema_version,
            expected: DOMAIN_PROJECT_FORMAT_VERSION,
        });
    }
    validate_project_paths("registries", &config.registries)?;
    validate_project_paths("manifests", &config.manifests)?;
    validate_project_paths("packs", &config.packs)?;
    Ok(())
}

fn validate_project_paths(field: &str, paths: &[String]) -> Result<(), DomainPackageError> {
    let mut previous = None;
    for (index, path) in paths.iter().enumerate() {
        if previous.is_some_and(|value: &str| value >= path.as_str())
            || !safe_relative_path(Path::new(path))
            || contains_sensitive(path)
        {
            return Err(DomainPackageError::InvalidProjectPath {
                field: format!("{field}[{index}]"),
            });
        }
        previous = Some(path);
    }
    Ok(())
}

fn validate_lock(lock: &DomainLock) -> Result<(), DomainPackageError> {
    let mut module_ids = BTreeSet::new();
    for module in &lock.modules {
        if !module_ids.insert(module.id.as_str())
            || Version::parse(&module.version).is_err()
            || !valid_sha256(&module.sha256)
        {
            return Err(DomainPackageError::LockMismatch);
        }
    }
    let mut pack_ids = BTreeSet::new();
    for pack in &lock.packs {
        if !pack_ids.insert((pack.module_id.as_str(), pack.pack_id.as_str()))
            || Version::parse(&pack.module_version).is_err()
            || Version::parse(&pack.version).is_err()
            || !valid_sha256(&pack.sha256)
        {
            return Err(DomainPackageError::LockMismatch);
        }
    }
    Ok(())
}

fn project_path(root: &Path, relative: &str, field: String) -> Result<PathBuf, DomainPackageError> {
    let relative = Path::new(relative);
    if !safe_relative_path(relative) {
        return Err(DomainPackageError::InvalidProjectPath { field });
    }
    let path = root.join(relative);
    let canonical = path
        .canonicalize()
        .map_err(|_| DomainPackageError::InvalidProjectPath {
            field: field.clone(),
        })?;
    if !canonical.starts_with(root) {
        return Err(DomainPackageError::InvalidProjectPath { field });
    }
    Ok(canonical)
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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

fn merge_digests(
    destination: &mut BTreeMap<DomainArtifactCoordinate, String>,
    source: BTreeMap<DomainArtifactCoordinate, String>,
) -> Result<(), DomainPackageError> {
    for (coordinate, digest) in source {
        if destination.insert(coordinate, digest).is_some() {
            return Err(DomainPackageError::InvalidRegistryEntry);
        }
    }
    Ok(())
}

fn summary_module_order(
    left: &DomainManifestSummary,
    right: &DomainManifestSummary,
) -> std::cmp::Ordering {
    left.id
        .cmp(&right.id)
        .then_with(|| semantic_version_order(&left.version, &right.version))
}

fn summary_pack_order(left: &DomainPackSummary, right: &DomainPackSummary) -> std::cmp::Ordering {
    left.module_id
        .cmp(&right.module_id)
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| semantic_version_order(&left.version, &right.version))
}

fn semantic_version_order(left: &str, right: &str) -> std::cmp::Ordering {
    match (Version::parse(left), Version::parse(right)) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn sorted_directories(path: &Path) -> Result<Vec<PathBuf>, DomainPackageError> {
    let mut directories = Vec::new();
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn ensure_registry_file(root: &Path, path: &Path) -> Result<(), DomainPackageError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(DomainPackageError::InvalidRegistryEntry);
    }
    let canonical = path.canonicalize().map_err(io_error)?;
    if !canonical.starts_with(root) {
        return Err(DomainPackageError::InvalidRegistryEntry);
    }
    Ok(())
}

fn ensure_write_confined(root: &Path, path: &Path) -> Result<(), DomainPackageError> {
    let mut ancestor = path.parent().unwrap_or_else(|| Path::new("."));
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or(DomainPackageError::InvalidRegistryEntry)?;
    }
    if !ancestor.canonicalize().map_err(io_error)?.starts_with(root) {
        return Err(DomainPackageError::InvalidRegistryEntry);
    }
    if path.exists()
        && fs::symlink_metadata(path)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
    {
        return Err(DomainPackageError::InvalidRegistryEntry);
    }
    Ok(())
}

fn ensure_project_sidecar(
    root: &Path,
    artifact: &Path,
    field: String,
) -> Result<(), DomainPackageError> {
    let sidecar = checksum_path(artifact).canonicalize().map_err(|_| {
        DomainPackageError::InvalidProjectPath {
            field: field.clone(),
        }
    })?;
    if !sidecar.starts_with(root) {
        return Err(DomainPackageError::InvalidProjectPath { field });
    }
    Ok(())
}

fn file_name(path: &Path) -> Result<String, DomainPackageError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or(DomainPackageError::InvalidRegistryEntry)
}

fn is_ron(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("ron")
}

fn checksum_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".sha256");
    PathBuf::from(value)
}

fn verify_checksum_sidecar(
    path: &Path,
    bytes: &[u8],
    required: bool,
) -> Result<(), DomainPackageError> {
    let checksum = checksum_path(path);
    let text = match fs::read_to_string(checksum) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !required => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(DomainPackageError::Checksum);
        }
        Err(error) => return Err(io_error(error)),
    };
    if text.len() > 512 {
        return Err(DomainPackageError::Checksum);
    }
    let expected = text
        .split_ascii_whitespace()
        .next()
        .filter(|value| valid_sha256(value))
        .ok_or(DomainPackageError::Checksum)?;
    if expected.eq_ignore_ascii_case(&sha256_hex(bytes)) {
        Ok(())
    } else {
        Err(DomainPackageError::Checksum)
    }
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

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn preflight_write(path: &Path, bytes: &[u8]) -> Result<(), DomainPackageError> {
    if !path.exists() {
        return Ok(());
    }
    if fs::read(path).map_err(io_error)? == bytes {
        Ok(())
    } else {
        Err(DomainPackageError::Conflict)
    }
}

fn write_new_or_same(path: &Path, bytes: &[u8]) -> Result<(), DomainPackageError> {
    preflight_write(path, bytes)?;
    if path.exists() {
        return Ok(());
    }
    atomic_write(path, bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), DomainPackageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(io_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(bytes).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary
        .persist(path)
        .map_err(|error| io_error(error.error))?;
    Ok(())
}

fn schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version_property: &str,
    version: u32,
) -> Result<String, DomainPackageError> {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| DomainError::Serialize)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut(version_property))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    crate::sort_json_keys(&mut value);
    Ok(crate::pretty_json(&value)?)
}

fn io_error(error: io::Error) -> DomainPackageError {
    DomainPackageError::Io(error)
}
