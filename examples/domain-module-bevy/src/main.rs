use std::error::Error;
use std::io;

use bevy::prelude::*;
use weave_core::ir::{DomainValueIr, StoryIr};

const TRACER_STORY: &str = include_str!("../../domain-modules/contract/tracer.story.ron");
const WORLD_STORY: &str =
    include_str!("../../domain-modules/weave-world/reference-place.story.ron");

#[derive(Resource, Debug, Clone, PartialEq)]
struct ConstellationReading {
    label: String,
    phase: String,
    intensity: f64,
}

#[derive(Resource, Debug, Clone, PartialEq)]
struct WorldReading {
    name: String,
    primary_biome: String,
    water_setting: String,
    minimum_annual_mean_temperature_c: f64,
    maximum_annual_mean_temperature_c: f64,
}

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

fn world_reading_from_story(story: &StoryIr) -> Result<WorldReading, io::Error> {
    let module = story
        .modules
        .get("world")
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
            &[
                "seed",
                "climate",
                "selected_station_annual_mean_temperature_c",
                "minimum",
            ],
            "world minimum annual mean temperature is invalid",
        )?,
        maximum_annual_mean_temperature_c: number(
            &[
                "seed",
                "climate",
                "selected_station_annual_mean_temperature_c",
                "maximum",
            ],
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

fn report_world(reading: Res<WorldReading>) {
    println!(
        "Bevy read {}: {} in an {} setting ({:.1}–{:.1} °C selected-station annual means)",
        reading.name,
        reading.primary_biome,
        reading.water_setting,
        reading.minimum_annual_mean_temperature_c,
        reading.maximum_annual_mean_temperature_c,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let story = ron::from_str::<StoryIr>(TRACER_STORY)?;
    let reading = reading_from_story(&story)?;
    let world_story = ron::from_str::<StoryIr>(WORLD_STORY)?;
    let world_reading = world_reading_from_story(&world_story)?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .insert_resource(world_reading)
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
    fn reads_the_portable_world_seed_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(WORLD_STORY).expect("checked world RON");
        assert_eq!(
            world_reading_from_story(&story).expect("read world module export"),
            WorldReading {
                name: "Aotearoa New Zealand".to_owned(),
                primary_biome: "temperate_broadleaf_and_mixed_forest".to_owned(),
                water_setting: "oceanic_islands".to_owned(),
                minimum_annual_mean_temperature_c: 8.8,
                maximum_annual_mean_temperature_c: 16.0,
            }
        );
    }
}
