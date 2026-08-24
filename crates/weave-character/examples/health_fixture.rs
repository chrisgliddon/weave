use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use weave_character::*;

const PROFILE: &str =
    include_str!("../../../examples/domain-modules/weave-character/profile.character.json");
const UNSAFE_EXPRESSION_PACK: &str = include_str!(
    "../../../examples/domain-modules/weave-character/expression/invalid/restricted-placeholder.expression-pack.json"
);

struct FixtureProject {
    name: &'static str,
    manifest: CharacterHealthManifest,
    sources: BTreeMap<String, String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--check".to_owned());
    let write = match mode.as_str() {
        "--write" => true,
        "--check" => false,
        _ => return Err("expected --write or --check".into()),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root")?
        .to_path_buf();
    let fixture = root.join("examples/domain-modules/weave-character/health");
    let schemas = root.join("schemas");

    let profile = clean_profile()?;
    let projects = vec![
        healthy_project(&profile)?,
        incomplete_project(&profile)?,
        stale_project(&profile)?,
        unsafe_project(&profile)?,
        malformed_project(&profile),
        migration_project(&profile)?,
    ];
    for project in projects {
        write_fixture_project(write, &fixture, project)?;
    }
    write_text(
        write,
        &schemas.join("weave-character-health-manifest-v1.schema.json"),
        &character_health_manifest_schema()?,
    )?;
    write_text(
        write,
        &schemas.join("weave-character-health-report-v1.schema.json"),
        &character_health_report_schema()?,
    )?;
    Ok(())
}

fn clean_profile() -> Result<CharacterProfile, Box<dyn std::error::Error>> {
    let mut profile = CharacterProfile::from_json(PROFILE)?;
    profile.extensions.clear();
    profile.suggestions.clear();
    validate_profile(&profile)?;
    Ok(profile)
}

fn collection(profile: CharacterProfile, id: &str) -> CharacterCollection {
    CharacterCollection {
        collection_format_version: CHARACTER_COLLECTION_FORMAT_VERSION,
        id: id.to_owned(),
        revision: 1,
        characters: BTreeMap::from([(profile.id.clone(), profile)]),
    }
}

fn healthy_project(
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let collection = collection(profile.clone(), "org.weave.character.health.healthy");
    let runtime = character_domain_pack(
        profile,
        "ari_vale_health",
        "1.0.0",
        "Ari Vale Health Runtime",
    )?;
    let documents = vec![
        source_ref(
            "org.weave.health.healthy.collection_json",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.healthy.collection",
            true,
            None,
        ),
        source_ref(
            "org.weave.health.healthy.collection_ron",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Ron,
            "collection.character-collection.ron",
            "org.weave.health.healthy.collection",
            false,
            None,
        ),
        source_ref(
            "org.weave.health.healthy.runtime_json",
            CharacterHealthDocumentKind::RuntimeDomainPack,
            CharacterHealthDocumentFormat::Json,
            "ari_vale.weave-domain.json",
            "org.weave.health.healthy.runtime",
            false,
            Some(&profile.id),
        ),
        source_ref(
            "org.weave.health.healthy.runtime_ron",
            CharacterHealthDocumentKind::RuntimeDomainPack,
            CharacterHealthDocumentFormat::Ron,
            "ari_vale.weave-domain.ron",
            "org.weave.health.healthy.runtime",
            false,
            Some(&profile.id),
        ),
    ];
    Ok(FixtureProject {
        name: "healthy",
        manifest: manifest(
            "org.weave.character.health.healthy_project",
            documents.clone(),
            CharacterHealthPolicy::default(),
            profile,
        ),
        sources: BTreeMap::from([
            (documents[0].id.clone(), collection.to_json()?),
            (documents[1].id.clone(), collection.to_ron()?),
            (documents[2].id.clone(), runtime.to_json()?),
            (documents[3].id.clone(), runtime.to_ron()?),
        ]),
    })
}

fn incomplete_project(
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let mut incomplete = profile.clone();
    incomplete.canon.personality.openness.creativity = None;
    recompute_derived(&mut incomplete);
    validate_profile(&incomplete)?;
    typed_single_project(
        "incomplete",
        "org.weave.character.health.incomplete",
        collection(incomplete, "org.weave.character.health.incomplete"),
        profile,
    )
}

fn stale_project(profile: &CharacterProfile) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let mut stale = profile.clone();
    stale.canon.identity.display_name.freshness = Freshness::Stale;
    raw_single_project(
        "stale",
        "org.weave.character.health.stale",
        weave_domain::to_pretty_json(&collection(stale, "org.weave.character.health.stale"))?,
        profile,
    )
}

fn unsafe_project(
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let collection = collection(profile.clone(), "org.weave.character.health.unsafe");
    let documents = vec![
        source_ref(
            "org.weave.health.unsafe.collection",
            CharacterHealthDocumentKind::CharacterCollection,
            CharacterHealthDocumentFormat::Json,
            "collection.character-collection.json",
            "org.weave.health.unsafe.collection",
            true,
            None,
        ),
        source_ref(
            "org.weave.health.unsafe.expression",
            CharacterHealthDocumentKind::ExpressionPack,
            CharacterHealthDocumentFormat::Json,
            "unsafe.expression-pack.json",
            "org.weave.health.unsafe.expression",
            false,
            None,
        ),
    ];
    Ok(FixtureProject {
        name: "unsafe",
        manifest: manifest(
            "org.weave.character.health.unsafe_project",
            documents.clone(),
            policy_without_pairs(),
            profile,
        ),
        sources: BTreeMap::from([
            (documents[0].id.clone(), collection.to_json()?),
            (documents[1].id.clone(), UNSAFE_EXPRESSION_PACK.to_owned()),
        ]),
    })
}

