import Std

set_option warningAsError true

namespace DiskTree

-- Component IDs stand for already normalized namespace names, including a
-- volume component. Windows parsing and alias resolution are external contracts.
abbrev Path := List Nat

def below (root path : Path) : Prop :=
  root ≠ [] ∧ root.length < path.length ∧ path.take root.length = root

instance (root path : Path) : Decidable (below root path) :=
  inferInstanceAs (Decidable (_ ∧ _ ∧ _))

structure Entry where
  path : Path
  objectId : Nat
  guardedOut : Bool
  deriving DecidableEq, Repr

-- This represents one reviewed target's reverse preorder, including its own
-- entry. The global Rust batch budget is stricter than this per-target bound.
def orderedDistinct (entries : List Entry) : Prop :=
  entries.Pairwise (fun earlier later =>
    earlier.path ≠ later.path ∧ ¬ below earlier.path later.path)

instance (entries : List Entry) : Decidable (orderedDistinct entries) :=
  inferInstanceAs (Decidable (entries.Pairwise _))

def valid (root : Path) (entries : List Entry) : Prop :=
  root ≠ [] ∧ entries.length ≤ 20000 ∧
    (∀ entry ∈ entries, below root entry.path ∧ entry.guardedOut = false) ∧
    orderedDistinct entries

instance (root : Path) (entries : List Entry) : Decidable (valid root entries) :=
  inferInstanceAs (Decidable (_ ∧ _ ∧ _ ∧ _))

abbrev Approved (root : Path) := { entries : List Entry // valid root entries }

def approve (root : Path) (entries : List Entry) : Option (Approved root) :=
  if h : valid root entries then some ⟨entries, h⟩ else none

theorem approve_rejects_invalid (root : Path) (entries : List Entry)
    (h : ¬ valid root entries) : approve root entries = none := by
  simp [approve, h]

-- Approval must accept every valid plan, not only the positive fixtures.
theorem approve_accepts_iff_valid (root : Path) (entries : List Entry) :
    (approve root entries).isSome = true ↔ valid root entries := by
  by_cases h : valid root entries <;> simp [approve, h]

theorem empty_root_rejected (entries : List Entry) : approve [] entries = none := by
  apply approve_rejects_invalid
  simp [valid]

theorem root_is_not_below_itself (root : Path) : ¬ below root root := by
  simp [below]

-- Each flag summarizes a fallible external operation, rather than claiming a
-- proof of Win32, handle lifetime, metadata, cloud discovery, or path parsing.
structure Preflight where
  rootMatches : Bool
  identitiesMatch : Bool
  membershipMatches : Bool
  guardsPass : Bool
  handlesPinned : Bool
  deriving Repr

def Preflight.passes (p : Preflight) : Bool :=
  p.rootMatches && p.identitiesMatch && p.membershipMatches &&
    p.guardsPass && p.handlesPinned

inductive Decision where
  | remove
  | cancel
  | failure
  | cloudRefusal
  deriving DecidableEq, Repr

-- A successful removal is recorded only after the external call succeeds.
-- Decisions are sampled before each next entry. A depleted schedule stops.
def execute : List Entry → List Decision → List Entry
  | [], _ => []
  | _, [] => []
  | entry :: rest, .remove :: decisions => entry :: execute rest decisions
  | _ :: _, .cancel :: _ => []
  | _ :: _, .failure :: _ => []
  | _ :: _, .cloudRefusal :: _ => []

def run (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) : List Entry :=
  if preflight.passes then execute plan.val decisions else []

-- Prefix witnesses expose the unattempted suffix and allow partial results.
theorem execute_prefix (entries : List Entry) (decisions : List Decision) :
    ∃ remaining, entries = execute entries decisions ++ remaining := by
  induction entries generalizing decisions with
  | nil => exact ⟨[], rfl⟩
  | cons entry rest ih =>
    cases decisions with
    | nil => exact ⟨entry :: rest, rfl⟩
    | cons decision decisions =>
      cases decision with
      | remove =>
        obtain ⟨remaining, h⟩ := ih decisions
        exact ⟨remaining, by simpa [execute] using congrArg (List.cons entry) h⟩
      | cancel => exact ⟨entry :: rest, rfl⟩
      | failure => exact ⟨entry :: rest, rfl⟩
      | cloudRefusal => exact ⟨entry :: rest, rfl⟩

theorem run_prefix (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) :
    ∃ remaining, plan.val = run plan preflight decisions ++ remaining := by
  unfold run
  split
  · exact execute_prefix _ _
  · exact ⟨plan.val, rfl⟩

theorem removed_was_approved (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry)
    (h : entry ∈ run plan preflight decisions) : entry ∈ plan.val := by
  obtain ⟨remaining, hprefix⟩ := run_prefix plan preflight decisions
  rw [hprefix]
  exact List.mem_append_left remaining h

theorem removed_stays_in_scope (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry)
    (h : entry ∈ run plan preflight decisions) :
    below root entry.path ∧ entry.guardedOut = false := by
  exact plan.property.2.2.1 entry (removed_was_approved plan preflight decisions entry h)

theorem unapproved_entry_never_removed (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry) (h : entry ∉ plan.val) :
    entry ∉ run plan preflight decisions := by
  intro removed
  exact h (removed_was_approved plan preflight decisions entry removed)

theorem preflight_failure_removes_nothing (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (h : preflight.passes = false) :
    run plan preflight decisions = [] := by
  simp [run, h]

theorem cancelled_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.cancel :: decisions) = [] := by
  cases entries <;> rfl

theorem failure_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.failure :: decisions) = [] := by
  cases entries <;> rfl

