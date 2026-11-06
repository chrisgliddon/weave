//! Regenerate the checked-in portable Character-pack fixtures.
//!
//! Every value in these fixtures is original synthetic Weave material released under the
//! repository MIT license. The generator reads no network resource and no editor state.
//!
//! Run from the repository root:
//!
//! ```bash
//! cargo run -p weave-example-character-pack-bevy --example generate_fixtures
//! ```

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use weave_character::{
    CharacterAuthoringPack, CharacterPackLicense, CharacterPackMetadata, CharacterProfile,
    HexacoProfile, derive_ocean,
};

const SOURCE_ID: &str = "character_pack_original";
const CALENDAR_SOURCE_ID: &str = "character_pack_calendar_original";
const CHARACTER_ID: &str = "org.weave.example.character.mira_thistlewood";
const PACK_ID: &str = "org.weave.example.character_pack.thistlewood";
const ALIGNMENT_ID: &str = "org.weave.example.alignment.harbor_compass";
const ALIGNMENT_PACK_SHA256: &str =
    "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const ALIGNMENT_REVIEW_SHA256: &str =
    "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
const ALIGNMENT_APPLIED_SHA256: &str =
    "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";
const DATE_PACK_SHA256: &str = "d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4";
const DATE_REVIEW_SHA256: &str = "e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5";

fn attributed(value: Value) -> Value {
    json!({
        "value": value,
        "state": "authored",
        "confidence": "high",
        "review": "not_required",
        "lock": "unlocked",
        "freshness": "current",
        "lineage": [SOURCE_ID],
    })
}

fn trait_score(score: f64) -> Value {
    attributed(json!({ "form": "score", "score": score }))
}

fn extension_header(namespace: &str, lineage: Vec<&str>) -> Value {
    json!({
        "namespace": namespace,
        "extension_version": 1,
        "authority": "org.weave.example.character_pack_reviewer",
        "rationale": "Original synthetic value authored for the portable Character pack example.",
        "state": "reviewed",
        "review": "accepted",
        "lock": "unlocked",
        "freshness": "current",
        "lineage": lineage,
        "canonical_personality_write_back": "forbidden",
    })
}

fn metadata() -> CharacterPackMetadata {
    CharacterPackMetadata {
        id: PACK_ID.to_owned(),
        version: "1.0.0".to_owned(),
        title: "Thistlewood Portable Character Pack".to_owned(),
        description: "Original synthetic authoring and runtime Character pack used by the offline \
                      Bevy and PixiJS examples."
            .to_owned(),
        license: CharacterPackLicense {
            license: "MIT".to_owned(),
            license_url: "https://opensource.org/license/mit".to_owned(),
            license_text: "MIT License\n\nCopyright (c) 2026 Weave contributors\n\nPermission is \
                           hereby granted, free of charge, to any person obtaining a copy of this \
                           software and associated documentation files (the \"Software\"), to \
                           deal in the Software without restriction, including without limitation \
                           the rights to use, copy, modify, merge, publish, distribute, \
                           sublicense, and/or sell copies of the Software, and to permit persons \
                           to whom the Software is furnished to do so, subject to the following \
                           conditions:\n\nThe above copyright notice and this permission notice \
                           shall be included in all copies or substantial portions of the \
                           Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF \
                           ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE \
                           WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND \
                           NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE \
                           LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION \
                           OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION \
                           WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."
                .to_owned(),
            notices: Vec::new(),
        },
    }
}

