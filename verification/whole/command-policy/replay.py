"""Compare policy decisions from the pinned native and generated Rust observers."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def fixtures():
    cases = []
    allow, deny = "true", "command_denied"
    path = b"/usr/bin/echo"

    def case(name, policy, expected, executable=path, principal=0, alias=None):
        raw = policy if isinstance(policy, str) else json.dumps(policy, separators=(',', ':'))
        row = '\t'.join([str(principal), executable.hex(), '-' if alias is None else alias.hex(), raw])
        cases.append((name, row, expected))

    def policy(defaults=None, keys=None):
        p = {"version": 1}
        if defaults is not None:
            p["defaults"] = defaults
        if keys is not None:
            p["keys"] = keys
        return p

    def key(which, **restrictions):
        return {"principal": f"{which:02x}" * 32, "restrictions": restrictions}

    named = {"allow_exec": [path.decode()]}
    case("one exact named path", policy(named), allow)
    case("unlisted sibling", policy(named), deny, b"/usr/bin/echo2")
    case("unlisted basename", policy(named), "invalid_request", b"echo")
    case("omitted defaults", policy(), allow)
    case("omitted allow", policy({}), allow)
    case("empty allow", policy({"allow_exec": []}), deny)
    case("empty deny", policy({"deny_exec": []}), allow)
    case("one of two allows", policy({"allow_exec": ["/bin/false", path.decode()]}), allow)
    case("same-layer deny precedence", policy({**named, "deny_exec": [path.decode()]}), deny)
    case("deny does not affect other executable", policy({**named, "deny_exec": ["/bin/false"]}), allow)
    layers = [key(0, allow_exec=["/usr/**"]), key(0, deny_exec=[path.decode()])]
    case("both matching key layers apply", policy(named, layers), deny)
    case("other principal unaffected", policy(named, layers), allow, principal=1)
    case("key cannot broaden defaults", policy({"allow_exec": []}, [key(0, **named)]), deny)
    case("unlisted key principal", policy(named, [key(2)]), "invalid_policy")
    case("unknown key invalid even if inactive", policy(named, [key(2)]), "invalid_policy", principal=1)
    case("untouched defaults for key without allow", policy(named, [key(0)]), allow)
    case("explicit empty key allow", policy(named, [key(0, allow_exec=[])]), deny)
    case("allowed path with denied alias", policy({**named, "deny_exec": ["/alias"]}), deny, alias=b"/alias")
    case("allowed alias cannot grant canonical path", policy({"allow_exec": ["/alias"]}), deny, alias=b"/alias")
    case("allowed path with different alias", policy(named), allow, alias=b"/alias")
    for glob, candidate, expected in [
        ("/usr/*", path, deny), ("/usr/**", path, allow), ("/usr/***", path, allow),
        ("/usr/bin/*", b"/usr/bin/\xff", allow), ("/caf\u00e9/*", "/café/tool".encode(), allow),
        (r"/bin/\*", b"/bin/*", allow), (r"/bin/\*", b"/bin/x", deny),
        (r"/bin/\\", b"/bin/\\", allow), ("/usr/b*n/echo", path, allow),
        ("/usr/b*in/echo", path, allow), ("/usr/bin/?", b"/usr/bin/x", deny),
    ]:
        case(f"glob {glob!r} on {candidate!r}", policy({"allow_exec": [glob]}), expected, candidate)
    for candidate in [b"", b"/", b"./bin/x", b"/bin//x", b"/bin/./x", b"/bin/../x", b"/bin/x/", b"/" + b"x"*4095]:
        case(f"invalid path {candidate[:20]!r} length {len(candidate)}", policy(), "invalid_request", candidate)
    case("4095 byte executable", policy(), allow, b"/"+b"x"*4094)
    for raw in ['null', '[]', '{}', '{"version":1,"version":1}', '{"version":1,"extra":1}',
                '{"version":1,"defaults":{"allow_exec":[],"allow_exec":[]}}',
                '{"version":1,"defaults":{"allow_exec":null}}', '{"version":1,"defaults":{"deny_exec":null}}',
                '{"version":1,"defaults":{"allow_exec":"/usr/bin/echo"}}', '{"version":1,"defaults":null}',
                '{"version":1,"keys":null}', '{"version":1,"keys":[{"restrictions":{}}]}',
                '{"version":1,"keys":[{"principal":"x"}]}', '{"version":1,"defaults":{"extra":[]}}']:
        case(f"schema {raw}", raw, "config_schema")
    principal_text = "00" * 32
    for name, raw, expected in [
        ("version only", [1], allow),
        ("omitted restriction members", [1, []], allow),
        ("explicit empty default allow", [1, [[]]], deny),
        ("default allow and deny positions", [1, [[path.decode()], [path.decode()]]], deny),
        ("allow then unrelated deny", [1, [[path.decode()], ["/bin/other"]]], allow),
        ("nested key permits named path", [1, [], [[principal_text, [[path.decode()]]]]], allow),
        ("nested key denies unlisted path", [1, [], [[principal_text, [["/bin/other"]]]]], deny),
        ("nested key member defaults", [1, [], [[principal_text, []]]], allow),
        ("unused nested key", [1, [], [["01" * 32, [[]]]]], allow),
        ("extra policy member", [1, [], [], 0], "config_schema"),
        ("null default allow", [1, [None]], "config_schema"),
        ("extra default member", [1, [[], [], []]], "config_schema"),
        ("key missing restrictions", [1, [], [[principal_text]]], "config_schema"),
        ("key members swapped", [1, [], [[[], principal_text]]], "config_schema"),
        ("extra key member", [1, [], [[principal_text, [], 3]]], "config_schema"),
        ("extra key restriction member", [1, [], [[principal_text, [[], [], []]]]], "config_schema"),
        ("scalar allow in key restrictions", [1, [], [[principal_text, [path.decode()]]]], "config_schema"),
    ]:
        case("positional " + name, raw, expected)
    for version, expected in [
        ("0", "config_policy_bounds"), ("4294967295", "config_policy_bounds"),
        ("4294967296", "config_schema"), ("-1", "config_schema"),
        ("1.0", "config_schema"), ("1e0", "config_schema"),
        ('"1"', "config_schema"), ("true", "config_schema"), ("null", "config_schema"),
    ]:
        case("version literal " + version, '{"version":' + version + '}', expected)
    # On /x the implicit /** uses 9 fuel. This 1002-token deny uses 1003
    # units per byte including each name's boundary, or 999991 at alias length 993.
    long_deny = "/" + "d" * 1001
    for size, expected in [(992, allow), (993, allow), (994, "invalid_policy")]:
        case(f"deny alias budget at {size} bytes", policy({"deny_exec": [long_deny]}), expected,
             b"/x", alias=b"/" + b"a" * (size - 1))
    case("allow patterns do not charge alias", policy({"allow_exec": ["/x", long_deny]}),
         allow, b"/x", alias=b"/" + b"a" * 4094)
    for pattern in ["relative", "", "/nul\0", "/tab\t", "/x\u0085", "/trailing\\", r"/bad\q"]:
        case(f"invalid pattern {pattern!r}", policy({"allow_exec": [pattern]}), "invalid_policy")
    case("unused key invalid pattern", policy(named, [key(1, deny_exec=["relative"])]), "invalid_policy")
    case("policy version", {"version": 2}, "config_policy_bounds")
    case("128 key entries", policy(named, [key(0)]*128), allow)
    case("129 key entries", policy(named, [key(0)]*129), "config_policy_bounds")
    case("256 explicit allows", policy({"allow_exec": [path.decode()]*256}), allow)
    case("257 explicit allows", policy({"allow_exec": [path.decode()]*257}), "config_policy_bounds")
    case("implicit allow counted", policy({"deny_exec": ["/x"]*256}), "config_policy_bounds")
    case("implicit allow at limit", policy({"deny_exec": ["/x"]*255}), allow)
    case("1024 byte pattern", policy({"allow_exec": ["/" + "a"*1023]}), deny)
    case("1025 byte utf8 pattern", policy({"allow_exec": ["/" + "é"*512]}), "config_policy_bounds")
    case("preflight fuel exhausted after match", policy({"allow_exec": ["/**", "/" + "a"*1023]}),
         "invalid_policy", b"/" + b"a"*1000)
    case("malformed principal", policy(keys=[{"principal":"wrong","restrictions":{}}]), "config_account")
    case("uppercase hex principal", policy(keys=[{"principal":"AA"*32,"restrictions":{}}]), "config_account")
    case("valid npub canonicalizes to matching principal",
         policy(keys=[{"principal":"npub1" + "q"*52 + "zqujme","restrictions":{"allow_exec":[]}}]), deny)
    return cases


def observe(binary, data, names):
    run = subprocess.run([binary], input=data, capture_output=True, text=True, check=True)
    outputs = run.stdout.splitlines()
    if len(outputs) != len(names):
        raise RuntimeError(f'{binary} returned {len(outputs)} observations for {len(names)} cases')
    return outputs


def verify(native_binary, generated_binary):
    names, rows, expectations = zip(*fixtures())
    data = '\n'.join(rows) + '\n'
    native = observe(native_binary, data, names)
    generated = observe(generated_binary, data, names)
    bad = []
    for name, expected, original, actual in zip(names, expectations, native, generated):
        if original != expected or actual != original:
            bad.append(f'{name}: expected {expected}, native {original}, generated {actual}')
    if bad:
        raise SystemExit('\n'.join(bad))
    print(f'{len(names)} policy cases matched the pinned native implementation and the independent expectations.')

    return {
        'cases': len(names),
        'inputsSha256': hashlib.sha256(data.encode()).hexdigest(),
        'observationsSha256': hashlib.sha256(('\n'.join(native) + '\n').encode()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('native', type=Path)
    parser.add_argument('generated', type=Path)
    args = parser.parse_args()
    verify(args.native.resolve(), args.generated.resolve())


if __name__ == '__main__':
    main()
