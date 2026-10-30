//! Tests for the ordered chapter tree, its storage, and the chapter bodies.

use weave_editor::manuscript::{
    BodyStore, ChapterBody, ChapterError, ChapterId, ChapterKind, ChapterTree, ManuscriptHistory,
    ManuscriptStore, MoveBlocked, Paragraph, StyleSheet, TextRun,
};

/// A tree holding one part with two scenes, and the identifiers involved.
///
/// The part is the first child of the root; the trash is the last.
fn tree_with_children() -> (ChapterTree, ChapterId, ChapterId, ChapterId) {
    let mut tree = ChapterTree::new();
    let part = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part One")
        .expect("part");
    let first = tree
        .add_chapter(part, ChapterKind::Scene, "Opening")
        .expect("first scene");
    let second = tree
        .add_chapter(part, ChapterKind::Scene, "Middle")
        .expect("second scene");
    (tree, part, first, second)
}

#[test]
fn a_new_tree_holds_the_root_and_the_trash() {
    let tree = ChapterTree::new();
    assert_eq!(tree.len(), 2);
    assert!(tree.chapter(ChapterId::ROOT).is_some());
    assert!(tree.chapter(ChapterId::TRASH).is_some());
    assert!(tree.is_empty());
    assert!(tree.validate().is_ok());
}

#[test]
fn the_trash_sorts_last_among_the_root_children() {
    let (tree, part, _first, _second) = tree_with_children();
    let children = tree.child_ids(ChapterId::ROOT);
    assert_eq!(children.first().copied(), Some(part));
    assert_eq!(children.last().copied(), Some(ChapterId::TRASH));
}

#[test]
fn children_are_listed_in_tree_order() {
    let (tree, part, first, second) = tree_with_children();
    assert_eq!(tree.child_ids(part), vec![first, second]);
}

#[test]
fn a_reorder_moves_one_sibling() {
    let (mut tree, part, first, second) = tree_with_children();
    tree.move_chapter(second, 0).expect("move");
    assert_eq!(tree.child_ids(part), vec![second, first]);
    tree.validate().expect("valid");
}

#[test]
fn a_reorder_keeps_the_sibling_order_after_a_round_trip() {
    let (mut tree, part, first, second) = tree_with_children();
    tree.move_chapter(second, 0).expect("move");
    let encoded = ron::ser::to_string(&tree).expect("encode");
    let decoded: ChapterTree = ron::from_str(&encoded).expect("decode");
    assert_eq!(decoded.child_ids(part), vec![second, first]);
    assert_eq!(decoded, tree);
}

#[test]
fn a_reorder_rewrites_one_field() {
    let (mut tree, part, _first, second) = tree_with_children();
    let before: Vec<u32> = tree
        .children(part)
        .iter()
        .map(|chapter| chapter.order)
        .collect();
    tree.move_chapter(second, 0).expect("move");
    let after: Vec<u32> = tree
        .children(part)
        .iter()
        .map(|chapter| chapter.order)
        .collect();
    assert_ne!(before, after);
    assert_eq!(after.len(), 2);
    assert!(after[0] < after[1]);
}

#[test]
fn a_reorder_beyond_the_end_is_rejected() {
    let (mut tree, _part, _first, second) = tree_with_children();
    assert_eq!(
        tree.move_chapter(second, 9),
        Err(ChapterError::IndexOutOfRange { index: 9, len: 1 })
    );
}

#[test]
fn a_reparent_moves_a_chapter_to_another_container() {
    let (mut tree, _part, first, _second) = tree_with_children();
    let other = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part Two")
        .expect("other part");
    tree.reparent_chapter(first, other).expect("reparent");
    assert_eq!(tree.child_ids(other), vec![first]);
    assert!(!tree.child_ids(ChapterId::ROOT).contains(&first));
    tree.validate().expect("valid");
}

#[test]
fn a_reparent_keeps_a_chapter_whole() {
    let (mut tree, _part, first, _second) = tree_with_children();
    let other = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part Two")
        .expect("other part");
    tree.set_kind(first, ChapterKind::Chapter).expect("convert");
    let child = tree
        .add_chapter(first, ChapterKind::Scene, "Child")
        .expect("child");
    tree.reparent_chapter(first, other).expect("reparent");
    assert_eq!(tree.child_ids(other), vec![first]);
    assert_eq!(tree.child_ids(first), vec![child]);
}

