"""Report, recursive inventory, and real Lean environment regressions."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from audit import discover_sources, inspect_source, read_environment, validate_report

ROOT = Path(__file__).parent.resolve()


def row(name="DiskTree.first", axioms=(), **changes):
    result = dict(name=name, user_name=name, kind="theorem", type="True", value="proof",
                  unsafe=False, partial=False, compiler_auxiliary=False,
                  implemented_by=None, extern=False, axioms=list(axioms))
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
                       dict(implemented_by="replacement"), dict(extern=True)):
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.check([row(**change)])

    def test_report_schema_is_required(self):
        for field in ("type", "extern"):
            record = row()
            del record[field]
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                self.check([record])

    def test_extern_attribute_must_be_an_explicit_boolean(self):
        for value in (None, 0, "", []):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                self.check([row(extern=value)])

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
    def check_logical_source(self, code):
        # Establish that refusal comes from the audit, not a malformed fixture.
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            directory = Path(raw).resolve()
            self.assertEqual(directory.parent, ROOT)
            source = directory / "Logical.lean"
            source.write_text("import Lean\n" + code, encoding="utf-8")
            result = subprocess.run(
                ["lake", "env", "lean", "-DwarningAsError=true", str(source)],
                cwd=ROOT, text=True, encoding="utf-8", capture_output=True, timeout=120,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

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
        for name in ("Other.indented", "Other.attributed", "Other.hidden", "Other.«space name»"):
            self.assertIn(name, names)
        self.assertFalse(any("fictional" in name for name in names))
        validate_report({"Extra.lean": records}, {"Extra.lean"}, {"Extra.lean": list(names)})

    def test_nested_file_is_kernel_checked_and_inventoried(self):
        records = self.inspect("theorem nested : True := by trivial\n", nested=True)
        self.assertIn("nested", {r["user_name"] for r in records if r["kind"] == "theorem"})

    def test_unused_axiom_is_rejected(self):
        with self.assertRaises(RuntimeError):
            records = self.inspect("axiom unused : False\ntheorem normal : True := by trivial\n")
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_native_shortcut_is_rejected(self):
        with self.assertRaises(RuntimeError):
            records = self.inspect("theorem native : 1 = 1 := by native_decide\n")
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_sorry_is_rejected_by_compiler(self):
        with self.assertRaises(RuntimeError):
            self.inspect("theorem unfinished : False := by sorry\n")

    def test_implemented_by_is_rejected_even_without_native_decide(self):
        with self.assertRaises(RuntimeError):
            records = self.inspect(
                "def implementation : Bool := false\n"
                "@[implemented_by implementation] def claimed : Bool := true\n"
                "theorem normal : True := by trivial\n"
            )
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_extern_definition_with_valid_logical_proof_is_rejected(self):
        code = (
            '@[extern "disktree_untrusted_runtime"] def claimed : Bool := true\n'
            "theorem logical : claimed = true := rfl\n"
        )
        self.check_logical_source(code)
        with self.assertRaisesRegex(RuntimeError, "prohibited source syntax"):
            self.inspect(code)

    def test_separately_applied_extern_attribute_is_rejected(self):
        code = (
            "def claimed : Bool := true\n"
            'attribute [extern "disktree_untrusted_runtime"] claimed\n'
            "theorem logical : claimed = true := rfl\n"
        )
        self.check_logical_source(code)
        with self.assertRaisesRegex(RuntimeError, "prohibited source syntax"):
            self.inspect(code)

    def test_extern_environment_attribute_is_reported_without_source_keyword(self):
        # Exercise the independent environment check through Lean's attribute
        # API. A source-word check cannot detect this programmatic attachment.
        code = (
            "def claimed : Bool := true\n"
            "run_cmd do\n"
            "  let env ← Lean.getEnv\n"
            "  match Lean.externAttr.setParam env `claimed\n"
            '      { entries := [.standard `all "disktree_untrusted_runtime"] } with\n'
            "  | .ok updated => Lean.setEnv updated\n"
            "  | .error message => throwError message\n"
            "theorem logical : claimed = true := rfl\n"
        )
        self.check_logical_source(code)
        records = self.inspect(code)
        claimed = next(record for record in records if record["user_name"] == "claimed")
        self.assertIs(claimed["extern"], True)
        self.assertEqual(claimed["axioms"], [])
        with self.assertRaisesRegex(RuntimeError, "prohibited declaration.*claimed"):
            validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_safe_recursion_keeps_compiler_helpers_without_trusting_extra_axioms(self):
        records = self.inspect(
            "def count : List Nat → Nat\n"
            "  | [] => 0\n"
            "  | _ :: rest => count rest + 1\n"
            "theorem normal : count [1, 2] = 2 := by decide\n"
        )
        self.assertTrue(any(r["compiler_auxiliary"] for r in records))
        validate_report({"Extra.lean": records}, {"Extra.lean"}, {})

    def test_source_unsafe_partial_and_spoofed_helper_are_rejected(self):
        cases = (
            "unsafe def unsafeValue : Nat := 1\n",
            "partial def forever (n : Nat) : Nat := forever n\n",
            "def safeParent : Nat := 1\n"
            "partial def safeParent._unsafe_rec : Nat := 1\n",
        )
        for code in cases:
            with self.subTest(code=code):
                with self.assertRaises(RuntimeError):
                    records = self.inspect(code + "theorem normal : True := by trivial\n")
                    validate_report({"Extra.lean": records}, {"Extra.lean"}, {})


if __name__ == "__main__":
    unittest.main()