theorem cloud_refusal_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.cloudRefusal :: decisions) = [] := by
  cases entries <;> rfl

theorem removed_count_bounded (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) : (run plan preflight decisions).length ≤ 20000 := by
  obtain ⟨remaining, h⟩ := run_prefix plan preflight decisions
  have lengths := congrArg List.length h
  simp only [List.length_append] at lengths
  have bound := plan.property.2.1
  omega

theorem removed_count_fits_u64 (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) :
    (run plan preflight decisions).length < 18446744073709551616 := by
  have bound := removed_count_bounded plan preflight decisions
  omega

-- A namespace name, not an object ID, is the key: distinct hardlink names can
-- share an object. This models only the worker's effect on that namespace.
abbrev World := Path → Option Nat

def erase (world : World) (entry : Entry) : World :=
  fun path => if path = entry.path ∧ world path = some entry.objectId then none else world path

def applyRemovals : World → List Entry → World
  | world, [] => world
  | world, entry :: rest => applyRemovals (erase world entry) rest

theorem apply_preserves_unselected (world : World) (entries : List Entry) (path : Path)
    (h : ∀ entry ∈ entries, entry.path ≠ path) :
    applyRemovals world entries path = world path := by
  induction entries generalizing world with
  | nil => rfl
  | cons entry rest ih =>
    have hn : path ≠ entry.path := Ne.symm (h entry (by simp))
    have ht : ∀ other ∈ rest, other.path ≠ path := by
      intro other member
      exact h other (by simp [member])
    simpa [applyRemovals, erase, hn] using ih (erase world entry) ht

theorem run_preserves_unselected (world : World) (plan : Approved root)
    (preflight : Preflight) (decisions : List Decision) (path : Path)
    (h : ∀ entry ∈ plan.val, entry.path ≠ path) :
    applyRemovals world (run plan preflight decisions) path = world path := by
  apply apply_preserves_unselected
  intro entry member
  exact h entry (removed_was_approved plan preflight decisions entry member)

-- Mathematical specification of Rust's unsigned saturating_add. Bounds on
-- entry counts alone cannot bound arbitrary supplied byte estimates.
def u64Max : Nat := 18446744073709551615

def saturatingAdd (left right : Nat) : Nat := min u64Max (left + right)

theorem saturating_add_is_bounded (left right : Nat) :
    saturatingAdd left right ≤ u64Max := by
  exact Nat.min_le_left _ _

theorem saturating_add_does_not_wrap (left right : Nat) (h : left ≤ u64Max) :
    left ≤ saturatingAdd left right := by
  unfold saturatingAdd
  omega

theorem saturating_add_preserves_in_range (left right : Nat)
    (h : left + right ≤ u64Max) : saturatingAdd left right = left + right := by
  exact Nat.min_eq_right h

theorem saturating_add_clamps_overflow (left right : Nat)
    (h : u64Max ≤ left + right) : saturatingAdd left right = u64Max := by
  exact Nat.min_eq_left h

-- Shared fixtures use three sorted children and their parent in deletion order.
def fixtureEntry (index : Nat) : Entry :=
  ⟨if index = 3 then [1, 2] else [1, 2, index + 10], index, false⟩

def fixtureOrder : List Entry := [fixtureEntry 2, fixtureEntry 1, fixtureEntry 0, fixtureEntry 3]

def allPass : Preflight := ⟨true, true, true, true, true⟩

def fixturePlan : Approved [1] := ⟨fixtureOrder, by decide⟩

def fixtureTrace (cancelAfter : Nat) : List Nat :=
  (run fixturePlan allPass (List.replicate cancelAfter .remove ++ [.cancel])).map Entry.objectId

-- These concrete propositions also exercise the guard and failure branches in
-- the kernel. They complement the universal theorems above.
theorem sibling_component_rejected : ¬ below [1, 2] [1, 20, 3] := by decide
theorem protected_entry_rejected : approve [1] [⟨[1, 2], 7, true⟩] = none := by decide
theorem ordinary_entry_accepted : (approve [1] [fixtureEntry 0]).isSome = true := by decide
theorem cancellation_keeps_partial_result : fixtureTrace 1 = [2] := by decide
theorem complete_trace_has_no_extra_entries : fixtureTrace 10 = [2, 1, 0, 3] := by decide

-- The deliberately weaker empty-prefix rule admits an unrelated path.
def weakBelow (root path : Path) : Prop := path.take root.length = root
theorem empty_prefix_counterexample : weakBelow [] [99] ∧ ¬ below [] [99] := by simp [weakBelow, below]

-- Fresh recursive enumeration would enroll a late entry; the frozen executor
-- cannot. This is a pure counterexample, with no filesystem operations.
theorem dynamic_enrollment_counterexample :
    fixtureEntry 9 ∈ execute (fixtureOrder ++ [fixtureEntry 9])
      (List.replicate 5 .remove) ∧
    fixtureEntry 9 ∉ execute fixtureOrder (List.replicate 5 .remove) := by decide

end DiskTree
