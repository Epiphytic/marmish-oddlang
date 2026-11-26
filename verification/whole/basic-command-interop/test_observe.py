"""Artifact substitution must fail before use."""
import os
from pathlib import Path
import tempfile
from threading import Event
import unittest

from corpus import digest
from observe import copy_verified, drain


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

    def test_pipe_drain_can_stop_while_writer_remains_open(self):
        descriptor, writer = os.pipe()
        stop = Event()
        try:
            with os.fdopen(descriptor, "rb") as stream:
                _, reader = drain(stream, stop)
            # Closing the buffered stream does not wait for the reader or writer.
            stop.set()
            reader.join(timeout=1)
            self.assertFalse(reader.is_alive())
        finally:
            os.close(writer)


if __name__ == "__main__":
    unittest.main()
