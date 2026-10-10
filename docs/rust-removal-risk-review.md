# Rust and disk cleanup failure review

Reviewed 2026-10-10 against the personal fork's Windows hardening in PR #8,
commit `55b77bf5a897439e50e85f7a8c9b3387e82e6bb3`, and the verification work in
PR #9. This is a source and regression review, not a certification that all
filesystems, builds, or executions are safe. No personal files, real cloud
account, or installed DiskTree GUI were used for this work.

## Language risks that remain relevant in Rust

| Risk and primary source | Actual code and assessment | Evidence or remaining action |
| --- | --- | --- |
| Memory safety does not establish filesystem authorization. The [standard filesystem docs](https://doc.rust-lang.org/std/fs/) describe time-of-check/time-of-use races. | Paths can be replaced between checking and deleting even when the Rust code is well typed. `RootSnapshot`, marked identities, `pin_parent`, `open_removal`, saved descendant stamps, and deletion by handle address this in the Windows worker. | Existing swapped-root, changed-kind, junction, outside-path and no-fallback regressions. Lean proves the abstract approved-set rule; path/handle correctness remains an external contract. |
| [`unsafe` and FFI still require valid pointers, values, aliasing, layouts and lifetimes](https://doc.rust-lang.org/stable/reference/behavior-considered-undefined.html). | Windows calls in `windows.rs` and `windows_removal.rs` cross this boundary. `handle_identity` initializes a `FILE_ID_INFO` buffer before use; `delete_opened` passes a live owned handle and correctly sized disposition buffer. Source comments state these contracts; no new violation was established in this review. | Native Windows x64/ARM64 tests support these operations. Lean does not prove `windows-sys`, raw-pointer soundness, Windows, or NTFS/ReFS. Further buffer/parser fuzzing and a dedicated unsafe review remain useful. |
| [Integer overflow](https://doc.rust-lang.org/book/ch03-02-data-types.html) can panic or wrap depending on compilation settings. | This repository already enables `overflow-checks = true` in release. Nevertheless, `Plan::bytes`, attribution totals, `unattributed`, and successful worker byte totals used unchecked additions. Extremely large estimates can therefore panic; the worker case is after approved files have been removed. | PR #9 adds regressions using two tiny owned files whose supplied estimates are `u64::MAX` and 1. This is an arithmetic fault-injection fixture, not a claim that these files occupy that much space. Entry-count proofs do not establish byte-total safety. |
| [Mutex poisoning is advisory](https://doc.rust-lang.org/std/sync/struct.Mutex.html), and type safety does not prevent deadlocks or a stuck external call. | The mount cache recovers poisoned locks with `into_inner`; the worker rereads mount information before deletion. Channel disconnection now produces one terminal error, while an idle connected channel is not treated as failure. | Existing startup/disconnection/idleness regressions cover those distinctions. There is no proof of worker liveness, no timeout that can safely undo a hung Win32 call, and no new poisoned-state authorization bug was reproduced here. |
| Recursive deletion is not transactional. [`remove_dir_all`](https://doc.rust-lang.org/std/fs/fn.remove_dir_all.html) documents partial removal and concurrent-directory mutation. | Modern Rust's implementation avoids following symbolic links and protects against symlink TOCTOU on supported platforms; it should not be described as inherently unsafe recursion. DiskTree additionally needs reviewed-list, mount and identity guarantees. Windows pins reviewed entries before deleting and never enrolls new children afterward. | Preflight failure preserves siblings. Late failure or cancellation can still leave an approved prefix deleted, with no rollback. The Lean prefix theorem intentionally allows that result. |
| Allocation/resource exhaustion and recursive depth are not prevented by memory-safe types. | Windows review is iterative and bounded to 20,000 entries per entire plan; each entry can require a retained handle. Refusal must not become an unbounded recursive fallback. | The limit regression and formal count bound support this policy. The cap does not prove all allocations or kernel resources always succeed, nor does it extend to every scanner/parser path. |
| Correct typing cannot make UI intent, external state, or supply-chain provenance correct. | Final confirmation consumes the saved plan; held/repeated keys and changed marks are covered by GUI tests. Cloud guards check known OneDrive roots, reparse/cloud flags and Cloud Files ancestry; unexpected query errors refuse. | Real disposable provider/VM runs remain pending. PE mitigations, Defender scans, locked dependencies and CI artifact provenance are separate evidence; they do not establish the absence of malware or all data-loss defects. |

The three overflow regressions were first committed without the fix at
`c47b418e65c97d94872d3b06ffc29dc36c4054b8` and reproduced `attempt to add with
overflow` in all three OS core jobs:
[Windows](https://github.com/th3nolo/disktree/actions/runs/38081425385/job/114298997502),
[Linux](https://github.com/th3nolo/disktree/actions/runs/38081425385/job/114298997613),
and [macOS](https://github.com/th3nolo/disktree/actions/runs/38081425385/job/114298997567).
PR #9 replaces these additions with [`u64::saturating_add`](https://doc.rust-lang.org/std/primitive.u64.html#method.saturating_add),
including the combined attribution subtraction. Estimates above the machine
range are capped rather than wrapped or treated as exact measurements. The
worker must still send `Done`, and the regression checks unselected canary
bytes after both approved files are deleted. No removal guard is relaxed.

## Historical failures in comparable operations

These are primary project issue records. A user report is distinguished from
a maintainer-confirmed fix; none is evidence of a current vulnerability in
the latest release or of the same bug in DiskTree.

| Record | Reported mechanism and status | Transfer to DiskTree |
| --- | --- | --- |
| [Steam for Linux #3671](https://github.com/ValveSoftware/steam-for-linux/issues/3671), opened 2015-01-14 | Users reported broad deletion during client recovery. The discussion identified an empty cleanup-root variable expanding a destructive wildcard to the filesystem root. Valve later stated the investigation had concluded and locked the issue as resolved. The record does not establish the exact initial trigger for every report. | Never turn an absent/empty/unverifiable root into broader authority. The Lean empty-root and empty-prefix counterexample checks use pure values, without executing a destructive shell command. Runtime guards also require the saved root identity. |
| [BleachBit #1389](https://github.com/bleachbit/bleachbit/issues/1389), opened 2022-08-19 | Reported Windows 4.4.2 Recycle Bin cleanup following junction/directory-link contents outside the link. A maintainer posted the fix and closed the issue on 2026-02-13; linked commits explicitly stop following these links. | Tests must verify destination bytes survive, including links introduced after review. DiskTree already has owned outside canaries and junction replacement/late-link tests. Windows recycling is disabled until its identity contract is safe. |
| [Czkawka #1187](https://github.com/qarmin/czkawka/issues/1187), opened 2024-01-15 | A Windows 6.1.0 user reported the same pathname listed twice as a duplicate, followed by losing that sole file during deduplication. The issue is closed; this review did not independently reproduce or establish its root cause. Czkawka itself is Rust software. | Memory safety does not prevent alias/selection mistakes. DiskTree folds normalized overlapping targets and tests case/verbatim aliases and hardlinks. Distinct hardlink names must not be mistaken for distinct file contents or a promise of reclaimable bytes. |
| [Czkawka #1188](https://github.com/qarmin/czkawka/issues/1188), opened 2024-01-15 | The same user reported a symbolic-link replacement action deleting duplicates without the expected replacement links. This is a closed user report, not a root cause reproduced here. | Multi-step operations need explicit failure ordering. DiskTree does not implement this deduplication action, but its deletion errors must report partial results without claiming rollback or using a weaker fallback. |

The practical priorities from these records are strict authority boundaries,
link/alias semantics, immutable review data, and honest partial-failure
reporting. This inference motivates the model and tests; the reports do not
prove the complete correctness of DiskTree's implementation.

## Formal evidence and its boundary

The [Lean package](../formal/lean/README.md) checks authorization, fixed-list
execution, preflight refusal, per-entry stopping, namespace preservation and
entry-count bounds. It includes executable approval, an accepted ordinary
case, rejected protected/root/sibling cases, and counterexamples to weaker
empty-prefix and dynamic-enrollment policies. Six cancellation vectors are
shared with actual Windows deletion tests, rather than copied into two
independent fixture lists.

Lean proves statements about its definitions. A proof of this model is not a
proof that the Rust binary implements it for all executions. In particular,
the model abstracts normalized paths and external observations; FFI,
concurrency, OS behavior, UI, power loss, real sync providers, and byte-size
metadata are not automatically verified. See [Lean's account of its
foundations](https://lean-lang.org/theorem_proving_in_lean4/Axioms-and-Computation/).
The package audits theorem dependencies and rejects unfinished proofs or
additional project axioms; foundational Lean principles remain trusted.

[Aeneas](https://github.com/AeneasVerif/aeneas) can translate a subset of safe
Rust through Charon/MIR to Lean and other backends. Its current README lists
unsafe and concurrent code as limitations. A future refinement proof should
begin with extracted pure selection/execution policy and explicitly model
external definitions. This work does not claim to have run Aeneas or verified
the Win32 worker through translation.

DeepWiki research was attempted early for the public upstream repository;
live Q&A/browser access was unavailable. No DeepWiki answer is claimed here.
Architecture and symbol assessments come from the pinned repository sources,
and the language/incident claims above use official documentation or primary
project issue records.

## Remaining verification work

1. Run the actual GUI candidate in an isolated disposable Windows VM and
   perform an end-to-end disposable cloud-provider check. Local registered
   Cloud Files roots and automated GUI harnesses are useful but narrower.
2. Fuzz Win32 directory/MFT parsing and normalization, including unusual
   Unicode, long paths, invalid buffers, aliases, stream names and ReFS IDs.
   Supply malformed data through owned fixtures, not filesystem corruption.
3. Audit all `unsafe` entry points and external-call contracts, then extract
   pure Rust policy for translation/refinement when tool support permits.
4. Extend deterministic worker failure/crash scheduling and report tests.
   Model process termination as partial progress; do not advertise rollback.
5. Re-review the Lean/Rust mapping whenever guards, traversal order, handle
   sharing, cloud refusal or worker semantics change. A green model alone
   must not be treated as proof of a changed executable.
