import os
import platform
import signal
import shutil
import subprocess
import unittest
from unittest.mock import patch

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
            private_binary = peer.root / "private-executable"
            shutil.copyfile("/usr/bin/true", private_binary)
            private_binary.chmod(0o700)
            peer.allow_executables((private_binary, "/usr/bin/touch"))
            self.assertEqual(peer.run([private_binary]).returncode, 0)
            self.assertNotEqual(peer.run(["/usr/bin/touch", private_binary]).returncode, 0)
            self.assertEqual(peer.run(["/usr/bin/touch", peer.root / "writable"]).returncode, 0)

    def test_cleanup_stops_worker_in_a_separate_session(self):
        with patch.object(Workspace, "owned_processes") as processes:
            with Workspace() as workspace:
                peer = workspace.instance("server")
                worker = subprocess.Popen(["/bin/sleep", "30"], cwd=peer.root, start_new_session=True)
                # Model the platform process inventory after a native worker creates a session.
                processes.side_effect = lambda: {worker.pid: "worker"} if worker.poll() is None else {}
            self.assertEqual(worker.returncode, -signal.SIGKILL)
            self.assertEqual(workspace.residual_workers_terminated, 1)

    def test_success_cleanup(self):
        with Workspace() as workspace:
            root = workspace.root
            for role in ("client", "server", "relay"):
                self.assertTrue(workspace.instance(role).root.is_dir())
        self.assertFalse(root.exists())


if __name__ == "__main__":
    unittest.main()
