"""Inventory checked Lean environments, not source declaration spelling."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ALLOWED_AXIOMS = {"propext", "Quot.sound", "Classical.choice"}
PREFIX = "DISKTREE_AUDIT "
INFRASTRUCTURE = {"AuditSupport.lean"}


def discover_sources(root: Path) -> list[Path]:
    root = root.resolve()
    sources = []
    for directory, subdirs, files in os.walk(root, followlinks=False):
        subdirs[:] = [name for name in subdirs if name != ".lake"]
        for name in [*subdirs, *files]:
            path = Path(directory) / name
            relative = path.relative_to(root)
            if path.is_symlink():
                raise RuntimeError(f"source symlinks are not permitted: {relative}")
            if not path.resolve().is_relative_to(root):
                raise RuntimeError(f"source outside project: {relative}")
            if path.suffix == ".lean" and path.is_file() and relative.as_posix() not in INFRASTRUCTURE:
                sources.append(path)
    if not sources:
        raise RuntimeError("no project modules checked")
    return sorted(sources)


def read_environment(output: str) -> list[dict]:
    records = [line[len(PREFIX):] for line in output.splitlines() if line.startswith(PREFIX)]
    if len(records) != 1:
        raise RuntimeError("expected exactly one complete environment record")
    rows = json.loads(records[0])
    if not isinstance(rows, list):
        raise RuntimeError("invalid environment record")
    return rows


def inspect_source(root: Path, source: Path) -> list[dict]:
    # Importing a module may hide private bodies. Check the source itself.
    # All declarations are inventoried at EOF, including private/generated ones.
    root = root.resolve()
    with tempfile.TemporaryDirectory(prefix=".audit-", dir=root) as raw:
        directory = Path(raw).resolve()
        if directory.parent != root:
            raise RuntimeError("audit temporary directory escaped project")
        copy = directory / "Inspected.lean"
        copy.write_text(
            "import AuditSupport\n" + source.read_text(encoding="utf-8") + "\n#audit_module\n",
            encoding="utf-8",
        )
        result = subprocess.run(
            ["lake", "env", "lean", "-DwarningAsError=true", str(copy)],
            cwd=root, text=True, encoding="utf-8", capture_output=True, timeout=120,
        )
        if result.returncode:
            raise RuntimeError(f"kernel check failed for {source.relative_to(root)}:\n"
                               f"{result.stdout}{result.stderr}")
        return read_environment(result.stdout)


def validate_report(report: dict, sources: set[str], required: dict[str, list[str]]) -> int:
    if set(report) != sources or not sources:
        raise RuntimeError(f"incomplete module audit: {sources ^ set(report)}")
    theorem_count = 0
    for source, rows in report.items():
        names = set()
        theorems = set()
        for row in rows:
            expected = {"name", "user_name", "kind", "type", "value", "unsafe", "partial",
                        "compiler_auxiliary", "implemented_by", "extern", "axioms"}
            if not isinstance(row, dict) or set(row) != expected:
                raise RuntimeError(f"invalid declaration record in {source}")
            if type(row["extern"]) is not bool:
                raise RuntimeError(f"invalid extern attribute record in {source}")
            name = row["name"]
            if name in names:
                raise RuntimeError(f"duplicate declaration in {source}: {name}")
            names.add(name)
            if row["kind"] == "axiom" or row["unsafe"] or row["implemented_by"] or row["extern"]:
                raise RuntimeError(f"prohibited declaration in {source}: {name}")
            if row["partial"] and not row["compiler_auxiliary"]:
                raise RuntimeError(f"source partial declaration in {source}: {name}")
            if unexpected := set(row["axioms"]) - ALLOWED_AXIOMS:
                raise RuntimeError(f"untrusted dependencies for {name}: {unexpected}")
            if row["kind"] == "theorem":
                theorems.add(row["user_name"])
                theorem_count += 1
        if missing := set(required.get(source, [])) - theorems:
            raise RuntimeError(f"missing required properties in {source}: {missing}")
    if absent := set(required) - sources:
        raise RuntimeError(f"missing required specification modules: {absent}")
    if not theorem_count:
        raise RuntimeError("no theorems checked")
    return theorem_count


def audit(root: Path, output: Path) -> None:
    root = root.resolve()
    sources = discover_sources(root)
    required = json.loads((root / "required-properties.json").read_text(encoding="utf-8"))
    report = {source.relative_to(root).as_posix(): inspect_source(root, source) for source in sources}
    # Retain checked declarations even when validation refuses one of them.
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    count = validate_report(report, {source.relative_to(root).as_posix() for source in sources}, required)
    print(f"{count} checked theorem declarations across {len(sources)} modules; "
          "required properties present; only Lean foundations permitted")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    audit(Path(__file__).parent, args.output)
