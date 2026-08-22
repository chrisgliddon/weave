//! Versioned, offline normalization for contributor-owned Weave World presets.
//!
//! The corpus format keeps reviewed source facts separate from generated domain packs. The
//! builder never contacts the network: it validates local records, applies fixed unit,
//! seasonality, daylight, hazard, and approximation policies, and emits canonical packs through
//! the shared [`weave_domain`] contract.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use semver::Version;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use weave_domain::{
    DomainError, DomainPack, DomainValue, ModuleManifest, ModuleRequirement, Provenance,
    ProvenanceKind, ProvenanceSource, ProvenanceTransformation, validate_pack,
};

/// Current contributor source-record and index format.
pub const WORLD_CORPUS_FORMAT_VERSION: u32 = 1;

const INDEX_SCHEMA_ID: &str = "urn:weave:schema:world-corpus-index:1";
const PRESET_SCHEMA_ID: &str = "urn:weave:schema:world-corpus-preset:1";
const WORLD_MODULE_ID: &str = "org.weave.world";
const MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const OPEN_LICENSES: &[&str] = &[
    "CC-BY-4.0",
    "CC0-1.0",
    "LicenseRef-NASA-Public-Data",
    "LicenseRef-Natural-Earth-Public-Domain",
    "MIT",
];

/// A redaction-safe corpus failure. Source values are deliberately never echoed.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// JSON did not conform to the selected closed record type.
    #[error("invalid Weave World corpus JSON at line {line}, column {column}")]
    InvalidJson {
        /// One-based line.
        line: usize,
        /// One-based column.
        column: usize,
    },
    /// A semantic corpus invariant failed.
    #[error("invalid Weave World corpus field `{path}`: {reason}")]
    InvalidField {
        /// Stable field path, never a field value.
        path: String,
        /// Static, redaction-safe policy explanation.
        reason: &'static str,
    },
    /// An index path was absolute or escaped its fixture directory.
    #[error("invalid confined corpus path at `{path}`")]
    InvalidPath {
        /// Index field path, not the supplied path value.
        path: String,
    },
    /// A generated artifact is missing or differs in check mode.
    #[error("checked Weave World corpus artifact is stale at entry {entry}")]
    Stale {
        /// Zero-based index entry.
        entry: usize,
    },
    /// Shared domain-contract validation failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Local filesystem access failed.
    #[error("could not access a Weave World corpus file")]
    Io(#[from] std::io::Error),
    /// Canonical corpus serialization failed.
    #[error("could not serialize the Weave World corpus")]
    Serialize,
}

/// One deterministic corpus build index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusIndex {
    /// Serialized corpus format.
    pub corpus_format_version: u32,
    /// Presets sorted by source path.
    pub presets: Vec<CorpusEntry>,
}

/// One local source record and its generated pack destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusEntry {
    /// Source-record path relative to the index directory.
    pub source: String,
    /// Generated pack path relative to the index directory.
    pub output: String,
}

/// One reviewed, contributor-owned environmental preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetSource {
    /// Serialized corpus format.
    pub corpus_format_version: u32,
    /// Stable pack and shorthand identifier.
    pub id: String,
    /// Semantic preset release.
    pub version: String,
    /// Human-readable preset title.
    pub title: String,
    /// Exact owning module requirement.
    pub module: ModuleRequirement,
    /// Environmental-only identity.
    pub identity: IdentityInput,
    /// Climate observations plus explicit measurement context.
    pub climate: ClimateInput,
    /// Representative ecoregion biome classes.
    pub biomes: Vec<Biome>,
    /// Dominant concise narrative biome.
    pub primary_biome: Biome,
    /// Conservative physical terrain categories.
    pub terrain: Vec<Terrain>,
    /// Conservative coastal and island context.
    pub water: WaterInput,
    /// Evidence-backed weather cues, not forecasts.
    pub weather_tendencies: Vec<WeatherTendency>,
    /// Compact attribution retained in every runtime export.
    pub attribution: String,
    /// Reviewed sources and their fact roles.
    pub provenance: CorpusProvenance,
}

/// Environmental reference identity. Culture must remain excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentityInput {
    /// Human-readable reference name.
    pub display_name: String,
    /// Stable shorthand, equal to the preset identifier.
    pub preset: String,
    /// Three-letter country reference code.
    pub reference_code: String,
    /// Geographic reference kind.
    pub reference_kind: ReferenceKind,
    /// Must be false; the corpus does not infer culture from place.
    pub culture_included: bool,
}

/// Climate observations and the policy context needed to normalize them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClimateInput {
    /// Broad narrative climate class.
    pub band: ClimateBand,
    /// Representation confidence.
    pub confidence: Confidence,
    /// Inclusive climatology period in `YYYY-YYYY` form.
    pub reference_period: String,
    /// Geographic scale represented by the normalized facts.
    pub spatial_resolution: SpatialResolution,
    /// Deterministic broad-area approximation policy.
    pub approximation: Approximation,
    /// Latitude used only for the idealized daylight calculation.
    pub representative_latitude_degrees: f64,
    /// Published source grid, station, or area resolution.
    pub source_resolution: String,
    /// Concise limitation or uncertainty statement.
    pub uncertainty: String,
    /// Declared source temperature unit.
    pub temperature_unit: TemperatureUnit,
    /// Declared source precipitation unit.
    pub precipitation_unit: PrecipitationUnit,
    /// Selected-station summary or one or more source grid points.
    pub data: ClimateData,
}

/// Two supported climate acquisition shapes with explicit unit policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum ClimateData {
    /// A reviewed published range already expressed in annual output units.
    SelectedStationSummary {
        /// Annual mean air temperature range in degrees Celsius.
        annual_mean_temperature_c: NumericRangeInput,
        /// Annual precipitation range in millimetres per year.
        annual_precipitation_mm: NumericRangeInput,
        /// Published or deterministically aggregated warmest months.
        warmest_months: Vec<Month>,
        /// Published or deterministically aggregated coolest month.
        coolest_month: Month,
    },
    /// Source-native representative points with monthly values and annual means.
    RepresentativeGridPoints {
        /// Non-empty set of reviewed points; the builder forms a range across them.
        points: Vec<GridPointInput>,
    },
}

