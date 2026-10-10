# Reviewed Windows removal model

This package checks an abstract safety model with Lean 4.34.1. It does not
translate the Rust program or prove the compiled executable. Rust's compiler
and clippy check the Rust types; Lean checks the types and proofs in this
package. Neither type checking alone proves that a filesystem action has the
right authorization.

The starting implementation is PR #8 at
`55b77bf5a897439e50e85f7a8c9b3387e82e6bb3`. Source changes covered by the formal
workflow must receive a correspondence review as well as a green Lean job:
running an unchanged model does not automatically verify changed Rust.

## What is modeled

`RemovalModel.lean` defines component paths, namespace entries, an executable
approval function returning a proof-carrying `Approved` subtype, fallible
preflight observations, and a frozen-list executor for **one reviewed target**.
Its input order is Rust's reverse preorder: children before their parents.
The executor records an entry only when its deletion succeeds and stops that
target on cancellation, failure, cloud refusal, or an exhausted schedule.

For every approved list and every decision schedule, the removed entries form
a prefix of the fixed deletion order. No entry absent from the review can be
enrolled later; each removed path is strictly below the scanned root and is
not marked as protected. Failed preflight removes nothing. The worker's
abstract namespace effect preserves names outside the approved list. Counts
stay within 20,000 and fit `u64`. These are universally quantified proofs,
not just checks of the six example traces.

Four arithmetic theorems specify unsigned saturating addition: it is bounded,
does not wrap below its left operand, preserves an in-range sum, and clamps
overflow to `u64::MAX`. PR #9 uses this operation for review totals, volume
attribution, and completed worker byte estimates. The primitive's Rust/Lean
correspondence is a trusted arithmetic specification, not translated Rust.

The model explicitly permits partial deletion. There is no proof of rollback
or atomic removal of a whole folder. Failure stops **this target**; Rust's
outer worker can continue to another approved target. Cancellation stops the
next attempt after the atomic flag is observed, not a Win32 call already in
flight. A schedule is a record of observations and successful/failed calls,
not a prediction of operating-system behavior.

## Correspondence and evidence

| Lean definition or theorem | Rust correspondence | Native regression evidence |
| --- | --- | --- |
| `below`, `empty_root_rejected`, `root_is_not_below_itself`, `removed_stays_in_scope` | `removal::normalize`, `guard_key`, `refuse`, `linked`, `RootSnapshot`, worker root checks | `paths_outside_the_root_are_refused`, `sibling_prefixes_are_not_treated_as_containment`, root replacement and protected-tree tests |
| `approve`, `Approved`, `removed_was_approved`, `unapproved_entry_never_removed` | `Plan::prepare_review`, private `ReviewedPlan`/`ReviewedTree`, saved target comparison | `an_unprepared_plan_never_starts_permanent_windows_deletion`, `the_worker_refuses_outside_paths_even_in_a_modified_plan` |
| `Preflight`, `preflight_failure_removes_nothing` | `pin_parent`, `open_removal`, `Stamp::capture`, directory membership checks in `remove_reviewed` | locked child, changed metadata, replaced descendant, and new descendant regressions |
| `execute_prefix`, `run_prefix` | `review.entries.iter().zip(opened).rev()`; `delete_opened`; error return before the next attempt | `a_child_added_during_deletion_is_never_added_to_the_approved_list` and shared cancellation traces |
| `cancelled_before_next`, `failure_before_next`, `cloud_refusal_before_next` | `check_cancel` then `refuse_cloud` then `delete_opened`; no path fallback | child cancellation, sharing violation, and cloud error tests |
| `run_preserves_unselected` | deletion by the checked handle; namespace name is separate from object identity | generated selections, outside canaries, junctions, and selected hardlink regressions |
| `removed_count_bounded`, `removed_count_fits_u64` | `REVIEW_ENTRY_LIMIT`, global budget in `prepare_review`, one attempt per reviewed entry | `review_limits_refuse_before_any_file_is_deleted` |
| `saturating_add_*` | `u64::saturating_add` in review, attribution, and worker totals | overflowing review/projection/worker estimate tests, reproduced failing before the fix |
| `empty_prefix_counterexample`, `dynamic_enrollment_counterexample` | rejected design alternatives, not production code | root/sibling guards and the late-child regression |

