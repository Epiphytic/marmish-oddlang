"""Compare native implicit-state and persisted-config initialization of an encrypted journal."""

import argparse
import hashlib
import hmac
import json
import os
import platform
from pathlib import Path
import subprocess
import tempfile
import time

from file_profile_probe import material


def run(binary, args, env, cwd):
    result = subprocess.run(
        [binary, *args],
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=90,
        umask=0o077,
    )
    return result.returncode, result.stdout.decode(), result.stderr.decode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("inspector", type=Path)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    original, candidate, inspector = (
        path.resolve() for path in (args.original, args.candidate, args.inspector)
    )
    baseline = Path(__file__).with_name(
        "macos-arm64.json" if platform.system() == "Darwin" else "linux-arm64.json"
    )

    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    assert (
        digest(original)
        == json.loads(baseline.read_bytes())["originalBinaries"]["marmish"]
    )
    cases = (
        ("home", "relative-xdg")
        if platform.system() == "Darwin"
        else ("home", "relative-xdg", "absolute-xdg")
    )
    completed = []
    with tempfile.TemporaryDirectory(prefix="ci-", dir="/tmp") as temp:
        work = Path(temp).resolve()
        for case in cases:
            for label, binary in (("original", original), ("candidate", candidate)):
                root = work / (case + "-" + label)
                home, runtime = root / "home", root / "tmp"
                home.mkdir(mode=0o700, parents=True)
                runtime.mkdir(mode=0o700)
                env = {"HOME": str(home), "TMPDIR": str(runtime), "PATH": os.defpath}
                if case == "absolute-xdg":
                    env["XDG_DATA_HOME"] = str(root / "xdg")
                elif case == "relative-xdg":
                    env["XDG_DATA_HOME"] = "ignore/relative/xdg"
                setup = run(
                    original,
                    [
                        "--key-storage",
                        "file",
                        "--relay",
                        "wss://127.0.0.1:9",
                        "--tor",
                        "off",
                        "setup",
                        "--role",
                        "client",
                        "--no-grants",
                    ],
                    env,
                    root,
                )
                assert setup[0] == 0 and not setup[2], setup
                if platform.system() == "Darwin":
                    profile = home / "Library/Application Support/Marmish/v2/default"
                elif case == "absolute-xdg":
                    profile = root / "xdg/marmish/v2/default"
                else:
                    profile = home / ".local/share/marmish/v2/default"
                manifest, keys = material(profile, "ready")
                config = profile / "config.json"
                config_before = config.read_bytes()
                secret = (
                    hmac.digest(
                        hmac.digest(b"marmish/v2/storage", keys[1], "sha256"),
                        b"journal\1",
                        "sha256",
                    )
                    .hex()
                    .encode()
                )

                def disk(*options):
                    result = subprocess.run(
                        [inspector, profile / "journal.db", *options],
                        input=secret,
                        capture_output=True,
                        check=True,
                        timeout=30,
                    )
                    return json.loads(result.stdout) if result.stdout else None

                default_settings = disk()["settings"]
                disk("seed")
                before = disk()
                assert before["binding"] is None
                assert (
                    before["received"],
                    before["pending"],
                    before["incoming"],
                    before["uncertain"],
                ) == (1, 2, 3, 0)
                start = int(time.time())
                actual = run(binary, ["init"], env, root)
                expect = {
                    field: before[field]
                    for field in ("identity", "conversations", "received", "pending")
                }
                assert actual == (
                    0,
                    json.dumps(expect, separators=(",", ":")) + "\n",
                    "",
                ), (case, label, actual)
                after = disk()
                assert after["uncertain"] == 1
                for key in ("floor", "commandFloor"):
                    assert start - 604800 <= after[key] <= int(time.time()) - 604800
                first_party_binding = [1]
                for role in ("root", "key"):
                    raw = (
                        f"marmish/v2/command-engine/{role}/1\0".encode()
                        + manifest["profile_id"].encode()
                        + bytes.fromhex(manifest["public_key"])
                    )
                    first_party_binding.append(hashlib.sha256(raw).hexdigest().upper())
                assert (
                    after["binding"] == first_party_binding
                    and after["settings"] == default_settings
                )
                assert (
                    material(profile, "ready") == (manifest, keys)
                    and config.read_bytes() == config_before
                )
                assert not list(runtime.iterdir())
                other = candidate if label == "original" else original
                reused = run(other, ["init"], env, root)
                assert reused == actual, (case, label, reused)
                second = disk()
                for key in ("floor", "commandFloor"):
                    assert second[key] >= after[key]
                stable_fields = set(after) - {"floor", "commandFloor"}
                assert all(second[key] == after[key] for key in stable_fields)
                after = second
                completed.append(
                    {
                        "case": case,
                        "implementation": label,
                        "counts": [
                            after[key]
                            for key in ("conversations", "received", "pending")
                        ],
                        "incomingRows": after["incoming"],
                        "preparedReclassified": after["uncertain"],
                        "version": after["version"],
                    }
                )
                for bad_bytes, message in (
                    (b"{", "marmish: invalid input: config_schema\n"),
                    (
                        config_before.replace(
                            b'"version":1}', b'"version":1,"version":1}', 1
                        ),
                        "marmish: invalid input: config_schema\n",
                    ),
                    (b" " * 1048577, "marmish: invalid input: config_size\n"),
                ):
                    config.write_bytes(bad_bytes)
                    result = run(binary, ["init"], env, root)
                    assert result == (1, "", message), (case, label, result)
                    assert disk() == after
                config.write_bytes(config_before)
                config.chmod(0o644)
                result = run(binary, ["init"], env, root)
                assert result == (1, "", "marmish: identity: unsafe_profile_file\n"), (
                    case,
                    label,
                    result,
                )
                config.chmod(0o600)
                assert (
                    material(profile, "ready") == (manifest, keys) and disk() == after
                )
                if label == "candidate":
                    disk("stale")
                    stale = disk()
                    result = run(binary, ["init"], env, root)
                    assert result == (
                        125,
                        "",
                        "marmish: stored profile outside qualified initialization scope\n",
                    ), result
                    assert disk() == stale and material(profile, "ready") == (
                        manifest,
                        keys,
                    )
    no_home = {"PATH": os.defpath}
    for binary in (original, candidate):
        actual = run(binary, ["init"], no_home, Path("/tmp"))
        assert actual == (1, "", "marmish: invalid input: home_directory\n"), actual
    args.receipt.write_text(
        json.dumps(
            {
                "platform": platform.system() + "-" + platform.machine(),
                "frameworkRevision": args.framework_revision,
                "originalBinarySha256": digest(original),
                "controls": [
                    "bidirectional reuse after first durable inspection",
                    "malformed config",
                    "duplicate policy field",
                    "oversized config",
                    "unsafe config mode",
                    "missing HOME",
                    "stale rows refuse counted success",
                ],
                "candidateBinarySha256": digest(candidate),
                "cases": completed,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
