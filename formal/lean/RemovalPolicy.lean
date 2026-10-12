import RemovalModel

set_option warningAsError true

namespace DiskTree.RemovalPolicy

inductive Step where
  | cancellation
  | cloud
  | delete
  deriving DecidableEq, Repr

def next : Step → Bool → Option Step
  | .cancellation, true => some .cloud
  | .cloud, true => some .delete
  | _, _ => none

def refusal : Step → DiskTree.Decision
  | .cancellation => .cancel
  | .cloud => .cloudRefusal
  | .delete => .failure

-- Booleans summarize external observations, sampled in this order. Fuel is
-- fixed to three below; it bounds the interpreter without a partial definition.
def evaluate : Nat → Step → (Step → Bool) → DiskTree.Decision
  | 0, _, _ => .failure
  | fuel + 1, step, observed =>
      match next step (observed step) with
      | some following => evaluate fuel following observed
      | none => if observed step then .remove else refusal step

def decision (cancelPass cloudPass deletePass : Bool) : DiskTree.Decision :=
  evaluate 3 .cancellation fun step => match step with
    | .cancellation => cancelPass
    | .cloud => cloudPass
    | .delete => deletePass

theorem next_failure_stops (step : Step) : next step false = none := by
  cases step <;> decide

theorem deletion_is_terminal (passed : Bool) : next .delete passed = none := by
  cases passed <;> rfl

theorem decision_rule (c g d : Bool) :
    decision c g d =
      if c then (if g then (if d then .remove else .failure) else .cloudRefusal)
      else .cancel := by
  cases c <;> cases g <;> cases d <;> decide

theorem success_iff_all_pass (c g d : Bool) :
    decision c g d = .remove ↔ c = true ∧ g = true ∧ d = true := by
  cases c <;> cases g <;> cases d <;> decide

theorem cancellation_stops (g d : Bool) : decision false g d = .cancel := rfl

theorem cloud_refusal_stops (d : Bool) :
    decision true false d = .cloudRefusal := rfl

theorem deletion_failure_stops : decision true true false = .failure := rfl

abbrev Observations := Bool × Bool × Bool

def classify (observed : Observations) : DiskTree.Decision :=
  decision observed.1 observed.2.1 observed.2.2

def execute : List DiskTree.Entry → List Observations → List DiskTree.Entry
  | [], _ => []
  | _, [] => []
  | entry :: rest, observed :: remaining =>
      match classify observed with
      | .remove => entry :: execute rest remaining
      | _ => []

theorem execute_matches_model (entries : List DiskTree.Entry)
    (observations : List Observations) :
    execute entries observations = DiskTree.execute entries (observations.map classify) := by
  induction entries generalizing observations with
  | nil => rfl
  | cons entry rest ih =>
      cases observations with
      | nil => rfl
      | cons observed remaining =>
          simp only [execute, List.map_cons]
          cases classify observed <;> simp [DiskTree.execute, ih]

theorem execute_prefix (entries : List DiskTree.Entry)
    (observations : List Observations) :
    ∃ remaining, entries = execute entries observations ++ remaining := by
  rw [execute_matches_model]
  exact DiskTree.execute_prefix _ _

def name : Option Step → String
  | some .cancellation => "cancel"
  | some .cloud => "cloud"
  | some .delete => "delete"
  | none => "stop"

def check : IO Unit := do
  let mut rows : List String := ["phase|passed|next"]
  for step in [Step.cancellation, Step.cloud, Step.delete] do
    for passed in [false, true] do
      rows := rows ++ [s!"{name (some step)}|{if passed then 1 else 0}|{name (next step passed)}"]
  let actual := String.intercalate "\n" rows ++ "\n"
  unless actual == (← IO.FS.readFile "policy-cases.txt") do
    throw (IO.userError "fixed continuation contract changed")
  IO.FS.writeFile "policy-lean-results.txt" actual
  IO.println "all 6 removal policy transitions checked"

end DiskTree.RemovalPolicy
