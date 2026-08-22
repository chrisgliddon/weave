use std::error::Error;
use std::io;

use bevy::prelude::*;
use weave_core::ir::{DomainValueIr, StoryIr};

const TRACER_STORY: &str = include_str!("../../domain-modules/contract/tracer.story.ron");
const WORLD_STORIES: [&str; 4] = [
    include_str!(
        "../../domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.story.ron"
    ),
    include_str!("../../domain-modules/weave-world/corpus/stories/hokkaido-japan.story.ron"),
    include_str!("../../domain-modules/weave-world/corpus/stories/maldives.story.ron"),
    include_str!("../../domain-modules/weave-world/reference-place.story.ron"),
];

#[derive(Resource, Debug, Clone, PartialEq)]
struct ConstellationReading {
    label: String,
    phase: String,
    intensity: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct WorldReading {
    name: String,
    primary_biome: String,
    water_setting: String,
    minimum_annual_mean_temperature_c: f64,
    maximum_annual_mean_temperature_c: f64,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct WorldReadings(Vec<WorldReading>);

fn reading_from_story(story: &StoryIr) -> Result<ConstellationReading, io::Error> {
    let module = story.modules.get("constellation").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "constellation module is absent")
    })?;
    let label = match module.value(&["observation", "label"]) {
        Some(DomainValueIr::String(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation observation label is invalid",
            ));
        }
    };
    let phase = match module.value(&["phase"]) {
        Some(DomainValueIr::Symbol(value)) => value.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation phase is invalid",
            ));
        }
    };
    let intensity = match module.value(&["observation", "intensity"]) {
        Some(DomainValueIr::Number(value)) => *value,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "constellation observation intensity is invalid",
            ));
        }
    };
    Ok(ConstellationReading {
        label,
        phase,
        intensity,
    })
}

fn world_reading_from_story(story: &StoryIr, alias: &str) -> Result<WorldReading, io::Error> {
    let module = story
        .modules
        .get(alias)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "world module is absent"))?;
    let string = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::String(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let symbol = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Symbol(value)) => Ok(value.clone()),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    let number = |path: &[&str], message: &'static str| match module.value(path) {
        Some(DomainValueIr::Number(value)) => Ok(*value),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
    };
    Ok(WorldReading {
        name: string(
            &["seed", "identity", "display_name"],
            "world display name is invalid",
        )?,
        primary_biome: symbol(&["seed", "primary_biome"], "world primary biome is invalid")?,
        water_setting: symbol(
            &["seed", "water", "setting"],
            "world water setting is invalid",
        )?,
        minimum_annual_mean_temperature_c: number(
            &["seed", "climate", "annual_mean_temperature_c", "minimum"],
            "world minimum annual mean temperature is invalid",
        )?,
        maximum_annual_mean_temperature_c: number(
            &["seed", "climate", "annual_mean_temperature_c", "maximum"],
            "world maximum annual mean temperature is invalid",
        )?,
    })
}

fn report_reading(reading: Res<ConstellationReading>) {
    println!(
        "Bevy read {}: {} at {} intensity",
        reading.label, reading.phase, reading.intensity
    );
}

fn report_world(readings: Res<WorldReadings>) {
    for reading in &readings.0 {
        println!(
            "Bevy read {}: {} in an {} setting ({:.1}–{:.1} °C annual means)",
            reading.name,
            reading.primary_biome,
            reading.water_setting,
            reading.minimum_annual_mean_temperature_c,
            reading.maximum_annual_mean_temperature_c,
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let story = ron::from_str::<StoryIr>(TRACER_STORY)?;
    let reading = reading_from_story(&story)?;
    let world_readings = WorldReadings(
        WORLD_STORIES
            .iter()
            .map(|source| -> Result<WorldReading, Box<dyn Error>> {
                let story = ron::from_str::<StoryIr>(source)?;
                Ok(world_reading_from_story(&story, "world")?)
            })
            .collect::<Result<_, _>>()?,
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .insert_resource(world_readings)
        .add_systems(Startup, (report_reading, report_world));
    app.update();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_portable_tracer_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(TRACER_STORY).expect("checked tracer RON");
        assert_eq!(
            reading_from_story(&story).expect("read module export"),
            ConstellationReading {
                label: "Glasswing constellation".to_owned(),
                phase: "twilight".to_owned(),
                intensity: 0.625,
            }
        );
    }

    #[test]
    fn reads_four_portable_world_seeds_without_editor_dependencies() {
        assert_eq!(
            WORLD_STORIES
                .iter()
                .map(|source| {
                    let story = ron::from_str::<StoryIr>(source).expect("checked world RON");
                    world_reading_from_story(&story, "world").expect("read world export")
                })
                .collect::<Vec<_>>(),
            vec![
                WorldReading {
                    name: "British Columbia Temperate Forests".to_owned(),
                    primary_biome: "temperate_conifer_forest".to_owned(),
                    water_setting: "open_ocean_coast".to_owned(),
                    minimum_annual_mean_temperature_c: 7.0,
                    maximum_annual_mean_temperature_c: 7.0,
                },
                WorldReading {
                    name: "Hokkaido, Japan".to_owned(),
                    primary_biome: "temperate_broadleaf_and_mixed_forest".to_owned(),
                    water_setting: "semi_enclosed_sea_coast".to_owned(),
                    minimum_annual_mean_temperature_c: 6.13,
                    maximum_annual_mean_temperature_c: 6.13,
                },
                WorldReading {
                    name: "Maldives".to_owned(),
                    primary_biome: "tropical_and_subtropical_moist_broadleaf_forest".to_owned(),
                    water_setting: "oceanic_atolls".to_owned(),
                    minimum_annual_mean_temperature_c: 27.71,
                    maximum_annual_mean_temperature_c: 27.71,
                },
                WorldReading {
                    name: "Aotearoa New Zealand".to_owned(),
                    primary_biome: "temperate_broadleaf_and_mixed_forest".to_owned(),
                    water_setting: "oceanic_islands".to_owned(),
                    minimum_annual_mean_temperature_c: 8.8,
                    maximum_annual_mean_temperature_c: 16.0,
                },
            ]
        );
    }
}