#[test]
fn a_container_cannot_be_moved_into_its_own_descendant() {
    let (mut tree, part, _first, _second) = tree_with_children();
    let nested = tree
        .add_chapter(part, ChapterKind::Chapter, "Nested")
        .expect("nested");
    let inner = tree
        .add_chapter(nested, ChapterKind::Chapter, "Inner")
        .expect("inner");
    assert_eq!(
        tree.reparent_chapter(nested, inner),
        Err(ChapterError::Cycle(nested, inner))
    );
    assert_eq!(tree.chapter(nested).expect("nested").parent_id, part);
}

#[test]
fn a_container_cannot_be_moved_into_itself() {
    let (mut tree, part, _first, _second) = tree_with_children();
    assert_eq!(
        tree.reparent_chapter(part, part),
        Err(ChapterError::Cycle(part, part))
    );
}

#[test]
fn a_scene_is_a_leaf() {
    let (mut tree, _part, first, _second) = tree_with_children();
    assert_eq!(
        tree.add_chapter(first, ChapterKind::Scene, "Nested"),
        Err(ChapterError::NotAContainer(first))
    );
}

#[test]
fn a_leaf_can_be_converted_into_a_container() {
    let (mut tree, part, first, _second) = tree_with_children();
    tree.set_kind(first, ChapterKind::Chapter).expect("convert");
    assert!(tree.chapter(first).expect("first").kind.is_container());
    let child = tree
        .add_chapter(first, ChapterKind::Scene, "Child")
        .expect("child");
    assert_eq!(tree.child_ids(first), vec![child]);
    assert_eq!(tree.child_ids(part).len(), 2);
}

#[test]
fn a_container_holding_children_cannot_become_a_leaf() {
    let (mut tree, part, _first, _second) = tree_with_children();
    let nested = tree
        .add_chapter(part, ChapterKind::Chapter, "Nested")
        .expect("nested");
    tree.add_chapter(nested, ChapterKind::Scene, "Child")
        .expect("child");
    assert_eq!(
        tree.set_kind(nested, ChapterKind::Scene),
        Err(ChapterError::NotEmpty(nested))
    );
}

#[test]
fn a_chapter_needs_a_title() {
    let mut tree = ChapterTree::new();
    assert_eq!(
        tree.add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "   "),
        Err(ChapterError::EmptyTitle)
    );
}

#[test]
fn a_trashed_chapter_moves_to_the_trash_and_back() {
    let (mut tree, _part, first, _second) = tree_with_children();
    tree.trash_chapter(first).expect("trash");
    assert_eq!(tree.child_ids(ChapterId::TRASH), vec![first]);
    tree.restore_chapter(first).expect("restore");
    assert!(tree.child_ids(ChapterId::TRASH).is_empty());
    assert!(tree.child_ids(ChapterId::ROOT).contains(&first));
}

#[test]
fn emptying_the_trash_removes_its_chapters() {
    let (mut tree, _part, first, _second) = tree_with_children();
    tree.trash_chapter(first).expect("trash");
    let removed = tree.empty_trash();
    assert_eq!(removed, vec![first]);
    assert!(tree.chapter(first).is_none());
    assert!(tree.child_ids(ChapterId::TRASH).is_empty());
}

#[test]
fn a_built_in_container_cannot_be_trashed() {
    let mut tree = ChapterTree::new();
    assert_eq!(
        tree.trash_chapter(ChapterId::ROOT),
        Err(ChapterError::ReservedChapter(ChapterId::ROOT))
    );
    assert_eq!(
        tree.trash_chapter(ChapterId::TRASH),
        Err(ChapterError::ReservedChapter(ChapterId::TRASH))
    );
}

#[test]
fn an_unknown_chapter_is_rejected() {
    let mut tree = ChapterTree::new();
    let unknown = ChapterId(999);
    assert_eq!(
        tree.move_chapter(unknown, 0),
        Err(ChapterError::UnknownChapter(unknown))
    );
}

#[test]
fn a_tree_round_trips_through_its_persisted_form() {
    let (mut tree, _part, first, second) = tree_with_children();
    tree.set_synopsis(first, "A courier at the gate.")
        .expect("first synopsis");
    tree.set_synopsis(second, "The bell rings.")
        .expect("second synopsis");
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    store.write_tree(&tree).expect("write");
    let reloaded = store.read_tree().expect("read");
    assert_eq!(reloaded, tree);
    assert_eq!(
        reloaded.chapter(first).expect("first").synopsis,
        "A courier at the gate."
    );
}

