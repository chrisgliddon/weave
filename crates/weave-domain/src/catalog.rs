use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};

use crate::{
    DomainError, DomainOverride, DomainPack, DomainValue, ModuleManifest, validate_manifest,
    validate_pack,
};

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
    /// Effective values after deterministic source-authored replacements.
    pub effective_values: BTreeMap<String, DomainValue>,
    /// Canonically ordered source-authored replacements.
    pub authored_overrides: Vec<DomainOverride>,
}

/// Complete deterministic dependency closure selected for one or more source activations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedDomainGraph {
    /// Exact selected manifests keyed by stable module identity.
    pub modules: BTreeMap<String, ModuleManifest>,
    /// Exact selected packs keyed by owning module and pack identity.
    pub packs: BTreeMap<(String, String), DomainPack>,
    /// Module identities ordered with dependencies before dependents.
    pub module_order: Vec<String>,
    /// Pack coordinates ordered with dependencies before dependents.
    pub pack_order: Vec<(String, String)>,
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

    /// Merge another explicit catalog without allowing coordinate replacement.
    pub fn extend(&mut self, other: Self) -> Result<(), DomainError> {
        for manifest in other
            .manifests
            .into_values()
            .flat_map(BTreeMap::into_values)
        {
            self.insert_manifest(manifest)?;
        }
        for pack in other.packs.into_values().flat_map(BTreeMap::into_values) {
            self.insert_pack(pack)?;
        }
        Ok(())
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
            effective_values: pack.values.clone(),
            authored_overrides: Vec::new(),
        })
    }

    /// Resolve and validate the complete dependency closure for exact root activations.
    pub fn resolve_graph<'a>(
        &self,
        roots: impl IntoIterator<Item = &'a ResolvedDomainModule>,
        current_weave: &Version,
    ) -> Result<ResolvedDomainGraph, DomainError> {
        let mut modules = BTreeMap::new();
        let mut packs = BTreeMap::new();
        for root in roots {
            insert_selected_module(&mut modules, root.manifest.clone())?;
            insert_selected_pack(&mut packs, root.pack.clone())?;
        }

        loop {
            let mut changed = false;
            for manifest in modules.values().cloned().collect::<Vec<_>>() {
                for dependency in &manifest.dependencies {
                    let requirement = parse_dependency_requirement(
                        &dependency.version,
                        "manifest.dependencies.version",
                    )?;
                    if let Some(selected) = modules.get(&dependency.id) {
                        let version = Version::parse(&selected.version).map_err(|_| {
                            DomainError::InvalidArtifactVersion {
                                path: "manifest.version",
                            }
                        })?;
                        if !requirement.matches(&version) {
                            return Err(DomainError::IncompatibleDependency {
                                module_id: manifest.id.clone(),
                                dependency_id: dependency.id.clone(),
                            });
                        }
                        continue;
                    }
                    let selected =
                        self.select_manifest(&dependency.id, &requirement, current_weave)?;
                    modules.insert(dependency.id.clone(), selected.clone());
                    changed = true;
                }
            }

            for pack in packs.values().cloned().collect::<Vec<_>>() {
                for dependency in &pack.dependencies {
                    let requirement = parse_dependency_requirement(
                        &dependency.version,
                        "pack.dependencies.version",
                    )?;
                    let coordinate = (dependency.module_id.clone(), dependency.pack_id.clone());
                    let selected_pack = if let Some(selected) = packs.get(&coordinate) {
                        let version = Version::parse(&selected.version).map_err(|_| {
                            DomainError::InvalidArtifactVersion {
                                path: "pack.version",
                            }
                        })?;
                        if !requirement.matches(&version) {
                            return Err(DomainError::IncompatibleDependency {
                                module_id: pack.module.id.clone(),
                                dependency_id: dependency.module_id.clone(),
                            });
                        }
                        (*selected).clone()
                    } else {
                        let selected = self.select_pack(
                            &dependency.module_id,
                            &dependency.pack_id,
                            &requirement,
                        )?;
                        packs.insert(coordinate, (*selected).clone());
                        changed = true;
                        (*selected).clone()
                    };
                    ensure_pack_manifest(
                        self,
                        &selected_pack,
                        &mut modules,
                        current_weave,
                        &mut changed,
                    )?;
                }
                ensure_pack_manifest(self, &pack, &mut modules, current_weave, &mut changed)?;
            }
            if !changed {
                break;
            }
        }

        let manifests = modules.values().cloned().collect::<Vec<_>>();
        let module_order = crate::resolve_module_order(&manifests, current_weave)?;
        for pack in packs.values() {
            let manifest =
                modules
                    .get(&pack.module.id)
                    .ok_or_else(|| DomainError::MissingDependency {
                        module_id: pack.module.id.clone(),
                        dependency_id: pack.module.id.clone(),
                    })?;
            validate_pack(pack, manifest, current_weave)?;
        }
        let pack_order = resolve_pack_order(&packs)?;
        Ok(ResolvedDomainGraph {
            modules,
            packs,
            module_order,
            pack_order,
        })
    }

    fn select_manifest(
        &self,
        id: &str,
        requirement: &VersionReq,
        current_weave: &Version,
    ) -> Result<&ModuleManifest, DomainError> {
        let manifests = self
            .manifests
            .get(id)
            .ok_or(DomainError::ModuleNotInstalled)?;
        let manifest = manifests
            .iter()
            .rev()
            .find_map(|(version, manifest)| requirement.matches(version).then_some(manifest))
            .ok_or(DomainError::ModuleVersionNotInstalled)?;
        validate_manifest(manifest, current_weave)?;
        Ok(manifest)
    }

    fn select_pack(
        &self,
        module_id: &str,
        pack_id: &str,
        requirement: &VersionReq,
    ) -> Result<&DomainPack, DomainError> {
        self.packs
            .get(&(module_id.to_owned(), pack_id.to_owned()))
            .ok_or(DomainError::PackNotInstalled)?
            .iter()
            .rev()
            .find_map(|(version, pack)| requirement.matches(version).then_some(pack))
            .ok_or(DomainError::PackVersionNotInstalled)
    }
}

