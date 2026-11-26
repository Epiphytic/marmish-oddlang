"""Observe a pinned foreground owner's direct relay admission and TLS failure."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import ssl
import subprocess
import tempfile

PIN = "af54339d250290490d62d4475a77539b7e739071"
ACTIVATING = '{"ok":true,"result":{"state":"activating"},"error":null}\n'
SHUTDOWN = ('{"schema":1,"engine_quiesced":false,"engine_stop_complete":true,'
            '"mdk_stop_complete":true,"runtime_cleanup_complete":true}\n')


def check(actual, expected, label):
    if actual != expected:
        raise ValueError(f"{label}: expected {expected!r}, observed {actual!r}")


def result(exit_code, stdout, stderr):
    return {"exit": exit_code, "stdout": stdout, "stderr": stderr}


def execute(args, env, root):
    child = subprocess.run(args, env=env, cwd=root, capture_output=True, text=True,
                           stdin=subprocess.DEVNULL, timeout=20, umask=0o077)
    return result(child.returncode, child.stdout, child.stderr)


def certificate(root):
    config = root / "openssl.cnf"
    config.write_text("[req]\nprompt=no\ndistinguished_name=subject\nx509_extensions=certificate\n"
                      "[subject]\nCN=127.0.0.1\n[certificate]\nsubjectAltName=IP:127.0.0.1\n"
                      "basicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\n"
                      "keyUsage=critical,digitalSignature,keyEncipherment\n")
    cert, key = root / "cert.pem", root / "key.pem"
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                    "-config", config, "-out", cert, "-keyout", key], capture_output=True,
                   check=True, timeout=30, umask=0o077)
    subprocess.run(["openssl", "verify", "-purpose", "sslserver", "-verify_ip", "127.0.0.1",
                    "-CAfile", cert, cert], capture_output=True, check=True, timeout=10)
    server = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    server.load_cert_chain(cert, key)
    return server


def observe(binaries):
    with tempfile.TemporaryDirectory(dir="/tmp", prefix="odtls-") as tmp:
        root = Path(tmp).resolve()
        runtime = root / "tmp"
        runtime.mkdir(mode=0o700)
        env = {"HOME": str(root), "TMPDIR": str(runtime), "LANG": "C", "PATH": "/usr/bin:/bin"}
        cli, daemon = binaries / "marmish", binaries / "marmishd"
        server = certificate(root)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(15)
            address = f"127.0.0.1:{listener.getsockname()[1]}"
            start = [daemon, "--foreground", "--tor", "off", "--key-storage", "file", "--state-dir"]
            plain_state = root / "plaintext-state"
            plain_state.mkdir(mode=0o700)
            plain = execute([*start, plain_state, "--relay", f"ws://{address}"], env, root)
            check(plain, result(1, "", "marmish: invalid input: plaintext_relay\n"), "direct plaintext")
            state = root / "tls-state"
            state.mkdir(mode=0o700)
            child = subprocess.Popen([*start, state, "--relay", f"wss://{address}"], env=env,
                                     stdin=subprocess.DEVNULL, cwd=root, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True, umask=0o077)
            try:
                conn, _ = listener.accept()
                with conn:
                    conn.settimeout(10)
                    # Peeking leaves all bytes available for the real TLS handshake.
                    typ = conn.recv(1, socket.MSG_PEEK)
                    check(typ, b"\x16", "TLS handshake record")
                    endpoints = [p for p in runtime.glob("mr2-*/s") if p.is_socket()]
                    check(len(endpoints), 1, "activation socket count")
                    status = execute([cli, "--state-dir", state, "status"], env, root)
                    check(status, result(0, ACTIVATING, ""), "discoverable activation")
                    try:
                        with server.wrap_socket(conn, server_side=True):
                            raise ValueError("the original trusted the private test certificate")
                    except ssl.SSLError as error:
                        if error.reason not in {"SSLV3_ALERT_CERTIFICATE_UNKNOWN", "TLSV1_ALERT_UNKNOWN_CA"}:
                            raise
                # A later attempt cannot occupy a pending TCP backlog.
                listener.close()
                stdout, stderr = child.communicate(timeout=25)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.communicate()
            terminal = result(child.returncode, stdout, stderr)
            check(terminal, result(1, SHUTDOWN, "marmish: transport: relay_connect\n"), "failed activation")
            check(endpoints[0].exists(), False, "activation socket cleanup")
    return {"plaintext": plain, "observedTlsHandshakeRecord": True,
            "statusWhileTlsPending": status, "privateCertificateRejected": True,
            "listenerClosedAfterFirstHandshake": True, "terminal": terminal, "socketRemoved": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original_bin_dir", "bridge_receipt", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    record = json.loads(args.bridge_receipt.read_bytes())
    check(record["commit"], PIN, "reference revision")
    names = {"marmish", "marmishd", "marmish-engine-bridge", "marmish-engine-worker",
             "marmish-worker", "marmish-keychain", "marmish-release-tool"}
    digests = record["builds"]["default"]["sha256"]
    check(set(digests), names, "default release executables")
    binaries = args.original_bin_dir.resolve()
    for name in names:
        check(hashlib.sha256((binaries / name).read_bytes()).hexdigest(), digests[name], name)
    data = observe(binaries)
    data.update(originalRevision=PIN, originalBinaries=digests,
                bridgeReceiptSha256=hashlib.sha256(args.bridge_receipt.read_bytes()).hexdigest())
    args.output.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
