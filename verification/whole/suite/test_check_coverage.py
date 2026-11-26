"""Check that changing evidence in a published receipt cannot increase coverage."""
from copy import deepcopy
import json
import subprocess
import sys
import unittest

import selected_tests
import parser_cases

from check_coverage import CORE, HERE, NATIVE, STREAM, inspect


class CoverageControls(unittest.TestCase):
    def test_each_native_receipt(self):
        for host in ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"):
            data = json.loads((HERE / f"combined-v7-{host}.json").read_bytes())
            with self.subTest(host=host):
                self.assertEqual(inspect(data)["passed"], 37)
            other = next(k for k,row in data["harnesses"].items() if row["unverified"])
            ignored = next(k for k,row in data["harnesses"].items() if row["ignored"])
            for label, alter in {
                "borrow later partial witness": lambda d: d.update(selectedTestSources=selected_tests.EXPECTED),
                "missing harness": lambda d: d["harnesses"].pop(other),
                "unqualified case": lambda d: d["harnesses"][other]["passed"].append(d["harnesses"][other]["unverified"].pop()),
                "duplicate pass": lambda d: d["harnesses"][CORE]["passed"].append(d["harnesses"][CORE]["passed"][0]),
                "ignored case": lambda d: d["harnesses"][ignored]["passed"].append(d["harnesses"][ignored]["ignored"].pop()),
                "missing stream test": lambda d: d["harnesses"][STREAM]["passed"].pop(),
                "substitute executable": lambda d: d["nativeProcessTest"]["executables"].update(marmish="0"*64),
                "substitute module": lambda d: d["upstreamModules"].update(wire="0"*64),
                "wrong harness": lambda d: d["harnesses"][NATIVE]["passed"].append("unrelated_test"),
            }.items():
                with self.subTest(host=host, change=label):
                    changed = deepcopy(data)
                    alter(changed)
                    with self.assertRaises(ValueError):
                        inspect(changed)


    def test_recovered_receipts(self):
        for host, expected in (("aarch64-apple-darwin",39), ("aarch64-unknown-linux-gnu",40)):
            data = json.loads((HERE / f"recovered-{host}.json").read_bytes())
            self.assertEqual(inspect(data)["passed"], expected)
            engine = selected_tests.ENGINE
            for label, alter in {
                "upgrade a subset to a complete harness": lambda d: d["harnesses"][engine]["passed"].append(d["harnesses"][engine]["unverified"][0]),
                "absent source seal": lambda d: d.pop("selectedTestSources"),
                "missing child": lambda d: d["passingProcesses"][engine].update({selected_tests.LOW: 1}),
                "wrong host selection": lambda d: d["harnesses"][engine]["passed"].append(selected_tests.LINUX),
                "substitute private helper bytes": lambda d: d["selectedTestSources"].update(engineLimitHelpers="0"*64),
                "omit vault case": lambda d: d["harnesses"][selected_tests.VAULT]["passed"].clear(),
                "borrow the predecessor compiler": lambda d: d.update(framework="7ed1c29acde2cc4cfdc58c2489935e703470ae70"),
            }.items():
                with self.subTest(host=host, change=label):
                    changed = deepcopy(data)
                    alter(changed)
                    with self.assertRaises(ValueError):
                        inspect(changed)


    def test_seven_member_receipts(self):
        for host, expected in (("aarch64-apple-darwin",40), ("aarch64-unknown-linux-gnu",41)):
            data = json.loads((HERE/f"seven-{host}.json").read_bytes())
            self.assertEqual(inspect(data)["passed"],expected)
            for label, alter in {
                "missing reader": lambda d: d["harnesses"][selected_tests.PROTOTYPE]["passed"].clear(),
                "promote untested prototype case": lambda d: d["harnesses"][selected_tests.PROTOTYPE]["passed"].extend(d["harnesses"][selected_tests.PROTOTYPE]["unverified"]),
                "wrong upstream reader": lambda d: d["selectedTestSources"].update(prototypeReader="0"*64),
                "missing physical MDK library": lambda d: d["builtLibraries"].pop("marmish-mdk"),
                "extra external library": lambda d: d["builtLibraries"].update(zeroize="1"*64),
                "incomplete library digest": lambda d: d["builtLibraries"].update({"marmish-prototype":"1234"}),
            }.items():
                with self.subTest(host=host,change=label):
                    changed = deepcopy(data)
                    alter(changed)
                    with self.assertRaises(ValueError):
                        inspect(changed)


    def test_parser_receipts(self):
        for host, total in (("aarch64-apple-darwin", 42), ("aarch64-unknown-linux-gnu", 43)):
            receipt = json.loads((HERE / f"manifest-{host}.json").read_bytes())
            self.assertEqual(inspect(receipt)["passed"], total)
            for label, change in {
                "modified included vector": lambda r: r["selectedTestSources"].update(releaseVersionVectors="0"*64),
                "missing test": lambda r: r["harnesses"][parser_cases.HARNESS]["passed"].pop(),
                "unexecuted case": lambda r: r["passingProcesses"][parser_cases.HARNESS].update(
                    {parser_cases.NAMES[0]: 0}),
            }.items():
                with self.subTest(host=host, label=label):
                    mutated = deepcopy(receipt)
                    change(mutated)
                    with self.assertRaises(ValueError):
                        inspect(mutated)

    def test_partial_parser_receipts_reject_full_certification(self):
        receipts = [HERE / f"{candidate}-{host}.json"
                    for candidate in ("manifest", "canonical")
                    for host in ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")]
        for receipt in receipts:
            result = subprocess.run(
                [sys.executable, "-B", HERE/"check_coverage.py", receipt, "--require-complete"],
                capture_output=True, text=True)
            with self.subTest(receipt=receipt.name):
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertGreater(json.loads(result.stdout)["unverified"], 0)

    def test_parser_case_requires_one_actual_execution(self):
        def no_match(*args, **kwargs):
            return b"running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored;"

        with self.assertRaises(ValueError):
            parser_cases.witness(no_match, HERE, "native_manifest::__projection_10",
                                 lambda _: [], selected_tests.check_execution)


if __name__ == "__main__":
    unittest.main()
