"""Construct boundary cases and independent expected public values."""
from itertools import product

NAMES = ("marmish", "marmishd", "marmish-worker", "THIRD-PARTY-LICENSES")
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin")
KEYS = ("profile", "journal", "engine", "config", "commands")
DIGESTS = tuple(bytes(range(i, i+32)).hex() for i in range(4))
LIMITS = (256*1024**2,)*3 + (16*1024**2,)
BAD = {"error":"Invalid","code":"install_manifest","display":"invalid input: install_manifest"}


def manifest(version="0.3.1", target=TARGETS[0], signing="none", notarized=False,
             minimum="glibc=2.39", formats=(2,7,1,1,1), sizes=(1,23,321,5)):
    commit = bytes(range(20)).hex()
    rows = ["marmish-manifest 1", f"version {version}", f"target {target}",
            f"commit {commit}", f"signing {signing} notarized={'yes' if notarized else 'no'}",
            f"minimum {minimum}", "state " + " ".join(f"{k}={v}" for k,v in zip(KEYS,formats))]
    files = []
    identity = f"{version}-{target}"
    for i,(name, digest, size) in enumerate(zip(NAMES,DIGESTS,sizes)):
        mode = 0o444 if i == 3 else 0o555
        rows.append(f"file {digest} {size} {mode:04o} {name}")
        files.append(dict(name=name,bytes=size,mode=mode,sha256=digest,download=f"{name}-{identity}"))
    return ("\n".join(rows+["end",""]).encode(),
            dict(version=version,target=target,commit=commit,signing=signing,notarized=notarized,
                 minimum=minimum,formats=list(formats),id=identity,files=files))


def cases():
    tests = {}
    def add(name, data, expected=BAD):
        assert name not in tests
        tests[name] = (data,expected)
    baseline, good = manifest()
    add("baseline",baseline,good)
    for i,t in enumerate(TARGETS):
        for family,sign,notarized in product(("glibc","macos"),("none","adhoc","developer-id"),(False,True)):
            raw, wanted = manifest(target=t,signing=sign,notarized=notarized,minimum=f"{family}=0.18446744073709551615")
            allowed = (family=="glibc" and sign=="none" and not notarized) if i<2 else (
                family=="macos" and (sign=="developer-id" or (sign=="adhoc" and not notarized)))
            add(f"platform-{i}-{family}-{sign}-{int(notarized)}",raw,wanted if allowed else BAD)
    for v in ("0.2.0","0.3.0-rc.1","0.2.18446744073709551615-rc.18446744073709551615"):
        add("version-valid-"+v,*manifest(version=v))
    for i,v in enumerate(("0.1.0","0.4.0","0.3.","0.3.00","+0.3.1","0.3.1-rc.0","0.3.1-rc.01",
                          "0.3.18446744073709551616","0.3.1-rc.18446744073709551616","0.3.1-rc.1+meta",
                          "0.3.1-rc.1-rc.2","0.3.é","0.3.١")):
        add(f"version-invalid-{i}",manifest(version=v)[0])
    for i,f in enumerate(("glibc=2","glibc=2.","glibc=.2","glibc=2.3.4","glibc=02.3",
                         "glibc=2.03","glibc=18446744073709551616.0","glibc=-1.0","glibc=+1.0","glibc==2.4")):
        add(f"minimum-invalid-{i}",manifest(minimum=f)[0])
    for i in range(5):
        for n in (0,1,2**32-1,2**32,2**64,):
            values = [2,7,1,1,1]
            values[i] = n
            raw,wanted=manifest(formats=values)
            add(f"format-{KEYS[i]}-{n}",raw,wanted if 0<n<2**32 else BAD)
        add(f"format-noncanonical-{KEYS[i]}",baseline.replace(f"{KEYS[i]}={good['formats'][i]}".encode(),
                                                            f"{KEYS[i]}=01".encode(),1))
    for i,limit in enumerate(LIMITS):
        for n in (0,1,limit-1,limit,limit+1,2**64-1,2**64):
            sizes=[1]*4
            sizes[i]=n
            raw,wanted=manifest(sizes=sizes)
            add(f"size-{i}-{n}",raw,wanted if 0<n<=limit else BAD)
        for token in ("01","+1","-1","١"):
            parts=baseline.splitlines(keepends=True)
            words=parts[7+i].decode().split(" ")
            words[2]=token
            parts[7+i]=" ".join(words).encode()
            add(f"size-spelling-{i}-{ord(token[0])}-{len(token)}",b"".join(parts))
        for j,token in enumerate((DIGESTS[i][:-1],DIGESTS[i]+"0",DIGESTS[i].upper(),"g"+DIGESTS[i][1:])):
            add(f"digest-{i}-{j}",baseline.replace(DIGESTS[i].encode(),token.encode(),1))
    add("aggregate-exact",*manifest(sizes=LIMITS))
    add("aggregate-over",manifest(sizes=LIMITS[:3]+(LIMITS[3]+1,))[0])
    for name,old,new in (
        ("unknown-target",TARGETS[0],"x86_64-apple-darwin"),("wrong-family","glibc","GLIBC"),
        ("schema","manifest 1","manifest 2"),("unknown-notarized","notarized=no","notarized=0"),
        ("commit-upper",good["commit"],good["commit"].upper()),("commit-short",good["commit"],good["commit"][:-1]),
        ("wrong-exec-mode","0555","0755"),("wrong-data-mode","0444","0555"),
        ("short-mode","0555","555"),("wrong-name","0555 marmish\n","0555 ./marmish\n"),
        ("double-space","state profile","state  profile"),("leading-space","version "," version "),
        ("trailing-space","end\n","end \n"),("wrong-footer","end\n","done\n"),
        ("extra-token","signing none","signing none extra"),("duplicate-state-key","commands=1","config=1"),
        ("state-key-reordering","engine=1 config=1","config=1 engine=1"),
        ("multiple-invalid","version 0.3.1","version bad\ntarget bad"),
    ):
        add(name,baseline.replace(old.encode(),new.encode(),1))
    lines=baseline.splitlines(keepends=True)
    for i in range(12):
        add(f"missing-line-{i}",b"".join(lines[:i]+lines[i+1:]))
        add(f"duplicate-line-{i}",b"".join(lines[:i+1]+lines[i:]))
    for i in range(1,11):
        swapped=lines.copy()
        swapped[i-1],swapped[i]=swapped[i],swapped[i-1]
        add(f"swapped-line-{i}",b"".join(swapped))
    for name,raw in (("missing-lf",baseline[:-1]),("crlf",baseline.replace(b"\n",b"\r\n")),
                     ("tab",baseline.replace(b" ",b"\t",1)),("nul",b"\x00"+baseline),
                     ("bad-utf8",b"\xff"+baseline),("empty",b"")):
        add(name,raw)
    for size in (4095,4096,4097):
        add(f"framing-{size}",b"x"*(size-1)+b"\n")
    return tests


def mutant_expected(tests):
    # The changed license cap admits one more octet until the aggregate ceiling applies.
    expected={k:v for k,(_,v) in tests.items()}
    expected[f"size-3-{LIMITS[3]+1}"]=manifest(sizes=(1,1,1,LIMITS[3]+1))[1]
    return expected