fn profile_value() -> Result<Value, Box<dyn std::error::Error>> {
    let personality = json!({
        "honesty_humility": {
            "factor": trait_score(0.68),
            "sincerity": trait_score(0.72),
        },
        "emotionality": {
            "factor": trait_score(0.44),
        },
        "extraversion": {
            "factor": trait_score(0.61),
        },
        "agreeableness": {
            "factor": trait_score(0.77),
        },
        "conscientiousness": {
            "factor": trait_score(0.55),
        },
        "openness": {
            "factor": trait_score(0.82),
            "creativity": trait_score(0.88),
        },
    });
    let decoded_personality: HexacoProfile = serde_json::from_value(personality.clone())?;
    let ocean = serde_json::to_value(derive_ocean(&decoded_personality))?;

    let alignment_application = format!("alignment_apply_{}", &ALIGNMENT_APPLIED_SHA256[..16]);
    let temporal_review = format!("temporal_review_{}", &DATE_REVIEW_SHA256[..16]);

    let profile = json!({
        "profile_format_version": 1,
        "id": CHARACTER_ID,
        "canon": {
            "identity": {
                "display_name": attributed(json!("Mira Thistlewood")),
                "aliases": attributed(json!(["Mira"])),
            },
            "birth_date": attributed(json!({
                "precision": "full",
                "calendar": "proleptic_gregorian",
                "year": 2001,
                "month": 6,
                "day": 21,
            })),
            "personality": personality,
            "inner_life": {
                "steady_hope": {
                    "id": "steady_hope",
                    "category": "value",
                    "content": attributed(json!(
                        "Keeps a small garden because patient work makes a place feel shared."
                    )),
                },
            },
            "voice": {
                "gentle_directness": {
                    "id": "gentle_directness",
                    "category": "tone",
                    "content": attributed(json!(
                        "Speaks plainly, then leaves room for the other person to decide."
                    )),
                },
            },
        },
        "extensions": {
            "org.weave.character.identity_presentation": {
                "kind": "identity_presentation",
                "record": {
                    "header": extension_header(
                        "org.weave.character.identity_presentation",
                        vec![SOURCE_ID],
                    ),
                    "value": {
                        "identity_refs": [],
                        "presentation_refs": [],
                        "pronouns": attributed(json!({
                            "subject": "they",
                            "object": "them",
                            "possessive_determiner": "their",
                            "possessive_pronoun": "theirs",
                            "reflexive": "themself",
                        })),
                        "palette": attributed(json!({
                            "colors": {
                                "accent": "#7BC96F",
                                "background": "#1B2A24",
                                "foreground": "#E8F5E9",
                            },
                        })),
                        "style_tags": attributed(json!(["earth_tone", "practical"])),
                    },
                },
            },
            "org.weave.character.alignment": {
                "kind": "alignment_view",
                "record": {
                    "header": extension_header(
                        "org.weave.character.alignment",
                        vec![alignment_application.as_str(), SOURCE_ID],
                    ),
                    "value": {
                        "view_id": ALIGNMENT_ID,
                        "pack": {
                            "id": ALIGNMENT_ID,
                            "version": "1.0.0",
                            "sha256": ALIGNMENT_PACK_SHA256,
                        },
                        "values": {
                            "horizon": {
                                "id": "horizon",
                                "label_id": "seeking",
                                "label": "Seeking",
                                "decision": "reviewed",
                                "score_micros": 640000,
                                "coverage_micros": 1000000,
                                "explanation": "A fictional shorthand for how this character \
                                                meets unfamiliar paths. It is not a diagnosis.",
                                "input_paths": ["canon.personality.openness.factor"],
                            },
                        },
                        "input_paths": ["canon.personality.openness.factor"],
                        "review_sha256": ALIGNMENT_REVIEW_SHA256,
                        "applied_sha256": ALIGNMENT_APPLIED_SHA256,
                    },
                },
            },
            "org.weave.character.date_context": {
                "kind": "date_context",
                "record": {
                    "header": extension_header(
                        "org.weave.character.date_context",
                        vec![CALENDAR_SOURCE_ID, SOURCE_ID, temporal_review.as_str()],
                    ),
                    "value": {
                        "context_pack": "org.weave.example.context.midsummer",
                        "context_version": "1.0.0",
                        "context_hash": DATE_PACK_SHA256,
                        "accepted_record_ids": ["long_summer_evening"],
                        "accepted_cues": {
                            "cue_000000000000000000000001": {
                                "id": "cue_000000000000000000000001",
                                "record_id": "long_summer_evening",
                                "pack": {
                                    "id": "org.weave.example.context.midsummer",
                                    "version": "1.0.0",
                                    "sha256": DATE_PACK_SHA256,
                                },
                                "kind": "memory",
                                "content": "Midsummer evenings stretched late enough to finish a \
                                            long walk before dusk.",
                                "relevance": 0.75,
                                "uncertainty": "exact",
                                "sensitivity": "low",
                                "decision": "accepted",
                                "fact_source_ids": [CALENDAR_SOURCE_ID],
                                "cue_source_ids": [SOURCE_ID],
                                "source_ids": [CALENDAR_SOURCE_ID, SOURCE_ID],
                                "review_sha256": DATE_REVIEW_SHA256,
                            },
                        },
                    },
                },
            },
        },
        "derived": {
            "ocean": ocean,
        },
        "provenance": {
            "sources": [
                {
                    "id": CALENDAR_SOURCE_ID,
                    "kind": "original",
                    "url": "https://github.com/chrisgliddon/weave",
                    "revision": "character-pack-example-v1",
                    "license": "MIT",
                    "license_url": "https://github.com/chrisgliddon/weave/blob/main/LICENSE",
                    "attribution": "Original synthetic calendar context authored for this example.",
                    "modified": false,
                },
                {
                    "id": SOURCE_ID,
                    "kind": "original",
                    "url": "https://github.com/chrisgliddon/weave",
                    "revision": "character-pack-example-v1",
                    "license": "MIT",
                    "license_url": "https://github.com/chrisgliddon/weave/blob/main/LICENSE",
                    "attribution": "Original synthetic character, presentation, and alignment data \
                                    authored for this example.",
                    "modified": false,
                },
            ],
            "transformations": [
                {
                    "id": alignment_application.clone(),
                    "inputs": [SOURCE_ID],
                    "description": "Apply the reviewed original alignment view to the synthetic \
                                    character.",
                },
                {
                    "id": temporal_review.clone(),
                    "inputs": [CALENDAR_SOURCE_ID],
                    "description": "Review and accept one original synthetic date-context cue.",
                },
            ],
            "claims": {
                "pack": [
                    alignment_application,
                    CALENDAR_SOURCE_ID,
                    SOURCE_ID,
                    temporal_review,
                ],
            },
        },
    });
    Ok(profile)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let profile: CharacterProfile = serde_json::from_value(profile_value()?)?;
    let pack = CharacterAuthoringPack::from_profile(metadata(), profile, Vec::new())?;
    let runtime = pack.to_runtime_pack()?;

    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    fs::create_dir_all(&fixtures)?;
    fs::write(
        fixtures.join("authoring-pack.character-pack.json"),
        pack.to_json()?,
    )?;
    fs::write(
        fixtures.join("runtime-pack.character-pack.json"),
        runtime.to_json()?,
    )?;

    println!(
        "Wrote {} authoring entries and {} runtime entries for {}@{}",
        pack.characters.len(),
        runtime.characters.len(),
        pack.id,
        pack.version,
    );
    Ok(())
}
