#[test]
fn manifest_contract_probe() {
    use crate::native_manifest::Manifest;
    use serde_json::{Value, json};
    let cases: Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("MANIFEST_CASES").unwrap()).unwrap(),
    )
    .unwrap();
    let rows: Vec<_> = cases.as_array().unwrap().iter().map(|case| {
        let bytes = hex::decode(case["hex"].as_str().unwrap()).unwrap();
        let result = match Manifest::parse(&bytes) {
            Ok(m) => json!({
                "version": m.version, "target": m.target, "commit": m.commit,
                "signing": m.signing, "notarized": m.notarized, "minimum": m.minimum,
                "formats": m.formats, "id": m.id(),
                "files": m.files.iter().map(|f| json!({
                    "name": f.name, "bytes": f.bytes, "mode": f.mode, "sha256": hex::encode(f.sha256),
                    "download": m.asset_name(f.name),
                })).collect::<Vec<_>>(),
            }),
            Err(error) => match error {
                crate::Error::Invalid(code) => json!({"error": "Invalid", "code": code, "display": error.to_string()}),
                other => panic!("unexpected error {other:?}"),
            },
        };
        json!({"case":case["case"],"result":result})
    }).collect();
    std::fs::write(
        std::env::var_os("MANIFEST_RESULT").unwrap(),
        serde_json::to_vec(&rows).unwrap(),
    )
    .unwrap();
}
