"""Prove that the coverage gate rejects an exact-name child that silently ran zero tests."""
import argparse
import json
from pathlib import Path
import runpy
import tempfile

from candidate_coverage import WHOLE, run, sha
from selected_tests import LOW, check_execution, stage


def control(original, compiler):
    test_names = runpy.run_path(str(WHOLE/"workspace/verify.py"))["passing_tests"]
    manifest = WHOLE.parents[1]/"oddlang/portable/engine-process-limits-package.json"
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        pkg = root/"marmish-engine"
        run(compiler, "build", "native-package", "rust", manifest, pkg)
        (root/"marmish-vault").mkdir()
        stage(original, root)
        cmd = ("cargo","+1.97.1","test","--offline","--locked","--release","--test",
               "selected_upstream","--","--exact",LOW,"--format","pretty")
        baseline = run(*cmd,cwd=pkg).decode()
        check_execution(LOW, 2, baseline, test_names(baseline))
        path = pkg/"tests/selected_upstream.rs"
        data = path.read_bytes()
        before = b'&format!("process_limits_tests::{name}")'
        if data.count(before) != 1:
            raise ValueError("unexpected child filter")
        path.write_bytes(data.replace(before, b'&format!("missing_child::{name}")'))
        # Cargo reports success for the parent when the child's exact filter misses.
        mutant = run(*cmd,cwd=pkg).decode()
        if test_names(mutant) != [LOW] or "test result: ok. 0 passed;" not in mutant:
            raise ValueError("control did not reproduce a vacuous child")
        try:
            check_execution(LOW, 2, mutant, test_names(mutant))
        except ValueError:
            return dict(case=LOW, packageManifestSha256=sha(manifest.read_bytes()),
                        baselinePassingProcesses=2, mutantPassingProcesses=1,
                        childMatchedZero=True, mutantCargoExit=0, coverageGateRejected=True)
        raise ValueError("coverage gate accepted a child with zero matches")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("original",type=Path)
    ap.add_argument("compiler",type=Path)
    args = ap.parse_args()
    print(json.dumps(control(args.original.resolve(),args.compiler.resolve()),indent=2))
