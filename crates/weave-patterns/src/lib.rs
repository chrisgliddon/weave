//! Serializable pattern-system extension boundary and built-in meaning systems.
//!
//! A [`PatternDefinition`] is immutable and shareable. A [`PatternState`] contains the
//! serializable per-story mutation, while callers supply a [`RandomSource`] so one story seed
//! controls grammar and pattern randomness together. The object-safe [`PatternSystem`] trait lets
//! built-in and data-authored systems use the same runtime path.

mod builtins;
mod data;
mod factory;
mod model;
mod package;

pub use builtins::{elder_futhark_definition, i_ching_definition, tarot_definition};
pub use data::DataPatternSystem;
pub use factory::system_from_ir;
pub use model::{
    DrawMethod, DrawRequest, DrawResult, DrawnElement, PatternDefinition, PatternElement,
    PatternError, PatternState, PatternSystem, PatternValue, RandomSource, ReversalPolicy,
    SeededRandom, SpreadDefinition,
};
pub use package::{
    COMMUNITY_PACKAGE_FORMAT_VERSION, COMMUNITY_REGISTRY_INDEX_VERSION, CommunityPackage,
    InstalledPackage, PackageAuthor, PackageDrawMethod, PackageElement, PackageError,
    PackageMetadata, PackagePattern, PackageRegistry, PackageRequirement, PackageSource,
    PackageSpread, PackageSummary, PackageSymbol, PackageValue, PublishedPackage, RegistryIndex,
    community_package_schema, load_package, publish_package, validate_package,
};

/// Current pattern data-model version.
pub const PATTERN_MODEL_VERSION: u32 = 1;
