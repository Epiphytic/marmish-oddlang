"""Controls for the live suite producer's case selection and registration."""
from copy import deepcopy
import json
import unittest

import selected_tests
import parser_cases
from candidate_coverage import CORE, HERE, check_default_baseline, check_registered_names


class ProducerControls(unittest.TestCase):
    def test_combined_named_baseline_rejects_equal_count_substitution(self):
        host = "aarch64-apple-darwin"
        previous = json.loads((HERE / f"canonical-{host}.json").read_bytes())
        baseline_sha = check_default_baseline(host, previous["harnesses"])
        self.assertEqual(len(baseline_sha), 64)
        exchanged = deepcopy(previous["harnesses"])
        native_lib = parser_cases.HARNESS
        exchanged[native_lib]["passed"][0], exchanged[native_lib]["unverified"][0] = (
            exchanged[native_lib]["unverified"][0], exchanged[native_lib]["passed"][0]
        )
        with self.assertRaisesRegex(ValueError, "named baseline differs"):
            check_default_baseline(host, exchanged)

    def test_combined_registration_denies_extra_and_duplicate_cases(self):
        expected = ["test_a", "test_b"]
        self.assertEqual(check_registered_names(CORE, "test_b: test\ntest_a: test\n", expected),
                         ["test_a", "test_b"])
        for listing in ("test_a: test\ntest_c: test\n",
                        "test_a: test\ntest_b: test\ntest_b: test\n",
                        ""):
            with self.assertRaisesRegex(ValueError, "unsafe or incomplete"):
                check_registered_names(CORE, listing, expected)

    def test_parser_case_requires_one_actual_execution(self):
        def no_match(*args, **kwargs):
            return b"running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored;"

        with self.assertRaises(ValueError):
            parser_cases.witness(no_match, HERE, "native_manifest::__projection_10",
                                 lambda _: [], selected_tests.check_execution)


if __name__ == "__main__":
    unittest.main()
