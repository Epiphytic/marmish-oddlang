"""Compare a native status client with a pinned daemon and a controlled Unix peer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
PIN = "af54339d250290490d62d4475a77539b7e739071"
ACTIVATING = '{"ok":true,"result":{"state":"activating"},"error":null}\n'
STATUS = b'{"operation":"status"}'


def frame(body):
    return struct.pack(">I", len(body)) + body


def expect(actual, expected, case):
    if actual != expected:
        raise ValueError(f"{case}: expected {expected!r}, observed {actual!r}")


def process(binary, state, path, env):
    return subprocess.Popen([binary, "--state-dir", state, "--socket", path, "status"],
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            env=env, cwd=state.parent, umask=0o077)


def result(child):
    try:
        out, err = child.communicate(timeout=5)
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate()
    return {"exit": child.returncode, "stdout": out.decode(), "stderr": err.decode()}


def local_daemon(daemon, clients, state, runtime, env):
    with socket.socket() as pending_relay:
        pending_relay.bind(("127.0.0.1", 0))
        pending_relay.listen()
        url = f"wss://127.0.0.1:{pending_relay.getsockname()[1]}"
        # The loopback listener never negotiates TLS. The real daemon serves the activation snapshot.
        owner = subprocess.Popen([daemon, "--foreground", "--state-dir", state, "--key-storage", "file",
                                  "--tor", "off", "--relay", url], stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, env=env,
                                 cwd=state.parent, umask=0o077)
        try:
            deadline = time.monotonic() + 5
            while True:
                paths = [p for p in runtime.glob("mr2-*/s") if p.is_socket()]
                if len(paths) == 1:
                    break
                if owner.poll() is not None or time.monotonic() >= deadline:
                    raise ValueError("original daemon did not expose its private status socket")
                time.sleep(0.01)
            rows = [result(process(client, state, paths[0], env)) for client in clients]
            for row in rows:
                expect(row, {"exit": 0, "stdout": ACTIVATING, "stderr": ""}, "original daemon status")
        finally:
            if owner.poll() is None:
                owner.terminate()
            try:
                shutdown = result(owner)
            except subprocess.TimeoutExpired as exc:
                raise ValueError("original daemon failed to stop in time") from exc
        expect(shutdown["exit"], 0, "daemon SIGTERM")
        cleanup = json.loads(shutdown["stdout"])
        for key in ("engine_stop_complete", "mdk_stop_complete", "runtime_cleanup_complete"):
            expect(cleanup[key], True, key)
        return rows[0], {k: cleanup[k] for k in ("engine_stop_complete","mdk_stop_complete","runtime_cleanup_complete")}


def fake_peer(client, listener, state, path, env, wire):
    child = process(client, state, path, env)
    try:
        conn, _ = listener.accept()
        with conn:
            conn.settimeout(4)
            request = bytearray()
            while True:
                data = conn.recv(128)
                if not data:
                    break
                request.extend(data)
                if len(request) > len(frame(STATUS)):
                    raise ValueError("unexpected extra request bytes")
            expect(bytes(request), frame(STATUS), "one status request followed by write EOF")
            if wire:
                conn.sendall(wire)
        return result(child)
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate()


def observe(daemon, clients):
    def good(out, code=0):
        return {"exit":code,"stdout":out+"\n","stderr":""}
    def error(text):
        return {"exit":1,"stdout":"","stderr":"marmish: "+text+"\n"}
    invalid = error("invalid input: ipc_frame_json")
    invalid_size = error("invalid input: ipc_frame_size")
    io_error = error("local I/O failed")
    minimal = b'{"ok":true}'
    raw = good('{"ok":true,"result":null,"error":null}')
    cases = [
        ("defaults", frame(minimal), raw),
        ("actor-failure", frame(b'{"error":"busy","ok":false}'), good('{"ok":false,"result":null,"error":"busy"}',1)),
        ("null-extensions", frame(b'{"stream":null,"command":null,"ok":true}'), raw),
        ("object-order", frame(b'{"result":{"z":1,"a":[3]},"ok":true}'), good('{"ok":true,"result":{"a":[3],"z":1},"error":null}')),
        ("unknown-field",frame(b'{"ok":true,"other":1}'),invalid),
        ("missing-success",frame(b'{}'),invalid),("wrong-success-type",frame(b'{"ok":1}'),invalid),
        ("duplicate-success",frame(b'{"ok":true,"ok":true}'),invalid),("malformed",frame(b'{'),invalid),
        ("max-frame",frame(minimal.ljust(65536,b' ')),raw),
        ("zero-frame",struct.pack(">I",0),invalid_size),("over-limit",struct.pack(">I",65537),invalid_size),
        ("no-header",b'',io_error),("partial-header",b'\x00\x00',io_error),
        ("short-body",struct.pack(">I",8)+b'{}',io_error),
        ("extra-frame",frame(minimal)+frame(b'{}'),raw),
    ]
    with tempfile.TemporaryDirectory(prefix="ods-",dir="/tmp") as temp:
        root=Path(temp).resolve(); state=root/"state"; runtime=root/"tmp"
        for p in (state,runtime): p.mkdir(mode=0o700)
        env={"LANG":"C","PATH":"/usr/bin:/bin","HOME":str(root),"TMPDIR":str(runtime)}
        real,shutdown = local_daemon(daemon,clients,state,runtime,env)
        path=root/"fixture.sock"
        rows={}
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(path)); listener.listen();listener.settimeout(4)
            for name,wire,expected in cases:
                for client in clients:
                    row=fake_peer(client,listener,state,path,env,wire)
                    expect(row,expected,name)
                rows[name]=row
        for client in clients:
            missing=result(process(client,state,root/"missing",env))
            expect(missing,io_error,"missing socket")
        rows["missing-socket"]=missing
        return {"realDaemon":real,"shutdown":shutdown,"expectedRequestHex":frame(STATUS).hex(),"controlledCases":rows}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("original_bin_dir",type=Path)
    parser.add_argument("receipt",type=Path)
    parser.add_argument("--candidate",type=Path)
    parser.add_argument("--build-receipt",type=Path,default=HERE.parent/"bridge/aarch64-apple-darwin.json")
    args=parser.parse_args()
    record=json.loads(args.build_receipt.read_bytes())
    expect(record["commit"],PIN,"original revision")
    expected=record["builds"]["default"]["sha256"]
    bins={n:(args.original_bin_dir/n).resolve() for n in expected}
    for name,path in bins.items():
        expect(hashlib.sha256(path.read_bytes()).hexdigest(),expected[name],name)
    clients=[bins["marmish"]]
    if args.candidate: clients.append(args.candidate.resolve())
    data=observe(bins["marmishd"],clients)
    data.update(originalRevision=PIN,originalBinaries={n:expected[n] for n in bins},
                mode="original-and-candidate" if args.candidate else "original")
    if args.candidate:
        data["candidateSha256"] = hashlib.sha256(clients[-1].read_bytes()).hexdigest()
    args.receipt.write_text(json.dumps(data,indent=2,sort_keys=True)+"\n")


if __name__=="__main__":main()
