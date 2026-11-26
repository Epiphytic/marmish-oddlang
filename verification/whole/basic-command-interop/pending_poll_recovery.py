#!/usr/bin/env python3
"""Verify deferred local resume against the independent retained-poll actor."""
import argparse
from base64 import b64decode, b64encode
from hashlib import sha256
import json
from pathlib import Path
from tempfile import TemporaryDirectory

import pending_poll_reference as pending

progress = pending.progress
ref = pending.ref


def reject(binary, case):
    with TemporaryDirectory(prefix="opn-", dir="/private/tmp") as directory:
        work = Path(directory)
        owner = pending.RetainedPoll()
        command = ["--detach", "--stdin-none", ref.NPUB, "--", "/usr/bin/true"]
        code, stdout, stderr = progress.run_client(binary, work, owner, command, ref.SUBMIT)
        handle = ref.compact(owner.handle)
        line = b"marmish handle: " + handle + b"\n"
        if (code, stdout, stderr) != (0, b"", line):
            raise ValueError("could not establish the exact detached handle")
        manifest = work / "state/default/profile.json"
        manifest.write_bytes(ref.compact(dict(format=2, name="default", phase="ready",
                                              public_key=ref.LOCAL_ACCOUNT)))
        manifest.chmod(0o600)
        pending.stop_with_poll(binary, work, owner, handle)

        outer = json.loads(owner.retained)
        command_reply = outer["command"]
        packet = json.loads(b64decode(command_reply["response_base64"], validate=True))
        expected = pending.RECOVERY[:3]
        diagnostic = b"command_response"
        if case == "wrong_session":
            packet["session"] = "11" * 16
        elif case == "wrong_request_hash":
            packet["body"]["request_sha256"] = "11" * 32
            expected = pending.RECOVERY[:4]
            diagnostic = b"command_cursors"
        elif case == "changed_cursors":
            owner.start = (pending.INITIAL[0] + 1, pending.INITIAL[1])
            expected = pending.RECOVERY[:4]
            diagnostic = b"command_cursors"
        elif case != "corrupt_digest":
            raise ValueError("unknown fixed refusal case")
        payload = ref.compact(packet, sort_keys=True)
        command_reply["response_base64"] = b64encode(payload).decode()
        owner.poll_response_digest = "11" * 32 if case == "corrupt_digest" else sha256(payload).hexdigest()
        command_reply["response_sha256"] = owner.poll_response_digest
        owner.retained = ref.compact(outer)
        prior = owner.start
        result = progress.run_client(binary, work, owner, ["wait", "--handle", handle.decode()], expected)
        if result != (255, b"", line + b"marmish: " + diagnostic + b"\n") or owner.start != prior:
            raise ValueError(f"{case}: expected refusal before output and consume")
    return dict(exit=255, output=False, consumed=False, priorCursors=prior,
                operations=expected, workspaceRemoved=not work.exists())


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("client", type=lambda value: Path(value).resolve(strict=True))
    args.add_argument("build_receipt", type=Path)
    args.add_argument("receipt", type=Path)
    values = args.parse_args()
    build_bytes = values.build_receipt.read_bytes()
    build = json.loads(build_bytes)
    selector = Path(__file__).parents[3] / "oddlang/portable/workspace-retained-poll.json"
    if build["stagedContracts"].get(selector.name) != ref.sha(selector):
        raise ValueError("build receipt selects a different workspace")
    expected = build["artifacts"]["marmish"]["sha256"]
    if ref.sha(values.client) != expected:
        raise ValueError("candidate executable digest differs")
    recovered = pending.observe(values.client, "upstream")
    recovered.pop("mode")
    cases = ("corrupt_digest", "wrong_session", "wrong_request_hash", "changed_cursors")
    result = dict(format="marmish-retained-local-poll-recovery-v1", clientSha256=expected,
                  buildReceiptSha256=sha256(build_bytes).hexdigest(),
                  fixtureSha256=ref.sha(Path(__file__)), restartFixtureSha256=ref.sha(Path(pending.__file__)),
                  recovery=recovered, refusals={case: reject(values.client, case) for case in cases})
    with values.receipt.open("x") as output:
        output.write(json.dumps(result, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
