"""Require weakened models to fail the pinned Lean/specification gate."""

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).parent.resolve()

APPROVAL_BODY = "if h : valid root entries then some ⟨entries, h⟩ else none"
SELECTIVE_APPROVAL_BODY = "if root = [1] then\n    " + APPROVAL_BODY + "\n  else none"

# Fixed witnesses for each controlled source mutation. These are independent
# of the changed definition; a file-level error is not a property failure.
EXPECTED_FAILURES = {
    "drop_rootMatches": ("root_mismatch_removes_nothing", "unsolved"),
    "drop_identitiesMatch": ("identity_mismatch_removes_nothing", "unsolved"),
    "drop_membershipMatches": ("membership_mismatch_removes_nothing", "unsolved"),
    "drop_guardsPass": ("guard_refusal_removes_nothing", "unsolved"),
    "drop_handlesPinned": ("unpinned_handles_remove_nothing", "unsolved"),
    "broaden_path_scope": ("sibling_component_rejected", "false_proposition"),
    "ignore_protected_flag": ("protected_entry_rejected", "false_proposition"),
    "increase_entry_limit": ("removed_count_bounded", "arithmetic"),
    "change_byte_ceiling": ("contract_u64_maximum", "definition"),
    "empty_run": ("complete_trace_has_no_extra_entries", "false_proposition"),
    "ignore_current_identity": ("replaced_object_survives", "false_proposition"),
    "ignore_observed_identity": ("replaced_object_refused", "false_proposition"),
    "allow_duplicate_paths": ("duplicate_path_rejected", "false_proposition"),
    "allow_parent_first": ("parent_before_child_rejected", "false_proposition"),
    "delete_required_theorem": ("contract_replaced_object_survives", "missing_requirement"),
    "weaken_required_statement": ("contract_replaced_object_survives", "statement"),
    "approve_only_fixture_root": ("approve_accepts_iff_valid", "unsolved"),
}

DIAGNOSTIC = re.compile(
    r"^(?P<lake>error: )?(?P<file>.+?\.lean):(?P<line>\d+):(?P<column>\d+): "
    r"(?:(?P<severity>error|warning|info)(?:\((?P<code>[^)\n]*)\))?: )?"
    r"(?P<message>[^\n]*)", re.MULTILINE,
)


def theorem_span(source: Path, theorem: str) -> tuple[int, int]:
    # These fixed test witnesses use indented continuation/proof lines.
    # This is not the proof inventory: audit.py uses Lean's environment for it.
    # Changed/missing anchors stop the driver instead of broadening coverage.
    lines = source.read_text(encoding="utf-8").splitlines()
    anchors = [index for index, line in enumerate(lines)
               if re.match(r"^theorem " + re.escape(theorem) + r"(?:\s|:)", line)]
    if len(anchors) != 1:
        raise RuntimeError(f"expected theorem anchor changed: {source.name}: {theorem}")
    start = anchors[0]
    end = start
    while end + 1 < len(lines) and lines[end + 1].strip():
        following = lines[end + 1]
        # A missing blank line must not extend a witness into another command,
        # including an indented declaration inside the surrounding namespace.
        if re.match(r"^\s*(?:@\[|(?:private |protected )?(?:theorem|lemma|def|abbrev|"
                    r"instance|structure|inductive|opaque|axiom|namespace|section|end|"
                    r"import|open|attribute|set_option)\b)", following):
            break
        if following == following.lstrip():
            raise RuntimeError(f"expected theorem continuation layout changed: {source.name}: {theorem}")
        end += 1
    return start + 1, end + 1


def error_diagnostics(output: str) -> list[dict]:
    matches = list(DIAGNOSTIC.finditer(output))
    diagnostics = []
    for index, match in enumerate(matches):
        severity = match["severity"] or ("error" if match["lake"] else None)
        if severity != "error":
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(output)
        message = output[match.start("message"):end].strip()
        diagnostics.append(dict(file=match["file"], line=int(match["line"]),
                                column=int(match["column"]), code=match["code"], message=message))
    return diagnostics


