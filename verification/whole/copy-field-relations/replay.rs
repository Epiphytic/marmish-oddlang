use base64::{Engine, engine::general_purpose::STANDARD};
use clap::{Parser, Subcommand};
use copy_manifest_replay::{
    Cases,
    manifest::{Record, Time, base, fault, frame},
    verify, workspace_cases,
};
#[cfg(test)]
use public_data_qualification::assembly::bytes_sha;
use public_data_qualification::{Result, whole};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const PIN: &str = "7588c8288b89e4a4abc7cfbd110c9b71305377f7";
const VERSION: &str = "v1_.a";
const TARGET: &str = "aarch64-apple-darwin";
const COMMIT: [u8; 20] = [111; 20];
const KEYS: [&str; 10] = [
    "version",
    "transfer",
    "client",
    "server",
    "direction",
    "source",
    "destination",
    "overwrite",
    "preserve_mode",
    "preserve_mtime",
];
fn launch(r: &Record, mode: &str) -> Value {
    let raw = json!(r);
    let mut value = json!({});
    for k in KEYS {
        value[k] = raw[k].clone();
    }
    for (key, identity) in [
        ("client", r.client),
        ("server", r.server),
        ("transfer", r.transfer),
    ] {
        value[key] = json!(
            identity
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    value["source"] = json!(STANDARD.encode(&r.source));
    value["destination"] = json!(STANDARD.encode(&r.destination));
    value["mode"] = json!(mode);
    value
}
fn launch_digest(
    r: &Record,
    mode: &str,
    codec: &str,
) -> std::result::Result<[u8; 32], &'static str> {
    r.header()?;
    let mut preimage = b"marmish-copy-launch/1\0".to_vec();
    preimage.extend(r.version.to_be_bytes());
    preimage.push(if mode == "create" { 1 } else { 2 });
    r.fields(&mut preimage);
    preimage.push(u8::from(codec == "zlib"));
    Ok(Sha256::digest(preimage).into())
}
fn context(r: &Record, digest: [u8; 32]) -> Vec<u8> {
    let mut b = b"marmish-copy-context/1\0".to_vec();
    b.extend(1u32.to_be_bytes());
    b.extend(1u32.to_be_bytes());
    b.extend(r.client);
    b.extend(r.server);
    frame(&mut b, &[0, 255, 128]);
    b.extend([42; 16]);
    b.extend([24; 16]);
    b.extend(r.transfer);
    b.push(r.tag());
    b.extend(digest);
    b.extend(604860u64.to_be_bytes());
    frame(&mut b, VERSION.as_bytes());
    frame(&mut b, TARGET.as_bytes());
    b.extend(COMMIT);
    frame(&mut b, b"__copy-helper-v1");
    b
}
fn cases() -> Cases {
    let b = base();
    let mut requests = vec![];
    let mut expected = vec![];
    {
        let mut matches = |entry: &Record,
                           mode: &str,
                           record: &Record,
                           force: Option<(&str, Value)>| {
            let mut req =
                json!({"operation":"match","launch":launch(entry,mode),"manifest":record.encode()});
            let mut decoded = json!(record);
            if let Some((k, v)) = force {
                req[format!("force_{k}")] = v.clone();
                decoded[k] = v;
            }
            let actual = json!(entry);
            expected.push(json!(KEYS.iter().all(|&k| actual[k] == decoded[k])));
            requests.push(req);
        };
        matches(&b, "create", &b, None);
        matches(
            &b,
            "resume",
            &Record {
                mode: Some(0o700),
                size: 0,
                sha256: [11; 32],
                mtime: Some(Time {
                    seconds: 99,
                    nanos: 5,
                }),
                ..b.clone()
            },
            None,
        );
        matches(
            &Record {
                version: 2,
                ..b.clone()
            },
            "create",
            &b,
            Some(("version", json!(2))),
        );
        matches(
            &Record {
                source: vec![0],
                ..b.clone()
            },
            "create",
            &b,
            Some(("source", json!([0]))),
        );
        for variant in variations(&b) {
            matches(&variant, "create", &b, None);
        }
    }
    for mode in ["create", "resume"] {
        for codec in ["none", "zlib"] {
            let digest = launch_digest(&b, mode, codec).unwrap();
            let bytes = context(&b, digest);
            let mut start = |entry: &Record, c: &str, invalid: bool| {
                requests.push(json!({"operation":"start","context":bytes,"invalid_context":invalid,"launch":launch(entry,mode),"codec":c}));
                expected.push(if invalid {
                    fault("copy_context")
                } else if (entry.client, entry.server, entry.transfer, &entry.direction)
                    != (b.client, b.server, b.transfer, &b.direction)
                {
                    fault("copy_context_launch")
                } else {
                    match launch_digest(entry, mode, c) {
                        Err(e) => fault(e),
                        Ok(v) if v != digest => fault("copy_context_launch"),
                        Ok(_) => json!({"ok":null}),
                    }
                });
            };
            start(&b, codec, false);
            start(&b, codec, true);
            start(&b, if codec == "none" { "zlib" } else { "none" }, false);
            start(
                &Record {
                    version: 0,
                    source: vec![0],
                    ..b.clone()
                },
                codec,
                false,
            );
            start(
                &Record {
                    source: vec![0],
                    ..b.clone()
                },
                codec,
                false,
            );
            start(
                &Record {
                    destination: vec![],
                    ..b.clone()
                },
                codec,
                false,
            );
            for mut r in [
                Record {
                    client: [6; 32],
                    ..b.clone()
                },
                Record {
                    server: [7; 32],
                    ..b.clone()
                },
                Record {
                    transfer: [5; 32],
                    ..b.clone()
                },
                Record {
                    direction: "get".into(),
                    ..b.clone()
                },
            ] {
                r.source = vec![0];
                start(&r, codec, false);
                start(&r, codec, true);
            }
        }
    }
    let ctx = context(&b, [37; 32]);
    for invalid in [false, true] {
        let mut build = |v: &str, t: &str, c: [u8; 20]| {
            requests.push(json!({"operation":"build","context":ctx,"invalid_context":invalid,"version":v,"target":t,"commit":c}));
            expected.push(if invalid {
                fault("copy_context")
            } else if (v, t, c) != (VERSION, TARGET, COMMIT) {
                fault("copy_context_build")
            } else {
                json!({"ok":null})
            });
        };
        build(VERSION, TARGET, COMMIT);
        for v in ["", "v1_.A", "é", "v/1", &"v".repeat(65)] {
            build(v, TARGET, COMMIT)
        }
        for t in ["", "Aarch64-apple-darwin", &"x".repeat(65)] {
            build(VERSION, t, COMMIT)
        }
        let mut changed = COMMIT;
        changed[19] = 1;
        for commit in [[0; 20], changed] {
            build(VERSION, TARGET, commit)
        }
        build("?", "!", [0; 20]);
    }
    let req = requests
        .iter()
        .zip(&expected)
        .find(|(q, e)| {
            q["operation"] == "start"
                && q["launch"]["source"] == "AA=="
                && q["invalid_context"] == false
                && **e == fault("copy_context_launch")
        })
        .unwrap()
        .0
        .clone();
    let mut counts = json!({});
    for op in ["match", "start", "build"] {
        counts[op] = json!(requests.iter().filter(|r| r["operation"] == op).count());
    }
    Cases {
        requests,
        expected,
        metadata: json!({"cases":counts}),
        mutant: vec![(req, fault("copy_path"))],
        negative: "advancing the fallible digest before identity checks converts copy_context_launch into copy_path",
    }
}
fn variations(b: &Record) -> Vec<Record> {
    macro_rules! edited {
        ($f:ident,$v:expr) => {
            Record {
                $f: $v,
                ..b.clone()
            }
        };
    }
    vec![
        edited!(version, 2),
        edited!(transfer, [5; 32]),
        edited!(client, [6; 32]),
        edited!(server, [7; 32]),
        edited!(direction, "get".into()),
        edited!(source, vec![12]),
        edited!(destination, vec![13]),
        edited!(overwrite, false),
        edited!(preserve_mode, false),
        edited!(preserve_mtime, false),
    ]
}
fn mutate(g: &mut Value) -> Result {
    let mut ops = g["relations"]
        .as_object_mut()
        .ok_or("missing relations")?
        .iter_mut()
        .filter(|(k, _)| k.ends_with(":validate-start"));
    let (_, op) = ops.next().ok_or("missing validate-start")?;
    assert!(ops.next().is_none());
    let steps = op["steps"].as_array_mut().ok_or("missing steps")?;
    let last = steps.pop().ok_or("empty steps")?;
    steps.insert(1, last);
    Ok(())
}
// The recovered-source control deliberately compares transfer with the server identity.
fn correspondence_rows() -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let cases = cases();
    let (mut requests, mut original): (Vec<_>, Vec<_>) = cases
        .requests
        .into_iter()
        .zip(cases.expected)
        .filter(|(q, _)| q["operation"] == "match")
        .unzip();
    let mut paired = base();
    std::mem::swap(&mut paired.server, &mut paired.transfer);
    let entry = Record {
        transfer: paired.server,
        ..paired.clone()
    };
    let mut changed = vec![json!(false); requests.len()];
    requests.push(
        json!({"operation":"match","launch":launch(&entry,"create"),"manifest":paired.encode()}),
    );
    original.push(json!(false));
    changed.push(json!(true));
    (requests, original, changed)
}
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Correspondence {
        original: PathBuf,
        generated: PathBuf,
        mutant_original: PathBuf,
        mutant_generated: PathBuf,
        work: PathBuf,
    },
    Verify {
        original: PathBuf,
        framework: PathBuf,
        receipt: PathBuf,
    },
    Workspace {
        original: PathBuf,
        generated: PathBuf,
        work: PathBuf,
    },
}
fn main() -> Result {
    match Args::parse().mode {
        Mode::Correspondence {
            original,
            generated,
            mutant_original,
            mutant_generated,
            work,
        } => {
            let (requests, normal, changed) = correspondence_rows();
            let caller = whole().join("copy-field-relations/caller.rs");
            for (label, root, lock, expected) in [
                (
                    "reference",
                    original.join("crates/marmish-stream"),
                    original.join("Cargo.lock"),
                    &normal,
                ),
                (
                    "recovered",
                    generated.clone(),
                    generated.join("Cargo.lock"),
                    &normal,
                ),
                (
                    "mutant-reference",
                    mutant_original.join("crates/marmish-stream"),
                    mutant_original.join("Cargo.lock"),
                    &changed,
                ),
                (
                    "mutant-recovered",
                    mutant_generated.clone(),
                    mutant_generated.join("Cargo.lock"),
                    &changed,
                ),
            ] {
                let exe =
                    copy_checkpoint_replay::consumer(&root, &work.join(label), &lock, &caller)?;
                copy_checkpoint_replay::compare(label, &exe, &requests, expected)?;
            }
            println!(
                "{}",
                json!({"casesPerImplementation":requests.len(),"changedCases":normal.iter().zip(changed).filter(|(a,b)|**a!=*b).count()})
            );
            Ok(())
        }
        Mode::Verify {
            original,
            framework,
            receipt,
        } => verify(
            "copy-field-relations",
            PIN,
            &original,
            &framework,
            &receipt,
            cases(),
            mutate,
        ),
        Mode::Workspace {
            original,
            generated,
            work,
        } => {
            let v = workspace_cases(
                &original,
                &generated,
                &work,
                &whole().join("copy-field-relations/caller.rs"),
                &cases(),
            )?;
            println!("{v}");
            Ok(())
        }
    }
}
#[test]
fn archived_observations() {
    let mut rows = json!(correspondence_rows());
    rows.sort_all_objects();
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&rows).unwrap()),
        "caa1f4729655ecd77335bbed7f9277ef07600401f3c215f03226adade4e57e3e"
    );
    let mut rows = json!(cases().requests);
    rows.sort_all_objects();
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&rows).unwrap()),
        "71e6e7be8b78d90c64367f3a5f16c115405b03dbb577e79b8b13b21d78b52702"
    );
    assert_eq!(
        bytes_sha(&serde_json::to_vec(&cases().expected).unwrap()),
        "2fdc49c664670af630a0deb4661321bd158922d3fef3dac4861e439d7f09bf87"
    );
}
