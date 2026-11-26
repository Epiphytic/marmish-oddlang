#!/usr/bin/env python3
"""Negative controls for the published shape archive using copies of real graphs."""

from pathlib import Path
import shutil
import sys
import tempfile
import unittest

from workspace_shapes import key, read, semantic_lock_sha256, sha, verify, write

ORIGINAL, FRAMEWORK, ARCHIVE = map(Path, sys.argv[1:4])
del sys.argv[1:4]


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="shape-controls-")
        self.addCleanup(temporary.cleanup)
        self.archive = Path(temporary.name) / "shape-archive"
        shutil.copytree(ARCHIVE, self.archive)
        self.release = self.archive / "release.json"
        self.manifest = read(self.release)
        self.cell = next(row for row in self.manifest["cells"]
                         if row["target"] == "marmish_core" and row["status"] == "candidate")
        self.graph = self.archive / "graphs" / f'{self.cell["graphSha256"]}.json'
        self.lock = self.archive / "locks" / f"{key(self.cell)}.json"

    def verify(self):
        verify(ORIGINAL, FRAMEWORK, self.archive)

    def test_target_missing(self):
        self.manifest["targets"].pop()
        write(self.release, self.manifest)
        with self.assertRaisesRegex(ValueError, "target inventory"):
            self.verify()

    def test_cell_missing(self):
        self.manifest["cells"].pop()
        write(self.release, self.manifest)
        with self.assertRaisesRegex(ValueError, "missing or extra profile cells"):
            self.verify()

    def test_graph_missing(self):
        self.graph.unlink()
        with self.assertRaises(FileNotFoundError):
            self.verify()

    def test_graph_digest(self):
        with self.graph.open("ab") as stream:
            stream.write(b" ")
        with self.assertRaisesRegex(ValueError, "graph digest mismatch"):
            self.verify()

    def test_lock_digest(self):
        with self.lock.open("ab") as stream:
            stream.write(b" ")
        with self.assertRaisesRegex(ValueError, "lock digest mismatch"):
            self.verify()

    def test_shared_graph_still_pins_features(self):
        cells = [row for row in self.manifest["cells"] if row["target"] == "marmish_engine"
                 and row["triple"] == "aarch64-apple-darwin"]
        baseline, fault = cells
        self.assertEqual(baseline["graphSha256"], fault["graphSha256"])
        locks = self.archive / "locks"
        wrong_lock = locks / f"{key(fault)}.json"
        shutil.copyfile(wrong_lock, locks / f"{key(baseline)}.json")
        baseline.update(lockSha256=sha(wrong_lock),
                        semanticLockSha256=semantic_lock_sha256(read(wrong_lock)))
        write(self.release, self.manifest)
        with self.assertRaisesRegex(ValueError, "profile pin mismatch"):
            self.verify()

    def test_field_order_even_with_updated_digests(self):
        graph = read(self.graph)
        fields = next(row["fields"] for row in graph["declarations"]
                      if len(row.get("fields", [])) >= 2)
        fields[0], fields[1] = fields[1], fields[0]
        write(self.graph, graph)
        digest = sha(self.graph)
        self.graph.rename(self.graph.with_name(f"{digest}.json"))
        lock = read(self.lock)
        lock["shapesSha256"] = digest
        write(self.lock, lock)
        self.cell.update(graphSha256=digest, lockSha256=sha(self.lock),
                         semanticLockSha256=semantic_lock_sha256(lock))
        write(self.release, self.manifest)
        with self.assertRaisesRegex(ValueError, "invalid field order"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
