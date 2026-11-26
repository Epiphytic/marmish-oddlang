"""Compare the native release-info contract independently of the projection."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib

PIN = "af54339d250290490d62d4475a77539b7e739071"
ROLES = ("marmish", "marmishd")
ORIGINAL_FIELDS = (
    ("profile", "crates/marmish-native/src/identity.rs", "PROFILE_FORMAT", "u8"),
    ("journal", "crates/marmish-native/src/journal.rs", "JOURNAL_FORMAT", "i64"),
    ("engine", "crates/marmish-engine/src/store.rs", "ENGINE_FORMAT", "i64"),
    ("configuration", "crates/marmish-native/src/command_config.rs", "CONFIG_FORMAT", "u32"),
    ("command", "crates/marmish-native/src/command_wire.rs", "COMMAND_FORMAT", "u32"),
)
CASES = (
    ("release-info",),
    ("--state-dir", "absent", "release-info"),
    ("release-info", "--state-dir", "absent"),
    ("--key-storage", "file", "--tor", "off", "--relay", "unused", "release-info"),
    ("--foreground", "release-info"),
    ("release-info", "--new-session"),
    ("--max-commands", "1", "release-info"),
    ("release-info", "--max-commands", "invalid"),
    ("release-info", "--max-uptime", "invalid"),
    ("--new-session", "--max-uptime", "1", "release-info"),
    ("--new-session", "--max-commands", "1", "release-info"),
    ("--foreground", "--max-commands", "1", "--new-session", "release-info"),
    ("release-info", "--not-an-option"),
    ("release-info", "--max-commands"),
    ("release-info", "extra"),
    ("release-info", "--help"),
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inspect_formats(original, portable):
    configured = json.loads((portable / "native-configured-init-v1/graph.json").read_bytes())
    generated = {
        "profile": configured["manifest"]["version"],
        "journal": configured["initialize"]["journal"]["version"],
        "configuration": configured["initialize"]["configured"]["fixed"]["version"],
    }
    facts = []
    for role, relative, symbol, declared_type in ORIGINAL_FIELDS:
        path = original / relative
        pattern = rf"\bconst {symbol}: {declared_type} = ([0-9]+);"
        value, = map(int, re.findall(pattern, path.read_text()))
        if not 0 <= value < 2**32 or (declared_type == "u8" and value >= 2**8):
            raise ValueError(f"format out of range: {symbol}")
        if role in generated and value != generated[role]:
            raise ValueError(f"generated format differs: {role}")
        facts.append(dict(role=role, source=relative, sourceSha256=digest(path),
                          identifier=symbol, declaredType=declared_type, value=value,
                          generatedValue=generated.get(role)))
    return facts


def observe(original, portable, original_binaries, candidate_binaries, target):
    source = (original / "crates/marmish-native/Cargo.toml").read_text()
    version = tomllib.loads(source)["package"]["version"]
    facts = inspect_formats(original, portable)
    expected = dict(schema=1, version=version, target=target, commit=PIN,
                    formats=[fact["value"] for fact in facts])
    report = json.dumps(expected, separators=(",", ":")) + "\n"
    result = {}
    with tempfile.TemporaryDirectory(prefix="release-info-") as temp:
        root = Path(temp)
        env = dict(HOME=temp, TMPDIR=temp, PATH=os.defpath, LANG="C", CARGO_PKG_VERSION="runtime-override",
                   TARGET="runtime-override", MARMISH_BUILD_COMMIT="runtime-override",
                   MARMISH_BUILD_TARGET="runtime-override")

        def capture(binary, argv):
            call = subprocess.run([binary, *argv], cwd=root, env=env, input=b"",
                                  capture_output=True, timeout=30, umask=0o077)
            return dict(exit=call.returncode, stdout=call.stdout.decode(), stderr=call.stderr.decode())

        for role in ROLES:
            cases = []
            for index, argv in enumerate(CASES):
                reference = capture(original_binaries / role, argv)
                candidate = capture(candidate_binaries / role, argv)
                if reference != candidate:
                    raise ValueError(f"{role} {argv!r} diverged: {reference!r} != {candidate!r}")
                if index < 4 and reference != dict(exit=0, stdout=report, stderr=""):
                    raise ValueError(f"{role} reports an unexpected build identity: {reference!r}")
                cases.append(dict(argv=argv, **reference))
            result[role] = cases
        if list(root.iterdir()):
            raise ValueError("release-info created runtime state")
    return dict(cases=result, formats=facts,
                originalBinaries={name: digest(original_binaries / name) for name in ROLES},
                generatedBinaries={name: digest(candidate_binaries / name) for name in ROLES})
