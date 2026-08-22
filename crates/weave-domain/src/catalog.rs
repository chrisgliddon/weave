use std::collections::BTreeMap;

use semver::{Version, VersionReq};

use crate::{DomainError, DomainPack, ModuleManifest, validate_manifest, validate_pack};

/// Explicit, in-memory set of available domain manifests and packs.
///
/// The catalog performs no ambient file-system or network discovery. Hosts populate it from
/// project-approved artifacts, then use [`Self::resolve`] for deterministic compatibility
/// selection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DomainCatalog {
    manifests: BTreeMap<String, BTreeMap<Version, ModuleManifest>>,
    packs: BTreeMap<(String, String), BTreeMap<Version, DomainPack>>,
}

/// One exact manifest and pack selected for a source activation.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDomainModule {
    /// Validated module manifest.
    pub manifest: ModuleManifest,
    /// Validated data pack.
    pub pack: DomainPack,
}

impl DomainCatalog {
    /// Create an empty explicit catalog.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manifests: BTreeMap::new(),
            packs: BTreeMap::new(),
        }
    }

    /// Build a catalog from host-supplied artifacts.
    pub fn from_artifacts(
        manifests: impl IntoIterator<Item = ModuleManifest>,
        packs: impl IntoIterator<Item = DomainPack>,
    ) -> Result<Self, DomainError> {
        let mut catalog = Self::new();
        for manifest in manifests {
            catalog.insert_manifest(manifest)?;
        }
        for pack in packs {
            catalog.insert_pack(pack)?;
        }
        Ok(catalog)
    }

    /// Add one manifest, rejecting duplicate identity/version coordinates.
    pub fn insert_manifest(&mut self, manifest: ModuleManifest) -> Result<(), DomainError> {
        let version =
            Version::parse(&manifest.version).map_err(|_| DomainError::InvalidArtifactVersion {
                path: "manifest.version",
            })?;
        let versions = self.manifests.entry(manifest.id.clone()).or_default();
        match versions.entry(version) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(manifest);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(DomainError::DuplicateManifestArtifact)
            }
        }
    }

    /// Add one pack, rejecting duplicate module/pack/version coordinates.
    pub fn insert_pack(&mut self, pack: DomainPack) -> Result<(), DomainError> {
        let version =
            Version::parse(&pack.version).map_err(|_| DomainError::InvalidArtifactVersion {
                path: "pack.version",
            })?;
        let key = (pack.module.id.clone(), pack.id.clone());
        let versions = self.packs.entry(key).or_default();
        match versions.entry(version) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(pack);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(DomainError::DuplicatePackArtifact)
            }
        }
    }

    /// Available manifests in deterministic identity and version order.
    pub fn manifests(&self) -> impl Iterator<Item = &ModuleManifest> {
        self.manifests.values().flat_map(BTreeMap::values)
    }

    /// Available packs in deterministic module, pack, and version order.
    pub fn packs(&self) -> impl Iterator<Item = &DomainPack> {
        self.packs.values().flat_map(BTreeMap::values)
    }

    /// Select and validate the highest installed versions matching one activation.
    pub fn resolve(
        &self,
        module_id: &str,
        module_requirement: &str,
        pack_id: &str,
        pack_requirement: &str,
        current_weave: &Version,
    ) -> Result<ResolvedDomainModule, DomainError> {
        let module_requirement = VersionReq::parse(module_requirement).map_err(|_| {
            DomainError::InvalidActivationRequirement {
                path: "module.version",
            }
        })?;
        let manifests = self
            .manifests
            .get(module_id)
            .ok_or(DomainError::ModuleNotInstalled)?;
        let manifest = manifests
            .iter()
            .rev()
            .find_map(|(version, manifest)| module_requirement.matches(version).then_some(manifest))
            .ok_or(DomainError::ModuleVersionNotInstalled)?;
        validate_manifest(manifest, current_weave)?;

        let pack_requirement = VersionReq::parse(pack_requirement).map_err(|_| {
            DomainError::InvalidActivationRequirement {
                path: "pack.version",
            }
        })?;
        let packs = self
            .packs
            .get(&(module_id.to_owned(), pack_id.to_owned()))
            .ok_or(DomainError::PackNotInstalled)?;
        let pack = packs
            .iter()
            .rev()
            .find_map(|(version, pack)| pack_requirement.matches(version).then_some(pack))
            .ok_or(DomainError::PackVersionNotInstalled)?;
        validate_pack(pack, manifest, current_weave)?;

        Ok(ResolvedDomainModule {
            manifest: manifest.clone(),
            pack: pack.clone(),
        })
    }
}