/// One representative climatology grid point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridPointInput {
    /// WGS84 latitude.
    pub latitude_degrees: f64,
    /// WGS84 longitude.
    pub longitude_degrees: f64,
    /// Source-reported annual mean temperature in degrees Celsius.
    pub annual_mean_temperature_c: f64,
    /// Source-reported annual mean precipitation rate in millimetres per day.
    pub annual_mean_precipitation_mm_per_day: f64,
    /// January through December mean temperatures in degrees Celsius.
    pub monthly_mean_temperature_c: [f64; 12],
}

/// Inclusive finite numeric range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumericRangeInput {
    /// Inclusive lower endpoint.
    pub minimum: f64,
    /// Inclusive upper endpoint.
    pub maximum: f64,
}

/// Broad water setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaterInput {
    /// Whether coast is a defining feature.
    pub coastal: bool,
    /// Whether the reference is represented as an island system.
    pub island_system: bool,
    /// Broad narrative water setting.
    pub setting: WaterSetting,
}

/// Reviewed public sources plus explicit source roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusProvenance {
    /// Public and original inputs; the builder sorts them by identifier.
    pub sources: Vec<ProvenanceSource>,
    /// Sources supporting each standard normalization step.
    pub lineage: SourceLineage,
}

/// Source identifiers assigned to each standard transformation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceLineage {
    /// Ecoregion or biome inputs.
    pub biomes: Vec<String>,
    /// Climate observations.
    pub climate: Vec<String>,
    /// Geographic identity and curation.
    pub identity: Vec<String>,
    /// Terrain, land, water, and curation inputs.
    pub terrain_water: Vec<String>,
}

macro_rules! symbols {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }
    };
}

symbols!(Biome {
    BorealForestTaiga => "boreal_forest_taiga",
    DesertsAndXericShrublands => "deserts_and_xeric_shrublands",
    FloodedGrasslandsAndSavannas => "flooded_grasslands_and_savannas",
    Mangroves => "mangroves",
    MediterraneanForestsWoodlandsAndScrub => "mediterranean_forests_woodlands_and_scrub",
    MontaneGrasslandAndShrubland => "montane_grassland_and_shrubland",
    TemperateBroadleafAndMixedForest => "temperate_broadleaf_and_mixed_forest",
    TemperateConiferForest => "temperate_conifer_forest",
    TemperateGrasslandSavannaAndShrubland => "temperate_grassland_savanna_and_shrubland",
    TropicalAndSubtropicalConiferousForest => "tropical_and_subtropical_coniferous_forest",
    TropicalAndSubtropicalDryBroadleafForest => "tropical_and_subtropical_dry_broadleaf_forest",
    TropicalAndSubtropicalGrasslandSavannaAndShrubland => "tropical_and_subtropical_grassland_savanna_and_shrubland",
    TropicalAndSubtropicalMoistBroadleafForest => "tropical_and_subtropical_moist_broadleaf_forest",
    Tundra => "tundra",
});

symbols!(ClimateBand {
    Alpine => "alpine",
    Arid => "arid",
    HumidContinental => "humid_continental",
    Polar => "polar",
    Subtropical => "subtropical",
    TemperateOceanic => "temperate_oceanic",
    TropicalOceanic => "tropical_oceanic",
});

symbols!(Confidence {
    High => "high",
    Low => "low",
    Moderate => "moderate",
    RepresentativeNotExhaustive => "representative_not_exhaustive",
    Unknown => "unknown",
});

symbols!(ReferenceKind {
    Country => "country",
    Ecosystem => "ecosystem",
    IslandGroup => "island_group",
    Region => "region",
    Subregion => "subregion",
});

symbols!(SpatialResolution {
    CountryScaleRepresentativePoint => "country_scale_representative_point",
    CountryScaleSelectedStations => "country_scale_selected_stations",
    EcosystemScaleRepresentativePoint => "ecosystem_scale_representative_point",
    RegionScaleRepresentativePoint => "region_scale_representative_point",
    SubregionScaleRepresentativePoint => "subregion_scale_representative_point",
});

symbols!(Approximation {
    RepresentativeGridPoint => "representative_grid_point",
    SelectedStationEnvelope => "selected_station_envelope",
});

symbols!(TemperatureUnit {
    DegreesCelsius => "degrees_celsius",
});

symbols!(PrecipitationUnit {
    MillimetresPerDay => "millimetres_per_day",
    MillimetresPerYear => "millimetres_per_year",
});

symbols!(Terrain {
    Atoll => "atoll",
    Coastal => "coastal",
    Island => "island",
    Mountain => "mountain",
    Plain => "plain",
    Volcanic => "volcanic",
});

symbols!(WaterSetting {
    FreshwaterDominated => "freshwater_dominated",
    OpenOceanCoast => "open_ocean_coast",
    OceanicArchipelago => "oceanic_archipelago",
    OceanicAtolls => "oceanic_atolls",
    OceanicIslands => "oceanic_islands",
    RiverDelta => "river_delta",
    SemiEnclosedSeaCoast => "semi_enclosed_sea_coast",
});

symbols!(WeatherTendency {
    AlpineCooling => "alpine_cooling",
    CoastalModeration => "coastal_moderation",
    CycloneExposure => "cyclone_exposure",
    DrySeason => "dry_season",
    FreezeThaw => "freeze_thaw",
    MonsoonCycle => "monsoon_cycle",
    RegionalRainfallContrast => "regional_rainfall_contrast",
    SeasonalTemperatureCycle => "seasonal_temperature_cycle",
    SnowSeason => "snow_season",
    TropicalRainfall => "tropical_rainfall",
});

