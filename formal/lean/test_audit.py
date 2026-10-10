"""Check that incomplete or unchecked theorem evidence fails closed."""

import unittest

from audit import validate_report


class ReportAuditTests(unittest.TestCase):
    def test_foundational_or_constructive_proofs_are_accepted(self):
        validate_report(
            "'DiskTree.first' depends on axioms: [propext, Quot.sound]\n"
            "'DiskTree.second' does not depend on any axioms\n",
            {"first", "second"},
        )

    def test_unfinished_or_extra_axiom_dependencies_are_rejected(self):
        for dependency in ("sorryAx", "Untrusted.assumption"):
            with self.subTest(dependency=dependency):
                with self.assertRaises(RuntimeError):
                    validate_report(
                        f"'DiskTree.first' depends on axioms: [{dependency}]\n",
                        {"first"},
                    )

    def test_missing_report_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report("", {"first"})

    def test_duplicate_report_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report(
                "'DiskTree.first' does not depend on any axioms\n" * 2,
                {"first"},
            )

    def test_empty_proof_suite_is_rejected(self):
        with self.assertRaises(RuntimeError):
            validate_report("", set())


if __name__ == "__main__":
    unittest.main()