The six lines in `cancellation-cases.txt` specify cancellation after 0, 1, 2,
3, 4, or 10 successful entries for three sorted children and their parent.
`lake exe checkFixtures` executes the model on these lines.
`shared_lean_cancellation_fixtures_match_native_windows_removal` reads the same
file and calls actual Win32 review/removal on owned temporary directories,
checking the exact order, remaining bytes, parent existence, and outside
canaries. Cancellation at 4 succeeds: it occurs after the final entry, so
there is no next operation to interrupt. The 10 case also completes without
adding entries. This is a finite correspondence check, not a refinement
proof for all Rust executions.

## External contracts and exclusions

- Component IDs assume correct Windows normalization, case/alias handling,
  volume identification, and ambiguous-name refusal. Those parsers are not
  formalized. The model does not follow links; the correspondence assumes
  Win32 opens the link entry rather than its destination.
- `Preflight` observations are explicit inputs. Lean cannot establish that
  Rust computed them correctly, that Windows honors sharing flags, or that
  a live handle refers to the expected object. FFI pointer/layout validity,
  access rights, filesystem implementations, and Windows are trusted here.
- Membership, IDs, kind, length, and mtime are runtime observations, not a
  content hash or snapshot. A write preserving the checked metadata is not
  ruled out by this proof. Directory ordering correctness is verified by
  the native traces, not by a formalized tree traversal.
- The namespace-preservation theorem describes only this worker's effect.
  Other programs can change files. Hardlink names are distinct even when
  they share an object ID; the theorem does not promise all external object
  metadata or cloud replicas remain unchanged.
- Unknown/legacy sync providers, real OneDrive/Dropbox accounts, process or
  power loss, disk corruption, malicious dependencies, GUI behavior, and
  worker liveness are outside the model. Local Cloud Files registrations in
  CI are not end-to-end tests of a real provider.
- The entry-count bound is not a byte-total bound. Byte estimates require
  their own arithmetic regression; `Nat` in Lean does not make Rust's `u64`
  sums safe automatically. Saturated estimates are capped summaries, not
  precise measurements beyond `u64::MAX`. Linux/macOS deletion backends are
  not covered by this Windows target executor proof, although their shared
  byte accounting uses the corrected arithmetic too.

## Reproduce and audit

With the pinned toolchain installed, from `formal/lean`:

```sh
lake build
lake env lean RemovalModel.lean | tee proof-audit.log
python3 audit.py proof-audit.log
python3 -m unittest test_audit.py
lake exe checkFixtures
```

CI downloads the official Linux archive, checks its recorded SHA-256 before
extraction, and runs these commands on a hosted Ubuntu runner. It installs
nothing on a user's PC. There are no Mathlib or third-party Lean dependencies.
Every theorem is included in `#print axioms` output. `audit.py` requires an
audit entry for every theorem, rejects incomplete proofs, added project
axioms and native proof shortcuts, and permits only Lean's standard
foundations: `propext`, `Quot.sound`, and `Classical.choice`. Dependencies on
`sorryAx` or other axioms fail CI. Lean warnings are errors.
Five negative/positive audit tests check missing, duplicated, empty and
untrusted proof reports. CI retains the toolchain version, checked merge
commit, dependency audit and model fixture output as evidence for 14 days.

The trusted base includes Lean's kernel and distribution, its foundations,
the CI runner/toolchain, and the external contracts above. The lexical audit
is additional hygiene, not a proof that the CI environment cannot be
compromised. The primary evidence is successful kernel checking.

For a future proof of actual Rust control logic, extract a small pure policy
module, translate it through a pinned tool such as Aeneas/Charon, and prove
refinement against this model. Aeneas currently targets a subset of safe
Rust; unsafe code and concurrency remain limitations. The Win32 boundary
would still need explicit contracts and native integration tests. See the
[risk review](../../docs/rust-removal-risk-review.md) for sources and remaining
work.