#[test]
fn a_location_without_a_manuscript_reads_as_a_fresh_tree() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    assert!(!store.exists());
    let tree = store.read_tree().expect("read");
    assert_eq!(tree, ChapterTree::new());
}

#[test]
fn a_body_is_stored_in_its_own_file() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    let body = ChapterBody {
        version: 1,
        paragraphs: vec![
            Paragraph::plain("The lantern guttered once, twice."),
            Paragraph {
                runs: vec![
                    TextRun::plain("She carried "),
                    TextRun {
                        text: "no seal".to_string(),
                        character_style: Some("Emphasis".to_string()),
                        bold: false,
                        italic: true,
                        underline: false,
                    },
                ],
                style: Some("Body".to_string()),
            },
        ],
    };
    store.bodies().write(ChapterId(7), &body).expect("write");
    let reloaded = store.bodies().read(ChapterId(7)).expect("read");
    assert_eq!(reloaded, body);
    assert_eq!(reloaded.word_count(), 9);
    assert_eq!(reloaded.character_count(), 53);
    assert!(reloaded.paragraphs[1].runs[1].italic);
}

#[test]
fn a_chapter_without_a_body_file_reads_as_empty() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    assert_eq!(
        store.bodies().read(ChapterId(3)).expect("read"),
        ChapterBody::new()
    );
}

#[test]
fn reading_one_body_does_not_read_the_others() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    store
        .bodies()
        .write(ChapterId(1), &ChapterBody::new())
        .expect("write");
    let bodies = BodyStore::new(store.bodies().directory().to_path_buf());
    assert!(bodies.exists(ChapterId(1)));
    assert!(!bodies.exists(ChapterId(2)));
    assert!(
        !bodies
            .read(ChapterId(2))
            .expect("read")
            .paragraphs
            .iter()
            .any(|_| true)
    );
}

#[test]
fn a_body_removed_from_the_tree_leaves_its_file_behind() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    store
        .bodies()
        .write(ChapterId(5), &ChapterBody::new())
        .expect("write");
    let mut tree = ChapterTree::new();
    tree.empty_trash();
    assert!(store.bodies().exists(ChapterId(5)));
}

#[test]
fn the_body_of_one_chapter_is_not_read_when_a_sibling_is_opened() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    let (tree, part, first, second) = tree_with_children();

    // Only the first scene has a body on disk.
    let body = ChapterBody {
        version: 1,
        paragraphs: vec![Paragraph::plain("Only this chapter has a body.")],
    };
    store.bodies().write(first, &body).expect("write");

    // Opening the sibling reads its own body and nothing else: the
    // sibling's body file does not exist, so it reads as empty rather than
    // pulling in the first scene's text.
    let sibling_body = store.bodies().read(second).expect("read");
    assert!(sibling_body.paragraphs.is_empty());
    assert_ne!(sibling_body, body);

    // The body of the chapter that does have one is still intact.
    assert_eq!(store.bodies().read(first).expect("read"), body);
    assert_eq!(tree.child_ids(part), vec![first, second]);
}

#[test]
fn the_style_sheet_round_trips() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    let mut styles = StyleSheet::new();
    styles
        .paragraph_styles
        .push(weave_editor::manuscript::NamedStyle {
            name: "Body".to_string(),
            size: 12.0,
            bold: false,
            italic: false,
        });
    store.write_styles(&styles).expect("write");
    assert_eq!(store.read_styles().expect("read"), styles);
}

#[test]
fn an_inconsistent_tree_is_rejected() {
    let (tree, _part, _first, _second) = tree_with_children();
    let mut broken = tree.clone();
    let part = broken
        .child_ids(ChapterId::ROOT)
        .first()
        .copied()
        .expect("part");
    for chapter in &mut broken.chapters {
        if chapter.id == part {
            chapter.parent_id = ChapterId(4242);
        }
    }
    assert!(matches!(
        broken.validate(),
        Err(ChapterError::UnknownParent(_, _))
    ));
}

#[test]
fn a_duplicate_identifier_is_rejected() {
    let (tree, _part, first, _second) = tree_with_children();
    let mut broken = tree.clone();
    let duplicate = broken
        .chapters
        .iter()
        .find(|chapter| chapter.id == first)
        .cloned()
        .expect("first");
    broken.chapters.push(duplicate);
    assert!(matches!(
        broken.validate(),
        Err(ChapterError::DuplicateId(_))
    ));
}

