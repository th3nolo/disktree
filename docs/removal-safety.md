# Removal safety changes in the personal fork

Base: `tobi/disktree` v0.11.0, commit
`6c8d4ce6211bf135ff4e42890faebff1040e1846`.

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

Windows recycling is disabled in this fork. A final identity check cannot
bind the shell's later lookup to the marked object. Every Windows trash
backend now returns an unsupported-operation error, including direct calls;
there is no shell handoff or permanent-deletion fallback. The review starts
with removal blocked until the user explicitly chooses permanent deletion,
which still requires confirmation. A recoverable identity-bound replacement
would need a separate recovery design before recycling can return.

Windows permanent confirmation now prepares an exact, bounded descendant
list in the background. It records identity, kind and directory membership;
file length and modification time are also checked. The final dialog uses
that saved plan, shows its entry count and first full selection path, and
requires a Delete click or a fresh Ctrl+Enter. Plain Enter and held keys
cannot approve deletion. Changed marks require another review.

Before removing a Windows target, the worker opens every reviewed entry
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