fn ensure_pack_manifest(
    catalog: &DomainCatalog,
    pack: &DomainPack,
    modules: &mut BTreeMap<String, ModuleManifest>,
    current_weave: &Version,
    changed: &mut bool,
) -> Result<(), DomainError> {
    let requirement = parse_dependency_requirement(&pack.module.version, "pack.module.version")?;
    if let Some(manifest) = modules.get(&pack.module.id) {
        let version =
            Version::parse(&manifest.version).map_err(|_| DomainError::InvalidArtifactVersion {
                path: "manifest.version",
            })?;
        if !requirement.matches(&version) {
            return Err(DomainError::IncompatibleModule {
                pack_id: pack.id.clone(),
                module_id: pack.module.id.clone(),
            });
        }
        validate_pack(pack, manifest, current_weave)?;
        return Ok(());
    }
    let manifest = catalog.select_manifest(&pack.module.id, &requirement, current_weave)?;
    validate_pack(pack, manifest, current_weave)?;
    modules.insert(pack.module.id.clone(), manifest.clone());
    *changed = true;
    Ok(())
}

fn insert_selected_module(
    modules: &mut BTreeMap<String, ModuleManifest>,
    manifest: ModuleManifest,
) -> Result<(), DomainError> {
    if let Some(existing) = modules.get(&manifest.id) {
        if existing.version == manifest.version {
            return Ok(());
        }
        return Err(DomainError::DuplicateModule {
            module_id: manifest.id,
        });
    }
    modules.insert(manifest.id.clone(), manifest);
    Ok(())
}

fn insert_selected_pack(
    packs: &mut BTreeMap<(String, String), DomainPack>,
    pack: DomainPack,
) -> Result<(), DomainError> {
    let coordinate = (pack.module.id.clone(), pack.id.clone());
    if let Some(existing) = packs.get(&coordinate) {
        if existing.version == pack.version {
            return Ok(());
        }
        return Err(DomainError::IncompatibleDependency {
            module_id: pack.module.id.clone(),
            dependency_id: pack.module.id,
        });
    }
    packs.insert(coordinate, pack);
    Ok(())
}

fn parse_dependency_requirement(
    requirement: &str,
    path: &'static str,
) -> Result<VersionReq, DomainError> {
    VersionReq::parse(requirement).map_err(|_| DomainError::InvalidActivationRequirement { path })
}

fn resolve_pack_order(
    packs: &BTreeMap<(String, String), DomainPack>,
) -> Result<Vec<(String, String)>, DomainError> {
    let mut dependencies = BTreeMap::<(String, String), BTreeSet<(String, String)>>::new();
    let mut dependents = BTreeMap::<(String, String), BTreeSet<(String, String)>>::new();
    for (coordinate, pack) in packs {
        let entry = dependencies.entry(coordinate.clone()).or_default();
        for dependency in &pack.dependencies {
            let dependency_coordinate = (dependency.module_id.clone(), dependency.pack_id.clone());
            if !packs.contains_key(&dependency_coordinate) {
                return Err(DomainError::MissingDependency {
                    module_id: pack.module.id.clone(),
                    dependency_id: dependency.module_id.clone(),
                });
            }
            entry.insert(dependency_coordinate.clone());
            dependents
                .entry(dependency_coordinate)
                .or_default()
                .insert(coordinate.clone());
        }
    }
    let mut ready = dependencies
        .iter()
        .filter_map(|(coordinate, requirements)| {
            requirements.is_empty().then_some(coordinate.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(packs.len());
    while let Some(coordinate) = ready.pop_first() {
        order.push(coordinate.clone());
        if let Some(children) = dependents.get(&coordinate) {
            for child in children {
                if let Some(requirements) = dependencies.get_mut(child) {
                    requirements.remove(&coordinate);
                    if requirements.is_empty() {
                        ready.insert(child.clone());
                    }
                }
            }
        }
    }
    if order.len() != packs.len() {
        return Err(DomainError::DependencyCycle);
    }
    Ok(order)
}
