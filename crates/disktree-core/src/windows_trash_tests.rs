use super::*;
use crate::windows::{REVIEW_ENTRY_LIMIT, pin_parent, review_tree};
use std::cell::RefCell;

fn fixture() -> (tempfile::TempDir, PathBuf, ReviewedTree) {
    let temp = tempfile::tempdir().expect("disposable tree");
    let marked = temp.path().join("marked tree");
    fs::create_dir_all(marked.join("nested/empty")).expect("folders");
    fs::write(marked.join("one.bin"), b"approved original").expect("file");
    fs::write(marked.join("nested/two.bin"), [0, 255, 2, 3]).expect("file");
    fs::write(temp.path().join("outside.bin"), b"outside sentinel")
        .expect("sentinel");
    let guard = pin_parent(&marked, temp.path(), entry_identity(temp.path()))
        .expect("root and ancestor pins");
    let review =
        review_tree(&guard.path, &AtomicBool::new(false), REVIEW_ENTRY_LIMIT)
            .expect("review");
    (temp, guard.path, review)
}

fn guarded(path: &Path) -> RemovalGuard {
    let root = path.parent().expect("fixture parent");
    pin_parent(path, root, entry_identity(root)).expect("fixture guard")
}

fn record(scenario: &str, recycled: usize, restored: usize) {
    let json = format!(
        "{{\"scenario\":\"{scenario}\",\"passed\":true,\"recycled\":{recycled},\"restored\":{restored}}}"
    );
    println!("WINDOWS_RECYCLE_EVIDENCE {json}");
    if let Some(directory) = std::env::var_os("DISKTREE_RECYCLE_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).expect("evidence folder");
        fs::write(directory.join(format!("{scenario}.json")), json)
            .expect("evidence record");
    }
}

#[test]
fn native_tree_recycles_and_restores_names_bytes_and_identities() {
    let _apartment = Apartment::new().expect("COM apartment");
    let (temp, path, review) = fixture();
    let cancel = Arc::new(AtomicBool::new(false));
    let receipt = recycle_with(guarded(&path), &review, &cancel, None)
        .expect("native recycling");
    assert!(!path.exists());
    let bin = item_path(&receipt).expect("native receipt path");
    assert!(bin.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("$Recycle.Bin")
    }));
    review
        .matches_at(&bin)
        .expect("recycled identities and exact membership");
    restore_receipt(&receipt, &path).expect("native restoration");
    review
        .matches_at(&path)
        .expect("original identities restored");
    assert_eq!(
        fs::read(path.join("one.bin")).expect("bytes"),
        b"approved original"
    );
    assert_eq!(
        fs::read(path.join("nested/two.bin")).expect("bytes"),
        [0, 255, 2, 3]
    );
    assert!(path.join("nested/empty").is_dir());
    assert_eq!(
        fs::read(temp.path().join("outside.bin")).expect("sentinel"),
        b"outside sentinel"
    );
    record("tree-restoration", review.len(), review.len());
}

#[test]
fn cancelled_recycling_leaves_the_entire_tree_intact() {
    let (temp, path, review) = fixture();
    let cancel = Arc::new(AtomicBool::new(true));
    assert!(recycle_reviewed(guarded(&path), &review, &cancel).is_err());
    review.matches_at(&path).expect("unchanged");
    assert_eq!(
        fs::read(temp.path().join("outside.bin")).expect("sentinel"),
        b"outside sentinel"
    );
    record("cancel-before-handoff", 0, 0);
}

#[test]
fn changed_contents_fail_without_a_permanent_fallback() {
    let (_temp, path, review) = fixture();
    fs::write(path.join("one.bin"), b"replacement data").expect("changed file");
    let result = recycle_reviewed(
        guarded(&path),
        &review,
        &Arc::new(AtomicBool::new(false)),
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(path.join("one.bin")).expect("survivor"),
        b"replacement data"
    );
    assert!(path.join("nested/two.bin").exists());
    record("changed-file-no-fallback", 0, 0);
}

