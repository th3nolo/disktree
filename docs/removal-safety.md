# Removal safety changes in the personal fork

Base: `tobi/disktree` v0.11.0, commit
`6c8d4ce6211bf135ff4e42890faebff1040e1846`.

The [Rust and cleanup risk review](rust-removal-risk-review.md) records
primary incident sources, reproduced byte-accounting overflow, and remaining
verification work. The [Lean model](../formal/lean/README.md) checks abstract
reviewed-removal rules and shares cancellation fixtures with native Windows
tests. It does not prove the Rust executable or external Windows contracts.

- Capture the canonical root and its filesystem identity before scanning.
  Review and the removal worker refuse a replaced or unverifiable root.
- Capture the identity of each marked entry, without following a final link.
  Capture its kind in the same metadata read or Windows handle and refuse a
  mismatch with the displayed scan. A file tile cannot authorize recursive
  deletion of a directory created at its path before marking. Planning and
  the opened Windows removal handle also refuse a directory/type mismatch.
  Missing or replaced entries lose their marks instead of authorizing a new
  occupant of the same path. Refresh drops marks whose directory scope
  changes, even if their identity remains the same; it never widens a leaf
  mark into permission to recurse. Refresh also rebuilds the mark index.
  Verified marks outside a new scan stay kept back: the absence of a path
  from a different root is not evidence that the entry disappeared.
- Removal projections always use measured bytes, even when Files mode
  weights treemap tiles by file count. Switching modes or refreshing marks
  cannot convert that count into purported recoverable bytes.
- Nested targets are ordered by the same normalized guard key used for
  containment. Windows case and verbatim-prefix aliases cannot put a child
  before its marked parent and count their bytes twice.
- A lossy name must never become an actionable path. Names containing U+FFFD
  are conservatively refused, including literal U+FFFD names, because the
  current tree cannot distinguish them from invalid UTF-16/UTF-8 names.
  Invalid crumbs and names that are not a single normal component also fail.
  The tree still measures and displays such entries; Node stays at 88 bytes.
- Windows locks canonical ancestors and parent directories while removal
  runs. Permanent deletion opens each entry without following reparse points,
  verifies the full 128-bit file ID, and deletes through that handle. A
  failed handle operation has no path-based deletion fallback.
- Unsupported file IDs or deletion flags cause a reported failure. Older
  Windows 10 builds and some filesystems/providers may therefore refuse an
  operation; refusal must never silently authorize a weaker removal.

Regression tests use temporary trees, preserve sentinel files, and cover
root retargeting, replaced entries, missing marks, lossy names, parent locks,
links, and read-only files. Validation uses the repository's unchanged
`cargo xtask lint` and `cargo xtask test` gates on Windows, Linux and macOS.

Windows recycling is the default, through `windows_trash.rs` and the native
`ITransferSource::RecycleItem` interface. Both Windows modes use the reviewed
plan and a confirmation dialog. The backend calls only the provider's explicit
recycling method; it never asks the general `IFileOperation::DeleteItem` engine
to remove an item. A real oversized sparse-file test reproduced permanent
deletion with that general engine despite recycling flags. The direct method
returns the quota error with the original object intact. Failure advice refuses
retries, overwrite and encryption loss. An error stops pending targets and never
calls `RemoveItem` or the permanent worker. Bare `move_to_trash(path, ...)`
calls on Windows
remain unsupported because they do not carry an approved identity and plan.

The worker pins the ancestry and checks all reviewed entries before handoff.
Immediately before the provider call it repeats metadata, cloud and membership
checks, then releases
entry pins so Windows can move them. The returned recycle receipt must match
the reviewed full file IDs, kinds, file lengths, modification times and exact
relative membership. A replaced item or late child can therefore be moved
temporarily by Windows, then detected and returned. The return releases this
operation's parent pins, refuses a changed parent and occupied original name,
and uses the provider's `MoveItem` with `TSF_FAIL_EXIST` and refused overwrite
advice to preserve a late occupant. Failed restoration reports recovery from
the Recycle Bin or original parent.

