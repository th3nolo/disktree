import RemovalModel

set_option warningAsError true

namespace DiskTree

theorem root_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.rootMatches = false) : run plan p ds = [] := by
  simp [run, Preflight.passes, h]

theorem identity_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.identitiesMatch = false) : run plan p ds = [] := by
  simp [run, Preflight.passes, h]

theorem membership_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.membershipMatches = false) : run plan p ds = [] := by
  simp [run, Preflight.passes, h]

theorem guard_refusal_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.guardsPass = false) : run plan p ds = [] := by
  simp [run, Preflight.passes, h]

theorem unpinned_handles_remove_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.handlesPinned = false) : run plan p ds = [] := by
  simp [run, Preflight.passes, h]

-- All 32 combinations, with the obligations stated independently of passes.
theorem preflight_exactly_five_checks (p : Preflight) :
    p.passes = true ↔ p.rootMatches = true ∧ p.identitiesMatch = true ∧
      p.membershipMatches = true ∧ p.guardsPass = true ∧ p.handlesPinned = true := by
  cases p with
  | mk a b c d e =>
    cases a <;> cases b <;> cases c <;> cases d <;> cases e <;> decide

theorem all_checks_pass : allPass.passes = true := by decide

theorem execute_all_success (entries : List Entry) (tail : List Decision) :
    execute entries (List.replicate entries.length .remove ++ tail) = entries := by
  induction entries with
  | nil => rfl
  | cons entry rest ih =>
    simpa [execute, List.replicate_succ] using congrArg (List.cons entry) ih

-- Covers nonempty plans too, without assuming that run already succeeds.
theorem run_complete (plan : Approved root) (p : Preflight) (tail : List Decision)
    (hr : p.rootMatches = true) (hi : p.identitiesMatch = true)
    (hm : p.membershipMatches = true) (hg : p.guardsPass = true)
    (hp : p.handlesPinned = true) :
    run plan p (List.replicate plan.val.length .remove ++ tail) = plan.val := by
  simp [run, Preflight.passes, hr, hi, hm, hg, hp, execute_all_success]

theorem approved_paths_unique (plan : Approved root) :
    plan.val.Pairwise (fun first later => first.path ≠ later.path) := by
  exact plan.property.2.2.2.imp (fun h => h.1)

theorem approved_children_before_parents (plan : Approved root) :
    plan.val.Pairwise (fun first later => ¬ below first.path later.path) := by
  exact plan.property.2.2.2.imp (fun h => h.2)

-- Assumption on the review order is explicit. This does not translate Rust's
-- DFS implementation; native nested fixtures exercise that correspondence.
theorem reverse_review_order (entries : List Entry)
    (h : entries.Pairwise (fun first later =>
      later.path ≠ first.path ∧ ¬ below later.path first.path)) :
    orderedDistinct entries.reverse := by
  exact List.pairwise_reverse.mpr h

theorem duplicate_path_rejected :
    approve [1] [fixtureEntry 0, fixtureEntry 0] = none := by decide

theorem parent_before_child_rejected :
    approve [1] [fixtureEntry 3, fixtureEntry 0] = none := by decide

-- Concrete identity observations, not a caller-supplied boolean. The actual
-- Win32 correspondence must establish this relation for the opened handles.
def identitiesAgree (world : World) (entries : List Entry) : Prop :=
  ∀ entry ∈ entries, world entry.path = some entry.objectId

instance (world : World) (entries : List Entry) : Decidable (identitiesAgree world entries) :=
  inferInstanceAs (Decidable (∀ entry ∈ entries, world entry.path = some entry.objectId))

def runObserved (plan : Approved root) (world : World) (p : Preflight)
    (ds : List Decision) : List Entry :=
  if identitiesAgree world plan.val then run plan p ds else []

theorem observed_identity_mismatch_refuses (plan : Approved root) (world : World)
    (p : Preflight) (ds : List Decision) (entry : Entry) (member : entry ∈ plan.val)
    (mismatch : world entry.path ≠ some entry.objectId) :
    runObserved plan world p ds = [] := by
  have h : ¬ identitiesAgree world plan.val := fun agreement => mismatch (agreement entry member)
  simp [runObserved, h]

