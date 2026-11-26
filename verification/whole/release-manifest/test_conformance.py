import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from conformance import cases, check, unique_object


HERE = Path(__file__).resolve().parent


class ManifestContractTests(unittest.TestCase):
    def test_portable_vectors_and_both_native_oracles(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "vectors.json"
            subprocess.run([sys.executable, "-B", HERE / "conformance.py", "vectors", path], check=True)
            self.assertEqual(path.read_bytes(), (HERE / path.name).read_bytes())
        observations = []
        for host in ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"):
            with self.subTest(host=host):
                receipt = json.loads((HERE / f"{host}.json").read_bytes(), object_pairs_hook=unique_object)
                self.assertIn(f"host: {host}", receipt.pop("compiler"))
                check(receipt["baseline"], cases())
                self.assertEqual(receipt["changedCases"], ["size-3-16777217"])
                observations.append(receipt)
        self.assertEqual(*observations)

    def test_malformed_observations(self):
        fixtures = {"sample": (b"x", {"count": 1, "enabled": False})}
        for rows in ([], [{"case": "sample", "result": {"count": True, "enabled": 0}}],
                     [{"case": "other", "result": fixtures["sample"][1]}],
                     [{"case": "sample", "result": fixtures["sample"][1], "extra": 1}]):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                check(rows, fixtures)
        with self.assertRaisesRegex(ValueError, "duplicate JSON member"):
            json.loads('{"id":1,"id":1}', object_pairs_hook=unique_object)


if __name__ == "__main__":
    unittest.main()