#[test]
fn replacement_after_the_final_check_is_returned_from_the_bin() {
    let _apartment = Apartment::new().expect("COM apartment");
    let temp = tempfile::tempdir().expect("fixture");
    let file = temp.path().join("marked.bin");
    fs::write(&file, b"approved original").expect("file");
    let saved = temp.path().join("approved-original.bin");
    fs::hard_link(&file, &saved).expect("retain original fixture identity");
    let path = fs::canonicalize(&file).expect("fixture path");
    let review =
        review_tree(&path, &AtomicBool::new(false), REVIEW_ENTRY_LIMIT)
            .expect("review");
    let victim = path.clone();
    let original_id = entry_identity(&path);
    let replacement_id = Rc::new(RefCell::new(None));
    let id = Rc::clone(&replacement_id);
    let hook = Rc::new(move || {
        fs::remove_file(&victim)?;
        fs::write(&victim, b"unapproved replacement")?;
        *id.borrow_mut() = entry_identity(&victim);
        Ok(())
    });
    let result = recycle_with(
        guarded(&path),
        &review,
        &Arc::new(AtomicBool::new(false)),
        Some(hook),
    );
    let error = result.expect_err("unexpected move must fail");
    assert!(
        error.to_string().contains("unexpected move restored"),
        "{error}"
    );
    assert_eq!(entry_identity(&path), *replacement_id.borrow());
    assert_eq!(
        fs::read(&path).expect("replacement survived"),
        b"unapproved replacement"
    );
    let saved = path.with_file_name("approved-original.bin");
    assert_eq!(entry_identity(&saved), original_id);
    assert_eq!(
        fs::read(saved).expect("original survived"),
        b"approved original"
    );
    record("replacement-after-final-check-restored", 1, 1);
}

#[test]
fn a_late_child_moved_with_its_folder_is_returned_without_success() {
    let _apartment = Apartment::new().expect("COM apartment");
    let (_temp, path, review) = fixture();
    let late = path.join("late-unapproved.bin");
    let created = late.clone();
    let hook = Rc::new(move || fs::write(&created, b"unapproved late child"));
    let result = recycle_with(
        guarded(&path),
        &review,
        &Arc::new(AtomicBool::new(false)),
        Some(hook),
    );
    let error = result.expect_err("membership changed");
    assert!(
        error.to_string().contains("unexpected move restored"),
        "{error}"
    );
    assert_eq!(
        fs::read(late).expect("late child restored"),
        b"unapproved late child"
    );
    assert_eq!(
        fs::read(path.join("one.bin")).expect("original"),
        b"approved original"
    );
    record("late-child-restored", review.len() + 1, review.len() + 1);
}

#[test]
fn restoration_refuses_to_overwrite_a_new_occupant() {
    let _apartment = Apartment::new().expect("COM apartment");
    let (_temp, path, review) = fixture();
    let receipt = recycle_with(
        guarded(&path),
        &review,
        &Arc::new(AtomicBool::new(false)),
        None,
    )
    .expect("recycled");
    fs::create_dir(&path).expect("new occupant");
    fs::write(path.join("new-data.bin"), b"keep new data").expect("data");
    assert_eq!(
        restore_receipt(&receipt, &path)
            .expect_err("collision")
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        fs::read(path.join("new-data.bin")).expect("untouched"),
        b"keep new data"
    );
    review
        .matches_at(&item_path(&receipt).expect("bin"))
        .expect("recoverable bin item");
    // Restore only this test's receipt to another vacant name for cleanup.
    let recovered = path.with_file_name("recovered-original");
    restore_receipt(&receipt, &recovered).expect("recover without overwrite");
    review.matches_at(&recovered).expect("exact recovery");
    record("restoration-collision", review.len(), review.len());
}

