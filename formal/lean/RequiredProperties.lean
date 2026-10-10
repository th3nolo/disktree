import SafetyProperties

set_option warningAsError true

-- Independent, fixed statement obligations. Do not regenerate this file in CI.
-- Changes here are specification changes and require explicit review.
namespace DiskTree.Required

theorem contract_approve_rejects_invalid (root : Path) (entries : List Entry)
    (h : ¬ valid root entries) : approve root entries = none := by
  apply DiskTree.approve_rejects_invalid <;> assumption

theorem contract_empty_root_rejected (entries : List Entry) : approve [] entries = none := by
  apply DiskTree.empty_root_rejected <;> assumption

theorem contract_root_is_not_below_itself (root : Path) : ¬ below root root := by
  apply DiskTree.root_is_not_below_itself <;> assumption

theorem contract_execute_prefix (entries : List Entry) (decisions : List Decision) :
    ∃ remaining, entries = execute entries decisions ++ remaining := by
  apply DiskTree.execute_prefix <;> assumption

theorem contract_run_prefix (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) :
    ∃ remaining, plan.val = run plan preflight decisions ++ remaining := by
  apply DiskTree.run_prefix <;> assumption

theorem contract_removed_was_approved (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry)
    (h : entry ∈ run plan preflight decisions) : entry ∈ plan.val := by
  apply DiskTree.removed_was_approved <;> assumption

theorem contract_removed_stays_in_scope (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry)
    (h : entry ∈ run plan preflight decisions) :
    below root entry.path ∧ entry.guardedOut = false := by
  apply DiskTree.removed_stays_in_scope <;> assumption

theorem contract_unapproved_entry_never_removed (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (entry : Entry) (h : entry ∉ plan.val) :
    entry ∉ run plan preflight decisions := by
  apply DiskTree.unapproved_entry_never_removed <;> assumption

theorem contract_preflight_failure_removes_nothing (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) (h : preflight.passes = false) :
    run plan preflight decisions = [] := by
  apply DiskTree.preflight_failure_removes_nothing <;> assumption

theorem contract_cancelled_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.cancel :: decisions) = [] := by
  apply DiskTree.cancelled_before_next <;> assumption

theorem contract_failure_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.failure :: decisions) = [] := by
  apply DiskTree.failure_before_next <;> assumption

theorem contract_cloud_refusal_before_next (entries : List Entry) (decisions : List Decision) :
    execute entries (.cloudRefusal :: decisions) = [] := by
  apply DiskTree.cloud_refusal_before_next <;> assumption

theorem contract_removed_count_bounded (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) : (run plan preflight decisions).length ≤ 20000 := by
  apply DiskTree.removed_count_bounded <;> assumption

theorem contract_removed_count_fits_u64 (plan : Approved root) (preflight : Preflight)
    (decisions : List Decision) :
    (run plan preflight decisions).length < 18446744073709551616 := by
  apply DiskTree.removed_count_fits_u64 <;> assumption

theorem contract_apply_preserves_unselected (world : World) (entries : List Entry) (path : Path)
    (h : ∀ entry ∈ entries, entry.path ≠ path) :
    applyRemovals world entries path = world path := by
  apply DiskTree.apply_preserves_unselected <;> assumption

theorem contract_run_preserves_unselected (world : World) (plan : Approved root)
    (preflight : Preflight) (decisions : List Decision) (path : Path)
    (h : ∀ entry ∈ plan.val, entry.path ≠ path) :
    applyRemovals world (run plan preflight decisions) path = world path := by
  apply DiskTree.run_preserves_unselected <;> assumption

theorem contract_saturating_add_is_bounded (left right : Nat) :
    saturatingAdd left right ≤ u64Max := by
  apply DiskTree.saturating_add_is_bounded <;> assumption

theorem contract_saturating_add_does_not_wrap (left right : Nat) (h : left ≤ u64Max) :
    left ≤ saturatingAdd left right := by
  apply DiskTree.saturating_add_does_not_wrap <;> assumption

