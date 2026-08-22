use std::error::Error;
use std::io;

use bevy::prelude::*;
use weave_core::ir::{DomainValueIr, StoryIr};

const STORY: &str = include_str!("../../domain-modules/contract/tracer.story.ron");

#[derive(Resource, Debug, Clone, PartialEq)]
struct ConstellationReading {
    label: String,
    phase: String,
    intensity: f64,
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

fn report_reading(reading: Res<ConstellationReading>) {
    println!(
        "Bevy read {}: {} at {} intensity",
        reading.label, reading.phase, reading.intensity
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let story = ron::from_str::<StoryIr>(STORY)?;
    let reading = reading_from_story(&story)?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .add_systems(Startup, report_reading);
    app.update();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_portable_tracer_without_editor_dependencies() {
        let story = ron::from_str::<StoryIr>(STORY).expect("checked tracer RON");
        assert_eq!(
            reading_from_story(&story).expect("read module export"),
            ConstellationReading {
                label: "Glasswing constellation".to_owned(),
                phase: "twilight".to_owned(),
                intensity: 0.625,
            }
        );
    }
}
