# Windows PC safety gate

This gate targets unintended deletion outside the approved selection. Test
counts, antivirus scans and successful builds are supporting evidence, not a
guarantee against data loss. Approval applies to one executable hash and one
tested environment, not every build or Windows/filesystem combination.

## Automated deletion acceptance

`crates/disktree-core/src/removal_acceptance.rs` runs the actual Windows
permanent-removal worker against owned temporary trees. Every case compares
the original bytes of an unmarked file inside the root and two files outside
it. No case executes deletion against a real profile or system directory.
Junction creation and filesystem errors are asserted, never silently skipped.

| Case | Required result |
| --- | --- |
| Eight outside junctions added to a marked directory after planning | Remove the marked tree; preserve every outside target |
| Marked directory replaced by an outside junction | Refuse; preserve the original tree and outside data |
| First target disappears from a two-target batch | Report one failure; remove only the other approved target |
| Target held with an exclusive Windows handle | Report failure; no weaker deletion fallback |
| Cancellation set before worker execution | No item attempted and no data changed |
| Valid plan modified with outside, sibling-prefix and traversal paths | Refuse each path even with valid target identities |
| 64 distinct filenames including spaces and Unicode | Remove only the ten selected files; block the root and outside target |
| Selected hardlink to an outside file | Unlink only the selected name; preserve outside name and bytes |

The existing regressions additionally cover protected system/profile aliases,
root and entry replacements, changed entry kinds, ambiguous names, parent
locks, mount boundaries and refusal of every Windows trash backend. The
window harness covers review, explicit permanent-mode selection, confirmation
and Escape cancellation. These tests must all pass without weakening the
repository's `cargo xtask lint` and `cargo xtask test` gates.

## Candidate gate

- Test the exact PR head on Windows x64 and ARM64, and run the normal Linux
  and macOS jobs. Any failing or skipped required job blocks acceptance.
- Require the existing release build, PE hardening fixtures, adjacent-Git
  executable regression, fresh Defender executable and ZIP scans, and help
  startup check. Preserve the build provenance and executable SHA-256.
- Open a candidate only when its hash and provenance match the successful
  run. Antivirus results cannot prove absence of malware or logic defects.

## Desktop gate

Run the six checks in `windows-validation.md` inside a Windows VM with no
writable host folders, real user files or cloud-drive mounts. Use the same
candidate and verify its hash before launch. Include actual mouse/keyboard
confirmation and cancellation, root retargeting, and replacement fixtures.
Record Windows build, filesystem, privilege level, executable hash, Defender
state and observed results. An unperformed desktop check is pending, not a
pass. Windows Server CI does not establish Windows 10 or desktop GPU support.

## Decision and remaining limits

Passing automation supports testing the candidate in that disposable VM.
PC deletion approval additionally requires recorded desktop results on the
intended environment. Keep important-data deletion blocked until those
results exist and a recoverable-copy strategy has been verified.

Windows recycling remains disabled. Permanent removal has no rollback and
may finish only part of a marked directory before an error or process exit.
Cancellation is checked between top-level targets, not between every child
of a directory. Selecting a directory authorizes its contents at execution;
entry identity does not freeze its contents. A same-kind replacement before
marking is not distinguished from the object shown by the old scan.

The executable is unsigned unless its provenance says otherwise. Hosted CI
does not validate every filesystem, remote share, storage provider, old
Windows version, kernel driver or hostile administrator. See
`removal-safety.md` and `windows-validation.md` for the existing boundaries.
