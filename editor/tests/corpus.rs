//! The public-domain example corpus must load through the real manuscript store.
//!
//! These tests are the reason the corpus is committed as data rather than
//! generated at test time: the editor has to read exactly the bytes a user
//! would open, and the recorded provenance has to match what the model sees.

use std::path::{Path, PathBuf};

use weave_editor::manuscript::{ChapterId, ManuscriptStore};

/// The corpus lives beside the workspace, not inside the editor crate.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("examples")
        .join("manuscripts")
}

fn projects() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(corpus_root())
        .expect("corpus root")
        .filter_map(|entry| {
            let path = entry.expect("entry").path();
            path.is_dir().then_some(path)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn the_corpus_is_present() {
    assert_eq!(projects().len(), 4, "expected four example manuscripts");
}

#[test]
fn every_project_loads_and_matches_its_provenance() {
    for directory in projects() {
        let store = ManuscriptStore::in_directory(&directory);
        let tree = store.read_tree().unwrap_or_else(|error| {
            panic!("{} did not load: {error}", directory.display());
        });
        tree.validate()
            .unwrap_or_else(|error| panic!("{} is inconsistent: {error}", directory.display()));

        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("provenance.json"))
                .unwrap_or_else(|_| panic!("{} has no provenance", directory.display())),
        )
        .expect("provenance is valid JSON");

        let chapters = manifest["chapters"].as_array().expect("chapter list");
        assert_eq!(
            tree.next_id,
            2 + chapters.len() as u64,
            "{}: next id does not follow the chapter count",
            directory.display()
        );

        let mut words = 0;
        for chapter in chapters {
            let id = ChapterId(chapter["id"].as_u64().expect("chapter id"));
            let record = tree
                .chapter(id)
                .unwrap_or_else(|| panic!("{}: chapter {id} is missing", directory.display()));
            assert_eq!(
                record.title,
                chapter["title"].as_str().expect("title"),
                "{}: chapter {id} title",
                directory.display()
            );

            let body = store.bodies().read(id).expect("body");
            assert_eq!(
                body.paragraphs.len(),
                chapter["paragraphs"].as_u64().expect("paragraph count") as usize,
                "{}: chapter {id} paragraph count",
                directory.display()
            );
            assert_eq!(
                body.word_count(),
                chapter["words"].as_u64().expect("word count") as usize,
                "{}: chapter {id} word count",
                directory.display()
            );
            words += body.word_count();
        }

        assert_eq!(
            words,
            manifest["totals"]["words"].as_u64().expect("total words") as usize,
            "{}: sum of chapter words does not match the total",
            directory.display()
        );
    }
}

#[test]
fn the_trash_sorts_last_in_every_project() {
    for directory in projects() {
        let tree = ManuscriptStore::in_directory(&directory)
            .read_tree()
            .expect("tree");
        let children = tree.child_ids(ChapterId::ROOT);
        assert_eq!(
            children.last().copied(),
            Some(ChapterId::TRASH),
            "{}: trash is not last",
            directory.display()
        );
    }
}

#[test]
fn the_sectioned_story_nests_roman_numeral_scenes() {
    let directory = corpus_root().join("the-adventures-of-sherlock-holmes");
    let tree = ManuscriptStore::in_directory(&directory)
        .read_tree()
        .expect("tree");

    let story = tree
        .child_ids(ChapterId::ROOT)
        .into_iter()
        .find(|id| {
            tree.chapter(*id)
                .is_some_and(|chapter| chapter.title == "A Scandal in Bohemia")
        })
        .expect("the sectioned story");

    let scenes = tree.child_ids(story);
    assert_eq!(scenes.len(), 3, "expected three numbered parts");
    for scene in scenes {
        let record = tree.chapter(scene).expect("scene");
        assert!(
            record
                .title
                .chars()
                .all(|character| "IVX".contains(character)),
            "expected a roman numeral scene title, found {:?}",
            record.title
        );
    }
}

#[test]
fn reading_one_corpus_chapter_does_not_pull_in_the_next() {
    let directory = corpus_root().join("alice-adventures-in-wonderland");
    let store = ManuscriptStore::in_directory(&directory);
    let tree = store.read_tree().expect("tree");
    let children = tree.child_ids(ChapterId::ROOT);

    let first = store.bodies().read(children[0]).expect("first body");
    let second = store.bodies().read(children[1]).expect("second body");

    assert!(!first.paragraphs.is_empty());
    assert!(!second.paragraphs.is_empty());
    assert_ne!(
        first.text(),
        second.text(),
        "two chapters of Alice should not share a body"
    );
}
