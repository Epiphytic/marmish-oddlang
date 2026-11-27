"""Pinned source identity for the remaining Python suite callers."""
import subprocess

PIN = "af54339d250290490d62d4475a77539b7e739071"

def git(upstream, *args):
    return subprocess.check_output(["git", "-C", str(upstream), *args])
