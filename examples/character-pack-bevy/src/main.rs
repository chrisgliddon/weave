//! Offline Bevy consumer of a portable Weave Character authoring pack and runtime pack.
//!
//! The example validates the checked-in authoring export, independently derives the compact
//! runtime export, and requires it to equal the checked-in runtime fixture. It then selects
//! approved identity, presentation, personality, alignment, birth, inner-life, voice, and
//! date-context values and reports them from an explicit `Startup` schedule. No editor,
//! network, or artifact discovery is involved.

use std::error::Error;
use std::io;

use bevy::prelude::*;
use serde_json::{Value, json};
use weave_character::{
    BirthDate, CharacterAuthoringPack, CharacterRuntimePack, CharacterRuntimeProfile, HexacoTrait,
    TraitMeasurement,
};

const AUTHORING_PACK_JSON: &str = include_str!("../fixtures/authoring-pack.character-pack.json");
const RUNTIME_PACK_JSON: &str = include_str!("../fixtures/runtime-pack.character-pack.json");

const PACK_FORMAT_VERSION: u32 = 1;
const ACCENT_SLOT: &str = "accent";
const ALIGNMENT_AXIS: &str = "horizon";
const INNER_LIFE_ID: &str = "steady_hope";
const VOICE_ID: &str = "gentle_directness";
const DATE_CUE_ID: &str = "cue_000000000000000000000001";

#[derive(Resource, Debug, Clone, PartialEq)]
struct CharacterPackReading {
    pack_id: String,
    pack_version: String,
    license: String,
    cards: Vec<CharacterCard>,
}

#[derive(Component, Debug, Clone, PartialEq)]
struct CharacterCard {
    id: String,
    display_name: String,
    accent_color: String,
    greeting: String,
    openness_band: String,
    alignment_label: String,
    birth_date: String,
    inner_life: String,
    voice: String,
    date_context: String,
}

fn unsupported_version_error(source: &str) -> Result<String, Box<dyn Error>> {
    let mut value: Value = serde_json::from_str(source)?;
    value["pack_format_version"] = json!(PACK_FORMAT_VERSION + 1);
    let error = CharacterAuthoringPack::from_json(&serde_json::to_string(&value)?)
        .err()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "an unsupported pack version was accepted",
            )
        })?;
    Ok(error.to_string())
}

fn missing_reference_error(source: &str) -> Result<String, Box<dyn Error>> {
    let mut value: Value = serde_json::from_str(source)?;
    value["characters"][0]["override_id"] = json!("org.weave.example.overlay.absent");
    let error = CharacterAuthoringPack::from_json(&serde_json::to_string(&value)?)
        .err()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "a missing reference was accepted",
            )
        })?;
    Ok(error.to_string())
}

fn trait_score(profile: &CharacterRuntimeProfile, trait_id: HexacoTrait) -> Result<f64, io::Error> {
    match profile.personality.get(&trait_id) {
        Some(TraitMeasurement::Score { score }) => Ok(*score),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a required personality factor is missing or is not a score",
        )),
    }
}

fn band(score: f64) -> &'static str {
    if score >= 0.7 {
        "high"
    } else if score >= 0.4 {
        "middle"
    } else {
        "low"
    }
}

fn greeting_for(alignment_label: &str, openness_band: &str) -> &'static str {
    match (alignment_label, openness_band) {
        ("Seeking", "high") => "Morning. I brought a spare map in case the path surprises us.",
        ("Seeking", _) => "Morning. Let us check the trail together.",
        (_, "high") => "Morning. There is more to notice here than yesterday.",
        _ => "Morning. One step at a time.",
    }
}

fn birth_date_label(date: &BirthDate) -> String {
    match date {
        BirthDate::Year { year, .. } => format!("{year:04}"),
        BirthDate::MonthDay { month, day, .. } => format!("{month:02}-{day:02}"),
        BirthDate::Full {
            year, month, day, ..
        } => format!("{year:04}-{month:02}-{day:02}"),
    }
}

fn card_from_runtime(profile: &CharacterRuntimeProfile) -> Result<CharacterCard, Box<dyn Error>> {
    let presentation = profile.presentation.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime character has no approved presentation",
        )
    })?;
    let accent_color = presentation
        .palette
        .get(ACCENT_SLOT)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "runtime presentation has no approved accent colour",
            )
        })?;
    let birth_date = profile
        .birth_date
        .as_ref()
        .map(birth_date_label)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "runtime character has no approved birth date",
            )
        })?;
    let openness = trait_score(profile, HexacoTrait::Openness)?;
    let openness_band = band(openness).to_owned();
    let alignment = profile.alignment.get(ALIGNMENT_AXIS).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime character has no approved alignment value",
        )
    })?;
    let inner_life = profile.inner_life.get(INNER_LIFE_ID).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime character has no approved inner-life record",
        )
    })?;
    let voice = profile.voice.get(VOICE_ID).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime character has no approved voice record",
        )
    })?;
    let date_context = profile.date_context.get(DATE_CUE_ID).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime character has no approved date-context cue",
        )
    })?;
    Ok(CharacterCard {
        id: profile.id.clone(),
        display_name: profile.display_name.clone(),
        accent_color,
        greeting: greeting_for(&alignment.label, &openness_band).to_owned(),
        openness_band,
        alignment_label: alignment.label.clone(),
        birth_date,
        inner_life: inner_life.content.clone(),
        voice: voice.content.clone(),
        date_context: date_context.content.clone(),
    })
}

