"""Independent corpus invariants and intentional mismatch checks."""
from dataclasses import replace
import hashlib
import unittest

from corpus import SHORT_CASES, finite_case, verify_output


class CorpusTests(unittest.TestCase):
    def test_short_cases_preserve_stream_and_exit_distinctions(self):
        self.assertEqual([case.status for case in SHORT_CASES], [0, 0, 0, 0, 7])
        self.assertEqual(SHORT_CASES[2].stdout + SHORT_CASES[2].stderr, b"")
        self.assertTrue(SHORT_CASES[3].stdout.endswith(b"\n\n"))
        self.assertFalse(SHORT_CASES[3].stderr.endswith(b"\n"))

    def test_comparison_rejects_each_changed_result(self):
        case = SHORT_CASES[1]
        verify_output(case, case)
        for field, value in [("stdout", b""), ("stderr", b""), ("status", 1)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                verify_output(case, replace(case, **{field: value}))

    def test_finite_pinned_bytes_and_vector(self):
        case = finite_case(0)
        self.assertEqual((len(case.stdin), len(case.stdout), len(case.stderr), case.status),
                         (16384, 131072, 131072, 7))
        self.assertEqual(case.args[-1], b"e\xffd")
        self.assertEqual(hashlib.sha256(case.stdin).hexdigest(),
                         "074fd0e99918d116fb92c6922db45eea4e4ecf3fe7e9f293af1f27c22313594d")
        with self.assertRaises(ValueError):
            finite_case(0, vector=1)

    def test_nonce_schedule_is_distinct_and_bounded(self):
        self.assertEqual(len({finite_case(i).args[0] for i in range(4)}), 4)
        for index in (-1, 4):
            with self.assertRaises(ValueError):
                finite_case(index)


if __name__ == "__main__":
    unittest.main()
