//! Independently authored limits and expected public values for release manifests.
use serde_json::{Value, json};
use std::fmt::Write;
const NAMES: [&str; 4] = [
    "marmish",
    "marmishd",
    "marmish-worker",
    "THIRD-PARTY-LICENSES",
];
const TARGETS: [&str; 3] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
];
const KEYS: [&str; 5] = ["profile", "journal", "engine", "config", "commands"];
const LIMITS: [u128; 4] = [256 << 20, 256 << 20, 256 << 20, 16 << 20];
fn bad() -> Value {
    json!({"error":"Invalid","code":"install_manifest","display":"invalid input: install_manifest"})
}
fn digest(i: usize) -> String {
    hex::encode((i as u8..i as u8 + 32).collect::<Vec<_>>())
}
#[derive(Clone)]
struct Manifest<'a> {
    version: &'a str,
    target: &'a str,
    sign: &'a str,
    notarized: bool,
    minimum: &'a str,
    formats: [u128; 5],
    sizes: [u128; 4],
}
impl Default for Manifest<'_> {
    fn default() -> Self {
        Self {
            version: "0.3.1",
            target: TARGETS[0],
            sign: "none",
            notarized: false,
            minimum: "glibc=2.39",
            formats: [2, 7, 1, 1, 1],
            sizes: [1, 23, 321, 5],
        }
    }
}
impl Manifest<'_> {
    fn render(&self) -> String {
        let commit = hex::encode((0..20).collect::<Vec<u8>>());
        let yes = if self.notarized { "yes" } else { "no" };
        let mut text = format!(
            "marmish-manifest 1\nversion {}\ntarget {}\ncommit {commit}\nsigning {} notarized={yes}\nminimum {}\nstate",
            self.version, self.target, self.sign, self.minimum
        );
        for (key, n) in KEYS.iter().zip(self.formats) {
            write!(text, " {key}={n}").unwrap();
        }
        text.push('\n');
        for (i, name) in NAMES.iter().enumerate() {
            writeln!(
                text,
                "file {} {} {} {name}",
                digest(i),
                self.sizes[i],
                if i == 3 { "0444" } else { "0555" }
            )
            .unwrap();
        }
        text.push_str("end\n");
        text
    }
    fn expected(&self) -> Value {
        let id = format!("{}-{}", self.version, self.target);
        let files=NAMES.iter().enumerate().map(|(i,name)|json!({"name":name,"bytes":u64::try_from(self.sizes[i]).unwrap(),"mode":if i==3{0o444}else{0o555},"sha256":digest(i),"download":format!("{name}-{id}")})).collect::<Vec<_>>();
        json!({"version":self.version,"target":self.target,"commit":hex::encode((0..20).collect::<Vec<u8>>()),"signing":self.sign,"notarized":self.notarized,"minimum":self.minimum,
            "formats":self.formats.map(|n|u64::try_from(n).unwrap()),"id":id,"files":files})
    }
}
pub struct Case {
    pub name: String,
    pub raw: Vec<u8>,
    pub result: Value,
}
#[derive(Default)]
struct Corpus(Vec<Case>);
impl Corpus {
    fn add(&mut self, name: impl Into<String>, raw: impl Into<Vec<u8>>, result: Value) {
        self.0.push(Case {
            name: name.into(),
            raw: raw.into(),
            result,
        });
    }
    fn invalid(&mut self, name: impl Into<String>, raw: impl Into<Vec<u8>>) {
        self.add(name, raw, bad());
    }
    fn manifest(&mut self, name: impl Into<String>, m: Manifest, valid: bool) {
        self.add(name, m.render(), if valid { m.expected() } else { bad() });
    }
}
pub fn cases(changed_limit: bool) -> Vec<Case> {
    let mut c = Corpus::default();
    let base = Manifest::default();
    let raw = base.render();
    c.manifest("baseline", base.clone(), true);
    for (i, target) in TARGETS.iter().enumerate() {
        for family in ["glibc", "macos"] {
            for sign in ["none", "adhoc", "developer-id"] {
                for notarized in [false, true] {
                    let valid = if i < 2 {
                        family == "glibc" && sign == "none" && !notarized
                    } else {
                        family == "macos"
                            && (sign == "developer-id" || (sign == "adhoc" && !notarized))
                    };
                    c.manifest(
                        format!("platform-{i}-{family}-{sign}-{}", u8::from(notarized)),
                        Manifest {
                            target,
                            sign,
                            notarized,
                            minimum: &format!("{family}=0.18446744073709551615"),
                            ..base.clone()
                        },
                        valid,
                    );
                }
            }
        }
    }
    for version in [
        "0.2.0",
        "0.3.0-rc.1",
        "0.2.18446744073709551615-rc.18446744073709551615",
    ] {
        c.manifest(
            format!("version-valid-{version}"),
            Manifest {
                version,
                ..base.clone()
            },
            true,
        );
    }
    for (i, version) in [
        "0.1.0",
        "0.4.0",
        "0.3.",
        "0.3.00",
        "+0.3.1",
        "0.3.1-rc.0",
        "0.3.1-rc.01",
        "0.3.18446744073709551616",
        "0.3.1-rc.18446744073709551616",
        "0.3.1-rc.1+meta",
        "0.3.1-rc.1-rc.2",
        "0.3.é",
        "0.3.١",
    ]
    .iter()
    .enumerate()
    {
        c.manifest(
            format!("version-invalid-{i}"),
            Manifest {
                version,
                ..base.clone()
            },
            false,
        );
    }
    for minimum in ["glibc=2.0", "glibc=18446744073709551615.0"] {
        c.manifest(
            format!("minimum-valid-{minimum}"),
            Manifest {
                minimum,
                ..base.clone()
            },
            true,
        );
    }
    for (i, minimum) in [
        "glibc=2",
        "glibc=2.",
        "glibc=.2",
        "glibc=2.3.4",
        "glibc=02.3",
        "glibc=2.03",
        "glibc=18446744073709551616.0",
        "glibc=-1.0",
        "glibc=+1.0",
        "glibc==2.4",
        "glibc=2.18446744073709551616",
    ]
    .iter()
    .enumerate()
    {
        c.manifest(
            format!("minimum-invalid-{i}"),
            Manifest {
                minimum,
                ..base.clone()
            },
            false,
        );
    }
    for (i, key) in KEYS.iter().enumerate() {
        for n in [0, 1, (1 << 32) - 1, 1 << 32, 1 << 64] {
            let mut m = base.clone();
            m.formats[i] = n;
            c.manifest(format!("format-{key}-{n}"), m, n > 0 && n < (1 << 32));
        }
        c.invalid(
            format!("format-noncanonical-{key}"),
            raw.replacen(
                &format!("{key}={}", base.formats[i]),
                &format!("{key}=01"),
                1,
            ),
        );
    }
    for (i, limit) in LIMITS.into_iter().enumerate() {
        for n in [0, 1, limit - 1, limit, limit + 1, (1 << 64) - 1, 1 << 64] {
            let mut m = Manifest {
                sizes: [1; 4],
                ..base.clone()
            };
            m.sizes[i] = n;
            let allowed = limit + u128::from(changed_limit && i == 3);
            c.manifest(format!("size-{i}-{n}"), m, n > 0 && n <= allowed);
        }
        let hash = digest(i);
        for token in ["01", "+1", "-1", "١"] {
            let prefix = format!("file {hash} ");
            c.invalid(
                format!(
                    "size-spelling-{i}-{}-{}",
                    token.chars().next().unwrap() as u32,
                    token.chars().count()
                ),
                raw.replacen(
                    &format!("{prefix}{} ", base.sizes[i]),
                    &format!("{prefix}{token} "),
                    1,
                ),
            );
        }
        for (j, v) in [
            hash[..63].to_owned(),
            format!("{hash}0"),
            hash.to_uppercase(),
            format!("g{}", &hash[1..]),
        ]
        .iter()
        .enumerate()
        {
            c.invalid(format!("digest-{i}-{j}"), raw.replacen(&hash, v, 1));
        }
    }
    c.manifest(
        "aggregate-exact",
        Manifest {
            sizes: LIMITS,
            ..base.clone()
        },
        true,
    );
    let mut sizes = LIMITS;
    sizes[3] += 1;
    c.manifest(
        "aggregate-over",
        Manifest {
            sizes,
            ..base.clone()
        },
        false,
    );
    corruptions(&mut c, &raw);
    c.0
}
fn corruptions(c: &mut Corpus, raw: &str) {
    let commit = hex::encode((0..20).collect::<Vec<u8>>());
    let hash = digest(0);
    for (name, old, new) in [
        ("unknown-target", TARGETS[0], "x86_64-apple-darwin"),
        ("wrong-family", "glibc", "GLIBC"),
        ("schema", "manifest 1", "manifest 2"),
        ("unknown-notarized", "notarized=no", "notarized=0"),
        ("commit-upper", &commit, &commit.to_uppercase()),
        ("commit-short", &commit, &commit[..39]),
        ("commit-long", &commit, &format!("{commit}0")),
        ("commit-nonhex", &commit, &format!("g{}", &commit[1..])),
        ("state-extra-token", "commands=1\n", "commands=1 extra=1\n"),
        ("file-extra-token", "0555 marmish\n", "0555 marmish extra\n"),
        (
            "file-keyword",
            &format!("file {hash}"),
            &format!("File {hash}"),
        ),
        ("wrong-exec-mode", "0555", "0755"),
        ("wrong-data-mode", "0444", "0555"),
        ("short-mode", "0555", "555"),
        ("wrong-name", "0555 marmish\n", "0555 ./marmish\n"),
        ("double-space", "state profile", "state  profile"),
        ("leading-space", "version ", " version "),
        ("trailing-space", "end\n", "end \n"),
        ("wrong-footer", "end\n", "done\n"),
        ("extra-token", "signing none", "signing none extra"),
        ("duplicate-state-key", "commands=1", "config=1"),
        (
            "state-key-reordering",
            "engine=1 config=1",
            "config=1 engine=1",
        ),
    ] {
        c.invalid(name, raw.replacen(old, new, 1));
    }
    c.invalid(
        "multiple-invalid",
        raw.replacen("version 0.3.1", "version bad", 1).replacen(
            &format!("target {}", TARGETS[0]),
            "target bad",
            1,
        ),
    );
    let lines: Vec<_> = raw.split_inclusive('\n').collect();
    for i in 0..12 {
        let mut omitted = lines.clone();
        omitted.remove(i);
        c.invalid(format!("missing-line-{i}"), omitted.concat());
        let mut duplicate = lines.clone();
        duplicate.insert(i, lines[i]);
        c.invalid(format!("duplicate-line-{i}"), duplicate.concat());
    }
    for i in 1..11 {
        let mut swapped = lines.clone();
        swapped.swap(i - 1, i);
        c.invalid(format!("swapped-line-{i}"), swapped.concat());
    }
    c.invalid("missing-lf", &raw[..raw.len() - 1]);
    c.invalid("crlf", raw.replace('\n', "\r\n"));
    c.invalid("tab", raw.replacen(' ', "\t", 1));
    c.invalid("nul", [&[0][..], raw.as_bytes()].concat());
    c.invalid("bad-utf8", [&[255][..], raw.as_bytes()].concat());
    c.invalid("empty", "");
    for n in [4095, 4096, 4097] {
        c.invalid(format!("framing-{n}"), "x".repeat(n - 1) + "\n");
    }
}
