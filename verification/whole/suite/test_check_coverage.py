"""Check that changing evidence in a published receipt cannot increase coverage."""
from copy import deepcopy
import json
import unittest

import selected_tests

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


if __name__ == "__main__":
    unittest.main()
