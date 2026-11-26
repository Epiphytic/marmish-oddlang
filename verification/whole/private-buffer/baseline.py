"""Observe only local buffer semantics in a disposable, pinned upstream checkout."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

PIN = "af54339d250290490d62d4475a77539b7e739071"
SOURCE = "crates/marmish-prototype/src/lib.rs"
HERE = Path(__file__).resolve().parent
PREFIX = "buffer_contract_observations::"
CASES = ["retained_span_and_consumed_bytes", "failed_refill_retains_prior_end",
         "zero_output_still_reads_and_eof_can_be_polled_again"]
ORIGINAL_CASE = "secret_request_tests::private_reader_wipes_consumed_spans"


def observe(original, target):
    suite = (HERE/"observations.rs").read_bytes()
    source = subprocess.check_output(["git","-C",original,"show",f"{PIN}:{SOURCE}"])
    toolchain = subprocess.check_output(["rustc","+1.97.1","-vV"],text=True)
    host, = [s[6:] for s in toolchain.splitlines() if s.startswith("host: ")]
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)/"reference"
        subprocess.run(["git","clone","-q","--no-checkout","--local",original,work],check=True)
        subprocess.run(["git","-C",work,"checkout","-q","--detach",PIN],check=True)
        lib = work/SOURCE

        def run(case, *, failure=None):
            result = subprocess.run(["cargo","+1.97.1","test","--release","--locked","--offline",
                                     "--target-dir",target,"--target",host,"-p","marmish-prototype",
                                     "--lib","--","--exact",case],cwd=work,capture_output=True,text=True)
            status = "ok" if failure is None else "FAILED"
            if (result.returncode != (0 if failure is None else 101)
                    or f"test {case} ... {status}" not in result.stdout.splitlines()
                    or (failure and failure not in result.stdout)):
                raise RuntimeError(result.stdout + result.stderr)
            return result.returncode

        lib.write_bytes(source+b"\n"+suite)
        passed = [ORIGINAL_CASE]+[PREFIX+name for name in CASES]
        for case in passed:
            run(case)
        controls = {}
        for label, old, new, case, failure in [
            ("retainConsumedBytes", b"self.bytes[self.start..end].zeroize();", b"", CASES[0], "consumed span must be erased"),
            ("clearEndBeforeRefill", b"self.end = self.inner.read(self.bytes.as_mut())?;",
             b"self.end = 0; self.end = self.inner.read(self.bytes.as_mut())?;", CASES[1], "failed refill preserves old end"),
        ]:
            if source.count(old) != 1:
                raise ValueError(f"ambiguous source mutation {label}")
            lib.write_bytes(source.replace(old,new)+b"\n"+suite)
            controls[label] = dict(test=PREFIX+case, expectedAssertion=failure, exit=run(PREFIX+case,failure=failure))
    return dict(revision=PIN, rustc=toolchain.strip(), target=host,
                upstreamFileSha256=hashlib.sha256(source).hexdigest(),
                callerSha256=hashlib.sha256(suite).hexdigest(), passed=passed, controls=controls)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("original","target","receipt"):
        ap.add_argument(name,type=Path)
    args = ap.parse_args()
    result = observe(args.original.resolve(),args.target.resolve())
    args.receipt.write_text(json.dumps(result,indent=2)+"\n")
