//! Review a bounded, exact Windows tree, then pin it before deleting anything.
//! Every opened handle survives preflight. Removal never lists fresh children.

use super::*;
use crate::removal::{EntryIdentity, addressable, entry_kind};
use crate::tree::NodeKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

// Each entry needs a handle for preflight. Bound memory and kernel resources;
// a bigger selection must be split, never silently use weaker recursion.
pub(crate) const REVIEW_ENTRY_LIMIT: usize = 20_000;

#[derive(Debug)]
pub(crate) struct ReviewedTree {
    entries: Vec<ReviewedEntry>,
}

#[derive(Debug)]
struct ReviewedEntry {
    path: PathBuf,
    stamp: Stamp,
    children: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    identity: EntryIdentity,
    kind: NodeKind,
    length: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn capture(file: &File) -> io::Result<Self> {
        let meta = file.metadata()?;
        let kind = entry_kind(&meta);
        // Deleting children changes their parent's timestamp. Directory
        // membership is checked separately; files retain length and mtime.
        let (length, modified) = if kind.is_dir() {
            (0, None)
        } else {
            (meta.len(), Some(meta.modified()?))
        };
        Ok(Self {
            identity: handle_identity(file)?,
            kind,
            length,
            modified,
        })
    }
}

impl ReviewedTree {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn identity(&self) -> Option<EntryIdentity> {
        self.entries.first().map(|entry| entry.stamp.identity)
    }
    pub(crate) fn is_directory(&self) -> bool {
        self.entries
            .first()
            .is_some_and(|entry| entry.stamp.kind.is_dir())
    }
}

fn inspection(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
    } else {
        Ok(())
    }
}

fn children(path: &Path, limit: usize) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        if paths.len() >= limit {
            return Err(too_large());
        }
        let path = entry?.path();
        if !addressable(&path) {
            return Err(io::Error::other("an entry has an ambiguous filename"));
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn too_large() -> io::Error {
    io::Error::other(
        "the selection exceeds the 20,000-entry safety limit; \
                      select smaller folders or batches",
    )
}

pub(crate) fn review_tree(
    path: &Path,
    cancel: &AtomicBool,
    limit: usize,
) -> io::Result<ReviewedTree> {
    let mut entries = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    let roots = known_sync_roots();
    let mut volume = None;
    while let Some(path) = pending.pop() {
        check_cancel(cancel)?;
        if entries.len() >= limit {
            return Err(too_large());
        }
        let file = inspection(&path)?;
        refuse_cloud(&path, &file, &roots)?;
        let stamp = Stamp::capture(&file)?;
        if volume.is_some_and(|volume| volume != stamp.identity.0) {
            return Err(io::Error::other(
                "a different volume was not reviewed",
            ));
        }
        volume = Some(stamp.identity.0);
        let children = if stamp.kind.is_dir() {
            children(&path, limit - entries.len())?
        } else {
            Vec::new()
        };
        if entries.len() + pending.len() + children.len() >= limit {
            return Err(too_large());
        }
        pending.extend(children.iter().rev().cloned());
        entries.push(ReviewedEntry {
            path,
            stamp,
            children,
        });
    }
    Ok(ReviewedTree { entries })
}

pub(crate) fn remove_reviewed(
    path: &Path,
    review: &ReviewedTree,
    cancel: &AtomicBool,
    mut on_removed: impl FnMut(&Path),
) -> io::Result<()> {
    if review
        .entries
        .first()
        .is_none_or(|entry| entry.path != path)
    {
        return Err(io::Error::other("the reviewed path changed"));
    }
    let mut opened = Vec::new();
    let roots = known_sync_roots();
    for entry in &review.entries {
        check_cancel(cancel)?;
        let file = open_removal(&entry.path)?;
        refuse_cloud(&entry.path, &file, &roots)?;
        if Stamp::capture(&file)? != entry.stamp {
            return Err(io::Error::other(
                "contents changed since confirmation; \
                                        review the selection again",
            ));
        }
        opened.push(file);
    }
    // All entries are now pinned against writes, replacement and renaming.
    // Recheck membership after opening, before the first destructive call.
    for entry in &review.entries {
        check_cancel(cancel)?;
        if entry.stamp.kind.is_dir()
            && children(&entry.path, REVIEW_ENTRY_LIMIT)? != entry.children
        {
            return Err(io::Error::other(
                "folder contents changed since \
                                        confirmation; review again",
            ));
        }
    }
    let mut removed = 0;
    for (entry, file) in review.entries.iter().zip(&opened).rev() {
        let result = check_cancel(cancel).and_then(|()| delete_opened(file));
        if let Err(error) = result {
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; {removed} reviewed entries already deleted; \
                 no rollback is available"
                ),
            ));
        }
        removed += 1;
        on_removed(&entry.path);
    }
    Ok(())
}

