//! Native Windows recycling, with no permanent-delete fallback.
//!
//! The shell accepts paths, not our pinned handles. Check both the handoff
//! and its recycle receipt; compensate an unexpected move without overwrite.
//! This is a recoverable operation, not the permanent worker's atomic binding.

#![allow(
    trivial_casts,
    clippy::ref_as_ptr,
    clippy::inline_always,
    reason = "Windows implement generates vtable casts and inline wrappers"
)]

use super::*;
use crate::windows::removal::ReviewedTree;
#[cfg(test)]
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::E_ABORT;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    BHID_Transfer, FOLDERID_RecycleBinFolder, IShellItem, ITransferAdviseSink,
    ITransferAdviseSink_Impl, ITransferSource, KF_FLAG_DEFAULT,
    SHCreateItemFromParsingName, SHGetKnownFolderItem, SIGDN_FILESYSPATH,
};
use windows::core::{Error, HRESULT, PCWSTR, PWSTR, Ref, Result, implement};

#[cfg(test)]
#[path = "windows_trash_tests.rs"]
mod tests;

struct Apartment;

impl Apartment {
    fn new() -> io::Result<Self> {
        // SAFETY: this worker owns its COM initialization. A changed apartment
        // fails before any operation; successful initialization is balanced.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(io::Error::other)?;
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances this scope's successful CoInitializeEx call after
        // all shell objects and callback references have been released.
        unsafe { CoUninitialize() };
    }
}

fn shell_item(path: &Path) -> Result<IShellItem> {
    // Shell parsing uses ordinary fully qualified names, including long names
    // on supported Windows versions. An unsupported name is an error.
    // A comparison key folds case; using it here would alter the original
    // spelling recorded by Windows for restoration.
    let mut spelling: Vec<_> = path.as_os_str().encode_wide().collect();
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        match prefix.kind() {
            Prefix::VerbatimDisk(_) => {
                spelling.drain(..4);
            }
            Prefix::VerbatimUNC(_, _) => {
                spelling.drain(..8);
                spelling.splice(..0, [u16::from(b'\\'), u16::from(b'\\')]);
            }
            _ => {}
        }
    }
    let path = PathBuf::from(OsString::from_wide(&spelling));
    let wide = super::wide(&path, false)
        .map_err(|error| Error::new(E_ABORT, error.to_string()))?;
    // SAFETY: the UTF-16 path is NUL-terminated and live for this call.
    unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
}

fn item_path(item: &IShellItem) -> Result<PathBuf> {
    // SAFETY: item is a live shell interface. The returned allocation belongs
    // to this caller and is freed once after copying its terminated name.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
    let path = PathBuf::from(OsString::from_wide(unsafe { name.as_wide() }));
    unsafe { CoTaskMemFree(Some(name.as_ptr().cast())) };
    Ok(path)
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::new(E_ABORT, "recycling cancelled"))
    } else {
        Ok(())
    }
}

#[implement(ITransferAdviseSink, Agile = false)]
struct TransferSink {
    cancel: Arc<AtomicBool>,
}

impl ITransferAdviseSink_Impl for TransferSink_Impl {
    fn UpdateProgress(
        &self,
        _size: u64,
        _total: u64,
        _files: i32,
        _files_total: i32,
        _folders: i32,
        _folders_total: i32,
    ) -> Result<()> {
        cancelled(&self.cancel)
    }
    fn UpdateTransferState(&self, _state: u32) -> Result<()> {
        cancelled(&self.cancel)
    }
    fn ConfirmOverwrite(
        &self,
        _source: Ref<'_, IShellItem>,
        _parent: Ref<'_, IShellItem>,
        _name: &PCWSTR,
    ) -> Result<()> {
        Err(E_ABORT.into())
    }
    fn ConfirmEncryptionLoss(
        &self,
        _source: Ref<'_, IShellItem>,
    ) -> Result<()> {
        Err(E_ABORT.into())
    }
    fn FileFailure(
        &self,
        _item: Ref<'_, IShellItem>,
        _name: &PCWSTR,
        error: HRESULT,
        _rename: PWSTR,
        _length: u32,
    ) -> Result<()> {
        Err(if error.is_err() {
            Error::from(error)
        } else {
            E_ABORT.into()
        })
    }
    fn SubStreamFailure(
        &self,
        _item: Ref<'_, IShellItem>,
        _stream: &PCWSTR,
        error: HRESULT,
    ) -> Result<()> {
        Err(if error.is_err() {
            Error::from(error)
        } else {
            E_ABORT.into()
        })
    }
    fn PropertyFailure(
        &self,
        _item: Ref<'_, IShellItem>,
        _key: *const PROPERTYKEY,
        error: HRESULT,
    ) -> Result<()> {
        Err(if error.is_err() {
            Error::from(error)
        } else {
            E_ABORT.into()
        })
    }
}