symbols!(Month {
    January => "january",
    February => "february",
    March => "march",
    April => "april",
    May => "may",
    June => "june",
    July => "july",
    August => "august",
    September => "september",
    October => "october",
    November => "november",
    December => "december",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HazardTendency {
    CoastalInundation,
    HeavyPrecipitation,
    SeasonalFreeze,
    SlopeInstability,
    TropicalCyclone,
}

impl HazardTendency {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CoastalInundation => "coastal_inundation",
            Self::HeavyPrecipitation => "heavy_precipitation",
            Self::SeasonalFreeze => "seasonal_freeze",
            Self::SlopeInstability => "slope_instability",
            Self::TropicalCyclone => "tropical_cyclone",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedClimate {
    temperature: NumericRangeInput,
    precipitation: NumericRangeInput,
    warmest_months: Vec<&'static str>,
    coolest_month: &'static str,
    monthly_minimum_temperature: Option<f64>,
}

/// Generate the canonical index JSON Schema.
pub fn corpus_index_schema() -> Result<String, CorpusError> {
    schema::<CorpusIndex>(INDEX_SCHEMA_ID, "Weave World corpus index")
}

/// Generate the canonical contributor preset JSON Schema.
pub fn corpus_preset_schema() -> Result<String, CorpusError> {
    schema::<PresetSource>(PRESET_SCHEMA_ID, "Weave World corpus preset")
}

/// Build or check every pack in an offline corpus index.
pub fn process_corpus(
    index_path: &Path,
    manifest_path: &Path,
    check: bool,
) -> Result<usize, CorpusError> {
    let index_source = fs::read_to_string(index_path)?;
    let index: CorpusIndex = parse_json(&index_source)?;
    validate_index(&index)?;
    let manifest_source = fs::read_to_string(manifest_path)?;
    let manifest = ModuleManifest::from_json(&manifest_source)?;
    let current_weave = Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("workspace package version is valid semver");
    let base = index_path.parent().unwrap_or_else(|| Path::new("."));

    for (entry_index, entry) in index.presets.iter().enumerate() {
        let source_path = confined_path(base, &entry.source, entry_index, "source")?;
        let output_path = confined_path(base, &entry.output, entry_index, "output")?;
        let source_text = fs::read_to_string(source_path)?;
        let preset: PresetSource = parse_json(&source_text)?;
        let pack = build_pack(&preset, &manifest, &current_weave)?;
        let generated = pack.to_json()?;
        if check {
            let checked = fs::read(&output_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    CorpusError::Stale { entry: entry_index }
                } else {
                    CorpusError::Io(error)
                }
            })?;
            if generated.as_bytes() != checked {
                return Err(CorpusError::Stale { entry: entry_index });
            }
        } else {
            write_atomic(&output_path, generated.as_bytes())?;
        }
    }
    Ok(index.presets.len())
}

/// Normalize one reviewed preset into a canonical domain pack.
pub fn build_pack(
    preset: &PresetSource,
    manifest: &ModuleManifest,
    current_weave: &Version,
) -> Result<DomainPack, CorpusError> {
    validate_preset(preset, manifest)?;
    let climate = normalize_climate(&preset.climate)?;
    let mut biomes = preset.biomes.clone();
    biomes.sort_unstable();
    biomes.dedup();
    let mut terrain = preset.terrain.clone();
    terrain.sort_unstable();
    terrain.dedup();
    let mut weather = preset.weather_tendencies.clone();
    weather.sort_unstable();
    weather.dedup();
    let hazards = derive_hazards(preset, &climate, &terrain, &weather);
    let (shortest_daylight, longest_daylight) =
        solstice_daylight(preset.climate.representative_latitude_degrees);
    let hemisphere = hemisphere(preset.climate.representative_latitude_degrees);

    let seed = object([
        ("attribution", string(&preset.attribution)),
        (
            "biomes",
            list(biomes.iter().map(|value| symbol(value.as_str()))),
        ),
        (
            "climate",
            object([
                ("annual_mean_temperature_c", range(climate.temperature)),
                ("annual_precipitation_mm", range(climate.precipitation)),
                ("band", symbol(preset.climate.band.as_str())),
                ("confidence", symbol(preset.climate.confidence.as_str())),
                ("reference_period", string(&preset.climate.reference_period)),
                (
                    "source_resolution",
                    string(&preset.climate.source_resolution),
                ),
                (
                    "spatial_resolution",
                    symbol(preset.climate.spatial_resolution.as_str()),
                ),
                ("uncertainty", string(&preset.climate.uncertainty)),
                (
                    "units",
                    object([
                        (
                            "precipitation",
                            symbol(PrecipitationUnit::MillimetresPerYear.as_str()),
                        ),
                        (
                            "temperature",
                            symbol(preset.climate.temperature_unit.as_str()),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "context",
            object([
                (
                    "approximation",
                    symbol(preset.climate.approximation.as_str()),
                ),
                (
                    "geographic_resolution",
                    symbol(preset.climate.spatial_resolution.as_str()),
                ),
                ("uncertainty", string(&preset.climate.uncertainty)),
            ]),
        ),
        (
            "daylight",
            object([
                ("longest_daylight_hours", number(longest_daylight)),
                ("method", symbol("astronomical_solstice_estimate")),
                (
                    "representative_latitude_degrees",
                    number(round_to(preset.climate.representative_latitude_degrees, 2)),
                ),
                ("shortest_daylight_hours", number(shortest_daylight)),
                ("unit", symbol("hours_per_day")),
            ]),
        ),
        (
            "hazards",
            object([
                ("confidence", symbol(preset.climate.confidence.as_str())),
                (
                    "method",
                    symbol("deterministic_environmental_thresholds_v1"),
                ),
                (
                    "tendencies",
                    list(hazards.iter().map(|value| symbol(value.as_str()))),
                ),
            ]),
        ),
        (
            "identity",
            object([
                (
                    "culture_included",
                    boolean(preset.identity.culture_included),
                ),
                ("display_name", string(&preset.identity.display_name)),
                ("preset", string(&preset.identity.preset)),
                ("reference_code", string(&preset.identity.reference_code)),
                (
                    "reference_kind",
                    symbol(preset.identity.reference_kind.as_str()),
                ),
            ]),
        ),
        ("primary_biome", symbol(preset.primary_biome.as_str())),
        (
            "seasonality",
            object([
                ("coolest_month", symbol(climate.coolest_month)),
                ("hemisphere", symbol(hemisphere)),
                (
                    "warmest_months",
                    list(climate.warmest_months.iter().map(|month| symbol(month))),
                ),
            ]),
        ),
        (
            "terrain",
            list(terrain.iter().map(|value| symbol(value.as_str()))),
        ),
        (
            "water",
            object([
                ("coastal", boolean(preset.water.coastal)),
                ("island_system", boolean(preset.water.island_system)),
                ("setting", symbol(preset.water.setting.as_str())),
            ]),
        ),
        (
            "weather_tendencies",
            list(weather.iter().map(|value| symbol(value.as_str()))),
        ),
    ]);

    let pack = DomainPack {
        pack_format_version: manifest.pack_format_version,
        id: preset.id.clone(),
        version: preset.version.clone(),
        title: preset.title.clone(),
        module: preset.module.clone(),
        dependencies: Vec::new(),
        values: BTreeMap::from([("seed".to_owned(), seed)]),
        provenance: build_provenance(preset),
    };
    validate_pack(&pack, manifest, current_weave)?;
    Ok(pack)
}

fn validate_index(index: &CorpusIndex) -> Result<(), CorpusError> {
    if index.corpus_format_version != WORLD_CORPUS_FORMAT_VERSION {
        return Err(invalid(
            "corpus_format_version",
            "unsupported corpus format version",
        ));
    }
    if index.presets.is_empty() {
        return Err(invalid("presets", "at least one preset is required"));
    }
    let mut previous: Option<&str> = None;
    let mut outputs = BTreeSet::new();
    for (index, entry) in index.presets.iter().enumerate() {
        if previous.is_some_and(|value| value >= entry.source.as_str()) {
            return Err(invalid(
                "presets",
                "preset sources must be unique and sorted",
            ));
        }
        previous = Some(&entry.source);
        if !outputs.insert(&entry.output) {
            return Err(invalid(
                format!("presets[{index}].output"),
                "generated outputs must be unique",
            ));
        }
    }
    Ok(())
}

fn validate_preset(preset: &PresetSource, manifest: &ModuleManifest) -> Result<(), CorpusError> {
    if preset.corpus_format_version != WORLD_CORPUS_FORMAT_VERSION {
        return Err(invalid(
            "corpus_format_version",
            "unsupported corpus format version",
        ));
    }
    Version::parse(&preset.version)
        .map_err(|_| invalid("version", "expected a semantic version"))?;
    if preset.module.id != WORLD_MODULE_ID || preset.module.id != manifest.id {
        return Err(invalid(
            "module.id",
            "preset must belong to the Weave World manifest",
        ));
    }
    if preset.identity.preset != preset.id {
        return Err(invalid(
            "identity.preset",
            "identity shorthand must equal the preset identifier",
        ));
    }
    if preset.identity.culture_included {
        return Err(invalid(
            "identity.culture_included",
            "environmental presets must exclude cultural inference",
        ));
    }
    if preset.identity.reference_code.len() != 3
        || !preset
            .identity
            .reference_code
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(invalid(
            "identity.reference_code",
            "expected a three-letter uppercase country code",
        ));
    }
    if preset.biomes.is_empty() || !preset.biomes.contains(&preset.primary_biome) {
        return Err(invalid(
            "primary_biome",
            "primary biome must occur in the non-empty biome set",
        ));
    }
    if preset.terrain.is_empty() || preset.weather_tendencies.is_empty() {
        return Err(invalid(
            "terrain",
            "terrain and weather tendency sets must be non-empty",
        ));
    }
    if preset.water.island_system && !preset.water.coastal {
        return Err(invalid("water", "an island system must also be coastal"));
    }
    let compatible_scale = matches!(
        (
            preset.identity.reference_kind,
            preset.climate.spatial_resolution
        ),
        (
            ReferenceKind::Country | ReferenceKind::IslandGroup,
            SpatialResolution::CountryScaleRepresentativePoint
                | SpatialResolution::CountryScaleSelectedStations
        ) | (
            ReferenceKind::Ecosystem,
            SpatialResolution::EcosystemScaleRepresentativePoint
        ) | (
            ReferenceKind::Region,
            SpatialResolution::RegionScaleRepresentativePoint
        ) | (
            ReferenceKind::Subregion,
            SpatialResolution::SubregionScaleRepresentativePoint
        )
    );
    if !compatible_scale {
        return Err(invalid(
            "climate.spatial_resolution",
            "geographic resolution contradicts the reference kind",
        ));
    }
    validate_climate(&preset.climate)?;
    validate_provenance_input(&preset.provenance)?;
    Ok(())
}

fn validate_climate(climate: &ClimateInput) -> Result<(), CorpusError> {
    let _ = mean_days_per_year(&climate.reference_period)?;
    if !climate.representative_latitude_degrees.is_finite()
        || !(-90.0..=90.0).contains(&climate.representative_latitude_degrees)
    {
        return Err(invalid(
            "climate.representative_latitude_degrees",
            "latitude must be finite and between -90 and 90 degrees",
        ));
    }
    if climate.source_resolution.is_empty() || climate.uncertainty.is_empty() {
        return Err(invalid(
            "climate.source_resolution",
            "resolution and uncertainty statements are required",
        ));
    }
    if climate.temperature_unit != TemperatureUnit::DegreesCelsius {
        return Err(invalid(
            "climate.temperature_unit",
            "corpus v1 accepts source temperature only in degrees Celsius",
        ));
    }
    match &climate.data {
        ClimateData::SelectedStationSummary {
            annual_mean_temperature_c,
            annual_precipitation_mm,
            warmest_months,
            coolest_month,
        } => {
            if climate.approximation != Approximation::SelectedStationEnvelope
                || climate.precipitation_unit != PrecipitationUnit::MillimetresPerYear
            {
                return Err(invalid(
                    "climate.data",
                    "selected-station summaries require envelope approximation and annual millimetres",
                ));
            }
            validate_range(
                "climate.data.annual_mean_temperature_c",
                *annual_mean_temperature_c,
                -100.0,
                100.0,
            )?;
            validate_range(
                "climate.data.annual_precipitation_mm",
                *annual_precipitation_mm,
                0.0,
                100_000.0,
            )?;
            if warmest_months.is_empty()
                || warmest_months.len() > 3
                || warmest_months.contains(coolest_month)
            {
                return Err(invalid(
                    "climate.data.warmest_months",
                    "warmest months must be non-empty, bounded, and distinct from the coolest month",
                ));
            }
        }
        ClimateData::RepresentativeGridPoints { points } => {
            if climate.approximation != Approximation::RepresentativeGridPoint
                || climate.precipitation_unit != PrecipitationUnit::MillimetresPerDay
            {
                return Err(invalid(
                    "climate.data",
                    "grid points require representative-point approximation and daily millimetres",
                ));
            }
            if climate.confidence != Confidence::RepresentativeNotExhaustive {
                return Err(invalid(
                    "climate.confidence",
                    "broad grid-point approximations must be marked representative_not_exhaustive",
                ));
            }
            if points.is_empty() || points.len() > 64 {
                return Err(invalid(
                    "climate.data.points",
                    "expected between 1 and 64 grid points",
                ));
            }
            let mut coordinates = BTreeSet::new();
            for (index, point) in points.iter().enumerate() {
                validate_grid_point(index, point)?;
                if !coordinates.insert((
                    point.latitude_degrees.to_bits(),
                    point.longitude_degrees.to_bits(),
                )) {
                    return Err(invalid(
                        "climate.data.points",
                        "grid-point coordinates must be unique",
                    ));
                }
            }
            if !points.iter().any(|point| {
                (point.latitude_degrees - climate.representative_latitude_degrees).abs() <= 0.01
            }) {
                return Err(invalid(
                    "climate.representative_latitude_degrees",
                    "representative latitude must identify one declared grid point",
                ));
            }
        }
    }
    Ok(())
}

fn validate_grid_point(index: usize, point: &GridPointInput) -> Result<(), CorpusError> {
    let path = format!("climate.data.points[{index}]");
    if !point.latitude_degrees.is_finite()
        || !(-90.0..=90.0).contains(&point.latitude_degrees)
        || !point.longitude_degrees.is_finite()
        || !(-180.0..=180.0).contains(&point.longitude_degrees)
    {
        return Err(invalid(path, "grid coordinates are outside WGS84 bounds"));
    }
    if !point.annual_mean_temperature_c.is_finite()
        || !(-100.0..=100.0).contains(&point.annual_mean_temperature_c)
        || !point.annual_mean_precipitation_mm_per_day.is_finite()
        || !(0.0..=1_000.0).contains(&point.annual_mean_precipitation_mm_per_day)
        || point
            .monthly_mean_temperature_c
            .iter()
            .any(|value| !value.is_finite() || !(-100.0..=100.0).contains(value))
    {
        return Err(invalid(
            path,
            "grid climate values are non-finite or out of bounds",
        ));
    }
    Ok(())
}

fn validate_range(
    path: &str,
    value: NumericRangeInput,
    minimum: f64,
    maximum: f64,
) -> Result<(), CorpusError> {
    if !value.minimum.is_finite()
        || !value.maximum.is_finite()
        || value.minimum > value.maximum
        || value.minimum < minimum
        || value.maximum > maximum
    {
        return Err(invalid(
            path,
            "numeric range is non-finite, inverted, or out of bounds",
        ));
    }
    Ok(())
}

fn validate_provenance_input(provenance: &CorpusProvenance) -> Result<(), CorpusError> {
    if provenance.sources.is_empty() {
        return Err(invalid(
            "provenance.sources",
            "at least one reviewed source is required",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, source) in provenance.sources.iter().enumerate() {
        if !ids.insert(source.id.as_str()) {
            return Err(invalid(
                format!("provenance.sources[{index}].id"),
                "source identifiers must be unique",
            ));
        }
        if !OPEN_LICENSES.contains(&source.license.as_str()) {
            return Err(invalid(
                format!("provenance.sources[{index}].license"),
                "source license is not in the reviewed open-license policy",
            ));
        }
        if source.kind != ProvenanceKind::Original && source.sha256.is_none() {
            return Err(invalid(
                format!("provenance.sources[{index}].sha256"),
                "public source bytes require an exact SHA-256",
            ));
        }
    }
    let groups = [
        ("biomes", &provenance.lineage.biomes),
        ("climate", &provenance.lineage.climate),
        ("identity", &provenance.lineage.identity),
        ("terrain_water", &provenance.lineage.terrain_water),
    ];
    let mut used = BTreeSet::new();
    for (name, references) in groups {
        if references.is_empty() {
            return Err(invalid(
                format!("provenance.lineage.{name}"),
                "every normalization role requires a source",
            ));
        }
        for reference in references {
            if !ids.contains(reference.as_str()) {
                return Err(invalid(
                    format!("provenance.lineage.{name}"),
                    "lineage references an undeclared source",
                ));
            }
            if !used.insert((name, reference)) {
                return Err(invalid(
                    format!("provenance.lineage.{name}"),
                    "lineage references must be unique",
                ));
            }
        }
    }
    Ok(())
}

fn normalize_climate(climate: &ClimateInput) -> Result<NormalizedClimate, CorpusError> {
    match &climate.data {
        ClimateData::SelectedStationSummary {
            annual_mean_temperature_c,
            annual_precipitation_mm,
            warmest_months,
            coolest_month,
        } => Ok(NormalizedClimate {
            temperature: *annual_mean_temperature_c,
            precipitation: *annual_precipitation_mm,
            warmest_months: warmest_months.iter().map(|month| month.as_str()).collect(),
            coolest_month: coolest_month.as_str(),
            monthly_minimum_temperature: None,
        }),
        ClimateData::RepresentativeGridPoints { points } => {
            let days_per_year = mean_days_per_year(&climate.reference_period)?;
            let temperature = range_from(
                points
                    .iter()
                    .map(|point| round_to(point.annual_mean_temperature_c, 2)),
            );
            let precipitation = range_from(points.iter().map(|point| {
                round_to(
                    point.annual_mean_precipitation_mm_per_day * days_per_year,
                    1,
                )
            }));
            let mut monthly = [0.0; 12];
            for point in points {
                for (target, value) in monthly.iter_mut().zip(point.monthly_mean_temperature_c) {
                    *target += value;
                }
            }
            for value in &mut monthly {
                *value /= points.len() as f64;
            }
            let warmest = monthly
                .iter()
                .enumerate()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .map(|(index, _)| index)
                .expect("validated non-empty points produce monthly means");
            let coolest = monthly
                .iter()
                .enumerate()
                .min_by(|left, right| left.1.total_cmp(right.1))
                .map(|(index, _)| index)
                .expect("validated non-empty points produce monthly means");
            let minimum = monthly
                .iter()
                .copied()
                .min_by(f64::total_cmp)
                .expect("fixed monthly values");
            Ok(NormalizedClimate {
                temperature,
                precipitation,
                warmest_months: vec![MONTH_NAMES[warmest]],
                coolest_month: MONTH_NAMES[coolest],
                monthly_minimum_temperature: Some(minimum),
            })
        }
    }
}

fn range_from(values: impl Iterator<Item = f64>) -> NumericRangeInput {
    values.fold(
        NumericRangeInput {
            minimum: f64::INFINITY,
            maximum: f64::NEG_INFINITY,
        },
        |range, value| NumericRangeInput {
            minimum: range.minimum.min(value),
            maximum: range.maximum.max(value),
        },
    )
}

fn mean_days_per_year(reference_period: &str) -> Result<f64, CorpusError> {
    let Some((start, end)) = reference_period.split_once('-') else {
        return Err(invalid(
            "climate.reference_period",
            "expected an inclusive YYYY-YYYY period",
        ));
    };
    if start.len() != 4
        || end.len() != 4
        || !start.chars().all(|value| value.is_ascii_digit())
        || !end.chars().all(|value| value.is_ascii_digit())
    {
        return Err(invalid(
            "climate.reference_period",
            "expected an inclusive YYYY-YYYY period",
        ));
    }
    let start: u32 = start.parse().map_err(|_| {
        invalid(
            "climate.reference_period",
            "expected an inclusive YYYY-YYYY period",
        )
    })?;
    let end: u32 = end.parse().map_err(|_| {
        invalid(
            "climate.reference_period",
            "expected an inclusive YYYY-YYYY period",
        )
    })?;
    if end < start || end - start > 199 {
        return Err(invalid(
            "climate.reference_period",
            "period must be ordered and no longer than 200 years",
        ));
    }
    let years = end - start + 1;
    let days: u32 = (start..=end)
        .map(|year| if leap_year(year) { 366 } else { 365 })
        .sum();
    Ok(f64::from(days) / f64::from(years))
}

const fn leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn derive_hazards(
    preset: &PresetSource,
    climate: &NormalizedClimate,
    terrain: &[Terrain],
    weather: &[WeatherTendency],
) -> Vec<HazardTendency> {
    let mut hazards = BTreeSet::new();
    if preset.water.coastal && terrain.contains(&Terrain::Atoll) {
        hazards.insert(HazardTendency::CoastalInundation);
    }
    if climate.precipitation.maximum >= 2_000.0 {
        hazards.insert(HazardTendency::HeavyPrecipitation);
    }
    if climate
        .monthly_minimum_temperature
        .is_some_and(|value| value <= 0.0)
        || weather.contains(&WeatherTendency::FreezeThaw)
        || weather.contains(&WeatherTendency::SnowSeason)
    {
        hazards.insert(HazardTendency::SeasonalFreeze);
    }
    if terrain.contains(&Terrain::Mountain) && climate.precipitation.maximum >= 2_000.0 {
        hazards.insert(HazardTendency::SlopeInstability);
    }
    if weather.contains(&WeatherTendency::CycloneExposure) {
        hazards.insert(HazardTendency::TropicalCyclone);
    }
    hazards.into_iter().collect()
}

fn solstice_daylight(latitude_degrees: f64) -> (f64, f64) {
    let latitude = latitude_degrees.to_radians();
    let declination = 23.44_f64.to_radians();
    let day_length = |solar_declination: f64| {
        let cosine = (-latitude.tan() * solar_declination.tan()).clamp(-1.0, 1.0);
        24.0 * cosine.acos() / PI
    };
    let first = day_length(declination);
    let second = day_length(-declination);
    (
        round_to(first.min(second), 1),
        round_to(first.max(second), 1),
    )
}

fn hemisphere(latitude: f64) -> &'static str {
    if latitude.abs() <= 5.0 {
        "equatorial"
    } else if latitude > 0.0 {
        "northern"
    } else {
        "southern"
    }
}

fn build_provenance(preset: &PresetSource) -> Provenance {
    let mut sources = preset.provenance.sources.clone();
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    let lineage = &preset.provenance.lineage;
    let mut transformations = vec![
        transformation(
            "normalize_biomes",
            &lineage.biomes,
            "Deduplicate reviewed ecoregion biome classes into the closed Weave World vocabulary and retain the contributor-selected primary narrative class.",
        ),
        transformation(
            "normalize_climate",
            &lineage.climate,
            "Validate declared source units, convert daily precipitation rates to annual millimetres using the exact inclusive reference-period day count, form deterministic ranges, and rank monthly temperatures without interpolation.",
        ),
        transformation(
            "normalize_daylight",
            &union(&lineage.climate, &lineage.identity),
            "Calculate idealized June and December solstice daylight from the declared representative WGS84 latitude and 23.44 degree solar declination, clamp polar results, and round to one decimal hour; atmospheric refraction, terrain, and twilight are excluded.",
        ),
        transformation(
            "normalize_hazards",
            &union(&lineage.climate, &lineage.terrain_water),
            "Apply corpus-v1 environmental thresholds only: annual precipitation at least 2000 mm, monthly freezing, wet mountain slopes, coastal atolls, and explicitly curated cyclone exposure. Results are tendencies, not event forecasts or risk scores.",
        ),
        transformation(
            "normalize_identity",
            &lineage.identity,
            "Bind the stable contributor-owned shorthand, display identity, country code, and geographic scope while requiring culture_included to remain false.",
        ),
        transformation(
            "normalize_terrain_water",
            &lineage.terrain_water,
            "Reduce reviewed physical and ecoregion inputs to conservative closed terrain, coast, island-system, and water-setting categories.",
        ),
        transformation(
            "normalize_weather",
            &union(&lineage.climate, &lineage.terrain_water),
            "Reduce reviewed climate observations plus physical context to conservative closed weather tendencies; these are broad narrative cues, not forecasts.",
        ),
    ];
    transformations.push(ProvenanceTransformation {
        id: "normalize_world_seed".to_owned(),
        inputs: vec![
            "normalize_biomes".to_owned(),
            "normalize_climate".to_owned(),
            "normalize_daylight".to_owned(),
            "normalize_hazards".to_owned(),
            "normalize_identity".to_owned(),
            "normalize_terrain_water".to_owned(),
            "normalize_weather".to_owned(),
        ],
        description: "Assemble one deterministic typed environmental WorldSeed with explicit attribution, scale, units, uncertainty, approximation, daylight, and hazard policy; no cultural traits are inferred.".to_owned(),
    });

    let claims = BTreeMap::from([
        (
            "values.seed".to_owned(),
            vec!["normalize_world_seed".to_owned()],
        ),
        (
            "values.seed.attribution".to_owned(),
            vec!["normalize_world_seed".to_owned()],
        ),
        (
            "values.seed.biomes".to_owned(),
            vec!["normalize_biomes".to_owned()],
        ),
        (
            "values.seed.climate".to_owned(),
            vec!["normalize_climate".to_owned()],
        ),
        (
            "values.seed.context".to_owned(),
            vec!["normalize_world_seed".to_owned()],
        ),
        (
            "values.seed.daylight".to_owned(),
            vec!["normalize_daylight".to_owned()],
        ),
        (
            "values.seed.hazards".to_owned(),
            vec!["normalize_hazards".to_owned()],
        ),
        (
            "values.seed.identity".to_owned(),
            vec!["normalize_identity".to_owned()],
        ),
        (
            "values.seed.primary_biome".to_owned(),
            vec!["normalize_biomes".to_owned()],
        ),
        (
            "values.seed.seasonality".to_owned(),
            vec!["normalize_climate".to_owned()],
        ),
        (
            "values.seed.terrain".to_owned(),
            vec!["normalize_terrain_water".to_owned()],
        ),
        (
            "values.seed.water".to_owned(),
            vec!["normalize_terrain_water".to_owned()],
        ),
        (
            "values.seed.weather_tendencies".to_owned(),
            vec!["normalize_weather".to_owned()],
        ),
    ]);
    Provenance {
        sources,
        transformations,
        claims,
    }
}

fn transformation(id: &str, inputs: &[String], description: &str) -> ProvenanceTransformation {
    let mut inputs = inputs.to_vec();
    inputs.sort();
    inputs.dedup();
    ProvenanceTransformation {
        id: id.to_owned(),
        inputs,
        description: description.to_owned(),
    }
}

fn union(first: &[String], second: &[String]) -> Vec<String> {
    first
        .iter()
        .chain(second)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn confined_path(
    base: &Path,
    raw: &str,
    entry: usize,
    field: &str,
) -> Result<PathBuf, CorpusError> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CorpusError::InvalidPath {
            path: format!("presets[{entry}].{field}"),
        });
    }
    Ok(base.join(path))
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), CorpusError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| CorpusError::Io(error.error))?;
    Ok(())
}

fn parse_json<T: for<'de> Deserialize<'de>>(source: &str) -> Result<T, CorpusError> {
    serde_json::from_str::<UniqueJson>(source).map_err(json_error)?;
    serde_json::from_str(source).map_err(json_error)
}

fn json_error(error: serde_json::Error) -> CorpusError {
    CorpusError::InvalidJson {
        line: error.line(),
        column: error.column(),
    }
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

fn schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CorpusError> {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| CorpusError::Serialize)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-world-corpus-format-version".to_owned(),
            serde_json::Value::from(WORLD_CORPUS_FORMAT_VERSION),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut("corpus_format_version"))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert(
                "const".to_owned(),
                serde_json::Value::from(WORLD_CORPUS_FORMAT_VERSION),
            );
        }
    }
    sort_json(&mut value);
    let mut output = serde_json::to_string_pretty(&value).map_err(|_| CorpusError::Serialize)?;
    output.push('\n');
    Ok(output)
}

