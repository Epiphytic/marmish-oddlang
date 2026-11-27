"""Build the pinned and generated observers in disposable workspace copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from replay import verify
from room_cases import check as check_room_config

HERE = Path(__file__).resolve().parent
PIN = 'af54339d250290490d62d4475a77539b7e739071'
TOOLCHAIN = '1.97.1'


def append(path, text):
    with path.open('a') as source:
        source.write('\n' + text + '\n')


def compile_bins(workspace, cache, bins):
    rustc = subprocess.check_output(['rustc', '+' + TOOLCHAIN, '-vV'], text=True)
    host = next(line.split()[1] for line in rustc.splitlines() if line.startswith('host:'))
    command = ['cargo', '+' + TOOLCHAIN, 'build', '--locked', '--offline', '--release', '--target', host,
               '--target-dir', cache, '-p', 'marmish-native']
    for binary in bins:
        command.extend(['--bin', binary])
    subprocess.run(command, cwd=workspace, env={**os.environ, 'MARMISH_BUILD_COMMIT': PIN}, check=True)
    return cache / host / 'release'


def upstream_probe(repo, cache):
    with tempfile.TemporaryDirectory(prefix='marmish-upstream-policy-') as temp:
        work = Path(temp)
        archive = work / 'sources.tar'
        subprocess.run(['git', '-C', repo, 'archive', '--output', archive, PIN], check=True)
        subprocess.run(['tar', '-xf', archive, '-C', work], check=True)
        src = work / 'crates/marmish-native/src'
        append(src / 'command_config.rs', (HERE / 'upstream_api.rs').read_text())
        append(src / 'lib.rs', 'pub use command_config::policy_observation;')
        (src / 'bin/policy-observer.rs').write_text(
            'use marmish_native::policy_observation;\n' + (HERE / 'driver.rs').read_text())
        return compile_bins(work, cache, ['policy-observer']) / 'policy-observer'


def generated_probes(workspace, cache):
    with tempfile.TemporaryDirectory(prefix='marmish-generated-policy-') as temp:
        work = Path(temp)
        shutil.copytree(workspace, work, dirs_exist_ok=True, ignore=shutil.ignore_patterns('target', '.git'))
        src = work / 'marmish-native/src'
        owner, = [path for path in src.glob('group_*/*.rs') if 'fn room_config(' in path.read_text()]
        exports = '{policy_observation, observe_room_config}'
        append(owner, (HERE / 'generated_api.rs').read_text())
        append(owner.with_name('mod.rs'), f'pub use {owner.stem}::{exports};')
        append(src / 'lib.rs', f'pub use foreground::{exports};')
        (src / 'bin/generated-observer.rs').write_text(
            'use marmish_native::policy_observation;\n' + (HERE / 'driver.rs').read_text())
        shutil.copyfile(HERE / 'room_observer.rs', src / 'bin/room-observer.rs')
        return compile_bins(work, cache, ['generated-observer', 'room-observer'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('upstream', type=Path)
    parser.add_argument('target_dir', type=Path)
    parser.add_argument('generated_workspace', type=Path)
    parser.add_argument('--receipt', type=Path)
    args = parser.parse_args()
    cache = args.target_dir.resolve()
    native = upstream_probe(args.upstream.resolve(), cache)
    out = generated_probes(args.generated_workspace.resolve(), cache)
    observations = verify(native, out / 'generated-observer')
    room_count = check_room_config(out / 'room-observer')
    if args.receipt:
        owner, = [path for path in args.generated_workspace.glob('marmish-native/src/group_*/*.rs')
                  if 'fn room_config(' in path.read_text()]
        receipt = {
            'upstreamCommit': PIN,
            'rustToolchain': TOOLCHAIN,
            'host': out.parent.name,
            'workspaceLockSha256': hashlib.sha256((args.generated_workspace / 'workspace.lock').read_bytes()).hexdigest(),
            'foregroundSourceSha256': hashlib.sha256(owner.read_bytes()).hexdigest(),
            'policyDecisions': observations,
            'roomConfigFileCases': room_count,
        }
        args.receipt.write_text(json.dumps(receipt, indent=2) + '\n')
