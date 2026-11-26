"""Check decoded command frames against separate role-specific room facts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2]/"oddlang/portable"
REQUEST = HERE.parent/"command-requests"
RESPONSE = HERE.parent/"command-responses"
ROOM = PORTABLE/"command-room-binding-v1"


def project(cli, kind, inputs, out):
    subprocess.run([cli, "build", kind, "rust", *inputs, out], check=True)


def verify(cli):
    import sys
    sys.path.insert(0, str(RESPONSE))
    from envelope import cases
    packet_sets = {
        "client": {op:raw for name,raw,_ in cases() for op,case in [name.split("/",1)] if case=="success"},
        "server": {op:raw for op,name,raw,_ in runpy.run_path(str(REQUEST/"verify.py"))["corpus"]()
                   if name=="canonical"},
    }
    operations = {"submit", "status", "poll", "cancel", "finish"}
    if any(set(packets) != operations for packets in packet_sets.values()):
        raise ValueError("expected one valid request and response per operation")

    with tempfile.TemporaryDirectory(prefix="command-room-binding-") as tmp:
        root=Path(tmp)
        for name in ["Cargo.toml","Cargo.lock"]:
            shutil.copyfile(REQUEST/name,root/name)
        shutil.copyfile(HERE/"observer.rs",root/"observer.rs")
        for role, folder, shape in [("server","command-requests-v1","command-requests-v1"),
                                    ("client","command-responses-v1","command-response-shapes-v1")]:
            d=PORTABLE/folder
            shapes=PORTABLE/shape
            inputs=([shapes/"shape.json",shapes/"shape.rust.json"] if role=="server" else
                    [shapes/"graph.json",shapes/"rust.json"])
            project(cli,"wire-protocol",[*inputs,d/"graph.json",d/"rust.json"],root/role)
        project(cli,"wire-schema",[ROOM/"shape.json",ROOM/"shape.rust.json"],root/"claim")
        project(cli,"field-relation",[ROOM/"graph.json",ROOM/"rust.json"],root/"relations")
        lines=[]
        expected=[]
        for role, packets in packet_sets.items():
            for op, raw in sorted(packets.items()):
                frame=json.loads(raw)
                client, server, session = "11"*32, "22"*32, "44"*16
                local, peer = (client, server) if role == "client" else (server, client)
                if (frame["client"], frame["server"], frame["session"]) != (client, server, session):
                    raise ValueError("fixture no longer names the independently configured room")
                for changes in ({}, {"client":server,"server":client}, {"client":"99"*32},
                                {"server":"99"*32}, {"session":"00"*16}):
                    packet = json.dumps(frame | changes, sort_keys=True, separators=(",", ":")).encode()
                    if not changes and packet != raw:
                        raise ValueError("fixture canonicalization changed")
                    lines.append(json.dumps([role,op,packet.hex(),local,peer,session]))
                    expected.append("command_wire_binding" if changes else "ok")
        observed=subprocess.check_output(["cargo","+1.97.1","run","--offline","--locked","--quiet"],
            cwd=root,input="\n".join(lines)+"\n",text=True,env={**os.environ,"RUSTFLAGS":"-Adead_code"})
        if observed.splitlines()!=expected:
            raise ValueError("bound frame admission differs")
        def digest(folder):
            return hashlib.sha256((root/folder/"projection.lock").read_bytes()).hexdigest()

        return {"accepted": expected.count("ok"), "wrongBindingRejected": expected.count("command_wire_binding"),
                "operationsPerRole": len(operations), "observationSha256": hashlib.sha256(observed.encode()).hexdigest(),
                "projectionLocks": {name: digest(name) for name in ("server", "client", "claim", "relations")}}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    print(json.dumps(verify(parser.parse_args().compiler.resolve()),indent=2))