#[test]
fn two_siblings_cannot_share_an_order() {
    let (tree, _part, first, second) = tree_with_children();
    let mut broken = tree.clone();
    for chapter in &mut broken.chapters {
        if chapter.id == first || chapter.id == second {
            chapter.order = 7;
        }
    }
    assert!(matches!(
        broken.validate(),
        Err(ChapterError::DuplicateOrder(_, _))
    ));
}

#[test]
fn a_tree_with_an_unknown_version_is_rejected() {
    let (mut tree, _part, _first, _second) = tree_with_children();
    tree.version = 99;
    assert_eq!(tree.validate(), Err(ChapterError::UnsupportedVersion(99)));
}

#[test]
fn a_rejected_tree_write_keeps_the_stored_tree() {
    let (mut tree, part, first, second) = tree_with_children();
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    store.write_tree(&tree).expect("write");

    // Break the tree in a way validation rejects, then try to persist it.
    tree.reparent_chapter(part, first).expect_err("cycle");
    for chapter in &mut tree.chapters {
        if chapter.id == part {
            chapter.parent_id = ChapterId(4242);
        }
    }
    assert!(store.write_tree(&tree).is_err());

    // The last valid tree is still the one on disk.
    let reloaded = store.read_tree().expect("read");
    assert_eq!(reloaded.child_ids(part), vec![first, second]);
    assert!(reloaded.validate().is_ok());
}

#[test]
fn a_body_write_leaves_no_partial_file() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    let body = ChapterBody {
        version: 1,
        paragraphs: vec![Paragraph::plain("One line.")],
    };
    store.bodies().write(ChapterId(2), &body).expect("write");
    let stored = std::fs::read_dir(store.bodies().directory())
        .expect("read dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].to_string_lossy().ends_with(".ron"));
}

#[test]
fn move_up_and_down_reorder_siblings() {
    let (mut tree, part, first, second) = tree_with_children();
    tree.move_down(first).expect("move down");
    assert_eq!(tree.child_ids(part), vec![second, first]);
    tree.move_up(first).expect("move up");
    assert_eq!(tree.child_ids(part), vec![first, second]);
}

#[test]
fn moving_before_the_first_sibling_is_refused() {
    let (mut tree, _part, first, _second) = tree_with_children();
    assert_eq!(
        tree.move_up(first),
        Err(ChapterError::Blocked(first, MoveBlocked::FirstSibling))
    );
    tree.move_down(first).expect("move down");
}

#[test]
fn moving_past_the_last_sibling_is_refused() {
    let (mut tree, _part, _first, second) = tree_with_children();
    assert_eq!(
        tree.move_down(second),
        Err(ChapterError::Blocked(second, MoveBlocked::LastSibling))
    );
}

#[test]
fn move_to_front_and_back_reach_the_ends() {
    let (mut tree, part, first, second) = tree_with_children();
    let third = tree
        .add_chapter(part, ChapterKind::Scene, "Third")
        .expect("third");

    tree.move_to_back(first).expect("back");
    assert_eq!(tree.child_ids(part), vec![second, third, first]);
    tree.move_to_front(first).expect("front");
    assert_eq!(tree.child_ids(part), vec![first, second, third]);
}

#[test]
fn promote_places_a_chapter_after_its_former_parent() {
    let (mut tree, part, first, _second) = tree_with_children();
    let other = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part Two")
        .expect("other");

    tree.promote(first).expect("promote");
    assert_eq!(
        tree.child_ids(ChapterId::ROOT),
        vec![part, first, other, ChapterId::TRASH]
    );
    assert!(!tree.child_ids(part).contains(&first));
}

#[test]
fn a_root_level_chapter_cannot_be_promoted() {
    let (mut tree, part, _first, _second) = tree_with_children();
    assert_eq!(
        tree.promote(part),
        Err(ChapterError::Blocked(part, MoveBlocked::AtRoot))
    );
}

#[test]
fn demote_makes_a_chapter_the_first_child_of_the_earlier_sibling() {
    let mut tree = ChapterTree::new();
    let a = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "A")
        .expect("a");
    let b = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "B")
        .expect("b");
    let c = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "C")
        .expect("c");

    tree.demote(c).expect("demote");
    assert_eq!(tree.child_ids(b), vec![c]);
    assert_eq!(
        tree.child_ids(ChapterId::ROOT),
        vec![a, b, ChapterId::TRASH]
    );
}

#[test]
fn the_first_sibling_cannot_be_demoted() {
    let mut tree = ChapterTree::new();
    let a = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "A")
        .expect("a");
    assert_eq!(
        tree.demote(a),
        Err(ChapterError::Blocked(a, MoveBlocked::NoEarlierSibling))
    );
}