fn load_pack() -> Result<CharacterPackReading, Box<dyn Error>> {
    let authoring = CharacterAuthoringPack::from_json(AUTHORING_PACK_JSON)?;
    let runtime = CharacterRuntimePack::from_json(RUNTIME_PACK_JSON)?;
    let derived = authoring.to_runtime_pack()?;
    if derived != runtime {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "checked-in runtime fixture does not match the authoring export",
        )
        .into());
    }
    let authoring_ids = authoring
        .characters
        .iter()
        .map(|entry| entry.profile.id.as_str())
        .collect::<Vec<_>>();
    let runtime_ids = runtime
        .characters
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<Vec<_>>();
    if authoring_ids != runtime_ids {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "authoring and runtime packs contain different character ids",
        )
        .into());
    }
    let cards = runtime
        .characters
        .iter()
        .map(card_from_runtime)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CharacterPackReading {
        pack_id: runtime.id,
        pack_version: runtime.version,
        license: runtime.license.license,
        cards,
    })
}

fn rejection_checks() -> Result<(String, String), Box<dyn Error>> {
    Ok((
        unsupported_version_error(AUTHORING_PACK_JSON)?,
        missing_reference_error(AUTHORING_PACK_JSON)?,
    ))
}

fn place_cards(mut commands: Commands, reading: Res<CharacterPackReading>) {
    for card in &reading.cards {
        commands.spawn(card.clone());
    }
}

fn report_cards(reading: Res<CharacterPackReading>, cards: Query<&CharacterCard>) {
    println!(
        "Bevy read Character pack {}@{} ({}) with {} approved character(s)",
        reading.pack_id,
        reading.pack_version,
        reading.license,
        reading.cards.len(),
    );
    let mut cards = cards.iter().collect::<Vec<_>>();
    cards.sort_by(|left, right| left.id.cmp(&right.id));
    for card in cards {
        println!(
            "Bevy selected {} in {}: \"{}\" (alignment {}, openness {}, born {}, inner life: {}, voice: {}, date context: {})",
            card.display_name,
            card.accent_color,
            card.greeting,
            card.alignment_label,
            card.openness_band,
            card.birth_date,
            card.inner_life,
            card.voice,
            card.date_context,
        );
    }
}

fn report_rejections() {
    match rejection_checks() {
        Ok((unsupported, missing)) => {
            println!("Bevy rejected unsupported pack version: {unsupported}");
            println!("Bevy rejected missing reference: {missing}");
        }
        Err(error) => println!("Bevy rejection check failed: {error}"),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let reading = load_pack()?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(reading)
        .add_systems(Startup, place_cards)
        .add_systems(PostStartup, (report_cards, report_rejections));
    app.update();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_runtime_fixture_from_the_authoring_export() {
        let reading = load_pack().expect("checked fixtures load");
        assert_eq!(
            reading.pack_id,
            "org.weave.example.character_pack.thistlewood"
        );
        assert_eq!(reading.pack_version, "1.0.0");
        assert_eq!(reading.license, "MIT");
        assert_eq!(reading.cards.len(), 1);
        let card = &reading.cards[0];
        assert_eq!(card.display_name, "Mira Thistlewood");
        assert_eq!(card.accent_color, "#7BC96F");
        assert_eq!(card.openness_band, "high");
        assert_eq!(card.alignment_label, "Seeking");
        assert_eq!(card.birth_date, "2001-06-21");
        assert_eq!(
            card.greeting,
            "Morning. I brought a spare map in case the path surprises us."
        );
        assert!(card.inner_life.contains("patient work"));
        assert!(card.voice.contains("Speaks plainly"));
        assert!(card.date_context.contains("Midsummer evenings"));
    }

    #[test]
    fn checked_runtime_fixture_equals_the_authoring_export() {
        let authoring =
            CharacterAuthoringPack::from_json(AUTHORING_PACK_JSON).expect("checked authoring JSON");
        let runtime =
            CharacterRuntimePack::from_json(RUNTIME_PACK_JSON).expect("checked runtime JSON");
        assert_eq!(
            authoring.to_runtime_pack().expect("runtime export"),
            runtime
        );
    }

    #[test]
    fn rejects_an_unsupported_pack_version() {
        let error = unsupported_version_error(AUTHORING_PACK_JSON).expect("rejection");
        assert!(error.contains("unsupported character pack format version"));
        assert!(error.contains("pack_format_version"));
    }

    #[test]
    fn rejects_a_missing_reference() {
        let error = missing_reference_error(AUTHORING_PACK_JSON).expect("rejection");
        assert!(error.contains("override id is absent from the pack"));
    }
}
