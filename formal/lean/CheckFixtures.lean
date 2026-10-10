import RemovalModel

set_option warningAsError true

def main : IO Unit := do
  let input ← IO.FS.readFile "cancellation-cases.txt"
  let mut count := 0
  for line in input.splitOn "\n" do
    let line := line.trimAscii.toString
    if line.isEmpty then continue
    let parts := line.splitOn ":"
    let [before, expected] := parts
      | throw (IO.userError s!"malformed fixture: {line}")
    let some cancelAfter := before.toNat?
      | throw (IO.userError s!"bad cancellation index: {line}")
    let actual := String.intercalate "," ((DiskTree.fixtureTrace cancelAfter).map toString)
    unless actual == expected do
      throw (IO.userError s!"fixture {cancelAfter}: expected {expected}; got {actual}")
    count := count + 1
  unless count == 6 do
    throw (IO.userError s!"expected six shared fixtures; got {count}")
  IO.println s!"{count} shared cancellation fixtures passed"
