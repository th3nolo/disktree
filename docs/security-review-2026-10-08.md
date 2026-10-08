# DiskTree security review — 2026-10-08

**Decision: fix automatic Git execution before using the current candidate on your main Windows account.** The review confirmed a helper execution issue. It requires an unexpected `git.exe` beside the app or an untrusted executable search path; the reviewed ZIP itself contains no such helper.

The application and candidate were not executed on your computer. Static artifact inspection and an inert reproduction using the application's Git module ran on GitHub's isolated Windows runner. No application source was changed during this review.

| Reviewed item | Identity |
| --- | --- |
| Merged application source | [`cc3ee988284cb85614db824d0d02f510cf7d4046`](https://github.com/th3nolo/disktree/commit/cc3ee988284cb85614db824d0d02f510cf7d4046) |
| Tested PR head | `b0696970d8b449170d6b713f5a594886cb0018a7` |
| Candidate checkout | `1ba077fc267d76937594fba984c281e255d70c52`, the GitHub test merge |
| Candidate and merged source tree | `5b0351a084c3842577708cd4095a4325a27eddd0` |
| Candidate artifact | [11564533643](https://github.com/th3nolo/disktree/actions/runs/37808619702/artifacts/11564533643) |
| Original build and tests | [CI run 37808619702](https://github.com/th3nolo/disktree/actions/runs/37808619702) |
| Supplemental security checks | [Run 37813894822](https://github.com/th3nolo/disktree/actions/runs/37813894822), review tools at `7a4843c55e0576f97a0ee2485f2b8daaf9a986ed` |

**1. Confirmed: Windows can execute an unexpected Git helper when a checkout is selected.**

Impact is arbitrary code execution with DiskTree's privileges. A normal launch exposes your account's permissions; an elevated launch can give the helper administrator privileges. This is a local executable planting issue, not evidence that the current candidate contains malware.

The [selection panel](https://github.com/th3nolo/disktree/blob/cc3ee988284cb85614db824d0d02f510cf7d4046/crates/disktree-app/src/views.rs#L350) automatically calls Git inspection. [`git.rs`](https://github.com/th3nolo/disktree/blob/cc3ee988284cb85614db824d0d02f510cf7d4046/crates/disktree-app/src/git.rs#L159) launches `Command::new("git")` with no absolute executable path. The pinned Rust toolchain searches the application's directory before the inherited PATH in this case. The existing flags disabling hooks, filters, signature checks and lazy fetching constrain real Git; they cannot constrain a different executable selected as Git.

The isolated test compiled the exact Git module with Rust 1.97.1, placed an inert `git.exe` beside the probe executable and queried a fixture directory containing `.git`. The adjacent helper ran four times: config, status and two rev-list calls. It recorded a marker file and returned a plausible clean Git state. Neither the full DiskTree executable nor a malicious payload was launched in this review. See [the reproduction source](https://github.com/th3nolo/disktree/blob/7a4843c55e0576f97a0ee2485f2b8daaf9a986ed/scripts/audit-git-probe.rs) and [Windows job logs](https://github.com/th3nolo/disktree/actions/runs/37813894822/job/113437430344).

Remediation: disable automatic Git subprocesses by default, or resolve and trust an explicit absolute Git executable before probing. Reject executable discovery from the app directory and relative or untrusted PATH entries. Add a regression that places an inert helper beside the application probe and verifies it is not executed. Keep the existing repository-config protections. Rebuild after this change; the current executable still has the issue.

**2. Existing limitation: Recycle Bin operations still have a target replacement window.**

[Permanent removal](https://github.com/th3nolo/disktree/blob/cc3ee988284cb85614db824d0d02f510cf7d4046/crates/disktree-core/src/windows.rs#L578) verifies full 128-bit identities and deletes through handles without following reparse points or using a weaker fallback. The review did not find a new escape from those Windows guards.

[Recycle Bin removal](https://github.com/th3nolo/disktree/blob/cc3ee988284cb85614db824d0d02f510cf7d4046/crates/disktree-core/src/removal.rs#L1277) checks identity immediately before a path-based shell operation. Parent locks prevent ancestor redirection, but the final target can change after that check. This remains a documented limitation, not a new exploit reproduced here. Marking a directory also authorizes its current contents: object identity does not freeze the child tree. Test removal on disposable files before using it on valuable data.

The pinned trash 5.2.9 backend uses `FOF_WANTNUKEWARNING`; Microsoft's documentation says this warns when destruction replaces recycling. Unsupported recycling and desktop dialog behavior still need controlled desktop validation.

**3. Hardening gaps: Control Flow Guard and publisher authentication.**

| Candidate property | Observed result |
| --- | --- |
| Architecture | x64 PE32+ |
| ASLR / high entropy ASLR | Enabled / enabled |
| DEP / NX | Enabled |
| Control Flow Guard flag | Absent |
| Authenticode | NotSigned |
| ZIP structure | Exactly seven expected files; no extra executable, DLL, duplicate entry or traversal path |
| EXE / ZIP hashes and source records | Consistent with the original validation evidence |
| Original Defender EXE and ZIP scans / help check | Passed |

Missing CFG is a defense gap, not proof of an exploitable memory error. Evaluate enabling compiler and linker CFG support, then validate the resulting binary and dependencies.

The unsigned build record is useful evidence but is not a cryptographic attestation. Hash agreement detects changed bytes relative to trusted records; it cannot independently prove that a compromised build produced the claimed source. Main CI still references mutable action tags and a toolchain action's `master` branch. Pin action revisions and consider signed build attestations or publisher signing before broader distribution.

The candidate's **EXE SHA-256** is:

```text
eefa8030ce121da8f22a64a6ed189f855d580897344b5af6bdaa63583f0704c0
```

The **inner candidate ZIP SHA-256** is:

```text
870307889cca983dd3af210e01921aad1fcd21c03e63ecf8813d43aba04adc6d
```

These are distinct from GitHub's outer artifact ZIP digest, `4b1771deed4bbc11e65b788f9e61832fe1262f267108d2edddf0e63b2c7eaa19`. All three were checked during download or inspection. Do not compare the EXE hash with the artifact ZIP digest.

**Dependency findings and checks that passed.**

Cargo-audit 0.22.2 checked all 864 lockfile entries against a fresh clone of the official RustSec database containing 1295 advisories, with no ignored advisory IDs. It reported **zero known vulnerabilities**, five unmaintained warnings, and no additional warning categories. This result is dated and does not rule out undisclosed problems or malicious package code.

| Maintenance warning | Locked version | Present in the resolved Windows normal/build graph |
| --- | --- | --- |
| [instant — RUSTSEC-2024-0384](https://rustsec.org/advisories/RUSTSEC-2024-0384.html) | 0.1.13 | Yes |
| [paste — RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) | 1.0.15 | Yes, as a proc macro |
| [rustls-pemfile — RUSTSEC-2025-0134](https://rustsec.org/advisories/RUSTSEC-2025-0134.html) | 2.2.0 | No |
| [rustybuzz — RUSTSEC-2026-0206](https://rustsec.org/advisories/RUSTSEC-2026-0206.html) | 0.20.1 | Yes |
| [ttf-parser — RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html) | 0.25.1 | Yes |

The source review covered Windows removal and identity handling, root and path reconstruction, UTF-16 names, MFT input parsing and raw volume access, mark refresh, confirmations, cancellation, elevation, Git commands, export and settings writes, manifests and build scripts, and candidate packaging. It also searched all repository-owned Rust files for process launches, writes and network entry points.

No application-owned telemetry, credential collection, automatic download/update, startup registration or antivirus-disabling code was found. This is a source review observation; it is not a complete audit of every transitive dependency or a runtime network trace.

The original exact-tree CI passed all six platform jobs and the removal regression workflow passed on Windows, Linux and macOS. Windows x64 passed 243 tests; one pre-existing whole-disk smoke test was intentionally ignored. The supplemental run verified actual candidate bytes, archive structure, source records, PE properties and the new helper reproduction. Its successful status means the inspection completed; it does not mean all findings are fixed.

**Next steps.**

1. Fix Git executable discovery or disable automatic Git probing, then rebuild and repeat artifact validation.
2. Perform the first full GUI and deletion tests in a disposable Windows VM or Windows Sandbox with networking disabled and only a read-only mapping of the candidate folder. Create test files inside the guest, not on a writable host share.
3. After controlled testing, use a fresh dedicated extraction folder, verify the EXE hash, and launch with your normal account. Start with a disposable directory; do not elevate for the first run.
4. Address CFG, action pinning and dependency maintenance separately. Real GPU/Explorer behavior and Windows 10 compatibility remain unverified.

[Candidate inspection evidence](https://github.com/th3nolo/disktree/actions/runs/37813894822/artifacts/11566386780) and [dependency audit evidence](https://github.com/th3nolo/disktree/actions/runs/37813894822/artifacts/11565354718) are retained until October 22, 2026. The review scripts and this report remain in the repository's audit branch.

Primary references: [Rust Windows executable lookup](https://github.com/rust-lang/rust/blob/1.97.0/library/std/src/sys/process/windows.rs#L528), [trash 5.2.9 Windows backend](https://github.com/Byron/trash-rs/blob/v5.2.9/src/windows.rs#L97), [Microsoft file-operation flags](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags), and [Microsoft Windows Sandbox guidance](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/).