fn malformed_project(profile: &CharacterProfile) -> FixtureProject {
    let id = "org.weave.health.malformed.collection";
    let documents = vec![source_ref(
        id,
        CharacterHealthDocumentKind::CharacterCollection,
        CharacterHealthDocumentFormat::Json,
        "collection.character-collection.json",
        "org.weave.health.malformed.collection",
        true,
        None,
    )];
    FixtureProject {
        name: "malformed",
        manifest: manifest(
            "org.weave.character.health.malformed_project",
            documents,
            policy_without_pairs(),
            profile,
        ),
        sources: BTreeMap::from([(
            id.to_owned(),
            "{\n  \"collection_format_version\": 1,\n  \"id\": \"org.weave.character.health.malformed\",\n  \"revision\": 1\n}\n"
                .to_owned(),
        )]),
    }
}

fn migration_project(
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let mut migration = collection(
        profile.clone(),
        "org.weave.character.health.migration_required",
    );
    migration.collection_format_version = 0;
    raw_single_project(
        "migration-required",
        "org.weave.character.health.migration_required",
        weave_domain::to_pretty_json(&migration)?,
        profile,
    )
}

fn typed_single_project(
    name: &'static str,
    id: &str,
    collection: CharacterCollection,
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    raw_single_project(name, id, collection.to_json()?, profile)
}

fn raw_single_project(
    name: &'static str,
    id: &str,
    source: String,
    profile: &CharacterProfile,
) -> Result<FixtureProject, Box<dyn std::error::Error>> {
    let document_id = format!("org.weave.health.{}.collection", name.replace('-', "_"));
    let documents = vec![source_ref(
        &document_id,
        CharacterHealthDocumentKind::CharacterCollection,
        CharacterHealthDocumentFormat::Json,
        "collection.character-collection.json",
        &format!("org.weave.health.{}.collection", name.replace('-', "_")),
        true,
        None,
    )];
    Ok(FixtureProject {
        name,
        manifest: manifest(id, documents, policy_without_pairs(), profile),
        sources: BTreeMap::from([(document_id, source)]),
    })
}

fn manifest(
    id: &str,
    documents: Vec<CharacterHealthDocumentRef>,
    policy: CharacterHealthPolicy,
    profile: &CharacterProfile,
) -> CharacterHealthManifest {
    CharacterHealthManifest {
        manifest_format_version: CHARACTER_HEALTH_MANIFEST_FORMAT_VERSION,
        id: id.to_owned(),
        documents,
        policy,
        suppressions: Vec::new(),
        provenance: profile.provenance.clone(),
    }
}

fn policy_without_pairs() -> CharacterHealthPolicy {
    CharacterHealthPolicy {
        require_portable_pairs: false,
        ..CharacterHealthPolicy::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn source_ref(
    id: &str,
    kind: CharacterHealthDocumentKind,
    format: CharacterHealthDocumentFormat,
    path: &str,
    logical_id: &str,
    primary: bool,
    character_id: Option<&str>,
) -> CharacterHealthDocumentRef {
    CharacterHealthDocumentRef {
        id: id.to_owned(),
        kind,
        format,
        path: path.to_owned(),
        logical_id: logical_id.to_owned(),
        primary,
        character_id: character_id.map(ToOwned::to_owned),
    }
}

fn write_fixture_project(
    write: bool,
    root: &Path,
    project: FixtureProject,
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = root.join(project.name);
    for reference in &project.manifest.documents {
        let source = project
            .sources
            .get(&reference.id)
            .ok_or("fixture source is absent from the generated project")?;
        write_text(write, &directory.join(&reference.path), source)?;
    }
    let loaded = CharacterHealthProject::new(project.manifest.clone(), project.sources)?;
    let report = audit_character_health(&loaded)?;
    write_pair(
        write,
        &directory,
        "project.health-manifest",
        &project.manifest,
    )?;
    write_pair(write, &directory, "report.health-report", &report)?;
    write_text(
        write,
        &directory.join("report.health-report.txt"),
        &render_character_health_text(&report)?,
    )
}

fn write_pair<T>(
    write: bool,
    directory: &Path,
    stem: &str,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::Serialize,
{
    write_text(
        write,
        &directory.join(format!("{stem}.json")),
        &weave_domain::to_pretty_json(value)?,
    )?;
    write_text(
        write,
        &directory.join(format!("{stem}.ron")),
        &weave_domain::to_pretty_ron(value)?,
    )
}

fn write_text(write: bool, path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        return Ok(());
    }
    let existing = fs::read_to_string(path)?;
    if existing != content {
        return Err(format!("checked fixture is stale: {}", path.display()).into());
    }
    Ok(())
}
