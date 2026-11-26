#[test]
fn inert_catalog_guard_observations() {
    use std::os::unix::fs::PermissionsExt;

    fn classify(value: std::result::Result<Manifest, crate::Error>) -> &'static str {
        match value {
            Ok(_) => "accepted",
            Err(crate::Error::Invalid("install_manifest_path")) => "candidate",
            Err(crate::Error::Invalid("install_digest")) => "digest",
            Err(crate::Error::Invalid("install_manifest")) => "syntax",
            Err(crate::Error::Invalid("unsafe_install_path")) => "descriptor",
            Err(crate::Error::Invalid("install_asset")) => "metadata",
            Err(error) => panic!("unexpected local directory error: {error:?}"),
        }
    }

    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    let check = |naming, pin| classify(verify_directory(dir, naming, pin));
    let selected = AssetNaming::VersionedDownloads;
    let fixed = AssetNaming::InstalledFixedNames;
    let text = sample().replace(&"b".repeat(64), &hex::encode(Sha256::digest(b"x")));
    let manifest = Manifest::parse(text.as_bytes()).unwrap();
    let pinned = Sha256::digest(text.as_bytes()).into();
    let name = dir.join(format!("{}.manifest", manifest.asset_name(NAMES[0])));
    let decoy = dir.join("marmish-0.2.decoy.manifest");
    let mut cases = Vec::new();

    cases.push(("none", check(selected, pinned), "candidate"));
    fs::write(&name, b"bad").unwrap();
    cases.push((
        "caller-digest-before-syntax",
        check(selected, pinned),
        "digest",
    ));
    cases.push((
        "bad-syntax",
        check(selected, Sha256::digest(b"bad").into()),
        "syntax",
    ));
    fs::write(&decoy, b"bad").unwrap();
    cases.push((
        "ambiguity-before-caller-digest",
        check(selected, pinned),
        "candidate",
    ));
    fs::remove_file(&name).unwrap();
    fs::write(&decoy, &text).unwrap();
    // Use the same accepted prefix as the main candidate but an inconsistent suffix body.
    let wrong = dir.join("marmish-0.2.wrong.manifest");
    fs::rename(&decoy, &wrong).unwrap();
    cases.push((
        "valid-content-wrong-name",
        check(selected, pinned),
        "candidate",
    ));
    fs::remove_file(&wrong).unwrap();
    fs::write(&name, vec![b' '; 4097]).unwrap();
    cases.push((
        "bounded-before-caller-digest",
        check(selected, pinned),
        "descriptor",
    ));
    fs::write(&name, &text).unwrap();
    for asset in NAMES {
        fs::write(dir.join(manifest.asset_name(asset)), b"x").unwrap();
    }
    cases.push(("complete-selected", check(selected, pinned), "accepted"));
    let content = dir.join(manifest.asset_name(NAMES[0]));
    fs::write(&content, b"y").unwrap();
    cases.push((
        "same-length-changed-asset",
        check(selected, pinned),
        "digest",
    ));
    fs::write(&content, b"").unwrap();
    cases.push(("short-asset", check(selected, pinned), "metadata"));
    fs::write(&content, b"xx").unwrap();
    cases.push(("long-asset", check(selected, pinned), "descriptor"));

    fs::write(dir.join("manifest"), &text).unwrap();
    for asset in &manifest.files {
        let path = dir.join(asset.name);
        fs::write(&path, b"x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(asset.mode)).unwrap();
    }
    cases.push(("complete-fixed", check(fixed, pinned), "accepted"));
    fs::set_permissions(dir.join(NAMES[0]), fs::Permissions::from_mode(0o600)).unwrap();
    cases.push(("fixed-mode-mismatch", check(fixed, pinned), "metadata"));
    for (name, result, expected) in cases {
        assert_eq!(result, expected, "{name}");
        println!("CATALOG_CASE={name}:{result}");
    }
}
