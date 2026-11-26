"""Compare the public vault helper mode type and a source-order mutation."""
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
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
DOMAIN = PORTABLE / "vault-mode-v1"
MANIFEST = "vault-fixed-value-mode-package.json"
SOURCE = Path("crates/marmish-vault/src/helper.rs")
FRAMEWORK = "18ea58006de4482c0ebc61de9e800edc78eba987"
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
JOURNAL = runpy.run_path(str(HERE.parent / "journal-records/verify.py"))
WORKSPACE = runpy.run_path(str(HERE.parent / "workspace/verify.py"))
run = JOURNAL["run"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reject_local_cargo_config(directory):
    for base in (directory, *directory.parents):
        for name in ("config", "config.toml"):
            config = base / ".cargo" / name
            if config.is_file():
                raise ValueError(f"unexpected Cargo configuration: {config}")


def host_build_env(cargo_home):
    registry_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    cargo_home.mkdir()
    for name in ("registry", "git"):
        cache = registry_home / name
        if cache.is_dir():
            (cargo_home / name).symlink_to(cache.resolve(), target_is_directory=True)
    env = JOURNAL["cargo_env"]()
    env = {key: value for key, value in env.items()
           if not key.startswith(("CARGO_", "RUSTC", "RUSTDOC", "RUSTFLAGS"))}
    env.update(CARGO_HOME=str(cargo_home), CARGO_NET_OFFLINE="true")
    return env


def check_inputs(args):
    for path, expected in ((args.framework, FRAMEWORK), (args.original, ORIGINAL)):
        reject_local_cargo_config(path)
        revision = run("git", "rev-parse", "HEAD", cwd=path).decode().strip()
        dirty = run("git", "status", "--porcelain", "--untracked-files=all", cwd=path)
        if revision != expected or dirty:
            raise ValueError(f"expected a clean checkout at {expected}: {path}")


def observe(library, scratch, host_deps):
    args = ["rustc", "+1.97.1", "--edition=2021", "--extern", f"marmish_vault={library}",
            "-L", f"dependency={library.parent / 'deps'}", "-L", f"dependency={host_deps}"]
    run(*args, HERE / "probe.rs", "-o", scratch / "caller")
    output = run(scratch / "caller").decode().splitlines()
    rejected = {}
    for bound in ("PartialEq", "Eq", "std::fmt::Debug", "std::fmt::Display", "Default", "std::hash::Hash", "Ord"):
        source = f"fn test<T: {bound}>() {{}} fn main() {{test::<marmish_vault::helper::AuthorizationMode>()}}"
        proc = subprocess.run([*args, "--emit=metadata", "--error-format=json", "-", "-o", scratch / "reject.rmeta"],
                              input=source.encode(), capture_output=True)
        errors = [d["code"]["code"] for line in proc.stderr.splitlines()
                  if (d := json.loads(line)).get("code") and d["level"] == "error"]
        if proc.returncode == 0 or errors != ["E0277"]:
            raise ValueError(f"unexpected {bound} result: {proc.stderr!r}")
        rejected[bound] = errors[0]
    return {"values": output, "absentTraits": rejected}


def qualify(args, scratch, work, host):
    source = args.original / SOURCE
    compiler = work / "oddlang"
    shutil.copy2(args.compiler, compiler)
    compiler_digest = sha(compiler)
    prefix, env = JOURNAL["sandbox"](WORKSPACE, work, [ROOT, args.original, args.framework, args.original_target, scratch], None)

    def inside(*cmd):
        return run(*prefix, *cmd, cwd=work, env=env)

    for path in (source, HERE / "probe.rs", args.framework / "src/lib.rs"):
        result = subprocess.run([*prefix, "/bin/cat", path], capture_output=True, env=env, cwd=work)
        if result.returncode != 1 or result.stdout or not any(error in result.stderr for error in
                                                               (b"No such file", b"Operation not permitted")):
            raise ValueError(f"failed source isolation check for {path}")

    with (scratch / "original.tar").open("wb") as archive:
        subprocess.run(["git", "-C", args.original, "archive", ORIGINAL], stdout=archive, check=True)
    mutant = scratch / "mutant"
    mutant.mkdir()
    run("tar", "-xf", scratch / "original.tar", "-C", mutant)
    old = b"pub enum AuthorizationMode {\n    Interactive,\n    Verify,\n}"
    new = b"pub enum AuthorizationMode {\n    Verify,\n    Interactive,\n}"
    raw = (mutant / SOURCE).read_bytes()
    if raw.count(old) != 1:
        raise ValueError("mutation requires the unique original variant declaration")
    (mutant / SOURCE).write_bytes(raw.replace(old, new))
    results = {}
    for name, checkout, expected in (("baseline", args.original, ["interactive=0", "verify=1"]),
                                     ("mutation", mutant, ["interactive=1", "verify=0"])):
        reject_local_cargo_config(checkout)
        reference_target = args.original_target / name
        native_build = run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--target", host,
                           "--target-dir", reference_target, "-p", "marmish-vault", "--lib",
                           "--message-format=json", cwd=checkout, env=args.build_env)
        lib, = [Path(file) for line in native_build.splitlines() if (r := json.loads(line))["reason"] == "compiler-artifact"
                and Path(r["manifest_path"]).resolve() == (checkout/"crates/marmish-vault/Cargo.toml").resolve()
                and r["target"]["kind"] == ["lib"] for file in r["filenames"] if file.endswith(".rlib")]
        original_sha = sha(lib)
        original = observe(lib, scratch, reference_target / "release/deps")
        select = json.loads((HERE / "selection.json").read_bytes())
        if name == "baseline" and select["sourceSha256"] != sha(checkout / SOURCE):
            raise ValueError("different original helper module")
        select["sourceSha256"] = sha(checkout / SOURCE)
        selector = HERE / "selection.json"
        if name == "mutation":
            selector = scratch / "mutated-selection.json"
            selector.write_text(json.dumps(select))
        output = scratch / name
        run(compiler, "recover", "plain-choice", checkout / SOURCE, selector, output)
        if name == "baseline" and (output / "observation.json").read_bytes() != (DOMAIN / "observation.json").read_bytes():
            raise ValueError("original recovery observation changed")
        manifest = json.loads((PORTABLE / MANIFEST).read_bytes())
        inputs = work / (name + "-inputs")
        inputs.mkdir()
        for pin in [manifest["cargoLock"], *(c[k] for c in manifest["components"] for k in ("graph", "overlay"))]:
            src = PORTABLE / pin["path"]
            if sha(src) != pin["sha256"]:
                raise ValueError("invalid package input digest")
            if src.parent == DOMAIN:
                if name == "baseline" and (output / src.name).read_bytes() != src.read_bytes():
                    raise ValueError("original recovery differs from published graph or overlay")
                src = output / src.name
                pin["sha256"] = sha(src)
            dest = inputs / pin["path"]
            dest.parent.mkdir(exist_ok=True, parents=True)
            shutil.copy2(src, dest)
        spec = inputs / MANIFEST
        spec.write_text(json.dumps(manifest, indent=2) + "\n")
        if name == "baseline":
            checked = work / "obligations"
            inside(compiler, "build", "checked-values", "rust", inputs / DOMAIN.name / "graph.json",
                   inputs / DOMAIN.name / "rust.json", checked)
            if (checked / "obligations.json").read_bytes() != (DOMAIN / "obligations.json").read_bytes():
                raise ValueError("published independent test obligations changed")
        product = work / (name + "-package")
        inside(compiler, "build", "native-package", "rust", spec, product)
        lock = sha(product / "package.lock")
        if name == "baseline" and lock != sha(DOMAIN / "package.lock"):
            raise ValueError("generated package lock changed")
        inside("cargo", "build", "--release", "--locked", "--offline", "--lib", "--manifest-path", product / "Cargo.toml")
        generated = product / "target/release/libmarmish_vault.rlib"
        actual = observe(generated, scratch, generated.parent / "deps")
        if actual != original or actual["values"] != expected:
            raise ValueError(f"{name}: original={original!r}, generated={actual!r}, expected={expected!r}")
        results[name] = dict(sourceSha256=sha(checkout/SOURCE), graphSha256=sha(output/"graph.json"),
                             overlaySha256=sha(output/"rust.json"), packageLockSha256=lock,
                             originalLibrarySha256=original_sha, generatedLibrarySha256=sha(generated), **actual)
    if results["baseline"]["graphSha256"] == results["mutation"]["graphSha256"]:
        raise ValueError("source change did not reach portable graph")
    return results, compiler_digest


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    for k in ("original", "framework", "compiler_target", "original_target", "receipt"):
        cli.add_argument(k, type=lambda s: Path(s).resolve())
    args = cli.parse_args()
    check_inputs(args)
    compiler = run("rustc", "+1.97.1", "-vV").decode().splitlines()
    host, = (line[6:] for line in compiler if line.startswith("host: "))
    if host not in ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"):
        raise ValueError(f"unsupported host {host}")
    with tempfile.TemporaryDirectory() as temp, tempfile.TemporaryDirectory() as isolated:
        scratch = Path(temp).resolve()
        args.build_env = host_build_env(scratch / "cargo-home")
        run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--locked", "--offline",
            "--target-dir", args.compiler_target, cwd=args.framework, env=args.build_env)
        check_inputs(args)
        args.compiler = args.compiler_target / "debug/oddlang"
        results, compiler_digest = qualify(args, scratch, Path(isolated).resolve(), host)
    check_inputs(args)
    witness = dict(original=ORIGINAL, host=host, compiler=compiler, framework=FRAMEWORK,
                   compilerSha256=compiler_digest, probeSha256=sha(HERE/"probe.rs"), results=results)
    args.receipt.write_text(json.dumps(witness, indent=2) + "\n")
