const MODULE_TEST: &str = r#"
#[cfg(test)]
mod oddlang_release_version_scope {
    #[test]
    fn matches_native_contract() {
        let cases: &[(&[u8], bool)] = &[@CASES@];
        for (input, expected) in cases {
            let value = ::std::str::from_utf8(input).unwrap();
            assert_eq!(super::version(value), *expected, "{value:?}");
        }
    }
}
"#;
use super::*;
pub type Cases = Vec<(String, bool)>;
pub fn cargo_test(root: &Path, target: &Path, name: &str) -> Result {
    let bytes = run(Command::new("cargo")
        .current_dir(root)
        .args([
            "+1.97.1",
            "test",
            "--locked",
            "--offline",
            "--release",
            "--target",
            HOST,
            "--target-dir",
        ])
        .arg(target)
        .args([
            "--package",
            "marmish-native",
            "--lib",
            "--",
            "--exact",
            name,
        ]))?;
    let output = std::str::from_utf8(&bytes)?;
    require(
        output
            .lines()
            .any(|line| line == format!("test {name} ... ok"))
            && output.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"),
        format!("missing exact native test {name}"),
    )
}
pub fn enclosing(reference: &Path, target: &Path, module: &str, cases: &Cases) -> Result<Value> {
    let rows = cases
        .iter()
        .map(|(s, expected)| format!("(&{:?}, {expected})", s.as_bytes()))
        .collect::<Vec<_>>()
        .join(",\n");
    let witness = MODULE_TEST.replace("@CASES@", &rows);
    fs::write(reference.join(SOURCE), format!("{module}{witness}"))?;
    let name = "native_manifest::oddlang_release_version_scope::matches_native_contract";
    cargo_test(reference, target, name)?;
    Ok(
        json!({"name":name,"matchingCases":cases.len(),"witnessSha256":bytes_sha(witness.as_bytes())}),
    )
}
pub fn observe(
    work: &Path,
    name: &str,
    code: &str,
    entry: &str,
    cases: &Cases,
    policy: Option<&str>,
) -> Result<String> {
    let rows = cases
        .iter()
        .map(|(s, _)| format!("&{:?}", s.as_bytes()))
        .collect::<Vec<_>>()
        .join(",\n");
    let source = work.join(format!("{name}.rs"));
    let bin = work.join(name);
    fs::write(
        &source,
        format!(
            "mod predicate {{\n{code}\n}}\nfn main() {{\n let inputs: &[&[u8]] = &[{rows}];\n for bytes in inputs {{println!(\"{{}}\", u8::from(predicate::{entry}(std::str::from_utf8(bytes).unwrap())));}}\n}}\n"
        ),
    )?;
    run(command(work, policy, "rustc")
        .args(["+1.97.1", "--edition=2024"])
        .arg(source)
        .arg("-o")
        .arg(&bin))?;
    let output = run(&mut command(work, policy, &bin))?;
    let expected = cases
        .iter()
        .map(|(_, v)| if *v { "1\n" } else { "0\n" })
        .collect::<String>();
    require(
        output == expected.as_bytes(),
        format!("unexpected {name} results"),
    )?;
    Ok(bytes_sha(&output))
}
