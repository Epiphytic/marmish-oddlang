"""Artifact substitution must fail before use."""
from pathlib import Path
import tempfile
import unittest

from corpus import digest
from observe import copy_verified


class ObserveTests(unittest.TestCase):
    def test_rejects_substituted_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            original = Path(tmp) / "original"
            original.write_bytes(b"original")
            other = Path(tmp) / "substitution"
            other.write_bytes(b"substitution")
            target = Path(tmp) / "staged-file"
            copy_verified(original, target, digest(b"original"))
            with self.assertRaisesRegex(ValueError, "artifact digest differs"):
                copy_verified(other, Path(tmp) / "bad-stage", digest(b"original"))


if __name__ == "__main__":
    unittest.main()
