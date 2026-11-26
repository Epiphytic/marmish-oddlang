import os
import platform
import signal
import subprocess
import unittest

from isolate import Workspace


@unittest.skipUnless(platform.system() == "Darwin", "macOS sandbox is required")
class WorkspaceTests(unittest.TestCase):
    def check_cleanup(self, reason):
        with self.assertRaises(reason):
            with Workspace() as workspace:
                root = workspace.root
                instance = workspace.instance("peer")
                child = instance.start(["/bin/sleep", "30"])
                if reason is InterruptedError:
                    os.kill(os.getpid(), signal.SIGTERM)
                if reason is subprocess.TimeoutExpired:
                    instance.run(["/bin/sleep", "30"], timeout=0.01)
                raise reason("injected failure")
        self.assertFalse(root.exists())
        self.assertIsNotNone(child.poll())

    def test_failure_cleanup(self):
        self.check_cleanup(ValueError)

    def test_timeout_cleanup(self):
        self.check_cleanup(subprocess.TimeoutExpired)

    def test_interruption_cleanup(self):
        self.check_cleanup(InterruptedError)

    def test_success_cleanup(self):
        with Workspace() as workspace:
            root = workspace.root
            for role in ("client", "server", "relay"):
                self.assertTrue(workspace.instance(role).root.is_dir())
        self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
