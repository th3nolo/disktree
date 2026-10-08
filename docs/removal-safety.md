# Removal safety changes in the personal fork

Base: `tobi/disktree` v0.11.0, commit
`6c8d4ce6211bf135ff4e42890faebff1040e1846`.

- Capture the canonical root and its filesystem identity before scanning.
  Review and the removal worker refuse a replaced or unverifiable root.
- Capture the identity of each marked entry, without following a final link.
  Missing or replaced entries lose their marks instead of authorizing a new
  occupant of the same path. Refresh also rebuilds the mark index.
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

Limits: a filesystem identity binds an object, not its contents. Another
process can change files inside a marked directory before deletion. The
Recycle Bin uses Windows' path-based shell API: pinned ancestors and a final
identity check reduce redirection risk, but this is not a handle-based,
race-free trash implementation. No claim is made that concurrent hostile
mutation is fully isolated on every supported platform.

Antivirus reports for upstream release binaries remain unresolved by these
source changes. No upstream executable was run on the user's PC. A clean
source review and passing tests do not establish that a downloaded binary
is malware-free.
