"""Report, recursive inventory, and real Lean environment regressions."""

import json
import os
from pathlib import Path
import tempfile
import unittest

from audit import discover_sources, inspect_source, read_environment, validate_report

ROOT = Path(__file__).parent.resolve()


def row(name="DiskTree.first", axioms=(), **changes):
    result = dict(name=name, user_name=name, kind="theorem", type="True", value="proof",
                  unsafe=False, partial=False, implemented_by=None, axioms=list(axioms))
    result.update(changes)
    return result


class ReportAuditTests(unittest.TestCase):
    def check(self, rows, required=None):
        return validate_report({"Model.lean": rows}, {"Model.lean"},
                               {"Model.lean": required or ["DiskTree.first"]})

    def test_foundational_or_constructive_proofs_are_accepted(self):
        self.assertEqual(self.check([row(axioms=("propext", "Quot.sound")),
                                     row("DiskTree.second")]), 2)

    def test_unfinished_or_extra_axiom_dependencies_are_rejected(self):
        for dependency in ("sorryAx", "Untrusted.assumption", "Lean.ofReduceBool"):
            with self.subTest(dependency=dependency), self.assertRaises(RuntimeError):
                self.check([row(axioms=(dependency,))])

    def test_missing_required_property_is_rejected(self):
        with self.assertRaises(RuntimeError):
            self.check([row()], ["DiskTree.first", "DiskTree.deleted"])

    def test_duplicate_report_is_rejected(self):
        with self.assertRaises(RuntimeError):
            self.check([row(), row()])

    def test_empty_proof_suite_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report({}, set(), {})

    def test_source_and_report_shrink_together_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report({"Model.lean": [row()]}, {"Model.lean"},
                            {"Model.lean": ["DiskTree.first"], "Nested/Required.lean": ["required"]})

    def test_nested_module_without_report_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report({"Model.lean": [row()]}, {"Model.lean", "Nested/Extra.lean"}, {})

    def test_axiom_unsafe_partial_and_replacement_implementation_are_rejected(self):
        for change in (dict(kind="axiom"), dict(unsafe=True), dict(partial=True),
                       dict(implemented_by="replacement")):
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.check([row(**change)])

    def test_report_schema_is_required(self):
        record = row()
        del record["type"]
        with self.assertRaises(RuntimeError):
            self.check([record])

    def test_environment_marker_missing_or_duplicated_is_rejected(self):
        for text in ("", "DISKTREE_AUDIT []\nDISKTREE_AUDIT []\n"):
            with self.subTest(text=text), self.assertRaises(RuntimeError):
                read_environment(text)

    def test_environment_record_ignores_ordinary_log_messages(self):
        self.assertEqual(read_environment("compiler output\nDISKTREE_AUDIT []\n"), [])

    def test_recursive_inventory_includes_nested_files(self):
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            root = Path(raw).resolve()
            self.assertEqual(root.parent, ROOT)
            (root / "Nested").mkdir()
            (root / ".lake").mkdir()
            for name in ("Model.lean", "Nested/Extra.lean", ".lake/Cached.lean", "AuditSupport.lean"):
                (root / name).write_text("import Std\n", encoding="utf-8")
            self.assertEqual({p.relative_to(root).as_posix() for p in discover_sources(root)},
                             {"Model.lean", "Nested/Extra.lean"})

    def test_temporary_looking_source_directory_is_not_ignored(self):
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            root = Path(raw).resolve()
            self.assertEqual(root.parent, ROOT)
            (root / ".audit-hidden").mkdir()
            source = root / ".audit-hidden/Extra.lean"
            source.write_text("import Std\n", encoding="utf-8")
            self.assertEqual(discover_sources(root), [source])

    @unittest.skipUnless(os.name == "posix", "symlink creation requires privileges on Windows")
    def test_symlink_directory_cannot_hide_a_module(self):
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            temporary = Path(raw).resolve()
            self.assertEqual(temporary.parent, ROOT)
            source = temporary / "project"
            source.mkdir()
            outside = temporary / "outside"
            outside.mkdir()
            (outside / "Extra.lean").write_text("import Std\n", encoding="utf-8")
            (source / "linked").symlink_to(outside, target_is_directory=True)
            with self.assertRaises(RuntimeError):
                discover_sources(source)


@unittest.skipUnless(os.environ.get("LEAN_AUDIT_INTEGRATION") == "1", "requires pinned Lean")
class LeanEnvironmentTests(unittest.TestCase):
    def inspect(self, code, nested=False):
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            directory = Path(raw).resolve()
            self.assertEqual(directory.parent, ROOT)
            source = directory / ("Nested/Extra.lean" if nested else "Extra.lean")
            source.parent.mkdir(exist_ok=True)
            source.write_text("import Std\n" + code, encoding="utf-8")
            return inspect_source(ROOT, source)

    def test_spelling_attributes_privacy_namespace_and_escaped_names(self):
        records = self.inspect(
            "/- theorem fictional : False := nonsense -/\n"
            "namespace Other\n"
            "  theorem indented : True := by trivial\n"
            "@[simp] theorem attributed (n : Nat) : 0 + n = n := Nat.zero_add n\n"
            "private theorem hidden : True := by trivial\n"
            "theorem «space name» : True := by trivial\n"
            "def prose : String := \"theorem fictional sorry axiom\"\n"
            "end Other\n"
        )
        names = {r["user_name"] for r in records if r["kind"] == "theorem"}
        for name in ("Other.indented", "Other.attributed", "Other.hidden", "Other.space name"):
            self.assertIn(name, names)
        self.assertFalse(any("fictional" in name for name in names))
        validate_report({"Extra.lean": records}, {"Extra.lean"}, {"Extra.lean": list(names)})

    def test_nested_file_is_kernel_checked_and_inventoried(self):
        records = self.inspect("theorem nested : True := by trivial\n", nested=True)
        self.assertIn("nested", {r["user_name"] for r in records if r["kind"] == "theorem"})

    def test_unused_axiom_is_rejected(self):
        records = self.inspect("axiom unused : False\ntheorem normal : True := by trivial\n")
        with self.assertRaises(RuntimeError):
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_native_shortcut_is_rejected(self):
        records = self.inspect("theorem native : 1 = 1 := by native_decide\n")
        with self.assertRaises(RuntimeError):
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_sorry_is_rejected_by_compiler(self):
        with self.assertRaises(RuntimeError):
            self.inspect("theorem unfinished : False := by sorry\n")

    def test_implemented_by_is_rejected_even_without_native_decide(self):
        records = self.inspect(
            "def implementation : Bool := false\n"
            "@[implemented_by implementation] def claimed : Bool := true\n"
            "theorem normal : True := by trivial\n"
        )
        with self.assertRaises(RuntimeError):
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})


if __name__ == "__main__":
    unittest.main()
