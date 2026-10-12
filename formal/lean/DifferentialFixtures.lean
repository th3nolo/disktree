import SafetyProperties

set_option warningAsError true

namespace DifferentialFixtures

structure Tree where
  seed : Nat
  entries : Array DiskTree.Entry
  directories : Array Bool
  order : List Nat

def number (text : String) : IO Nat :=
  match text.toNat? with
  | some value => pure value
  | none => throw (IO.userError s!"invalid fixture number: {text}")

def numbers (text : String) : IO (List Nat) :=
  if text.isEmpty then pure [] else (text.splitOn ",").mapM number

def csv (values : List Nat) : String :=
  String.intercalate "," (values.map toString)

def parseTree (seedText nodesText orderText : String) : IO Tree := do
  let seed ← number seedText
  let mut entries : Array DiskTree.Entry := #[]
  let mut directories : Array Bool := #[]
  for node in nodesText.splitOn ";" do
    let [parentText, kind] := node.splitOn ","
      | throw (IO.userError "invalid tree node")
    unless kind == "D" || kind == "F" do
      throw (IO.userError "invalid tree kind")
    let id := entries.size
    let path ← if id == 0 then do
      unless parentText == "-" && kind == "D" do
        throw (IO.userError "directory root required")
      pure [1, 2]
    else do
      let parent ← number parentText
      let some entry := entries[parent]?
        | throw (IO.userError "parent must precede child")
      unless directories[parent]?.getD false do
        throw (IO.userError "parent must be a directory")
      pure (entry.path ++ [id + 2])
    entries := entries.push ⟨path, id, false⟩
    directories := directories.push (kind == "D")
  unless entries.size > 0 && entries.size ≤ 32 do
    throw (IO.userError "bounded nonempty tree required")
  let order ← numbers orderText
  unless order.length == entries.size &&
      (List.range entries.size).all order.contains do
    throw (IO.userError "order must contain every node exactly once")
  pure ⟨seed, entries, directories, order⟩

def canaries : List DiskTree.Entry :=
  [⟨[1, 1000000], 1000000, false⟩,
   ⟨[1000001], 1000001, false⟩,
   ⟨[1000002], 1000002, false⟩]

def worldOf (entries : List DiskTree.Entry) : DiskTree.World :=
  fun path => (entries.find? (fun entry => entry.path == path)).map DiskTree.Entry.objectId

def checkCase (trees : List Tree) (fields : List String) : IO String := do
  let ["C", identifier, seedText, mode, stopText, expectedRemoved,
      expectedRemaining, lateText] := fields
    | throw (IO.userError "invalid differential case")
  let seed ← number seedText
  let some tree := trees.find? (fun tree => tree.seed == seed)
    | throw (IO.userError "unknown fixture tree")
  let stop ← number stopText
  let late ← if lateText == "-" then pure none else do
    let parent ← number lateText
    pure (some parent)
  let decision ← match mode with
    | "complete" => do
      unless stop == tree.entries.size && late.isNone do
        throw (IO.userError "invalid completion boundary")
      pure DiskTree.Decision.remove
    | "cancel" => do
      unless stop < tree.entries.size && late.isNone do
        throw (IO.userError "invalid cancellation boundary")
      pure DiskTree.Decision.cancel
    | "failure" => do
      let some parent := late | throw (IO.userError "failure needs a late child")
      unless stop > 0 && stop < tree.entries.size &&
          tree.order[stop]? == some parent && tree.directories[parent]?.getD false do
        throw (IO.userError "failure must precede the next reviewed directory")
      pure DiskTree.Decision.failure
    | _ => throw (IO.userError "unknown schedule")
  let ordered ← tree.order.mapM fun id =>
    match tree.entries[id]? with
    | some entry => pure entry
    | none => throw (IO.userError "invalid ordered node")
  let some plan := DiskTree.approve [1] ordered
    | throw (IO.userError "generated plan failed real runtime approval")
  let initial := worldOf (tree.entries.toList ++ canaries)
  let schedule := List.replicate stop DiskTree.Decision.remove ++ [decision]
  let removed := DiskTree.runObserved plan initial DiskTree.allPass schedule
  unless removed == DiskTree.run plan DiskTree.allPass schedule do
    throw (IO.userError "observed and ordinary model routes differ")
  let after := DiskTree.applyRemovals initial removed
  -- The native callback introduces this unapproved child after the successful
  -- prefix, immediately before the directory's failing system call.
  let latePath ← match late with
    | none => pure none
    | some parent =>
      match tree.entries[parent]? with
      | some entry => pure (some (entry.path ++ [1000003]))
      | none => throw (IO.userError "unknown late parent")
  let final : DiskTree.World := fun path =>
    if latePath == some path then some 1000003 else after path
  let remaining := tree.entries.toList.filterMap fun entry =>
    if final entry.path == some entry.objectId then some entry.objectId else none
  let preserved := canaries.filterMap fun entry =>
    if final entry.path == some entry.objectId then some entry.objectId else none
  let actualRemoved := csv (removed.map DiskTree.Entry.objectId)
  unless actualRemoved == expectedRemoved && csv remaining == expectedRemaining &&
      preserved == [1000000, 1000001, 1000002] do
    throw (IO.userError s!"independent generator/model effects differ: {identifier}")
  let latePresent := match latePath with
    | none => false
    | some path => final path == some 1000003
  pure s!"{identifier}|{actualRemoved}|{csv remaining}|{if latePresent then 1 else 0}|{csv preserved}"

def check : IO Unit := do
  let input ← IO.FS.readFile "differential-cases.txt"
  let mut trees : List Tree := []
  let mut identifiers : List String := []
  let mut rows : List String := []
  for raw in input.splitOn "\n" do
    let line := raw.trimAscii.toString
    if line.isEmpty || line.startsWith "#" then continue
    let fields := line.splitOn "|"
    match fields with
    | ["T", seed, nodes, order] =>
      let tree ← parseTree seed nodes order
      if trees.any (fun other => other.seed == tree.seed) then
        throw (IO.userError "duplicate tree seed")
      trees := trees ++ [tree]
    | "C" :: identifier :: _ =>
      if identifiers.contains identifier then
        throw (IO.userError "duplicate scenario identifier")
      identifiers := identifier :: identifiers
      rows := rows ++ [← checkCase trees fields]
    | _ => throw (IO.userError "unknown differential record")
  unless trees.length == 16 && rows.length == 322 do
    throw (IO.userError "complete fixed corpus required: 16 trees, 322 scenarios")
  IO.FS.writeFile "differential-lean-results.txt"
    ("case|removed|remaining|late|canaries\n" ++ String.intercalate "\n" rows ++ "\n")
  IO.println "16 generated trees and 322 differential model effect scenarios passed"

end DifferentialFixtures
