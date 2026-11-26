import os
import platform
import signal
import subprocess
import unittest

from isolate import SYSTEM_HOME, Workspace


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

    def test_engine_account_directory_stat(self):
        with Workspace() as workspace:
            peer = workspace.instance("server")
            stat = peer.run(["/usr/bin/stat", "-f", "%HT", SYSTEM_HOME])
            self.assertEqual((stat.returncode, stat.stdout, stat.stderr), (0, b"Directory\n", b""))
            protected = peer.run(["/usr/bin/stat", "-f", "%HT", SYSTEM_HOME / "Library"])
            self.assertNotEqual(protected.returncode, 0)
            self.assertEqual(protected.stdout, b"")
            listing = peer.run(["/bin/ls", "-A", SYSTEM_HOME])
            self.assertNotEqual(listing.returncode, 0)
            self.assertEqual(listing.stdout, b"")

    def test_process_exec_requires_admitted_binary(self):
        with Workspace() as workspace:
            peer = workspace.instance("server")
            denied = peer.run(["/usr/bin/true"])
            self.assertNotEqual(denied.returncode, 0)
            peer.allow_executables(("/usr/bin/true",))
            self.assertEqual(peer.run(["/usr/bin/true"]).returncode, 0)

    def test_success_cleanup(self):
        with Workspace() as workspace:
            root = workspace.root
            for role in ("client", "server", "relay"):
                self.assertTrue(workspace.instance(role).root.is_dir())
        self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
