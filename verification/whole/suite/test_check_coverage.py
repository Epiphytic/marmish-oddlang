"""Check that changing evidence in a published receipt cannot increase coverage."""
from copy import deepcopy
import json
import unittest

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


if __name__ == "__main__":
    unittest.main()