/// A real shell move back from the bin. Never overwrite a new source occupant.
/// The transfer provider refuses both initial and late name collisions.
fn restore_receipt(receipt: &IShellItem, original: &Path) -> io::Result<()> {
    let identity =
        entry_identity(&item_path(receipt).map_err(io::Error::other)?)
            .ok_or_else(|| {
                io::Error::other("the recycle receipt cannot be verified")
            })?;
    if original.try_exists()? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the original name is occupied; the item remains in the Recycle Bin",
        ));
    }
    let parent = original.parent().ok_or_else(|| {
        io::Error::other("the original parent is unavailable")
    })?;
    let name: Vec<_> = original
        .file_name()
        .ok_or_else(|| io::Error::other("the original name is unavailable"))?
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination = shell_item(parent).map_err(io::Error::other)?;
    // SAFETY: initialized COM and live interfaces. Use the bin provider's
    // direct MoveItem with TSF_FAIL_EXIST (zero), never an overwrite/rename
    // fallback. All failure and overwrite advice is refused.
    let source_parent =
        unsafe { receipt.GetParent() }.map_err(io::Error::other)?;
    let source: ITransferSource =
        unsafe { source_parent.BindToHandler(None, &BHID_Transfer) }
            .map_err(io::Error::other)?;
    let sink: ITransferAdviseSink = TransferSink {
        cancel: Arc::new(AtomicBool::new(false)),
    }
    .into();
    let moved = unsafe {
        let cookie = source.Advise(&sink).map_err(io::Error::other)?;
        let result =
            source.MoveItem(receipt, &destination, PCWSTR(name.as_ptr()), 0);
        source.Unadvise(cookie).map_err(io::Error::other)?;
        result.map_err(io::Error::other)?
    };
    if guard_key(&item_path(&moved).map_err(io::Error::other)?)
        != guard_key(original)
    {
        return Err(io::Error::other(
            "Windows restored the item to a different location",
        ));
    }
    if entry_identity(original) != Some(identity) {
        return Err(io::Error::other(
            "a restoration collision occurred; inspect the original parent and Recycle Bin",
        ));
    }
    Ok(())
}

pub fn recycle_reviewed(
    guard: RemovalGuard,
    review: &ReviewedTree,
    cancel: &Arc<AtomicBool>,
) -> io::Result<()> {
    let _apartment = Apartment::new()?;
    recycle_with(guard, review, cancel, None).map(|_| ())
}

fn recycle_with(
    guard: RemovalGuard,
    review: &ReviewedTree,
    cancel: &Arc<AtomicBool>,
    #[cfg(test)] after_check: Option<Rc<dyn Fn() -> io::Result<()>>>,
    #[cfg(not(test))] _after_check: Option<()>,
) -> io::Result<IShellItem> {
    let original = guard.path.clone();
    let path = original.as_path();
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    let parent_identity = entry_identity(parent).ok_or_else(|| {
        io::Error::other("the recycling parent cannot be verified")
    })?;
    // Pin and verify before constructing shell items. Unlike the permanent
    // worker, recycling then releases these pins for Windows to do the move.
    drop(review.pin_for_recycling(path, cancel)?);
    let item = shell_item(path).map_err(io::Error::other)?;
    let parent_item = shell_item(parent).map_err(io::Error::other)?;
    // SAFETY: initialized COM and live shell interfaces. BHID_Transfer exposes
    // the filesystem's lower-level transfer provider for this source parent.
    let source: ITransferSource =
        unsafe { parent_item.BindToHandler(None, &BHID_Transfer) }
            .map_err(io::Error::other)?;
    // SAFETY: a documented known-folder identifier, default flags, and the
    // current token. The returned interface owns the real Windows bin.
    let bin: IShellItem = unsafe {
        SHGetKnownFolderItem(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT, None)
    }
    .map_err(io::Error::other)?;
    let sink: ITransferAdviseSink = TransferSink {
        cancel: Arc::clone(cancel),
    }
    .into();
    // Recheck immediately before calling only RecycleItem. IFileOperation's
    // DeleteItem can silently call RemoveItem after a quota failure; this
    // provider call returns that failure directly and has no such fallback.
    drop(review.pin_for_recycling(path, cancel)?);
    cancelled(cancel).map_err(io::Error::other)?;
    #[cfg(test)]
    if let Some(hook) = after_check {
        hook()?;
    }
    // SAFETY: the source, real bin, and cancel/failure sink remain live for the
    // synchronous call. Flags zero requests no overwrite or provider fallback.
    let receipt = unsafe {
        let cookie = source.Advise(&sink).map_err(io::Error::other)?;
        let result = source.RecycleItem(&item, &bin, 0);
        let unadvised = source.Unadvise(cookie);
        unadvised.map_err(io::Error::other)?;
        result.map_err(io::Error::other)?
    };
    drop(source);
    drop(sink);
    drop(item);
    let receipt = &receipt;
    let recycled = item_path(receipt).map_err(io::Error::other)?;
    if let Err(error) = review.matches_at(&recycled) {
        // Shell restoration needs write access to the destination directory.
        // Release this operation's pins only after checking the moved item,
        // then refuse a changed parent or an occupied original name.
        drop(guard);
        if entry_identity(parent) != Some(parent_identity)
            || fs::symlink_metadata(parent)?.file_type().is_symlink()
        {
            return Err(io::Error::other(format!(
                "{error}; original parent changed; recover from the Recycle Bin ({})",
                recycled.display()
            )));
        }
        let restored = restore_receipt(receipt, path);
        return Err(io::Error::other(match restored {
            Ok(()) => {
                format!("{error}; unexpected move restored; recycling stopped")
            }
            Err(restore) => format!(
                "{error}; restoration failed ({restore}); \
                recover the item from the Windows Recycle Bin ({})",
                recycled.display()
            ),
        }));
    }
    // The receipt proves this item completed. A late cancellation stops the
    // next target; it must not turn this completed move into an unknown error.
    Ok(receipt.clone())
}