theorem contract_saturating_add_preserves_in_range (left right : Nat)
    (h : left + right ≤ u64Max) : saturatingAdd left right = left + right := by
  apply DiskTree.saturating_add_preserves_in_range <;> assumption

theorem contract_saturating_add_clamps_overflow (left right : Nat)
    (h : u64Max ≤ left + right) : saturatingAdd left right = u64Max := by
  apply DiskTree.saturating_add_clamps_overflow <;> assumption

theorem contract_sibling_component_rejected : ¬ below [1, 2] [1, 20, 3] := by
  apply DiskTree.sibling_component_rejected <;> assumption

theorem contract_protected_entry_rejected : approve [1] [⟨[1, 2], 7, true⟩] = none := by
  apply DiskTree.protected_entry_rejected <;> assumption

theorem contract_ordinary_entry_accepted : (approve [1] [fixtureEntry 0]).isSome = true := by
  apply DiskTree.ordinary_entry_accepted <;> assumption

theorem contract_cancellation_keeps_partial_result : fixtureTrace 1 = [2] := by
  apply DiskTree.cancellation_keeps_partial_result <;> assumption

theorem contract_complete_trace_has_no_extra_entries : fixtureTrace 10 = [2, 1, 0, 3] := by
  apply DiskTree.complete_trace_has_no_extra_entries <;> assumption

theorem contract_empty_prefix_counterexample : weakBelow [] [99] ∧ ¬ below [] [99] := by
  apply DiskTree.empty_prefix_counterexample <;> assumption

theorem contract_dynamic_enrollment_counterexample :
    fixtureEntry 9 ∈ execute (fixtureOrder ++ [fixtureEntry 9])
      (List.replicate 5 .remove) ∧
    fixtureEntry 9 ∉ execute fixtureOrder (List.replicate 5 .remove) := by
  apply DiskTree.dynamic_enrollment_counterexample <;> assumption

theorem contract_root_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.rootMatches = false) : run plan p ds = [] := by
  apply DiskTree.root_mismatch_removes_nothing <;> assumption

theorem contract_identity_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.identitiesMatch = false) : run plan p ds = [] := by
  apply DiskTree.identity_mismatch_removes_nothing <;> assumption

theorem contract_membership_mismatch_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.membershipMatches = false) : run plan p ds = [] := by
  apply DiskTree.membership_mismatch_removes_nothing <;> assumption

theorem contract_guard_refusal_removes_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.guardsPass = false) : run plan p ds = [] := by
  apply DiskTree.guard_refusal_removes_nothing <;> assumption

theorem contract_unpinned_handles_remove_nothing (plan : Approved root) (p : Preflight)
    (ds : List Decision) (h : p.handlesPinned = false) : run plan p ds = [] := by
  apply DiskTree.unpinned_handles_remove_nothing <;> assumption

theorem contract_preflight_exactly_five_checks (p : Preflight) :
    p.passes = true ↔ p.rootMatches = true ∧ p.identitiesMatch = true ∧
      p.membershipMatches = true ∧ p.guardsPass = true ∧ p.handlesPinned = true := by
  apply DiskTree.preflight_exactly_five_checks <;> assumption

theorem contract_all_checks_pass : allPass.passes = true := by
  apply DiskTree.all_checks_pass <;> assumption

theorem contract_execute_all_success (entries : List Entry) (tail : List Decision) :
    execute entries (List.replicate entries.length .remove ++ tail) = entries := by
  apply DiskTree.execute_all_success <;> assumption

theorem contract_run_complete (plan : Approved root) (p : Preflight) (tail : List Decision)
    (hr : p.rootMatches = true) (hi : p.identitiesMatch = true)
    (hm : p.membershipMatches = true) (hg : p.guardsPass = true)
    (hp : p.handlesPinned = true) :
    run plan p (List.replicate plan.val.length .remove ++ tail) = plan.val := by
  apply DiskTree.run_complete <;> assumption

theorem contract_approved_paths_unique (plan : Approved root) :
    plan.val.Pairwise (fun first later => first.path ≠ later.path) := by
  apply DiskTree.approved_paths_unique <;> assumption

