import Lean
import Lean.Compiler.Old

-- Trusted audit infrastructure, not part of the removal specification. Inspect
-- the checked current-module environment, including private/generated proofs.
-- Python appends this command; no per-theorem reports are maintained by hand.
open Lean Elab Command

elab "#audit_module" : command => do
  let env := (← getEnv).setExporting false
  let declarations := env.checked.get.constants.foldStage2
    (fun acc name info => acc.push (name, info)) #[]
  let mut rows : Array Json := #[]
  for (name, info) in declarations do
    let axioms ← collectAxioms name
    let ranges ← findDeclarationRanges? name
    let compilerAuxiliary := match Compiler.isUnsafeRecName? name with
      | some parent => match env.checked.get.find? parent with
        | some (.defnInfo value) =>
          info.isPartial && !ranges.isSome &&
            (value.safety == DefinitionSafety.safe) && (value.type == info.type)
        | _ => false
      | none => false
    let kind := match info with
      | .thmInfo _ => "theorem"
      | .axiomInfo _ => "axiom"
      | .defnInfo _ => "definition"
      | .opaqueInfo _ => "opaque"
      | .inductInfo _ => "inductive"
      | .ctorInfo _ => "constructor"
      | .recInfo _ => "recursor"
      | .quotInfo _ => "quotient"
    rows := rows.push <| Json.mkObj [
      ("name", toJson name.toString),
      ("user_name", toJson (privateToUserName name).toString),
      ("kind", toJson kind),
      ("type", toJson (reprStr info.type)),
      ("value", toJson (info.value? (allowOpaque := true) |>.map reprStr)),
      ("unsafe", toJson info.isUnsafe),
      ("partial", toJson info.isPartial),
      ("compiler_auxiliary", toJson compilerAuxiliary),
      ("implemented_by", toJson (Compiler.getImplementedBy? env name |>.map Name.toString)),
      ("axioms", toJson (axioms.map Name.toString))]
  liftIO <| IO.println ("DISKTREE_AUDIT " ++ (Json.arr rows).compress)
