"""Exercise the generated room_config file entry point."""
import json
from pathlib import Path
import subprocess
import tempfile


def check(binary):
    invalid = 'marmish: identity: invalid_manifest'
    cases = [
        ('omitted policy', {}, invalid),
        ('null policy', {'command_policy': None}, invalid),
        ('missing version', {'command_policy': {}}, invalid),
        ('unsupported version', {'command_policy': {'version': 2}}, invalid),
        ('omitted default allow', {'command_policy': {'version': 1}}, 'loaded-allow'),
    ]
    for name, allow, expected in [
        ('explicit named', ['/usr/bin/echo'], 'loaded-allow'),
        ('unlisted', ['/bin/cat'], 'loaded-deny'),
        ('empty allow', [], 'loaded-deny'),
        ('null allow', None, invalid),
        ('scalar allow', '/usr/bin/echo', invalid),
        ('non-string item', [3], invalid),
    ]:
        cases.append((name, {'command_policy': {'version': 1, 'defaults': {'allow_exec': allow}}}, expected))
    endpoint = 'wss://relay.example/'
    with tempfile.TemporaryDirectory(prefix='policy-room-') as temp:
        document = Path(temp) / 'config.json'
        document.touch(mode=0o600)
        for name, fields, expected in cases:
            config = {'version': 1, 'role': 'server', 'relays': [endpoint], 'tor': 'auto',
                      'trusted_remotes': [], 'execution_clients': [], 'cwd': '/', 'limits': {}, 'aliases': {}}
            document.write_text(json.dumps(config | fields))
            actual = subprocess.check_output([binary, temp], text=True).rstrip('\n')
            if actual != expected:
                raise AssertionError(f'{name}: expected {expected!r}, observed {actual!r}')
    print(f'{len(cases)} generated foreground room_config file observations matched.')
    return len(cases)