def property_failure(message: str, category: str) -> bool:
    if category == "unsolved":
        return message.startswith("unsolved goals\n")
    if category == "false_proposition":
        return (message.startswith("Tactic `decide` proved that the proposition\n")
                and re.search(r"^is false$", message, re.MULTILINE) is not None)
    if category == "arithmetic":
        return message.startswith("omega could not prove the goal:")
    if category == "definition":
        return message.startswith("Not a definitional equality:")
    if category == "missing_requirement":
        return message.splitlines()[0] == "Unknown identifier `DiskTree.replaced_object_survives`"
    if category == "statement":
        return message.startswith(
            "Tactic `apply` failed: could not unify the type of `replaced_object_survives`")
    raise RuntimeError(f"unknown expected failure category: {category}")


def classify_failure(result, root: Path, expected: str, theorem: str, category: str) -> dict:
    output = result.stdout + result.stderr
    if result.returncode == 0:
        raise RuntimeError("mutated build succeeded")
    span = theorem_span(root / expected, theorem)
    diagnostics = error_diagnostics(output)

    def within_witness(diagnostic):
        reported = Path(diagnostic["file"])
        same_file = (diagnostic["file"] == expected or
                     (reported.is_absolute() and reported.resolve() == (root / expected).resolve()))
        return same_file and span[0] <= diagnostic["line"] <= span[1]

    for diagnostic in diagnostics:
        # A typo/parse/setup/resource failure must not accidentally accompany
        # and validate a semantic failure. Only the deliberate missing named
        # requirement may use an unknown-identifier diagnostic.
        if re.match(r"Unknown (?:identifier|constant|module)|unexpected token|"
                    r"unterminated|invalid field|failed to synthesize|"
                    r"maximum (?:recursion|heartbeat)|object file|cannot find",
                    diagnostic["message"], re.IGNORECASE):
            deliberate = (
                category == "missing_requirement"
                and within_witness(diagnostic)
                and property_failure(diagnostic["message"], category)
            )
            if not deliberate:
                raise RuntimeError(f"unrelated elaboration/setup failure: {diagnostic}")
    for diagnostic in diagnostics:
        if within_witness(diagnostic) and property_failure(diagnostic["message"], category):
            return dict(expected_theorem=theorem, source_span=list(span),
                        failure_category=category, diagnostic=diagnostic)
    raise RuntimeError(f"no intended property failure in {expected}:{span} ({theorem}, {category}):\n{output}")


def checked_build(root):
    return subprocess.run(["lake", "build"], cwd=root, text=True, encoding="utf-8",
                          capture_output=True, timeout=180)


def mutate(root, file, old, new):
    path = root / file
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor changed: {file}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def selective_approval_probe(root: Path, temporary: Path) -> dict:
    # Compile the actual mutated approval body against the unchanged baseline
    # policy/data definitions. This setup witness cannot count as the mutation
    # rejection: the complete mutated project must still fail its named proof.
    source = (root / "RemovalModel.lean").read_text(encoding="utf-8")
    start = "def approve (root : Path) (entries : List Entry) : Option (Approved root) :=\n"
    end = "\ntheorem approve_rejects_invalid"
    if source.count(start) != 1 or source.count(end) != 1:
        raise RuntimeError("approval probe definition anchor changed")
    definition = start + source.split(start, 1)[1].split(end, 1)[0]
    probe = (
        "import RemovalModel\nimport NestedFixtures\n"
        "set_option warningAsError true\nopen DiskTree\n"
        "namespace ApprovalMutationProbe\n"
        + definition.replace("def approve ", "def candidate ", 1) + "\n"
        "#guard (candidate [1] [fixtureEntry 0]).isSome = true\n"
        "#guard (candidate [1] fixtureOrder).isSome = true\n"
        "#guard (candidate [1] nestedOrder).isSome = true\n"
        "#guard valid [42] [⟨[42, 1], 314, false⟩]\n"
        "#guard (candidate [42] [⟨[42, 1], 314, false⟩]).isSome = false\n"
        "end ApprovalMutationProbe\n"
    )
    temporary = temporary.resolve()
    if temporary.parent != ROOT or not root.resolve().is_relative_to(ROOT):
        raise RuntimeError("approval probe directory escaped project")
    path = temporary / "ApprovalMutationProbe.lean"
    path.write_text(probe, encoding="utf-8")
    result = subprocess.run(
        ["lake", "env", "lean", "-DwarningAsError=true", str(path)],
        cwd=ROOT, text=True, encoding="utf-8", capture_output=True, timeout=120,
    )
    if result.returncode:
        raise RuntimeError(f"selective approval setup probe failed:\n{result.stdout}{result.stderr}")
    return dict(exit_code=result.returncode, source=probe, log=result.stdout + result.stderr,
                accepted_baseline_examples=3, independently_valid_refused_root=[42])


