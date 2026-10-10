import NestedFixtures

set_option warningAsError true

def main : IO Unit := do
  let input ← IO.FS.readFile "cancellation-cases.txt"
  let mut count := 0
  for line in input.splitOn "\n" do
    let line := line.trimAscii.toString
    if line.isEmpty then continue
    let [before, expected] := line.splitOn ":"
      | throw (IO.userError s!"malformed fixture: {line}")
    let some cancelAfter := before.toNat?
      | throw (IO.userError s!"bad cancellation index: {line}")
    let actual := String.intercalate "," ((DiskTree.fixtureTrace cancelAfter).map toString)
    unless actual == expected do
      throw (IO.userError s!"fixture {cancelAfter}: expected {expected}; got {actual}")
    count := count + 1
  unless count == 6 do
    throw (IO.userError s!"expected six flat fixtures; got {count}")
  let nested ← IO.FS.readFile "nested-cases.txt"
  let mut nestedCount := 0
  for line in nested.splitOn "\n" do
    let line := line.trimAscii.toString
    if line.isEmpty then continue
    let [kind, before, expected] := line.splitOn ":"
      | throw (IO.userError s!"malformed nested fixture: {line}")
    let some stopAfter := before.toNat?
      | throw (IO.userError s!"bad stop index: {line}")
    let stop ← match kind with
      | "cancel" => pure DiskTree.Decision.cancel
      | "failure" => pure DiskTree.Decision.failure
      | _ => throw (IO.userError s!"bad stop kind: {line}")
    let actual := String.intercalate "," ((DiskTree.nestedTrace stopAfter stop).map toString)
    unless actual == expected do
      throw (IO.userError s!"nested {line}: got {actual}")
    nestedCount := nestedCount + 1
  unless nestedCount == 13 do
    throw (IO.userError s!"expected thirteen nested fixtures; got {nestedCount}")
  IO.println s!"{count} flat and {nestedCount} nested shared fixtures passed through run"
