"""Artifact substitution must fail before use."""
import json
import os
from pathlib import Path
import tempfile
from subprocess import CompletedProcess
from threading import Event
import unittest
from unittest.mock import Mock

from corpus import SHORT_CASES, digest, expected_receipt
from observe import copy_verified, drain, reference_commands


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

    def test_single_foreground_probe_matches_receiver(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            short = root / "short"
            short.write_bytes(b"fixture-image")
            receipts = root / "receipts"
            receipts.mkdir()
            case = SHORT_CASES[0]
            nonce = digest(b"success:0").encode()
            arguments = (b"success", nonce)
            (receipts / f"{nonce.decode()}.receipt").write_text(
                json.dumps(expected_receipt(case, short, arguments)))
            client = Mock()
            client.run.return_value = CompletedProcess([], case.status, case.stdout, case.stderr)
            attempts = []
            reference_commands(attempts, client, ["client"], "relay", "target", "unused",
                               (short, root / "finite"), first_foreground_only=True)
            client.run.assert_called_once_with(
                ["client", "--relay", "relay", "--stdin", "target", "--", short, *arguments],
                case.stdin)
            self.assertTrue(attempts[0]["server_receipt_verified"])
            self.assertTrue(attempts[0]["matched"])


if __name__ == "__main__":
    unittest.main()
