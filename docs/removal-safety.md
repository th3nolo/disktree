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

Limits: entry identity is captured when marking, not for every scanned node.
A same-kind replacement made before marking can therefore be marked as the
current occupant; the type guard does not prove scan-time object identity.
A filesystem identity binds an object, not its contents. Another
process can change files inside a marked directory before deletion. The
Linux and macOS trash backends are unchanged; no claim is made that
concurrent hostile mutation is fully isolated on every supported platform.

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

