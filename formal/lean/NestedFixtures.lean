import RemovalModel

set_option warningAsError true

namespace DiskTree

-- Two branches and a deeper directory. IDs match the native fixture mapping.
def nestedEntry (index : Nat) : Entry :=
  ⟨match index with
    | 0 => [1, 2, 10, 0]
    | 1 => [1, 2, 10, 11, 1]
    | 2 => [1, 2, 10, 11]
    | 3 => [1, 2, 10]
    | 4 => [1, 2, 20, 2]
    | 5 => [1, 2, 20]
    | _ => [1, 2], index, false⟩

def nestedOrder : List Entry :=
  [nestedEntry 4, nestedEntry 5, nestedEntry 1, nestedEntry 2,
   nestedEntry 0, nestedEntry 3, nestedEntry 6]

def nestedPlan : Approved [1] := ⟨nestedOrder, by decide⟩

def nestedTrace (stopAfter : Nat) (stop : Decision) : List Nat :=
  (run nestedPlan allPass (List.replicate stopAfter .remove ++ [stop])).map Entry.objectId

theorem nested_plan_is_accepted : (approve [1] nestedOrder).isSome = true := by decide
theorem nested_complete_trace : nestedTrace 10 .cancel = [4, 5, 1, 2, 0, 3, 6] := by decide
theorem nested_partial_failure : nestedTrace 3 .failure = [4, 5, 1] := by decide

end DiskTree
