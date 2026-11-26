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


def nodes(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child)


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

    def graph_for(self, cell):
        return self.archive / "graphs" / f'{cell["graphSha256"]}.json'

    def update_lock(self, cell, lock):
        path = self.archive / "locks" / f"{key(cell)}.json"
        write(path, lock)
        cell.update(lockSha256=sha(path), semanticLockSha256=semantic_lock_sha256(lock))

    def update_graph(self, path, graph):
        old_digest = path.stem
        write(path, graph)
        digest = sha(path)
        path.rename(path.with_name(f"{digest}.json"))
        for cell in self.manifest["cells"]:
            if cell.get("graphSha256") == old_digest:
                cell["graphSha256"] = digest
                lock = read(self.archive / "locks" / f"{key(cell)}.json")
                lock["shapesSha256"] = digest
                self.update_lock(cell, lock)
        write(self.release, self.manifest)

    def stream_graph(self):
        cell = next(row for row in self.manifest["cells"] if row["target"] == "marmish_stream"
                    and row["triple"] == "aarch64-apple-darwin")
        path = self.graph_for(cell)
        return path, read(path)

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
        self.update_graph(self.graph, graph)
        with self.assertRaisesRegex(ValueError, "invalid field order"):
            self.verify()


    def test_missing_or_null_source_revision_even_with_updated_digests(self):
        linux = next(cell for cell in self.manifest["cells"] if "linux" in cell["triple"]
                     and cell["status"] == "candidate")
        for cell in (self.cell, linux):
            path = self.archive / "locks" / f"{key(cell)}.json"
            original_lock = read(path)
            for deleted in (False, True):
                with self.subTest(cell=key(cell), deleted=deleted):
                    lock = {**original_lock, "pin": original_lock["pin"].copy()}
                    if deleted:
                        del lock["pin"]["sourceRevision"]
                    else:
                        lock["pin"]["sourceRevision"] = None
                    self.update_lock(cell, lock)
                    write(self.release, self.manifest)
                    with self.assertRaisesRegex(ValueError, "missing or invalid source revision"):
                        self.verify()
            self.update_lock(cell, original_lock)

    def test_missing_unresolved_gap_even_with_updated_digests(self):
        path, graph = self.stream_graph()
        graph["gaps"] = [gap for gap in graph["gaps"] if gap["reason"] != "array-length"]
        self.update_graph(path, graph)
        with self.assertRaisesRegex(ValueError, "missing required gaps"):
            self.verify()

    def test_unmatched_unresolved_gap_even_with_updated_digests(self):
        graph = read(self.graph)
        declaration = graph["declarations"][0]
        graph["gaps"].append({"owner": declaration["id"], "reason": "array-length"})
        graph["gaps"].sort(key=lambda gap: (gap["owner"], gap["reason"]))
        self.update_graph(self.graph, graph)
        with self.assertRaisesRegex(ValueError, "unmatched unresolved gap"):
            self.verify()

    def test_unknown_gap_reason_even_with_updated_digests(self):
        graph = read(self.graph)
        graph["gaps"][0]["reason"] = "fabricated-reason"
        graph["gaps"].sort(key=lambda gap: (gap["owner"], gap["reason"]))
        self.update_graph(self.graph, graph)
        with self.assertRaisesRegex(ValueError, "unknown gap reason"):
            self.verify()

    def test_unknown_local_declaration_even_with_updated_digests(self):
        path, graph = self.stream_graph()
        reference = next(node for node in nodes(graph) if node.get("kind") == "named"
                         and node["id"].startswith("local::"))
        reference["id"] = "local::marmish_stream::missing::{type}"
        self.update_graph(path, graph)
        with self.assertRaisesRegex(ValueError, "unknown local declaration"):
            self.verify()

    def test_invalid_integer_width_even_with_updated_digests(self):
        path, graph = self.stream_graph()
        scalar = next(node for node in nodes(graph) if node.get("kind") == "unsigned-integer")
        scalar["bits"] = 7
        self.update_graph(path, graph)
        with self.assertRaisesRegex(ValueError, "invalid unsigned-integer width"):
            self.verify()

    def test_unlisted_root_file(self):
        (self.archive / "extra.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "invalid archive root inventory"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
