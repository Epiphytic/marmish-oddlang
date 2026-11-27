//! Compile real downstream programs against both sides of a source mutation.
use crate::{Result, capture, require, run, stdout, whole};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

#[derive(Clone, Copy, ValueEnum)]
pub enum Role {
    Records,
    Errors,
    Native,
}

pub fn observe(
    role: Role,
    library: &Path,
    deps: &Path,
    serde: &Path,
    work: &Path,
) -> Result<(String, Value)> {
    let library_name = match role {
        Role::Native => "marmish_native",
        _ => "marmish_engine",
    };
    fs::create_dir(work)?;
    let probe = work.join("probe");
    let compile = || -> Result<Command> {
        let mut cmd = Command::new("rustc");
        cmd.args(["+1.97.1", "--edition=2021", "-L"])
            .arg(format!("dependency={}", deps.display()))
            .arg("-L")
            .arg(format!(
                "dependency={}",
                serde.parent().ok_or("missing serde path")?.display()
            ))
            .arg("--extern")
            .arg(format!("{library_name}={}", library.display()))
            .arg("--extern")
            .arg(format!("serde={}", serde.display()))
            .arg("-o")
            .arg(&probe);
        Ok(cmd)
    };
    let fixture = match role {
        Role::Native => "engine-native-records/native.rs",
        Role::Records => "engine-native-records/engine.rs",
        Role::Errors => "engine-public-error/probe.rs",
    };
    run(compile()?.arg(whole().join(fixture)))?;
    let output = stdout(&mut Command::new(&probe))?;
    let mut negatives = json!({});
    let mut check = |name: &str, body: &str, expected: &str| -> Result {
        let source = work.join("negative.rs");
        fs::write(&source, body)?;
        let result = capture(compile()?.arg("--error-format=json").arg(source), 60, None)?;
        let mut codes = BTreeSet::new();
        for line in std::str::from_utf8(&result.stderr)?.lines() {
            let v: Value = serde_json::from_str(line)?;
            if v["level"] == "error"
                && let Some(code) = v["code"]["code"].as_str()
            {
                codes.insert(code.to_owned());
            }
        }
        require(
            !result.status.success() && codes == BTreeSet::from([expected.to_owned()]),
            format!("{name}: expected {expected}, got {codes:?}"),
        )?;
        negatives[name] = json!(expected);
        Ok(())
    };
    let types: &[(&str, &[&str])] = match role {
        Role::Native => &[(
            "marmish_native::transport::ArtiDirs",
            &[
                "Copy",
                "Default",
                "Eq",
                "PartialEq",
                "Ord",
                "PartialOrd",
                "Hash",
                "Display",
                "Serialize",
                "Deserialize",
            ],
        )],
        Role::Records => &[
            (
                "marmish_engine::InputReceipt",
                &[
                    "Copy",
                    "Default",
                    "Eq",
                    "PartialEq",
                    "Ord",
                    "PartialOrd",
                    "Hash",
                    "Display",
                    "Serialize",
                    "Deserialize",
                ],
            ),
            (
                "marmish_engine::MaintenanceReport",
                &[
                    "Copy",
                    "Eq",
                    "PartialEq",
                    "Ord",
                    "PartialOrd",
                    "Hash",
                    "Display",
                    "Serialize",
                    "Deserialize",
                ],
            ),
        ],
        Role::Errors => {
            const REASON: &[&str] = &[
                "Default",
                "Hash",
                "PartialOrd",
                "Ord",
                "Serialize",
                "Deserialize",
                "Display",
                "Error",
            ];
            &[
                (
                    "EngineError",
                    &[
                        "Clone",
                        "Copy",
                        "Default",
                        "Hash",
                        "PartialOrd",
                        "Ord",
                        "Serialize",
                        "Deserialize",
                        "FromIo",
                    ],
                ),
                ("NotFoundReason", REASON),
                ("NotExecutableReason", REASON),
                ("ResolutionReason", REASON),
            ]
        }
    };
    let imports = match role {
        Role::Records | Role::Native => "",
        Role::Errors => {
            "use marmish_engine::{EngineError,NotFoundReason,NotExecutableReason,ResolutionReason};"
        }
    };
    for &(ty, traits) in types {
        for &name in traits {
            let bound = match name {
                "Hash" => "std::hash::Hash",
                "Display" => "std::fmt::Display",
                "Serialize" => "serde::Serialize",
                "Deserialize" => "for<'de> serde::Deserialize<'de>",
                "Error" => "std::error::Error",
                "FromIo" => "From<std::io::Error>",
                other => other,
            };
            check(
                &format!("{ty}.{name}"),
                &format!(
                    "{imports} fn require<T: {bound}>(){{}} fn main(){{ require::<{ty}>(); }}"
                ),
                "E0277",
            )?;
        }
    }
    let cases: &[(&str, &str, &str)] = match role {
        Role::Native => &[("rootExposure", "E0432", "use marmish_native::ArtiDirs;")],
        Role::Records => &[
            (
                "privateModule",
                "E0603",
                "use marmish_engine::model::InputReceipt;",
            ),
            (
                "creditWidth",
                "E0308",
                "let v=0u64; let _=marmish_engine::InputReceipt{accepted:0,available_credit:v,duplicate:false};",
            ),
        ],
        Role::Errors => &[
            (
                "mixedReason",
                "E0308",
                "let _=EngineError::CommandNotFound(NotExecutableReason::NotRegularFile);",
            ),
            (
                "offsetWidth",
                "E0308",
                "let _=EngineError::InputBackpressure{next_offset:u128::MAX,available_credit:0};",
            ),
            (
                "creditWidth",
                "E0308",
                "let _=EngineError::InputBackpressure{next_offset:0,available_credit:u64::MAX};",
            ),
            (
                "privateModule",
                "E0603",
                "use marmish_engine::model::EngineError as Hidden;",
            ),
            (
                "nonExhaustiveMatch",
                "E0004",
                "match EngineError::Unauthorized { EngineError::Unauthorized => () }",
            ),
        ],
    };
    for &(name, code, body) in cases {
        check(name, &format!("{imports} fn main(){{{body}}}"), code)?;
    }
    Ok((output, negatives))
}