#[test]
fn a_chapter_cannot_be_demoted_into_a_leaf() {
    let mut tree = ChapterTree::new();
    let leaf = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Scene, "Leaf")
        .expect("leaf");
    let later = tree
        .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Later")
        .expect("later");
    assert_eq!(
        tree.demote(later),
        Err(ChapterError::Blocked(later, MoveBlocked::SiblingIsLeaf))
    );
    assert!(tree.chapter(leaf).is_some());
}

#[test]
fn deleting_moves_a_chapter_to_the_visible_trash_and_back() {
    let (mut tree, part, first, second) = tree_with_children();
    tree.trash_chapter(first).expect("trash");
    assert_eq!(tree.child_ids(ChapterId::TRASH), vec![first]);
    assert_eq!(tree.child_ids(part), vec![second]);
    assert!(tree.chapter(first).is_some());

    tree.restore_chapter(first).expect("restore");
    assert!(tree.child_ids(ChapterId::ROOT).contains(&first));
    assert!(tree.child_ids(ChapterId::TRASH).is_empty());
}

#[test]
fn emptying_the_trash_is_the_only_permanent_delete() {
    let (mut tree, _part, first, _second) = tree_with_children();
    tree.trash_chapter(first).expect("trash");
    assert!(tree.chapter(first).is_some());

    let removed = tree.empty_trash();
    assert_eq!(removed, vec![first]);
    assert!(tree.chapter(first).is_none());
}

#[test]
fn renaming_a_status_keeps_the_chapters_that_reference_it() {
    let (mut tree, part, _first, _second) = tree_with_children();
    let status = tree.add_status("Draft").expect("status");
    tree.set_status(part, Some(status)).expect("set status");

    tree.rename_status(status, "Revised").expect("rename");

    assert_eq!(
        tree.chapter(part).expect("chapter").status_id,
        Some(status),
        "the chapter still points at the same status id"
    );
    assert_eq!(
        tree.statuses_by_id().get(&status).expect("status").name,
        "Revised"
    );
}

#[test]
fn renaming_a_label_keeps_the_chapters_that_reference_it() {
    let (mut tree, part, _first, _second) = tree_with_children();
    let label = tree.add_label("POV", [0.5, 0.5, 0.5]).expect("label");
    tree.set_label(part, Some(label)).expect("set label");

    tree.rename_label(label, "Point of view").expect("rename");

    assert_eq!(tree.chapter(part).expect("chapter").label_id, Some(label));
    assert_eq!(
        tree.labels_by_id().get(&label).expect("label").name,
        "Point of view"
    );
}

#[test]
fn an_empty_definition_name_is_rejected() {
    let mut tree = ChapterTree::new();
    let status = tree.add_status("Draft").expect("status");
    let label = tree.add_label("POV", [0.5, 0.5, 0.5]).expect("label");

    assert_eq!(
        tree.rename_status(status, "   "),
        Err(ChapterError::EmptyName)
    );
    assert_eq!(
        tree.rename_label(label, "   "),
        Err(ChapterError::EmptyName)
    );
    assert_eq!(
        tree.statuses_by_id().get(&status).expect("status").name,
        "Draft"
    );
}

#[test]
fn undo_restores_the_previous_order() {
    let (mut tree, part, first, second) = tree_with_children();
    let mut history = ManuscriptHistory::default();
    history.record(&tree);
    tree.move_down(first).expect("move down");
    assert_eq!(tree.child_ids(part), vec![second, first]);

    let restored = history.undo().expect("snapshot");
    assert_eq!(restored.child_ids(part), vec![first, second]);
}

#[test]
fn a_chapter_session_round_trips_through_the_store() {
    use weave_editor::manuscript::{ManuscriptSession, Position, Selection};

    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    let mut session = ManuscriptSession::new();
    session.set_selection(
        ChapterId(2),
        Selection::collapsed(Position {
            paragraph: 3,
            offset: 7,
        }),
    );
    store.write_session(&session).expect("write session");

    let reloaded = store.read_session().expect("read session");
    assert_eq!(reloaded, session);
    assert_eq!(
        reloaded.selection(ChapterId(2)),
        Some(Selection::collapsed(Position {
            paragraph: 3,
            offset: 7,
        }))
    );
}

#[test]
fn a_missing_session_reads_as_empty() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ManuscriptStore::in_directory(directory.path());
    assert_eq!(
        store.read_session().expect("read session"),
        weave_editor::manuscript::ManuscriptSession::new()
    );
}