#[test]
fn cancellation_during_a_native_batch_keeps_survivors_and_restores_moves() {
    let _apartment = Apartment::new().expect("COM apartment");
    let temp = tempfile::tempdir().expect("disposable batch");
    let cancel = Arc::new(AtomicBool::new(false));
    let mut receipts = Vec::new();
    let mut reviews = Vec::new();
    for index in 0..80_u8 {
        let path = temp.path().join(format!("file-{index:03}.bin"));
        fs::write(&path, vec![index; 8192]).expect("bytes");
        let path = fs::canonicalize(&path).expect("fixture path");
        let review =
            review_tree(&path, &cancel, REVIEW_ENTRY_LIMIT).expect("review");
        reviews.push((path, review, index));
    }
    for (path, review, index) in &reviews {
        if *index == 5 {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Ok(receipt) = recycle_with(guarded(path), review, &cancel, None)
        {
            receipts.push((receipt, path));
        } else {
            assert!(cancel.load(Ordering::Relaxed));
            break;
        }
    }
    assert_eq!(receipts.len(), 5);
    for (path, review, index) in &reviews[5..] {
        review
            .matches_at(path)
            .expect("remaining identities intact");
        assert_eq!(fs::read(path).expect("survivor"), vec![*index; 8192]);
    }
    for (receipt, path) in &receipts {
        restore_receipt(receipt, path).expect("restore completed move");
    }
    for (path, review, index) in &reviews {
        review.matches_at(path).expect("restored identity");
        assert_eq!(fs::read(path).expect("bytes"), vec![*index; 8192]);
    }
    record("batch-cancel-5-of-80", receipts.len(), receipts.len());
}

#[test]
fn an_oversized_sparse_file_is_not_permanently_deleted() {
    use windows_sys::Win32::System::IO::DeviceIoControl;
    const SET_SPARSE: u32 = 0x0009_00c4;
    let _apartment = Apartment::new().expect("COM apartment");
    let temp = tempfile::Builder::new()
        .prefix("disktree-native-trash-quota-")
        .tempdir()
        .expect("disposable sparse fixture");
    let path = temp.path().join("quota-sparse.bin");
    let file = File::create(&path).expect("fixture");
    let mut returned = 0;
    // FSCTL_SET_SPARSE sets a metadata flag on this owned file. Expanding its
    // logical length below must not allocate a drive-sized data payload.
    // SAFETY: a live writable fixture handle, no input or output buffer and a
    // live result count. The handle is synchronous, so no OVERLAPPED is used.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "sparse fixture: {}", io::Error::last_os_error());
    let logical = crate::space::space_info(temp.path())
        .expect("capacity")
        .total
        .checked_mul(2)
        .expect("bounded volume size");
    file.set_len(logical).expect("sparse logical length");
    drop(file);
    let scanned =
        crate::scan::scan(temp.path(), crate::scan::ScanOptions::default())
            .expect("allocated-size scan");
    assert_eq!(scanned.bytes, 0, "sparse fixture must not allocate payload");
    let path = fs::canonicalize(path).expect("path");
    let review =
        review_tree(&path, &AtomicBool::new(false), REVIEW_ENTRY_LIMIT)
            .expect("review");
    let result = recycle_with(
        guarded(&path),
        &review,
        &Arc::new(AtomicBool::new(false)),
        None,
    );
    match result {
        Err(error) => {
            review
                .matches_at(&path)
                .expect("failure must leave the file intact");
            println!("WINDOWS_RECYCLE_QUOTA_REFUSAL {error}");
            record("quota-refusal-no-permanent-fallback", 0, 0);
        }
        Ok(receipt) => {
            // A host may count sparse allocation rather than logical length.
            // Success still requires an actual recoverable bin item.
            restore_receipt(&receipt, &path).expect("restore sparse file");
            review
                .matches_at(&path)
                .expect("sparse identity and length restored");
            record("quota-sparse-recycled-and-restored", 1, 1);
        }
    }
}
