"""Qualify current generated status alongside the independent original harness."""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

from observe import fake_peer, frame


HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
INPUTS = PORTABLE / "native-status-v1"


def run(*argv):
    subprocess.run(argv, check=True, timeout=60)


def observe(binary, args, directory):
    result = subprocess.run(
        [binary, *args], cwd=directory, input=b"", capture_output=True, timeout=5,
        env={"PATH": "/usr/bin:/bin", "LANG": "C", "HOME": str(directory), "TMPDIR": str(directory)},
    )
    return result.returncode, result.stdout, result.stderr


def guard_checks(original, candidate):
    cases = [
        (["--foreground", "--max-commands", "1", "--new-session", "--tor", "invalid"], "foreground_operation"),
        (["--max-commands", "1", "--new-session"], "server_limit_operation"),
        (["--max-uptime", "1h", "--new-session"], "server_limit_operation"),
        (["--new-session", "--tor", "invalid"], "new_session_operation"),
        (["--tor", "invalid"], "route_mode"),
    ]
    with tempfile.TemporaryDirectory(prefix="odd-status-", dir="/tmp") as name:
        root = Path(name).resolve()
        common = ["--state-dir", root, "--socket", root / "unopened.sock", "status"]
        for opts, label in cases:
            expected = (1, b"", f"marmish: invalid input: {label}\n".encode())
            for program in (original, candidate):
                actual = observe(program, [*common, *opts], root)
                if actual != expected:
                    raise ValueError(f"unexpected status guard response for {opts!r}: {actual!r}")
        # The bounded replacement recognizes status but leaves these lanes open.
        gaps = ((["--socket", root / "unopened.sock"], "state directory"),
                (["--state-dir", root], "socket"))
        for opts, gap in gaps:
            expected = (125, b"", f"marmish: explicit {gap} required by this contract\n".encode())
            actual = observe(candidate, ["status", *opts], root)
            if actual != expected:
                raise ValueError(f"unexpected uncovered status lane: {actual!r}")
    return len(cases), len(gaps)


def unsupported_reply_checks(original, candidate):
    cases = [
        ("positional-response", b"[true,null,null]", ""),
        ("non-null-command", b'{"ok":true,"command":{"state":"local_busy","extra":1}}',
         ',"command":{"state":"local_busy"}'),
        ("non-null-stream", b'{"ok":true,"stream":{"state":"pending"}}',
         ',"stream":{"state":"pending"}'),
    ]
    rows = {}
    with tempfile.TemporaryDirectory(prefix="odd-status-types-", dir="/tmp") as name:
        root = Path(name).resolve()
        path = root / "s"
        env = {"HOME": str(root), "TMPDIR": str(root), "PATH": "/usr/bin:/bin", "LANG": "C"}
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(path))
            listener.listen(1)
            listener.settimeout(4)
            for label, body, extension in cases:
                original_row, candidate_row = (
                    fake_peer(binary, listener, root, path, env, frame(body))
                    for binary in (original, candidate)
                )
                prefix = '{"ok":true,"result":null,"error":null'
                expected_original = {"exit": 0, "stdout": prefix + extension + "}\n", "stderr": ""}
                expected_candidate = {"exit": 1, "stdout": "", "stderr": "marmish: invalid input: ipc_frame_json\n"}
                if (original_row, candidate_row) != (expected_original, expected_candidate):
                    raise ValueError(f"difference fixture changed at {label}")
                rows[label] = {"original": original_row, "candidate": candidate_row}
    return rows


def parser_counts(bins):
    output = subprocess.check_output(
        ["cargo", "+1.96.1", "run", "--quiet", "--locked", "--offline",
         "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
         "--bin", "cli-transcript", "--", "verify", bins], text=True, timeout=60,
    )
    counts = ast.literal_eval(output)
    if set(counts) != {"marmish", "marmishd"} or any(not isinstance(count, int) for count in counts.values()):
        raise ValueError("invalid native parser replay counts")
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original_bins", type=lambda value: Path(value).resolve())
    parser.add_argument("generated_bins", type=lambda value: Path(value).resolve())
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    candidate = args.generated_bins / "marmish"
    revision = subprocess.check_output(
        ["git", "-C", HERE, "rev-parse", "HEAD"], text=True,
    ).strip()
    if subprocess.check_output(["git", "-C", HERE, "status", "--porcelain", "--untracked-files=all"]):
        raise ValueError("companion qualification requires a clean source revision")
    with tempfile.TemporaryDirectory(prefix="odd-status-evidence-") as temp:
        mixed = Path(temp) / "mixed.json"
        run(sys.executable, HERE / "observe.py", args.original_bins, mixed, "--candidate", candidate)
        evidence = json.loads(mixed.read_bytes())
    if evidence["candidateSha256"] != hashlib.sha256(candidate.read_bytes()).hexdigest():
        raise ValueError("independent observation did not bind the generated client")
    original = args.original_bins / "marmish"
    guards, uncovered_lanes = guard_checks(original, candidate)
    evidence.update(
        frameworkRevision=args.framework_revision,
        companionRevision=revision,
        cleanCompanion=True,
        workspaceManifestSha256=hashlib.sha256((PORTABLE / "native-status-workspace.json").read_bytes()).hexdigest(),
        platform="aarch64-apple-darwin",
        graphSha256=hashlib.sha256((INPUTS / "graph.json").read_bytes()).hexdigest(),
        packageLockSha256=hashlib.sha256((INPUTS / "package.lock").read_bytes()).hexdigest(),
        workspaceLockSha256=hashlib.sha256((INPUTS / "workspace.lock").read_bytes()).hexdigest(),
        parserObservations=parser_counts(args.generated_bins),
        orderedGuardsCompared=guards,
        uncoveredLanes=uncovered_lanes,
        observedDifferences=unsupported_reply_checks(original, candidate),
        unqualified=[
            "default socket discovery and profile validation",
            "installed executable validation, process-limit initialization and private startup routing",
            "the production-length status deadline against a stalled peer",
            "native status server generation and other command effects",
            "non-null typed command and stream replies",
            "typed tuple and list representations outside the observed JSON status result",
            "different-UID peer behavior in a mixed-process acceptance run",
        ],
    )
    args.receipt.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