fn sort_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                sort_json(value);
            }
            values.sort_keys();
        }
        _ => {}
    }
}

fn object<const N: usize>(values: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        values
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn list(values: impl IntoIterator<Item = DomainValue>) -> DomainValue {
    DomainValue::List(values.into_iter().collect())
}

fn range(value: NumericRangeInput) -> DomainValue {
    object([
        ("maximum", number(value.maximum)),
        ("minimum", number(value.minimum)),
    ])
}

fn string(value: &str) -> DomainValue {
    DomainValue::String(value.to_owned())
}

fn symbol(value: &str) -> DomainValue {
    DomainValue::Symbol(value.to_owned())
}

const fn number(value: f64) -> DomainValue {
    DomainValue::Number(value)
}

const fn boolean(value: bool) -> DomainValue {
    DomainValue::Bool(value)
}

fn round_to(value: f64, places: i32) -> f64 {
    let factor = 10_f64.powi(places);
    (value * factor).round() / factor
}

fn invalid(path: impl Into<String>, reason: &'static str) -> CorpusError {
    CorpusError::InvalidField {
        path: path.into(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str =
        include_str!("../../../examples/domain-modules/weave-world/module.weave-module.json");
    const HOKKAIDO: &str = include_str!(
        "../../../examples/domain-modules/weave-world/corpus/presets/hokkaido_japan.json"
    );

    fn hokkaido() -> PresetSource {
        parse_json(HOKKAIDO).expect("checked Hokkaido source")
    }

    #[test]
    fn grid_units_seasonality_daylight_and_hazards_are_deterministic() {
        let manifest = ModuleManifest::from_json(MANIFEST).expect("world manifest");
        let pack = build_pack(
            &hokkaido(),
            &manifest,
            &Version::parse(env!("CARGO_PKG_VERSION")).expect("version"),
        )
        .expect("Hokkaido pack");
        let DomainValue::Object(seed) = &pack.values["seed"] else {
            panic!("seed object")
        };
        let DomainValue::Object(climate) = &seed["climate"] else {
            panic!("climate object")
        };
        assert_eq!(
            climate["annual_precipitation_mm"],
            range(NumericRangeInput {
                minimum: 1267.5,
                maximum: 1267.5,
            })
        );
        let DomainValue::Object(seasonality) = &seed["seasonality"] else {
            panic!("seasonality object")
        };
        assert_eq!(seasonality["coolest_month"], symbol("january"));
        assert_eq!(seasonality["warmest_months"], list([symbol("august")]));
        let DomainValue::Object(hazards) = &seed["hazards"] else {
            panic!("hazards object")
        };
        assert_eq!(hazards["tendencies"], list([symbol("seasonal_freeze")]));
    }

    #[test]
    fn contradictory_and_overbroad_inputs_fail_closed() {
        let manifest = ModuleManifest::from_json(MANIFEST).expect("world manifest");
        let mut preset = hokkaido();
        preset.primary_biome = Biome::Tundra;
        let error = build_pack(
            &preset,
            &manifest,
            &Version::parse(env!("CARGO_PKG_VERSION")).expect("version"),
        )
        .expect_err("contradictory biome must fail");
        assert!(error.to_string().contains("primary_biome"));

        let mut preset = hokkaido();
        preset.climate.confidence = Confidence::High;
        let error = build_pack(
            &preset,
            &manifest,
            &Version::parse(env!("CARGO_PKG_VERSION")).expect("version"),
        )
        .expect_err("broad point must not claim high confidence");
        assert!(error.to_string().contains("climate.confidence"));

        let mut preset = hokkaido();
        preset.identity.reference_kind = ReferenceKind::Country;
        let error = build_pack(
            &preset,
            &manifest,
            &Version::parse(env!("CARGO_PKG_VERSION")).expect("version"),
        )
        .expect_err("contradictory geographic scale must fail");
        assert!(error.to_string().contains("climate.spatial_resolution"));
    }

    #[test]
    fn missing_and_unknown_unit_inputs_fail_without_value_disclosure() {
        let unknown_unit = "credential-shaped-unknown-unit";
        let source = HOKKAIDO.replace("degrees_celsius", unknown_unit);
        let error = parse_json::<PresetSource>(&source).expect_err("unknown unit must fail");
        assert!(matches!(error, CorpusError::InvalidJson { .. }));
        assert!(!error.to_string().contains(unknown_unit));

        let source = HOKKAIDO.replacen(
            "    \"source_resolution\": \"NASA MERRA-2 meteorology grid: 0.5 degree latitude by 0.625 degree longitude.\",\n",
            "",
            1,
        );
        let error = parse_json::<PresetSource>(&source).expect_err("missing context must fail");
        assert!(matches!(error, CorpusError::InvalidJson { .. }));

        let source = HOKKAIDO.replacen(
            "  \"corpus_format_version\": 1,",
            "  \"corpus_format_version\": 1,\n  \"corpus_format_version\": 1,",
            1,
        );
        let error = parse_json::<PresetSource>(&source).expect_err("duplicate key must fail");
        assert!(matches!(error, CorpusError::InvalidJson { .. }));
    }

    #[test]
    fn schemas_pin_the_format_and_closed_unit_vocabulary() {
        let preset_schema = corpus_preset_schema().expect("preset schema");
        let index_schema = corpus_index_schema().expect("index schema");
        assert!(preset_schema.contains("millimetres_per_day"));
        assert!(preset_schema.contains("degrees_celsius"));
        assert!(index_schema.contains("\"const\": 1"));
        assert_eq!(
            preset_schema,
            include_str!("../../../schemas/weave-world-corpus-preset-v1.schema.json")
        );
        assert_eq!(
            index_schema,
            include_str!("../../../schemas/weave-world-corpus-index-v1.schema.json")
        );
    }

    #[test]
    fn checked_index_rebuilds_four_exact_packs_without_network_access() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let count = process_corpus(
            &root.join("examples/domain-modules/weave-world/corpus.weave-world.json"),
            &root.join("examples/domain-modules/weave-world/module.weave-module.json"),
            true,
        )
        .expect("checked corpus is byte-exact");
        assert_eq!(count, 4);
    }
}
