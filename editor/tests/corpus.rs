//! The public-domain example corpus must load through the real manuscript store.
//!
//! These tests are the reason the corpus is committed as data rather than
//! generated at test time: the editor has to read exactly the bytes a user
//! would open, and the recorded provenance has to match what the model sees.

use std::path::{Path, PathBuf};

use weave_editor::manuscript::{ChapterId, ManuscriptStore};
use weave_editor::manuscript_view::{ManuscriptProjection, ProjectionOptions};

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

#[test]
fn the_alice_board_shows_twelve_titled_cards() {
    let directory = corpus_root().join("alice-adventures-in-wonderland");
    let store = ManuscriptStore::in_directory(&directory);
    let tree = store.read_tree().expect("tree");
    let projection =
        ManuscriptProjection::build(&tree, Some(store.bodies()), &ProjectionOptions::default());

    assert_eq!(projection.container_title(), "Manuscript");
    assert_eq!(projection.cards().len(), 12);
    assert_eq!(projection.cards()[0].title, "Down the Rabbit-Hole");
    assert_eq!(projection.cards()[0].ordinal, 1);
    assert_eq!(projection.cards()[0].total, 12);
    assert!(
        projection.cards()[0].word_count > 1000,
        "the first chapter should report its real word count"
    );
    assert!(!projection.cards()[0].excerpt.is_empty());
    assert!(
        projection
            .cards()
            .iter()
            .all(|card| card.accessible_label.contains("index cards")),
        "every card should name its ordinal for a screen reader"
    );
}

#[test]
fn selecting_a_story_shows_its_numbered_scenes() {
    let directory = corpus_root().join("the-adventures-of-sherlock-holmes");
    let store = ManuscriptStore::in_directory(&directory);
    let tree = store.read_tree().expect("tree");
    let story = tree
        .child_ids(ChapterId::ROOT)
        .into_iter()
        .find(|id| {
            tree.chapter(*id)
                .is_some_and(|chapter| chapter.title == "A Scandal in Bohemia")
        })
        .expect("the sectioned story");

    let options = ProjectionOptions {
        selected: story,
        ..ProjectionOptions::default()
    };
    let projection = ManuscriptProjection::build(&tree, Some(store.bodies()), &options);
    assert_eq!(projection.container_title(), "A Scandal in Bohemia");
    assert_eq!(projection.cards().len(), 3);
    assert_eq!(
        projection
            .cards()
            .iter()
            .map(|card| card.title.as_str())
            .collect::<Vec<_>>(),
        ["I", "II", "III"]
    );
    assert!(projection.cards().iter().all(|card| card.word_count > 0));
}

#[test]
fn an_empty_container_names_itself_on_the_board() {
    let directory = corpus_root().join("alice-adventures-in-wonderland");
    let store = ManuscriptStore::in_directory(&directory);
    let tree = store.read_tree().expect("tree");
    let chapter = tree.child_ids(ChapterId::ROOT)[0];

    let options = ProjectionOptions {
        selected: chapter,
        ..ProjectionOptions::default()
    };
    let projection = ManuscriptProjection::build(&tree, Some(store.bodies()), &options);
    assert!(projection.cards().is_empty());
    assert!(projection.is_empty());
    assert!(
        projection.empty_state().contains("Down the Rabbit-Hole"),
        "the empty state should name the container, got {:?}",
        projection.empty_state()
    );
}

#[test]
fn editing_a_real_chapter_updates_its_word_count() {
    use weave_editor::manuscript::{
        ManuscriptFormat, Position, Selection, StyleSheet, insert, layout,
    };

    let directory = corpus_root().join("alice-adventures-in-wonderland");
    let store = ManuscriptStore::in_directory(&directory);
    let tree = store.read_tree().expect("tree");
    let chapter = tree.child_ids(ChapterId::ROOT)[0];
    let mut body = store.bodies().read(chapter).expect("body");
    let before = body.word_count();

    let selection = insert(
        &mut body,
        Selection::collapsed(Position {
            paragraph: 0,
            offset: 0,
        }),
        "Preface. ",
    );
    assert_eq!(body.word_count(), before + 1);
    assert_eq!(selection.caret.offset, 9);

    let lines = layout(&body, &StyleSheet::new(), ManuscriptFormat::default());
    assert!(lines.len() >= body.paragraphs.len());
    assert_eq!(lines[0].paragraph, 0);
    assert!(lines[0].segments[0].text.starts_with("Preface."));
}
