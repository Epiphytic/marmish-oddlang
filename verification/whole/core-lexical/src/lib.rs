//! Source seals and pinned compiler observers shared by core qualification.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

pub type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
pub const ORIGINAL: &str = "af54339d250290490d62d4475a77539b7e739071";
pub const RUST_OBSERVER: &str = "b832a8cd9414f15a2c698761029cb76044da9a8f";
pub const LEXICAL_OBSERVER: &str = "d32e179075a660fffa0d3f0905bbe8ae8957c570";
pub const INHERENT_OBSERVER: &str = "ff480c701e02f1d11530303193990d643c1c7da9";
pub const PROJECTION: &str = "8a20ea283d0d43da42f054a7c7774eb9e6a20da7";
pub const LOCK: &str = "5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4";
pub const HOSTS: [&str; 2] = ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"];

pub fn lexical() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
pub fn whole() -> &'static Path {
    lexical().parent().unwrap()
}
pub fn portable() -> PathBuf {
    lexical()
        .ancestors()
        .nth(3)
        .unwrap()
        .join("oddlang/portable")
}
pub fn package() -> PathBuf {
    portable().join("core-package.json")
}
pub fn workspace_lock() -> PathBuf {
    portable().join("workspace-v7-canonical-offline/workspace.lock")
}
pub fn require(condition: bool, message: impl Into<String>) -> Result {
    if !condition {
        return Err(message.into().into());
    }
    Ok(())
}
pub fn txt(v: &Value) -> Result<&str> {
    Ok(v.as_str().ok_or("expected a string")?)
}
pub fn arr(v: &Value) -> Result<&Vec<Value>> {
    Ok(v.as_array().ok_or("expected an array")?)
}
pub fn object(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    Ok(v.as_object().ok_or("expected an object")?)
}
pub fn one<T>(mut values: impl Iterator<Item = T>) -> Result<T> {
    let value = values
        .next()
        .ok_or("expected exactly one matching entry, found none")?;
    require(
        values.next().is_none(),
        "expected exactly one matching entry, found several",
    )?;
    Ok(value)
}
pub fn sha(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}
pub fn load(path: &Path) -> Result<Value> {
    Ok(serde_json::from_reader(fs::File::open(path)?)?)
}
/// Keep the original ASCII-escaped, sorted JSON receipt encoding.
pub fn pretty(doc: &Value) -> Result<String> {
    let mut text = String::new();
    for ch in serde_json::to_string_pretty(doc)?.chars() {
        if ch < '\u{7f}' {
            text.push(ch);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                text.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    Ok(text)
}
pub fn save(path: &Path, doc: &Value) -> Result {
    fs::write(path, format!("{}\n", pretty(doc)?))?;
    Ok(())
}
pub fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output()?;
    require(
        out.status.success(),
        format!("{cmd:?}: {}", String::from_utf8_lossy(&out.stderr)),
    )?;
    Ok(String::from_utf8(out.stdout)?)
}
pub fn pinned_git(root: &Path, pin: &str) -> Result {
    require(
        output(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
            == pin,
        format!("unexpected Git revision in {}", root.display()),
    )?;
    require(
        output(Command::new("git").current_dir(root).args([
            "status",
            "--porcelain",
            "--untracked-files=all",
        ]))?
        .is_empty(),
        format!("expected a clean checkout at {}", root.display()),
    )
}

pub struct Observer {
    root: PathBuf,
    pub revision: String,
    target: Option<TempDir>,
}
impl Observer {
    pub fn new(root: &Path, historical: &str) -> Result<Self> {
        let revision = output(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "HEAD"]),
        )?
        .trim()
        .to_owned();
        require(
            revision == RUST_OBSERVER || revision == historical,
            format!("unexpected observer revision in {}", root.display()),
        )?;
        pinned_git(root, &revision)?;
        let target = if revision == RUST_OBSERVER {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        Ok(Self {
            root: root.to_owned(),
            revision,
            target,
        })
    }
    pub fn check(&self) -> Result {
        pinned_git(&self.root, &self.revision)
    }
    pub fn capture(
        &self,
        source: &Path,
        host: &str,
        dest: &Path,
        original: bool,
        inherent: bool,
    ) -> Result {
        let mut cmd = if let Some(build) = &self.target {
            let mut c = Command::new("cargo");
            c.args([
                "+1.96.1",
                "run",
                "--quiet",
                "--locked",
                "--offline",
                "--target-dir",
            ])
            .arg(build.path())
            .arg("--manifest-path")
            .arg(
                self.root
                    .join("verification/provider-validation/Cargo.toml"),
            )
            .args(["--bin", "release-rustdoc", "--"]);
            c
        } else {
            let mut c = Command::new("python3");
            c.arg("-B").arg(
                self.root
                    .join("verification/rust-recovery/release_rustdoc.py"),
            );
            c
        };
        cmd.arg(source)
            .args(["marmish-core", "lib", "marmish_core", host]);
        if inherent {
            cmd.arg("--public-inherent");
        }
        cmd.arg("--out").arg(dest);
        if original {
            cmd.args(["--revision", ORIGINAL]);
        }
        let status = cmd.status()?;
        require(
            status.success(),
            format!("pinned release observer exited {status}: {cmd:?}"),
        )
    }
}

pub fn sealed_files(lock: &Value, lock_hash: &str) -> Result<Value> {
    let mut files = json!({"package.lock":lock_hash,"Cargo.toml":txt(&lock["cargoManifestSha256"])?,"Cargo.lock":txt(&lock["cargoLockSha256"])?});
    for (name, digest) in object(&lock["generatedFiles"])? {
        files[name] = digest.clone();
    }
    Ok(files)
}

fn contents(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> Result {
    for entry in fs::read_dir(dir)? {
        let e = entry?;
        let path = e.path();
        let kind = e.file_type()?;
        if kind.is_dir() {
            contents(root, &path, files)?;
        } else if kind.is_file() {
            files.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF-8 path")?
                    .into(),
                sha(&path)?,
            );
        } else {
            return Err("generated package includes an unexpected filesystem entry".into());
        }
    }
    Ok(())
}
pub fn snapshot(candidate: &Path) -> Result<Value> {
    require(sha(&workspace_lock())? == LOCK, "canonical lock changed")?;
    let lock = load(&workspace_lock())?;
    let member = &lock["members"]["marmish-core"];
    let pkg_hash = sha(&package())?;
    require(
        member["inputSha256"] == pkg_hash,
        "core package input changed",
    )?;
    let workspace = portable().join("workspace-v7-canonical-offline.json");
    let ws_hash = sha(&workspace)?;
    require(lock["inputSha256"] == ws_hash, "canonical manifest changed")?;
    require(
        load(&workspace)?["members"]["marmish-core"]["manifest"]
            == json!({"path":"core-package.json","sha256":pkg_hash}),
        "canonical workspace points to a different core manifest",
    )?;
    let layout = load(&package())?;
    let check = |pin: &Value| -> Result {
        let name = txt(&pin["path"])?;
        require(
            pin["sha256"] == sha(&portable().join(name))?,
            format!("core input changed: {name}"),
        )
    };
    check(&layout["cargoLock"])?;
    for c in arr(&layout["components"])? {
        check(&c["graph"])?;
        check(&c["overlay"])?;
    }
    let product_lock = candidate.join("package.lock");
    let lock_hash = sha(&product_lock)?;
    require(
        member["standalonePackageLockSha256"] == lock_hash,
        "standalone package lock changed",
    )?;
    let product = load(&product_lock)?;
    require(
        product["manifestSha256"] == pkg_hash,
        "package lock points to another manifest",
    )?;
    let expected = sealed_files(&product, &lock_hash)?;
    let mut actual = BTreeMap::new();
    contents(candidate, candidate, &mut actual)?;
    require(
        object(&expected)?.keys().eq(actual.keys()),
        "unexpected generated file inventory",
    )?;
    for (name, digest) in &actual {
        require(
            expected[name] == *digest,
            format!("generated file changed: {name}"),
        )?;
    }
    Ok(
        json!({"workspaceManifestSha256":ws_hash,"workspaceLockSha256":LOCK,"coreManifestSha256":pkg_hash,"generatedFiles":actual}),
    )
}
