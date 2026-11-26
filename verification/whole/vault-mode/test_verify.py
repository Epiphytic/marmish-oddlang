"""Regression controls for the checkouts used to qualify the vault package."""
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import verify


class InputPinTest(unittest.TestCase):
    def test_private_cargo_home_inherits_only_dependency_caches(self):
        with TemporaryDirectory() as tmp:
            base = Path(tmp).resolve()
            cache = base / "ambient"
            (cache / "registry").mkdir(parents=True)
            (cache / "config.toml").write_text("[build]\njobs = 7\n")
            private = base / "private"
            with patch.dict("os.environ", {"CARGO_HOME": str(cache), "RUSTC_WRAPPER": "injected",
                                           "CARGO_PROFILE_RELEASE_OPT_LEVEL": "0"}):
                env = verify.host_build_env(private)
            self.assertEqual(env["CARGO_HOME"], str(private))
            self.assertNotIn("CARGO_PROFILE_RELEASE_OPT_LEVEL", env)
            self.assertNotIn("RUSTC_WRAPPER", env)
            self.assertEqual(list(private.iterdir()), [private / "registry"])
            self.assertEqual((private / "registry").resolve(), cache / "registry")

    def test_checkout_changes_and_untracked_cargo_config_are_rejected(self):
        with TemporaryDirectory() as tmp:
            repo = Path(tmp) / "checkout"
            repo.mkdir()
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
                with self.assertRaisesRegex(ValueError, "Cargo configuration"):
                    verify.check_inputs(inputs)
                (repo / ".git/info/exclude").write_text(".cargo/\n")
                self.assertEqual(git("status", "--porcelain"), "")
                with self.assertRaisesRegex(ValueError, "Cargo configuration"):
                    verify.check_inputs(inputs)
                config.unlink()
                ancestor = Path(tmp) / ".cargo/config"
                ancestor.parent.mkdir()
                ancestor.touch()
                with self.assertRaisesRegex(ValueError, "Cargo configuration"):
                    verify.check_inputs(inputs)
                ancestor.unlink()
                git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "commit", "--allow-empty", "-qm", "moved")
                with self.assertRaisesRegex(ValueError, "clean checkout"):
                    verify.check_inputs(inputs)


if __name__ == "__main__":
    unittest.main()