fn known_sync_roots() -> Vec<PathBuf> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .flat_map(|path| {
            let canonical = fs::canonicalize(&path).ok();
            std::iter::once(path).chain(canonical)
        })
        .collect()
}

fn overlaps_sync_root(path: &Path, roots: &[PathBuf]) -> bool {
    let path = guard_key(path);
    roots
        .iter()
        .map(|root| guard_key(root))
        .any(|root| path.starts_with(&root) || root.starts_with(&path))
}

fn cloud_result(result: i32) -> io::Result<bool> {
    use windows_sys::Win32::Foundation::{
        ERROR_CLOUD_FILE_NOT_UNDER_SYNC_ROOT, ERROR_NOT_A_CLOUD_FILE,
        ERROR_NOT_A_CLOUD_SYNC_ROOT,
    };
    if result >= 0 {
        return Ok(true);
    }
    let code = result as u32;
    if [
        ERROR_NOT_A_CLOUD_FILE,
        ERROR_CLOUD_FILE_NOT_UNDER_SYNC_ROOT,
        ERROR_NOT_A_CLOUD_SYNC_ROOT,
    ]
    .into_iter()
    .any(|error| code == (0x8007_0000 | error))
    {
        return Ok(false);
    }
    Err(io::Error::other(format!(
        "could not verify cloud-sync status ({code:#x}); removal refused"
    )))
}

