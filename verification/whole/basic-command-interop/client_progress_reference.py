#!/usr/bin/env python3
"""Check multi-poll and detached command delivery against a fixed Unix owner."""
from base64 import b64encode
from hashlib import sha256
import json
import socket
import struct
import subprocess
from pathlib import Path
import argparse
from tempfile import TemporaryDirectory, TemporaryFile

import client_delivery_reference as ref
from corpus import finite_case


class PollOwner(ref.Owner):
    def __init__(self, stdout=ref.STDOUT, stderr=ref.STDERR, widths=(5, 7), *, separate_finish=False):
        super().__init__()
        self.outputs = (stdout, stderr)
        self.widths = widths
        self.start = (0, 0)
        self.end = (0, 0)
        self.separate_finish = separate_finish
        self.finish_id = None
        self.finish_digest = None

    def exchange(self, operation, actual, *, bad_digest=False):
        if operation == 'command_response' and self.finish_id is not None:
            expected = dict(operation=operation, handle=self.handle, request_id=self.finish_id)
            request_digest = sha256(ref.packet('finish', 'request', self.finish_id, self.finish_body)).hexdigest()
            answer, self.finish_digest = self.answer('finish', self.finish_id, request_digest, dict(finished=True))
            return ref.compact(expected), ref.compact(answer)
        if operation == 'command_consume' and self.finish_digest is not None:
            expected = dict(operation=operation, handle=self.handle, request_id=self.finish_id,
                            response_sha256=self.finish_digest, expected_cursors=None)
            return ref.compact(expected), ref.compact(ref.local('consumed', stdout=self.start[0], stderr=self.start[1],
                             finish_request_id=None, finish_confirmed=True))
        if operation == 'command_cursors':
            return (ref.compact(dict(operation=operation, handle=self.handle)),
                    ref.compact(ref.local('cursors', stdout=self.start[0], stderr=self.start[1])))
        if operation == 'command_poll':
            self.poll_id = ref.checked_id(actual['request_id'], self.used_ids)
            begin = self.start
            sizes = tuple(len(data) for data in self.outputs)
            self.end = tuple(min(start + width, size) for start, width, size in zip(begin, self.widths, sizes))
            finished = self.end == sizes
            expected = dict(operation=operation, handle=self.handle, request_id=self.poll_id,
                            stdout_cursor=begin[0], stderr_cursor=begin[1], max_bytes=16384)
            body = dict(invocation=self.invocation, max_bytes=16384, stderr_cursor=begin[1], stdout_cursor=begin[0])
            digest = sha256(ref.packet('poll', 'request', self.poll_id, body)).hexdigest()
            status = ref.status(self.invocation, finished)
            status['stdout'].update(produced=self.end[0], retained=self.end[0])
            status['stderr'].update(produced=self.end[1], retained=self.end[1])
            if not finished:
                status['input'].update(health='eof_pending', eof_sent=False)
            elif self.separate_finish:
                # An independent synthetic command room binding for Finish.
                group = b'local finish fixture room'
                terminal = dict(client=ref.LOCAL_ACCOUNT, server=ref.ACCOUNT, session=ref.ROOM,
                    invocation=self.invocation, submit_sha256=self.handle['submit_sha256'], outcome=status['outcome'],
                    stdout=dict(produced=self.end[0], gaps=[]), stderr=dict(produced=self.end[1], gaps=[]))
                terminal_sha256 = sha256(b'marmish-command/1/finish\0' + len(group).to_bytes(4, 'big')
                    + group + ref.compact(terminal, sort_keys=True)).hexdigest()
                self.finish_body = dict(invocation=self.invocation, submit_sha256=self.handle['submit_sha256'],
                    stdout_cursor=self.end[0], stderr_cursor=self.end[1], terminal_sha256=terminal_sha256)
            result = dict(status=status, **{
                channel: dict(bytes=b64encode(data[start:end]).decode(), next_cursor=end)
                for channel, data, start, end in zip(('stdout', 'stderr'), self.outputs, begin, self.end)
            })
            reply, self.poll_response_digest = self.answer('poll', self.poll_id, digest, result)
            return ref.compact(expected), ref.compact(reply)
        if operation == 'command_consume' and self.submit_consumed:
            expected = dict(operation=operation, handle=self.handle, request_id=self.poll_id,
                response_sha256=self.poll_response_digest, expected_cursors=dict(stdout=self.start[0], stderr=self.start[1]))
            self.start = self.end
            complete = self.start == tuple(map(len, self.outputs))
            if complete and self.separate_finish:
                self.finish_id = ref.checked_id('123456789abc00000000000000000001', self.used_ids)
            return ref.compact(expected), ref.compact(ref.local('consumed', stdout=self.start[0], stderr=self.start[1],
                finish_request_id=self.finish_id, finish_confirmed=complete and not self.separate_finish))
        return super().exchange(operation, actual, bad_digest=bad_digest)


