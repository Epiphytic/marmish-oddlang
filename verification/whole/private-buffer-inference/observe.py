"""Capture portable reader behavior from a pinned original and independent source changes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
PIN = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = "crates/marmish-prototype/src/lib.rs"
CASE = "portable_buffer_behavior::collect"
CHANGES = {
    "emptyRead": (b"let available = self.fill_buf()?;", b"if output.is_empty() { return Ok(0); } let available = self.fill_buf()?;"),
    "consumedBytes": (b"self.bytes[self.start..end].zeroize();", b""),
    "refillError": (b"self.end = self.inner.read(self.bytes.as_mut())?;", b"self.end = 0; self.end = self.inner.read(self.bytes.as_mut())?;"),
}
LANES = {"original":[], "empty": ["emptyRead"], "consume": ["consumedBytes"], "error":["refillError"],"combined":list(CHANGES)}


def capture(checkout, target, host, name=CASE):
    raw = subprocess.check_output(["cargo","+1.97.1","test","--release","--locked","--offline",
                                   "--target-dir",target,"--target",host,"-p","marmish-prototype","--lib",
                                   "--","--exact",name,"--nocapture"],cwd=checkout,text=True)
    if raw.splitlines().count(f"test {name} ... ok")!=1:
        raise ValueError("test harness did not execute the exact observation")
    trace = {"format":"oddlang-buffered-input-trace-v1"}
    for k in ["emptyRead","consume","refillError"]:
        line, = [line[len(k)+1:] for line in raw.splitlines() if line.startswith(k+" ")]
        trace[k] = json.loads(line)
    return trace


def observe(original,target):
    host = next(s[6:] for s in subprocess.check_output(["rustc","+1.97.1","-vV"],text=True).splitlines() if s.startswith("host: "))
    observer = (HERE/"trace.rs").read_bytes()
    with tempfile.TemporaryDirectory() as tmp:
        checkout = Path(tmp)/"reference"
        subprocess.run(["git","clone","-q","--local","--no-checkout",original,checkout],check=True)
        subprocess.run(["git","-C",checkout,"checkout","-q","--detach",PIN],check=True)
        lib = checkout/SOURCE
        source = lib.read_bytes()
        output = {}
        for lane, keys in LANES.items():
            changed = source
            for key in keys:
                old,new = CHANGES[key]
                if changed.count(old)!=1:
                    raise ValueError(f"ambiguous source change for {key}")
                changed = changed.replace(old,new)
            lib.write_bytes(changed+b"\n"+observer)
            output[lane] = {"sourceSha256":hashlib.sha256(changed).hexdigest(),"trace":capture(checkout,target,host)}
    return dict(revision=PIN,target=host,observerSha256=hashlib.sha256(observer).hexdigest(),lanes=output)


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    for n in ("original","cargo_target","receipt"):
        ap.add_argument(n,type=Path)
    args=ap.parse_args()
    args.receipt.write_text(json.dumps(observe(args.original.resolve(),args.cargo_target.resolve()),indent=2)+"\n")