theorem contract_approved_children_before_parents (plan : Approved root) :
    plan.val.Pairwise (fun first later => ¬ below first.path later.path) := by
  apply DiskTree.approved_children_before_parents <;> assumption

theorem contract_reverse_review_order (entries : List Entry)
    (h : entries.Pairwise (fun first later =>
      later.path ≠ first.path ∧ ¬ below later.path first.path)) :
    orderedDistinct entries.reverse := by
  apply DiskTree.reverse_review_order <;> assumption

theorem contract_duplicate_path_rejected :
    approve [1] [fixtureEntry 0, fixtureEntry 0] = none := by
  apply DiskTree.duplicate_path_rejected <;> assumption

theorem contract_parent_before_child_rejected :
    approve [1] [fixtureEntry 3, fixtureEntry 0] = none := by
  apply DiskTree.parent_before_child_rejected <;> assumption

theorem contract_observed_identity_mismatch_refuses (plan : Approved root) (world : World)
    (p : Preflight) (ds : List Decision) (entry : Entry) (member : entry ∈ plan.val)
    (mismatch : world entry.path ≠ some entry.objectId) :
    runObserved plan world p ds = [] := by
  apply DiskTree.observed_identity_mismatch_refuses <;> assumption

theorem contract_observed_removal_has_approved_identity (plan : Approved root) (world : World)
    (p : Preflight) (ds : List Decision) (entry : Entry)
    (member : entry ∈ runObserved plan world p ds) :
    world entry.path = some entry.objectId := by
  apply DiskTree.observed_removal_has_approved_identity <;> assumption

theorem contract_pinned_identity_agrees (observed current : World) (entries : List Entry)
    (agreement : identitiesAgree observed entries) (pinned : Pinned observed current entries) :
    identitiesAgree current entries := by
  apply DiskTree.pinned_identity_agrees <;> assumption

theorem contract_erase_requires_approved_identity (world : World) (entry : Entry) (path : Path)
    (changed : erase world entry path ≠ world path) :
    path = entry.path ∧ world path = some entry.objectId := by
  apply DiskTree.erase_requires_approved_identity <;> assumption

theorem contract_wrong_identity_is_preserved (world : World) (entry : Entry)
    (mismatch : world entry.path ≠ some entry.objectId) : erase world entry = world := by
  apply DiskTree.wrong_identity_is_preserved <;> assumption

theorem contract_apply_preserves_absence (world : World) (entries : List Entry) (path : Path)
    (absent : world path = none) : applyRemovals world entries path = none := by
  apply DiskTree.apply_preserves_absence <;> assumption

theorem contract_apply_removes_matching_selected (world : World) (entries : List Entry) (entry : Entry)
    (member : entry ∈ entries) (identity : world entry.path = some entry.objectId) :
    applyRemovals world entries entry.path = none := by
  apply DiskTree.apply_removes_matching_selected <;> assumption

theorem contract_observed_run_complete (plan : Approved root) (world : World) (tail : List Decision)
    (agreement : identitiesAgree world plan.val) :
    runObserved plan world allPass (List.replicate plan.val.length .remove ++ tail) = plan.val := by
  apply DiskTree.observed_run_complete <;> assumption

theorem contract_complete_run_removes_selected (plan : Approved root) (world : World)
    (tail : List Decision) (agreement : identitiesAgree world plan.val)
    (entry : Entry) (member : entry ∈ plan.val) :
    applyRemovals world
      (runObserved plan world allPass (List.replicate plan.val.length .remove ++ tail))
      entry.path = none := by
  apply DiskTree.complete_run_removes_selected <;> assumption

theorem contract_replaced_object_refused :
    runObserved replacementPlan replacedWorld allPass [.remove] = [] := by
  apply DiskTree.replaced_object_refused <;> assumption

theorem contract_replaced_object_survives :
    applyRemovals replacedWorld replacementPlan.val [1, 2] = some 99 := by
  apply DiskTree.replaced_object_survives <;> assumption

end DiskTree.Required
