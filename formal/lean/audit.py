"""Fail CI if a project theorem has unchecked proof dependencies."""

from pathlib import Path
import re
import sys


def audit(log_path: str) -> None:
    sources = list(Path(".").glob("*.lean"))
    theorem_names = set()
    for source in sources:
        text = source.read_text(encoding="utf-8")
        # The project uses line comments only; reject block comments rather
        # than risk overlooking a prohibited declaration inside one.
        if "/-" in text:
            raise RuntimeError(f"audit expects line comments: {source}")
        code = re.sub(r"--[^\n]*", "", text)
        forbidden = re.search(
            r"\b(sorry|admit|axiom|native_decide|unsafe|implemented_by|partial)\b",
            code,
        )
        if forbidden:
            raise RuntimeError(f"prohibited construct in {source}: {forbidden[0]}")
        theorem_names.update(re.findall(r"^theorem\s+(\w+)", code, re.MULTILINE))

    log = Path(log_path).read_text(encoding="utf-8")
    reports = re.findall(
        r"'DiskTree\.(\w+)' (?:does not depend on any axioms|depends on axioms:\s*\[([^\]]*)\])",
        log,
    )
    reported = {name for name, _ in reports}
    if reported != theorem_names or len(reports) != len(theorem_names):
        raise RuntimeError(f"incomplete proof audit: {theorem_names ^ reported}")
    allowed = {"propext", "Quot.sound", "Classical.choice"}
    for name, raw in reports:
        dependencies = {item.strip() for item in raw.split(",") if item.strip()}
        if unexpected := dependencies - allowed:
            raise RuntimeError(f"untrusted dependencies for {name}: {unexpected}")
    if not theorem_names:
        raise RuntimeError("no theorems checked")
    print(f"{len(theorem_names)} theorems audited; only Lean foundations permitted")


if __name__ == "__main__":
    audit(sys.argv[1])
