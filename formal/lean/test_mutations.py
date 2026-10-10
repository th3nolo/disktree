"""Require weakened models to fail the pinned Lean/specification gate."""

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).parent.resolve()


def checked_build(root):
    return subprocess.run(["lake", "build"], cwd=root, text=True, encoding="utf-8",
                          capture_output=True, timeout=180)


def mutate(root, file, old, new):
    path = root / file
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor changed: {file}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


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
    ]
    evidence = []
    for name, file, old, new, expected in cases:
        with tempfile.TemporaryDirectory(prefix=".audit-mutation-", dir=ROOT) as raw:
            temporary = Path(raw).resolve()
            if temporary.parent != ROOT:
                raise RuntimeError("mutation directory escaped project")
            copy = temporary / "project"
            shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns(".lake", ".audit-*", "__pycache__", "*.log", "*.json"))
            mutate(copy, file, old, new)
            result = checked_build(copy)
            output = result.stdout + result.stderr
            if result.returncode == 0 or not re.search(re.escape(expected) + r":\d+:\d+: error:", output):
                # Lake currently prefixes these diagnostics with "error:".
                if result.returncode == 0 or not re.search(r"error: .*" + re.escape(expected) + r":\d+:\d+:", output):
                    raise RuntimeError(f"mutation {name} failed for an unexpected reason:\n{output}")
            evidence.append(dict(mutation=name, exit_code=result.returncode, failed_module=expected, log=output))
            print(f"{name}: rejected in {expected}", flush=True)
    (ROOT / "mutation-results.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(f"{len(evidence)} weakened models rejected; baseline kernel build passed")


if __name__ == "__main__":
    main()
