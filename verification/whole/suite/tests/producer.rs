//! Exercise the same selection CLI that supplies the live producer's registration decisions.
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};
use suite_receipts::{read, suite};

fn selection(args: &[&str], input: &Value) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_suite-selection"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input).unwrap())
        .unwrap();
    process.wait_with_output().unwrap()
}
#[test]
fn rejects_an_equal_count_substitution_in_the_named_baseline() {
    let host = "aarch64-apple-darwin";
    let mut baseline =
        read(&suite().join(format!("canonical-{host}.json"))).unwrap()["harnesses"].clone();
    let args = ["check-baseline", host];
    let accepted = selection(&args, &baseline);
    assert!(accepted.status.success());
    assert_eq!(
        serde_json::from_slice::<String>(&accepted.stdout)
            .unwrap()
            .len(),
        64
    );
    let native = &mut baseline["marmish-native/lib/marmish_native"];
    let passed = native["passed"][0].clone();
    native["passed"][0] = native["unverified"][0].clone();
    native["unverified"][0] = passed;
    let wrong = selection(&args, &baseline);
    assert!(
        !wrong.status.success()
            && String::from_utf8_lossy(&wrong.stderr).contains("named baseline differs")
    );
}
#[test]
fn requires_the_exact_registration_list() {
    let args = ["registered", "marmish-core/test/contracts"];
    let expected = json!(["test_a", "test_b"]);
    let ok = selection(&args, &json!(["test_b: test\ntest_a: test\n", expected]));
    assert!(ok.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&ok.stdout).unwrap(),
        expected
    );
    for listing in [
        "test_a: test\ntest_c: test\n",
        "test_a: test\ntest_b: test\ntest_b: test\n",
        "",
    ] {
        let bad = selection(&args, &json!([listing, expected]));
        assert!(
            !bad.status.success()
                && String::from_utf8_lossy(&bad.stderr).contains("unsafe or incomplete")
        );
    }
}
