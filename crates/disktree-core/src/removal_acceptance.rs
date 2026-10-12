//! Windows acceptance cases use only owned temporary data. Every worker run
//! checks the bytes of unselected and outside files, not just their existence.

use super::*;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    outside: PathBuf,
    canaries: Vec<(PathBuf, Vec<u8>)>,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("temporary fixture");
        let root = temp.path().join("scan");
        let outside = temp.path().join("scan-neighbour");
        fs::create_dir(&root).expect("scan root");
        fs::create_dir(&outside).expect("outside directory");
        let canaries = [
            (root.join("unmarked.bin"), b"unmarked contents".to_vec()),
            (outside.join("precious.bin"), b"outside contents".to_vec()),
            (temp.path().join("outside.bin"), b"outside sibling".to_vec()),
        ]
        .into_iter()
        .collect::<Vec<_>>();
        for (path, contents) in &canaries {
            fs::write(path, contents).expect("write canary");
        }
        Self {
            _temp: temp,
            root,
            outside,
            canaries,
        }
    }

    fn marked(&self, path: &Path) -> Plan {
        let marked = target(path);
        let plan = plan(&[marked], &self.root);
        assert_eq!(plan.targets.len(), 1, "{:?}", plan.blocked);
        assert!(plan.blocked.is_empty());
        plan.prepare_review(&AtomicBool::new(false))
            .expect("review fixture")
    }

    fn assert_canaries(&self) {
        for (path, contents) in &self.canaries {
            assert_eq!(
                fs::read(path).expect("canary must survive"),
                *contents,
                "unselected data changed: {}",
                path.display()
            );
        }
    }
}

fn target(path: &Path) -> Target {
    let snapshot = entry_snapshot(path).expect("fixture identity and kind");
    Target {
        path: path.to_path_buf(),
        bytes: 0,
        is_dir: snapshot.kind.is_dir(),
        hidden: false,
        identity: Some(snapshot.identity),
    }
}

fn events(plan: &Plan, cancelled: bool) -> Vec<RemovalEvent> {
    let (sender, receiver) = mpsc::channel();
    run(
        plan,
        RemovalMode::Permanent,
        TrashBackend::Unavailable,
        &AtomicBool::new(cancelled),
        &sender,
    );
    drop(sender);
    receiver.into_iter().collect()
}

fn assert_totals(events: &[RemovalEvent], removed: u64, failed: usize) {
    assert!(
        matches!(
            events.last(),
            Some(RemovalEvent::Done { removed: actual, failed: errors, .. })
                if *actual == removed && *errors == failed
        ),
        "unexpected worker result: {events:?}"
    );
}

#[test]
fn a_junction_added_after_review_never_deletes_its_outside_contents() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("marked directory");
    fs::write(directory.join("selected.bin"), b"selected").expect("write");
    let plan = fixture.marked(&directory);
    // The directory identity stays the same, but its reviewed membership
    // changes. Refuse the target before deleting even its original child.
    for depth in 0..8 {
        let parent = directory.join(format!("level-{depth}"));
        fs::create_dir(&parent).expect("nested directory");
        assert!(crate::windows::make_junction(
            &parent.join("outside"),
            &fixture.outside
        ));
    }
    let result = events(&plan, false);
    assert_totals(&result, 0, 1);
    assert_eq!(
        fs::read(directory.join("selected.bin")).expect("survives"),
        b"selected"
    );
    fixture.assert_canaries();
}

#[test]
fn a_marked_directory_swapped_for_a_junction_stops_the_worker() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("marked directory");
    fs::write(directory.join("selected.bin"), b"original").expect("write");
    let plan = fixture.marked(&directory);
    let original = fixture.root.join("original");
    fs::rename(&directory, &original).expect("move original");
    assert!(crate::windows::make_junction(&directory, &fixture.outside));

    let result = events(&plan, false);
    assert_totals(&result, 0, 1);
    assert_eq!(
        fs::read(original.join("selected.bin")).expect("original contents"),
        b"original"
    );
    assert!(fs::symlink_metadata(&directory).is_ok());
    fixture.assert_canaries();
}

#[test]
fn a_missing_target_does_not_expand_the_remaining_batch() {
    let fixture = Fixture::new();
    let first = fixture.root.join("a-selected.bin");
    let second = fixture.root.join("b-selected.bin");
    fs::write(&first, b"first").expect("write");
    fs::write(&second, b"second").expect("write");
    let plan = plan(&[target(&first), target(&second)], &fixture.root);
    assert_eq!(plan.targets.len(), 2);
    let plan = plan
        .prepare_review(&AtomicBool::new(false))
        .expect("review");
    let saved = fixture.root.join("saved-original.bin");
    fs::rename(&first, &saved).expect("move first after review");

    let result = events(&plan, false);
    assert_totals(&result, 1, 1);
    assert_eq!(fs::read(&saved).expect("original survives"), b"first");
    assert!(!second.exists());
    fixture.assert_canaries();
}