theorem observed_removal_has_approved_identity (plan : Approved root) (world : World)
    (p : Preflight) (ds : List Decision) (entry : Entry)
    (member : entry ∈ runObserved plan world p ds) :
    world entry.path = some entry.objectId := by
  by_cases h : identitiesAgree world plan.val
  · simp only [runObserved, ite_eq_left h] at member
    exact h entry (removed_was_approved plan p ds entry member)
  · simp [runObserved, h] at member

-- Pinning is a relation on observations, not a theorem about Windows sharing.
def Pinned (observed current : World) (entries : List Entry) : Prop :=
  ∀ entry ∈ entries, current entry.path = observed entry.path

theorem pinned_identity_agrees (observed current : World) (entries : List Entry)
    (agreement : identitiesAgree observed entries) (pinned : Pinned observed current entries) :
    identitiesAgree current entries := by
  intro entry member
  rw [pinned entry member]
  exact agreement entry member

theorem erase_requires_approved_identity (world : World) (entry : Entry) (path : Path)
    (changed : erase world entry path ≠ world path) :
    path = entry.path ∧ world path = some entry.objectId := by
  unfold erase at changed
  split at changed
  · assumption
  · contradiction

theorem wrong_identity_is_preserved (world : World) (entry : Entry)
    (mismatch : world entry.path ≠ some entry.objectId) : erase world entry = world := by
  funext path
  by_cases h : path = entry.path
  · subst path
    simp [erase, mismatch]
  · simp [erase, h]

theorem apply_preserves_absence (world : World) (entries : List Entry) (path : Path)
    (absent : world path = none) : applyRemovals world entries path = none := by
  induction entries generalizing world with
  | nil => exact absent
  | cons entry rest ih =>
    apply ih (erase world entry)
    simp [erase, absent]

theorem apply_removes_matching_selected (world : World) (entries : List Entry) (entry : Entry)
    (member : entry ∈ entries) (identity : world entry.path = some entry.objectId) :
    applyRemovals world entries entry.path = none := by
  induction entries generalizing world with
  | nil => simp at member
  | cons first rest ih =>
    change applyRemovals (erase world first) rest entry.path = none
    rcases List.mem_cons.mp member with head | tail
    · subst first
      apply apply_preserves_absence (erase world entry) rest entry.path
      simp [erase, identity]
    · by_cases changed : entry.path = first.path ∧ world entry.path = some first.objectId
      · apply apply_preserves_absence (erase world first) rest entry.path
        simp [erase, changed]
      · apply ih (erase world first) tail
        simpa [erase, changed] using identity

theorem observed_run_complete (plan : Approved root) (world : World) (tail : List Decision)
    (agreement : identitiesAgree world plan.val) :
    runObserved plan world allPass (List.replicate plan.val.length .remove ++ tail) = plan.val := by
  rw [runObserved, ite_eq_left agreement]
  exact run_complete plan allPass tail rfl rfl rfl rfl rfl

theorem complete_run_removes_selected (plan : Approved root) (world : World)
    (tail : List Decision) (agreement : identitiesAgree world plan.val)
    (entry : Entry) (member : entry ∈ plan.val) :
    applyRemovals world
      (runObserved plan world allPass (List.replicate plan.val.length .remove ++ tail))
      entry.path = none := by
  rw [observed_run_complete plan world tail agreement]
  exact apply_removes_matching_selected world plan.val entry member (agreement entry member)

-- Even if the external boolean incorrectly says true, the observed state
-- rejects object 99 at a name approved for object 7.
def replacementPlan : Approved [1] := ⟨[⟨[1, 2], 7, false⟩], by decide⟩
def replacedWorld : World := fun _ => some 99

theorem replaced_object_refused :
    runObserved replacementPlan replacedWorld allPass [.remove] = [] := by decide

theorem replaced_object_survives :
    applyRemovals replacedWorld replacementPlan.val [1, 2] = some 99 := by decide

end DiskTree