class RefusedOwner(ref.Owner):
    def answer(self, operation, request_id, request_sha, result):
        assert operation == 'submit'
        result = dict(error=dict(code='command_denied', detail=None), ok=False, request_sha256=request_sha, result=None)
        response = ref.packet(operation, 'response', request_id, result)
        checksum = sha256(response).hexdigest()
        return ref.local('answer', handle=self.handle, request_id=request_id,
                         response_base64=b64encode(response).decode(), response_sha256=checksum), checksum


def run_client(binary, work, owner, argv, operations, *, bad_digest=False):
    """Drain both finite streams to private files while servicing the owner socket."""
    state = work / 'state'
    if not state.exists():
        ref.config(state)
    path = work / 'o'
    sandbox, option, policy = ref.TOOLS.isolated(work, (binary.parent,))
    policy += f'(allow network-outbound (remote unix-socket (literal {json.dumps(str(path))})))'
    policy += '(deny mach-lookup (global-name "com.apple.securityd") (global-name "com.apple.trustd"))'
    policy += f'(deny process-exec)(allow process-exec (literal {json.dumps(str(binary))}))'
    env = {'HOME': str(work), 'TMPDIR': str(work), 'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}
    command = [sandbox, option, policy, binary, '--state-dir', state, '--socket', path, '--no-autostart', *argv]
    with (socket.socket(socket.AF_UNIX) as listener,
          TemporaryFile(dir=work) as stdout, TemporaryFile(dir=work) as stderr):
        listener.bind(str(path))
        listener.listen(1)
        listener.settimeout(4)
        with subprocess.Popen(command, env=env, cwd=work, stdin=subprocess.DEVNULL,
                              stdout=stdout, stderr=stderr) as client:
            try:
                for operation in operations:
                    try:
                        connection, _ = listener.accept()
                    except TimeoutError as error:
                        raise ValueError(f'client did not request {operation}; exit {client.poll()}') from error
                    with connection, connection.makefile('rb') as reader:
                        connection.settimeout(4)
                        length, = struct.unpack('>I', reader.read(4))
                        if not 0 < length <= 65536:
                            raise ValueError('invalid request frame size')
                        actual = reader.read(length)
                        if len(actual) != length or reader.read(1):
                            raise ValueError('invalid request frame or half-close')
                        expected, reply = owner.exchange(operation, json.loads(actual), bad_digest=bad_digest)
                        if actual != expected:
                            raise ValueError(f'local request differs at {operation}')
                        connection.sendall(ref.frame(reply))
                status = client.wait(timeout=4)
                listener.setblocking(False)
                try:
                    unexpected, _ = listener.accept()
                except BlockingIOError:
                    pass
                else:
                    with unexpected:
                        raise ValueError('unexpected additional owner request')
            finally:
                if client.poll() is None:
                    client.kill()
                    client.wait(timeout=4)
        for stream in (stdout, stderr):
            if stream.tell() > 132096:
                raise ValueError('bounded local fixture output overflow')
            stream.seek(0)
        result = status, stdout.read(), stderr.read()
    path.unlink()
    return result


MULTI = ref.POLL + ('command_unconsumed',) + ref.POLL


def check(program):
    output = dict(allFramesCompared=True, cases={})
    with TemporaryDirectory(prefix='opg-', dir='/private/tmp') as directory:
        root = Path(directory)
        command = ['--stdin-none', ref.NPUB, '--', '/usr/bin/true']

        def instance(name):
            work = root / name
            work.mkdir(mode=0o700)
            return work

        def resumed(work):
            path = work / 'state/default/profile.json'
            path.write_bytes(ref.compact(dict(format=2, name='default', phase='ready', public_key=ref.LOCAL_ACCOUNT)))
            path.chmod(0o600)

        def equal(name, actual, expected, operations):
            if actual != expected:
                raise ValueError(f'{name}: exit/stdout/stderr differ by status or byte length')
            status, out, err = actual
            output['cases'][name] = dict(exit=status, stdoutSha256=sha256(out).hexdigest(),
                stderrSha256=sha256(err).hexdigest() if not err.startswith(b'marmish handle: ') else None,
                handlePrefix=err.startswith(b'marmish handle: '), operations=operations)
            if err.startswith(b'marmish handle: '):
                _, _, diagnostic = err.partition(b'\n')
                output['cases'][name]['diagnosticSha256'] = sha256(diagnostic).hexdigest()

        operations = ref.SUBMIT + MULTI
        result = run_client(program, instance('foreground'), PollOwner(widths=(8, 9)), command, operations)
        equal('foreground', result, (7, ref.STDOUT, ref.STDERR), operations)
        work = instance('resume')
        owner = PollOwner(widths=(8, 9))
        result = run_client(program, work, owner, ['--detach', *command], ref.SUBMIT)
        handle = ref.compact(owner.handle)
        handle_line = b'marmish handle: ' + handle + b'\n'
        equal('detach', result, (0, b'', handle_line), ref.SUBMIT)
        resumed(work)
        operations = ('status', 'command_unconsumed') + MULTI
        result = run_client(program, work, owner, ['wait', '--handle', handle.decode()], operations)
        equal('wait', result, (7, ref.STDOUT, ref.STDERR), operations)
        work = instance('nonzero')
        owner = PollOwner(widths=(8, 9))
        result = run_client(program, work, owner, ['--detach', *command], ref.SUBMIT)
        handle = ref.compact(owner.handle)
        if result != (0, b'', b'marmish handle: ' + handle + b'\n'):
            raise ValueError('nonzero resume setup did not produce the bound detached handle')
        resumed(work)
        owner.start = (8, 9)
        operations = ('status', 'command_unconsumed') + ref.POLL
        result = run_client(program, work, owner, ['wait', '--handle', handle.decode()], operations)
        equal('waitNonzero', result, (7, ref.STDOUT[8:], ref.STDERR[9:]), operations)

        owner = PollOwner(widths=(8, 9))
        operations = ref.SUBMIT[:-1]
        result = run_client(program, instance('corrupt'), owner, command, operations, bad_digest=True)
        equal('badDigest', result, (255, b'', b'marmish handle: '+ref.compact(owner.handle)+
              b'\nmarmish: command_response\n'), operations)
        finite = finite_case(0)
        loops = 16
        owner = PollOwner(finite.stdout, finite.stderr, widths=(8192, 8192))
        operations = ref.SUBMIT + ref.POLL + (('command_unconsumed',) + ref.POLL) * (loops - 1)
        result = run_client(program, instance('finite'), owner, command, operations)
        equal('finiteOutput', result, (finite.status, finite.stdout, finite.stderr), operations)
        owner = PollOwner(widths=(8, 9), separate_finish=True)
        operations = ref.SUBMIT + MULTI + ('command_response', 'command_consume')
        result = run_client(program, instance('finish'), owner, command, operations)
        equal('separateFinish', result, (7, ref.STDOUT, ref.STDERR), operations)
        owner = RefusedOwner()
        result = run_client(program, instance('refused'), owner, command, ref.SUBMIT)
        equal('commandDenied', result, (255, b'', b'marmish handle: '+ref.compact(owner.handle)+
            b"\nmarmish: command_denied: the server's command policy does not allow this command\n"), ref.SUBMIT)
    output['workspaceRemoved'] = not root.exists()
    return output


class StaleCursorOwner(ref.Owner):
    def exchange(self, operation, actual, *, bad_digest=False):
        if operation == 'command_cursors':
            expected = dict(operation=operation, handle=self.handle)
            return ref.compact(expected), ref.compact(ref.local('cursors', stdout=1, stderr=0))
        return super().exchange(operation, actual, bad_digest=bad_digest)


class WrongSessionOwner(PollOwner):
    def answer(self, operation, request_id, request_sha, result):
        if operation != 'poll':
            return super().answer(operation, request_id, request_sha, result)
        answer, _ = super().answer(operation, request_id, request_sha, result)
        body = dict(error=None, ok=True, request_sha256=request_sha, result=result)
        packet = json.loads(ref.packet(operation, 'response', request_id, body))
        packet['session'] = '11' * 16
        response = ref.compact(packet, sort_keys=True)
        checksum = sha256(response).hexdigest()
        answer['command'].update(response_base64=b64encode(response).decode(), response_sha256=checksum)
        return answer, checksum


def verify_progress_admission(program):
    cases = (
        ('staleInitialCursors', StaleCursorOwner(), ('command_cursors',), 'command_cursors'),
        ('digestValidWrongSession', WrongSessionOwner(), ('command_cursors', 'command_poll'), 'command_poll'),
    )
    result = {}
    with TemporaryDirectory(prefix='opb-', dir='/private/tmp') as directory:
        command = ['--stdin-none', ref.NPUB, '--', '/usr/bin/true']
        for name, owner, suffix, error in cases:
            work = Path(directory) / name
            work.mkdir(mode=0o700)
            observed = run_client(program, work, owner, command, ref.SUBMIT + suffix)
            diagnostic = b'marmish handle: ' + ref.compact(owner.handle) + b'\nmarmish: ' + error.encode() + b'\n'
            if observed != (255, b'', diagnostic):
                raise ValueError(f'{name}: progress admission response differs')
            result[name] = dict(exit=255, output=False, pollConsumed=False)
    return result


def verify_resume_admission(program):
    handle = dict(format=1, profile='default', remote=ref.ACCOUNT, session=ref.ROOM,
                  invocation='123456789abc00000000000000000002', submit_sha256='11' * 32)
    scope = (125, b'', b'marmish: command client option outside generated handoff scope\n')
    unsafe = (1, b'', b'marmish: identity: unsafe_profile_file\n')
    untrusted = (1, b'', b'marmish: invalid input: untrusted_remote\n')
    cases = (
        ('wrongProfile', ref.compact(handle | {'profile': 'alternate'}), 0o600, scope),
        ('duplicateHandleMember', ref.compact(handle)[:-1] + b',"format":1}', 0o600, scope),
        ('unauthorizedAccount', ref.compact(handle | {'remote': '22' * 32}), 0o600, untrusted),
        ('unsafeManifest', ref.compact(handle), 0o644, unsafe),
    )
    result = {}
    with TemporaryDirectory(prefix='opa-', dir='/private/tmp') as directory:
        for name, input_handle, mode, expected in cases:
            work = Path(directory) / name
            work.mkdir(mode=0o700)
            ref.config(work / 'state')
            manifest = work / 'state/default/profile.json'
            manifest.write_bytes(ref.compact(dict(format=2, name='default', phase='ready',
                                                 public_key=ref.LOCAL_ACCOUNT)))
            manifest.chmod(mode)
            observed = run_client(program, work, ref.Owner(), ['wait', '--handle', input_handle.decode()], ())
            if observed != expected:
                raise ValueError(f'{name}: resume admission response differs')
            result[name] = dict(exit=observed[0], ownerContact=False)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('client', type=lambda value: Path(value).resolve(strict=True))
    parser.add_argument('sha256')
    parser.add_argument('receipt', type=Path)
    parser.add_argument('--check-resume-admission', action='store_true',
                        help='exercise candidate-only matching authority and cursor refusals')
    args = parser.parse_args()
    actual = ref.sha(args.client)
    if actual != args.sha256:
        raise ValueError('client digest differs')
    evidence = check(args.client)
    if args.check_resume_admission:
        evidence['resumeAdmission'] = verify_resume_admission(args.client)
        evidence['progressAdmission'] = verify_progress_admission(args.client)
    evidence['clientSha256'] = actual
    evidence['fixtureSha256'] = ref.sha(Path(__file__))
    evidence['format'] = 'marmish-basic-command-bounded-local-progress-v1'
    with args.receipt.open('x') as stream:
        stream.write(json.dumps(evidence, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
