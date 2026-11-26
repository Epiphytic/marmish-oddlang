"""Regression controls for the checkouts used to qualify the vault package."""
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import verify


class InputPinTest(unittest.TestCase):
    def test_checkout_changes_and_untracked_cargo_config_are_rejected(self):
        with TemporaryDirectory() as tmp:
            repo = Path(tmp)
            def git(*args):
                return subprocess.check_output(["git", "-C", repo, *args], text=True).strip()

            git("init", "-q")
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "--allow-empty", "-qm", "baseline")
            pinned = git("rev-parse", "HEAD")
            inputs = SimpleNamespace(original=repo, framework=repo)
            with patch.object(verify, "FRAMEWORK", pinned), patch.object(verify, "ORIGINAL", pinned):
                verify.check_inputs(inputs)
                config = repo / ".cargo/config.toml"
                config.parent.mkdir()
                config.write_text('[build]\njobs = 1\n')
                with self.assertRaisesRegex(ValueError, "clean checkout"):
                    verify.check_inputs(inputs)
                config.unlink()
                git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "commit", "--allow-empty", "-qm", "moved")
                with self.assertRaisesRegex(ValueError, "clean checkout"):
                    verify.check_inputs(inputs)


if __name__ == "__main__":
    unittest.main()