This is compensation after a native move, not an atomic identity-bound shell
handoff. A crash, external mutation or restore failure can leave an unexpected
item in the Recycle Bin; cancellation does not undo completed moves. The
permanent worker's handle-binding guarantee and the Lean filesystem model
do not prove this native recycling path. Recycling does not immediately free
space; the application shows zero projected reclamation for that mode.

Windows confirmation prepares an exact, bounded descendant
list in the background. It records identity, kind and directory membership;
file length and modification time are also checked. The final dialog uses
that saved plan, shows its entry count and first full selection path, and
requires a Recycle/Delete click or a fresh Ctrl+Enter. Plain Enter and held keys
cannot approve deletion. Changed marks require another review.

Before permanently removing a Windows target, the worker opens every reviewed entry
without following links, retains all handles without sharing writes or
deletion, and compares metadata and membership again. A child that is
already locked, replaced or newly added therefore refuses that target
before any of its siblings is deleted. Deletion uses only those handles,
in child-before-parent order. A later addition is never enrolled in the
operation; it can make the final directory deletion fail instead.

The review is capped at 20,000 entries per plan, including files, folders
and links. Larger selections must be split; there is no weaker recursive
fallback. Cancellation is checked during preparation, preflight and before
each Windows deletion. It cannot interrupt a Win32 call already in flight.
Actual deleted entries are counted separately from completed top-level
targets. Partial errors say how many entries were already deleted. Failed
and unattempted marks survive completion if their objects still exist.
Worker startup failure or a disconnected worker channel ends with an error,
rather than leaving the UI running forever. A process crash is not rollback.
Multiple selected hardlinks to the same object can conflict with the
preflight's own no-delete-sharing handles; refusal requires splitting that
selection, rather than weakening the sharing contract.

Native recycle regression tests use owned temporary trees and the real
Windows bin. They check restore bytes, names and file identities, an 80-file
batch cancelled after five completed moves, quota failure without deletion,
changed files, replacement after the final check, a late child and a
restore collision. Each successful scenario writes a JSON record when
`DISKTREE_RECYCLE_EVIDENCE_DIR` is set; CI retains x64 and ARM64 evidence.
Successful test moves are restored by their exact receipts; tests never
empty the user's bin or restore unrelated items. These filesystem tests do
not establish native desktop rendering, every Windows version or provider.

Registered Windows Cloud Files sync roots, known OneDrive environment roots,
online-only attributes and unsupported non-link reparse providers are
refused during preparation and checked again during removal. Detection asks
canonical ancestors too: an ordinary hydrated child can be non-cloud while
its parent is a registered sync root. An unexpected
cloud-status query error also refuses deletion. Native regression fixtures
register only owned temporary roots and unregister them afterwards. They
exercise fully local files and registration after review; they do not use a
real cloud account or establish end-to-end OneDrive/Dropbox behavior.

Limits: entry identity is captured when marking, not for every scanned node.
A same-kind replacement before marking can still be marked as the current
occupant; the type guard does not prove scan-time identity. Review metadata
is not a content hash or a filesystem snapshot. File contents changed while
preserving identity, length and modification time are not distinguished.
Changes, cancellation, filesystem errors or process exit after deletion
starts can leave a partial result, without rollback. Unregistered legacy
sync providers outside known OneDrive roots are not universally detectable.
The Linux and macOS deletion/trash backends are unchanged; their child-level
cancellation and concurrent-mutation guarantees have not been extended.

Antivirus reports for upstream release binaries remain unresolved by these
source changes. No upstream executable was run on the user's PC. A clean
source review and passing tests do not establish that a downloaded binary
is malware-free.

The Windows CI continuation also records the built candidate's source,
toolchain, dependencies, PE imports, hashes, and Defender scan results.
Candidate uploads require successful executable and ZIP scans and a
non-GUI startup check; failed validation preserves evidence only. These
build records are unsigned and do not establish the provenance of upstream
downloads. See [windows-validation.md](windows-validation.md) for the
procedure and the desktop checks that remain separate from hosted CI.

