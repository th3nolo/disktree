"""A killed semantic mutation needs its named proof failure, not any error."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_mutations import (
    APPROVAL_BODY, ROOT, SELECTIVE_APPROVAL_BODY, classify_failure,
    selective_approval_probe, theorem_span,
)


class MutationClassifierTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.assertEqual(self.root.parent, ROOT)
        (self.root / "SafetyProperties.lean").write_text(
            "import Std\n\n"
            "theorem intended : True := by\n  trivial\n\n"
            "theorem other : True := by trivial\n", encoding="utf-8",
        )

    def classify(self, output, category="unsolved", exit_code=1):
        result = subprocess.CompletedProcess([], exit_code, output, "")
        return classify_failure(result, self.root, "SafetyProperties.lean", "intended", category)

    def test_named_proof_failure_is_accepted_with_its_span(self):
        witness = self.classify("error: SafetyProperties.lean:3:29: unsolved goals\n⊢ True\n")
        self.assertEqual(witness["source_span"], [3, 4])
        self.assertEqual(witness["expected_theorem"], "intended")

    def test_raw_lean_diagnostic_and_absolute_path_are_accepted(self):
        source = self.root / "SafetyProperties.lean"
        for severity in ("error", "error(lean.unsolvedGoals)"):
            self.classify(f"{source}:4:2: {severity}: unsolved goals\n⊢ True\n")

    def test_successful_build_cannot_kill_a_mutation(self):
        with self.assertRaises(RuntimeError):
            self.classify("error: SafetyProperties.lean:3:2: unsolved goals\n⊢ True\n", exit_code=0)

    def test_unrelated_typo_even_in_expected_theorem_is_rejected(self):
        for prefix in ("error: SafetyProperties.lean:4:2:",
                       "SafetyProperties.lean:4:2: error(lean.unknownIdentifier):"):
            with self.subTest(prefix=prefix), self.assertRaisesRegex(RuntimeError, "unrelated elaboration/setup"):
                self.classify(f"{prefix} Unknown identifier `unrelated_typo`\n")

    def test_typo_alongside_proof_failure_is_rejected(self):
        with self.assertRaises(RuntimeError):
            self.classify("error: SafetyProperties.lean:3:2: unsolved goals\n⊢ True\n"
                          "error: SafetyProperties.lean:6:2: Unknown identifier `unrelated_typo`\n")

    def test_warning_promoted_to_error_cannot_kill_a_mutation(self):
        for message in ("Variable name `unused` is not explicitly referenced.",
                        "This simp argument is unused:\n  h"):
            with self.subTest(message=message), self.assertRaises(RuntimeError):
                self.classify(f"error: SafetyProperties.lean:4:2: {message}\n")

    def test_warning_severity_cannot_kill_a_mutation(self):
        with self.assertRaises(RuntimeError):
            self.classify("SafetyProperties.lean:3:2: warning: unsolved goals\n⊢ True\n")

    def test_failure_in_another_theorem_or_module_is_rejected(self):
        for location in ("SafetyProperties.lean:6:2", "Other.lean:3:2",
                         "NotSafetyProperties.lean:3:2", "Other/SafetyProperties.lean:3:2"):
            with self.subTest(location=location), self.assertRaises(RuntimeError):
                self.classify(f"error: {location}: unsolved goals\n⊢ True\n")
        for indentation in ("", "  "):
            (self.root / "SafetyProperties.lean").write_text(
                "import Std\n\ntheorem intended : True := by trivial\n"
                f"{indentation}theorem other : False := by skip\n", encoding="utf-8",
            )
            with self.subTest(indentation=indentation), self.assertRaises(RuntimeError):
                self.classify("error: SafetyProperties.lean:4:29: unsolved goals\n⊢ False\n")

    def test_parse_setup_and_unknown_error_categories_are_rejected(self):
        for output in ("error: SafetyProperties.lean:3:2: unexpected token 'bad'\n",
                       "error: build failed\n", "error: SafetyProperties.lean:3:2: arbitrary failure\n"):
            with self.subTest(output=output), self.assertRaises(RuntimeError):
                self.classify(output)

    def test_false_proposition_requires_decide_and_false_result(self):
        self.classify("error: SafetyProperties.lean:3:2: Tactic `decide` proved that the proposition\n"
                      "  True\nis false\n", category="false_proposition")
        with self.assertRaises(RuntimeError):
            self.classify("error: SafetyProperties.lean:3:2: Tactic `decide` failed\n",
                          category="false_proposition")

    def test_missing_requirement_accepts_only_exact_named_requirement(self):
        self.classify("error: SafetyProperties.lean:4:2: Unknown identifier `DiskTree.replaced_object_survives`\n",
                      category="missing_requirement")
        with self.assertRaises(RuntimeError):
            self.classify("error: SafetyProperties.lean:4:2: Unknown identifier `DiskTree.unrelated`\n",
                          category="missing_requirement")

    def test_missing_or_duplicate_theorem_anchor_stops_classification(self):
        source = self.root / "SafetyProperties.lean"
        with self.assertRaises(RuntimeError):
            theorem_span(source, "missing")
        source.write_text(source.read_text(encoding="utf-8") + "\ntheorem intended : True := by trivial\n",
                          encoding="utf-8")
        with self.assertRaises(RuntimeError):
            theorem_span(source, "intended")


@unittest.skipUnless(os.environ.get("LEAN_AUDIT_INTEGRATION") == "1", "requires pinned Lean")
class CompilerClassifierTests(unittest.TestCase):
    def check(self, declaration, category="unsolved"):
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            directory = Path(raw).resolve()
            self.assertEqual(directory.parent, ROOT)
            source = directory / "SafetyProperties.lean"
            source.write_text("import Std\n" + declaration, encoding="utf-8")
            result = subprocess.run(
                ["lake", "env", "lean", "-DwarningAsError=true", str(source)],
                cwd=ROOT, text=True, encoding="utf-8", capture_output=True, timeout=120,
            )
            self.assertNotEqual(result.returncode, 0, "negative fixture unexpectedly compiled")
            return classify_failure(result, directory, source.name, "intended", category)

    def test_actual_unrelated_typo_is_not_a_killed_mutation(self):
        with self.assertRaisesRegex(RuntimeError, "unrelated elaboration/setup"):
            self.check("theorem intended : True := by exact unrelated_typo\n")

    def test_actual_warning_only_is_not_a_killed_mutation(self):
        with self.assertRaisesRegex(RuntimeError, "no intended property failure"):
            self.check("set_option linter.unusedVariables true\n"
                       "theorem intended (unused : Nat) : True := by trivial\n")

    def test_actual_unsolved_named_property_is_a_killed_mutation(self):
        self.check("theorem intended : False := by skip\n")
        with self.assertRaisesRegex(RuntimeError, "no intended property failure"):
            self.check("theorem intended : True := by trivial\n"
                       "theorem other : False := by skip\n")

    def test_selective_approval_probe_preserves_examples_and_refuses_valid_other_root(self):
        source = (ROOT / "RemovalModel.lean").read_text(encoding="utf-8")
        self.assertEqual(source.count(APPROVAL_BODY), 1)
        with tempfile.TemporaryDirectory(prefix=".audit-test-", dir=ROOT) as raw:
            directory = Path(raw).resolve()
            self.assertEqual(directory.parent, ROOT)
            model = directory / "RemovalModel.lean"
            model.write_text(source, encoding="utf-8")
            # The ordinary definition must fail the claimed selective refusal.
            with self.assertRaisesRegex(RuntimeError, "selective approval setup probe failed"):
                selective_approval_probe(directory, directory)
            model.write_text(source.replace(APPROVAL_BODY, SELECTIVE_APPROVAL_BODY), encoding="utf-8")
            result = selective_approval_probe(directory, directory)
            self.assertEqual(result["accepted_baseline_examples"], 3)
            self.assertEqual(result["independently_valid_refused_root"], [42])


if __name__ == "__main__":
    unittest.main()