fn refuse_cloud(path: &Path, file: &File, roots: &[PathBuf]) -> io::Result<()> {
    use windows_sys::Win32::Storage::CloudFilters::{
        CF_SYNC_ROOT_BASIC_INFO, CF_SYNC_ROOT_INFO_BASIC,
        CfGetSyncRootInfoByHandle,
    };
    let meta = file.metadata()?;
    if overlaps_sync_root(path, roots) {
        return Err(io::Error::other(
            "a synchronized folder is protected; \
                                    deletion could propagate to the cloud",
        ));
    }
    // A name-surrogate link is unlinked, not followed into its destination.
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.file_attributes() & (EVICTED | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(io::Error::other(
            "cloud placeholders and unsupported \
                                    reparse providers are protected",
        ));
    }
    let mut info = CF_SYNC_ROOT_BASIC_INFO { SyncRootFileId: 0 };
    // SAFETY: an attribute-only/no-follow live handle, correctly aligned
    // output of exactly the requested size, and no optional length pointer.
    let result = unsafe {
        CfGetSyncRootInfoByHandle(
            file.as_raw_handle(),
            CF_SYNC_ROOT_INFO_BASIC,
            (&raw mut info).cast(),
            size_of::<CF_SYNC_ROOT_BASIC_INFO>() as u32,
            std::ptr::null_mut(),
        )
    };
    if cloud_result(result)? {
        return Err(io::Error::other(
            "a registered cloud-sync folder is \
                                    protected; use its provider to free space",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_roots_use_component_and_alias_aware_boundaries() {
        let roots = vec![PathBuf::from(r"C:\Users\Test\OneDrive")];
        assert!(overlaps_sync_root(
            Path::new(r"\\?\C:\USERS\TEST\onedrive\file"),
            &roots
        ));
        assert!(overlaps_sync_root(Path::new(r"C:\Users\Test"), &roots));
        assert!(!overlaps_sync_root(
            Path::new(r"C:\Users\Test\OneDrive-backup\file"),
            &roots
        ));
    }

    #[test]
    fn cloud_query_failure_never_becomes_permission_to_delete() {
        assert!(cloud_result(0).expect("registered"));
        let not_cloud = i32::from_ne_bytes(0x8007_0178_u32.to_ne_bytes());
        assert!(!cloud_result(not_cloud).expect("ordinary"));
        let denied = i32::from_ne_bytes(0x8007_0005_u32.to_ne_bytes());
        assert!(cloud_result(denied).is_err());
    }

    #[test]
    fn review_limits_refuse_before_any_file_is_deleted() {
        let temp = tempfile::TempDir::new().expect("temp");
        let file = temp.path().join("file.bin");
        fs::write(&file, b"keep").expect("write");
        let cancel = AtomicBool::new(false);
        assert!(review_tree(temp.path(), &cancel, 1).is_err());
        assert_eq!(fs::read(file).expect("survived"), b"keep");
    }

    #[derive(Debug)]
    struct RegisteredRoot {
        wide: Vec<u16>,
    }

    impl RegisteredRoot {
        fn new(path: &Path) -> Self {
            use windows_sys::Win32::Storage::CloudFilters::{
                CF_HYDRATION_POLICY, CF_HYDRATION_POLICY_FULL,
                CF_POPULATION_POLICY, CF_POPULATION_POLICY_FULL,
                CF_REGISTER_FLAG_NONE, CF_SYNC_POLICIES, CF_SYNC_REGISTRATION,
                CfRegisterSyncRoot,
            };
            let wide = super::super::wide(path, false).expect("path");
            let name = "DiskTree disposable regression"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let version = [u16::from(b'1'), 0];
            let registration = CF_SYNC_REGISTRATION {
                StructSize: size_of::<CF_SYNC_REGISTRATION>() as u32,
                ProviderName: name.as_ptr(),
                ProviderVersion: version.as_ptr(),
                ..CF_SYNC_REGISTRATION::default()
            };
            let policies = CF_SYNC_POLICIES {
                StructSize: size_of::<CF_SYNC_POLICIES>() as u32,
                Hydration: CF_HYDRATION_POLICY {
                    Primary: CF_HYDRATION_POLICY_FULL,
                    Modifier: 0,
                },
                Population: CF_POPULATION_POLICY {
                    Primary: CF_POPULATION_POLICY_FULL,
                    Modifier: 0,
                },
                ..CF_SYNC_POLICIES::default()
            };
            // SAFETY: live NUL-terminated strings and sized native structs.
            // Only this test's owned temporary directory is registered.
            let result = unsafe {
                CfRegisterSyncRoot(
                    wide.as_ptr(),
                    &raw const registration,
                    &raw const policies,
                    CF_REGISTER_FLAG_NONE,
                )
            };
            assert!(result >= 0, "register disposable root: {result:#x}");
            Self { wide }
        }
    }

    impl Drop for RegisteredRoot {
        fn drop(&mut self) {
            use windows_sys::Win32::Storage::CloudFilters::CfUnregisterSyncRoot;
            // SAFETY: the path is the owned registered fixture, and the
            // UTF-16 buffer stays live until the unregistration returns.
            let result = unsafe { CfUnregisterSyncRoot(self.wide.as_ptr()) };
            assert!(result >= 0, "unregister disposable root: {result:#x}");
        }
    }

    #[test]
    fn hydrated_files_inside_a_registered_sync_root_are_protected() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path().join("cloud");
        fs::create_dir(&root).expect("directory");
        let file = root.join("fully-local.bin");
        fs::write(&file, b"must never reach the cloud as a deletion")
            .expect("file");
        let _registered = RegisteredRoot::new(&root);
        let cancel = AtomicBool::new(false);
        assert!(review_tree(&file, &cancel, REVIEW_ENTRY_LIMIT).is_err());
        assert_eq!(
            fs::read(file).expect("survived"),
            b"must never reach the cloud as a deletion"
        );
    }

    #[test]
    fn a_sync_root_registered_after_review_refuses_before_deleting() {
        let temp = tempfile::TempDir::new().expect("temp");
        let root = temp.path().join("cloud");
        fs::create_dir(&root).expect("directory");
        let file = root.join("fully-local.bin");
        fs::write(&file, b"keep").expect("file");
        let cancel = AtomicBool::new(false);
        let review = review_tree(&file, &cancel, REVIEW_ENTRY_LIMIT)
            .expect("ordinary before registration");
        let _registered = RegisteredRoot::new(&root);
        assert!(remove_reviewed(&file, &review, &cancel, |_| {}).is_err());
        assert_eq!(fs::read(file).expect("survived"), b"keep");
    }
}