#[test]
fn a_sharing_violation_has_no_weaker_deletion_fallback() {
    let fixture = Fixture::new();
    let selected = fixture.root.join("selected.bin");
    fs::write(&selected, b"locked original").expect("write");
    let plan = fixture.marked(&selected);
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&selected)
        .expect("exclusive fixture handle");

    let result = events(&plan, false);
    assert_totals(&result, 0, 1);
    drop(locked);
    assert_eq!(
        fs::read(&selected).expect("locked file"),
        b"locked original"
    );
    fixture.assert_canaries();
}

#[test]
fn cancellation_before_the_worker_starts_touches_no_target() {
    let fixture = Fixture::new();
    let selected = fixture.root.join("selected.bin");
    fs::write(&selected, b"cancelled original").expect("write");
    let plan = fixture.marked(&selected);

    let result = events(&plan, true);
    assert_totals(&result, 0, 0);
    assert!(
        !result
            .iter()
            .any(|event| matches!(event, RemovalEvent::Item { .. }))
    );
    assert_eq!(
        fs::read(&selected).expect("selected survives"),
        b"cancelled original"
    );
    fixture.assert_canaries();
}

#[test]
fn the_worker_refuses_outside_paths_even_in_a_modified_plan() {
    let fixture = Fixture::new();
    let selected = fixture.root.join("selected.bin");
    fs::write(&selected, b"selected survives").expect("write");
    let original_plan = fixture.marked(&selected);
    // Plan targets are public. A future caller bypassing planning must not
    // turn a valid root snapshot into permission to remove its neighbours.
    for outside in [
        fixture.outside.join("precious.bin"),
        fixture.root.join("../scan-neighbour/precious.bin"),
        fixture
            .root
            .parent()
            .expect("fixture parent")
            .join("outside.bin"),
    ] {
        let mut modified = original_plan.clone();
        modified.targets = vec![target(&outside)];
        assert_totals(&events(&modified, false), 0, 1);
        fixture.assert_canaries();
    }
    assert_eq!(
        fs::read(&selected).expect("selected survives"),
        b"selected survives"
    );
}

#[test]
fn a_generated_batch_changes_only_the_explicitly_selected_files() {
    let fixture = Fixture::new();
    let mut expected = Vec::new();
    let mut targets = Vec::new();
    for index in 0..64 {
        let name = format!("item {index:02} - \u{e9} - \u{4e2d}.bin");
        let path = fixture.root.join(name);
        let contents = format!("unique payload {index}").into_bytes();
        fs::write(&path, &contents).expect("write generated file");
        let selected = index % 7 == 0;
        if selected {
            targets.push(target(&path));
        }
        expected.push((path, contents, selected));
    }
    targets.push(target(&fixture.root));
    targets.push(target(&fixture.outside.join("precious.bin")));
    let plan = plan(&targets, &fixture.root);
    assert_eq!(plan.targets.len(), 10);
    assert_eq!(plan.blocked.len(), 2);
    let plan = plan
        .prepare_review(&AtomicBool::new(false))
        .expect("review");
    assert_totals(&events(&plan, false), 10, 0);
    for (path, contents, selected) in expected {
        if selected {
            assert!(!path.exists(), "selected file was not removed");
        } else {
            assert_eq!(fs::read(&path).expect("unselected file"), contents);
        }
    }
    fixture.assert_canaries();
}

#[test]
fn deleting_a_selected_hardlink_preserves_the_outside_name_and_bytes() {
    let fixture = Fixture::new();
    let selected = fixture.root.join("selected-link.bin");
    let outside = fixture.outside.join("precious.bin");
    fs::hard_link(&outside, &selected).expect("fixture hardlink");
    assert_eq!(entry_identity(&selected), entry_identity(&outside));
    let plan = fixture.marked(&selected);

    assert_totals(&events(&plan, false), 1, 0);
    assert!(!selected.exists());
    fixture.assert_canaries();
}

#[test]
fn a_locked_child_is_found_before_any_sibling_is_deleted() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("marked directory");
    for index in 0..8 {
        fs::write(directory.join(format!("{index}.bin")), b"keep on failure")
            .expect("fixture file");
    }
    // Lock the last entry in the filesystem's listing, so the old recursive
    // worker deletes its preceding siblings before discovering the error.
    let paths = fs::read_dir(&directory)
        .expect("listing")
        .map(|entry| entry.expect("entry").path())
        .collect::<Vec<_>>();
    let plan = fixture.marked(&directory);
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(paths.last().expect("last child"))
        .expect("exclusive lock");
    assert_totals(&events(&plan, false), 0, 1);
    drop(locked);
    for path in &paths {
        assert_eq!(
            fs::read(path).expect("no sibling was deleted"),
            b"keep on failure"
        );
    }
    fixture.assert_canaries();
}