def main():
    baseline = checked_build(ROOT)
    if baseline.returncode:
        raise RuntimeError(f"baseline failed; mutation results invalid:\n{baseline.stdout}{baseline.stderr}")
    model = "RemovalModel.lean"
    safety = "SafetyProperties.lean"
    cases = []
    for flag in ("rootMatches", "identitiesMatch", "membershipMatches", "guardsPass", "handlesPinned"):
        # Preserve well-typed Bool expressions; remove just one requirement.
        cases.append((f"drop_{flag}", model, f"p.{flag}", "true", safety))
    cases += [
        ("broaden_path_scope", model,
         "root ≠ [] ∧ root.length < path.length ∧ path.take root.length = root",
         "root ≠ [] ∧ root.length < path.length ∧ path.take root.length = path.take root.length", model),
        ("ignore_protected_flag", model,
         "(∀ entry ∈ entries, below root entry.path ∧ entry.guardedOut = false)",
         "(∀ entry ∈ entries, below root entry.path ∧ entry.guardedOut = entry.guardedOut)", model),
        ("increase_entry_limit", model, "entries.length ≤ 20000", "entries.length ≤ 20001", model),
        ("change_byte_ceiling", model, "def u64Max : Nat := 18446744073709551615",
         "def u64Max : Nat := 1000", "RequiredProperties.lean"),
        ("empty_run", model, "if preflight.passes then execute plan.val decisions else []", "[]", model),
        ("ignore_current_identity", model,
         "path = entry.path ∧ world path = some entry.objectId", "path = entry.path", safety),
        ("ignore_observed_identity", safety,
         "if identitiesAgree world plan.val then run plan p ds else []", "run plan p ds", safety),
        ("allow_duplicate_paths", model, "earlier.path ≠ later.path", "True", safety),
        ("allow_parent_first", model, "¬ below earlier.path later.path", "True", safety),
        ("delete_required_theorem", safety,
         "theorem replaced_object_survives :\n"
         "    applyRemovals replacedWorld replacementPlan.val [1, 2] = some 99 := by decide",
         "", "RequiredProperties.lean"),
        ("weaken_required_statement", safety,
         "theorem replaced_object_survives :\n"
         "    applyRemovals replacedWorld replacementPlan.val [1, 2] = some 99 := by decide",
         "theorem replaced_object_survives : True := by trivial", "RequiredProperties.lean"),
        ("approve_only_fixture_root", model,
         APPROVAL_BODY, SELECTIVE_APPROVAL_BODY, model),
    ]
    if {case[0] for case in cases} != set(EXPECTED_FAILURES):
        raise RuntimeError("mutation witnesses do not match controlled cases")
    evidence = []
    for name, file, old, new, expected in cases:
        with tempfile.TemporaryDirectory(prefix=".audit-mutation-", dir=ROOT) as raw:
            temporary = Path(raw).resolve()
            if temporary.parent != ROOT:
                raise RuntimeError("mutation directory escaped project")
            copy = temporary / "project"
            shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns(
                ".lake", ".audit-*", "__pycache__", "*.log",
                "proof-audit.json", "mutation-results.json",
            ))
            mutate(copy, file, old, new)
            setup_probe = (selective_approval_probe(copy, temporary)
                           if name == "approve_only_fixture_root" else None)
            result = checked_build(copy)
            output = result.stdout + result.stderr
            theorem, category = EXPECTED_FAILURES[name]
            witness = classify_failure(result, copy, expected, theorem, category)
            evidence.append(dict(mutation=name, exit_code=result.returncode,
                                 failed_module=expected, **witness, log=output,
                                 setup_probe=setup_probe))
            print(f"{name}: rejected by {expected}:{theorem} ({category})", flush=True)
    (ROOT / "mutation-results.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(f"{len(evidence)} weakened models rejected; baseline kernel build passed")


if __name__ == "__main__":
    main()