#[test]
fn a_new_descendant_after_confirmation_refuses_the_whole_target() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir_all(directory.join("nested")).expect("directories");
    let original = directory.join("nested/original.bin");
    fs::write(&original, b"reviewed").expect("original");
    let plan = fixture.marked(&directory);
    let added = directory.join("nested/new.bin");
    fs::write(&added, b"not approved").expect("late addition");
    assert_totals(&events(&plan, false), 0, 1);
    assert_eq!(fs::read(original).expect("original survived"), b"reviewed");
    assert_eq!(fs::read(added).expect("new file survived"), b"not approved");
    fixture.assert_canaries();
}

#[test]
fn changed_file_metadata_after_confirmation_refuses_all_its_siblings() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("directory");
    let first = directory.join("first.bin");
    let changed = directory.join("last.bin");
    fs::write(&first, b"reviewed").expect("first");
    fs::write(&changed, b"reviewed").expect("last");
    let plan = fixture.marked(&directory);
    fs::write(&changed, b"changed contents with a different length")
        .expect("change");
    assert_totals(&events(&plan, false), 0, 1);
    assert_eq!(fs::read(first).expect("first survived"), b"reviewed");
    assert_eq!(
        fs::read(changed).expect("changed survived"),
        b"changed contents with a different length"
    );
    fixture.assert_canaries();
}

#[test]
fn replaced_descendant_after_confirmation_refuses_the_target() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("directory");
    let file = directory.join("file.bin");
    fs::write(&file, b"original").expect("file");
    let plan = fixture.marked(&directory);
    fs::rename(&file, fixture.root.join("original.bin")).expect("move");
    fs::write(&file, b"replacement").expect("replace");
    assert_totals(&events(&plan, false), 0, 1);
    assert_eq!(
        fs::read(file).expect("replacement survived"),
        b"replacement"
    );
    fixture.assert_canaries();
}

#[test]
fn cancellation_during_a_directory_stops_before_the_next_entry() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("directory");
    let files = (0..8)
        .map(|index| directory.join(format!("{index}.bin")))
        .collect::<Vec<_>>();
    for file in &files {
        fs::write(file, b"reviewed").expect("file");
    }
    let cancel = AtomicBool::new(false);
    let review = crate::windows::review_tree(
        &directory,
        &cancel,
        crate::windows::REVIEW_ENTRY_LIMIT,
    )
    .expect("review");
    let mut deleted = Vec::new();
    let error =
        crate::windows::remove_reviewed(&directory, &review, &cancel, |path| {
            deleted.push(path.to_path_buf());
            cancel.store(true, Ordering::Relaxed);
        })
        .expect_err("cancelled between children");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(
        error
            .to_string()
            .contains("1 reviewed entries already deleted")
    );
    assert_eq!(deleted.len(), 1);
    for file in &files {
        if deleted.contains(file) {
            assert!(!file.exists());
        } else {
            assert_eq!(fs::read(file).expect("remaining child"), b"reviewed");
        }
    }
    assert!(directory.exists());
    fixture.assert_canaries();
}

#[test]
fn a_child_added_during_deletion_is_never_added_to_the_approved_list() {
    let fixture = Fixture::new();
    let directory = fixture.root.join("marked");
    fs::create_dir(&directory).expect("directory");
    fs::write(directory.join("reviewed.bin"), b"reviewed").expect("file");
    let cancel = AtomicBool::new(false);
    let review = crate::windows::review_tree(
        &directory,
        &cancel,
        crate::windows::REVIEW_ENTRY_LIMIT,
    )
    .expect("review");
    let added = directory.join("not-reviewed.bin");
    let error =
        crate::windows::remove_reviewed(&directory, &review, &cancel, |_| {
            fs::write(&added, b"never approved").expect("late file");
        })
        .expect_err("directory is no longer empty");
    assert!(
        error
            .to_string()
            .contains("1 reviewed entries already deleted")
    );
    assert_eq!(
        fs::read(added).expect("late file survived"),
        b"never approved"
    );
    fixture.assert_canaries();
}

#[test]
fn an_unprepared_plan_never_starts_permanent_windows_deletion() {
    let fixture = Fixture::new();
    let file = fixture.root.join("selected.bin");
    fs::write(&file, b"unapproved").expect("file");
    let plan = plan(&[target(&file)], &fixture.root);
    assert_totals(&events(&plan, false), 0, 1);
    assert_eq!(fs::read(file).expect("not deleted"), b"unapproved");
    fixture.assert_canaries();
}
