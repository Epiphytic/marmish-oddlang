//! Common pin staging and locked external test consumer for the codec and window contracts.
use public_data_qualification::{Result, assembly, capture, load, portable, require};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn stage(work: &Path, filename: &str) -> Result<(PathBuf, Value)> {
    fs::create_dir_all(work)?;
    let root = portable();
    let plan = root.join(filename);
    let manifest = load(&plan)?;
    assembly::pinned_copy(&root, &manifest["cargoLock"], work)?;
    for c in manifest["components"]
        .as_array()
        .ok_or("expected components")?
    {
        for key in ["graph", "overlay"] {
            assembly::pinned_copy(&root, &c[key], work)?;
        }
    }
    let staged = work.join(filename);
    fs::copy(plan, &staged)?;
    Ok((staged, manifest))
}

pub fn caller(
    crate_dir: &Path,
    work: &Path,
    lock: &Path,
    main: &Path,
    suite: &[u8],
    tests: &[&str],
) -> Result<PathBuf> {
    fs::create_dir_all(work.join("src"))?;
    fs::create_dir(work.join("tests"))?;
    fs::copy(main, work.join("src/main.rs"))?;
    fs::write(work.join("tests/contracts.rs"), suite)?;
    fs::write(
        work.join("Cargo.toml"),
        format!(
            "[package]\nname=\"stream-caller\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nserde_json=\"=1.0.151\"\nmarmish-stream={{path={}}}\n",
            json!(crate_dir)
        ),
    )?;
    fs::copy(lock, work.join("Cargo.lock"))?;
    let cargo = |args: &[&str]| -> Result<String> {
        let out = capture(
            Command::new("cargo")
                .env_remove("CARGO_TARGET_DIR")
                .current_dir(work)
                .arg("+1.97.1")
                .args(args),
            600,
            None,
        )?;
        require(
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )?;
        Ok(String::from_utf8(out.stdout)?)
    };
    cargo(&["metadata", "--offline", "--format-version=1"])?;
    cargo(&["build", "--locked", "--offline", "--release"])?;
    let output = cargo(&[
        "test",
        "--locked",
        "--offline",
        "--release",
        "--test",
        "contracts",
    ])?;
    for name in tests {
        require(
            output.lines().any(|s| s == format!("test {name} ... ok")),
            format!("missing upstream test {name}"),
        )?;
    }
    Ok(work.join("target/release/stream-caller"))
}

pub fn test_module(source: &str) -> Result<&str> {
    let marker = "#[cfg(test)]\nmod tests {";
    require(
        source.matches(marker).count() == 1,
        "expected one original test module",
    )?;
    Ok(&source[source.find(marker).unwrap()..])
}

/// The archived producers used CPython Random with one unsigned 32-bit seed.
pub struct Random {
    state: [u32; 624],
    at: usize,
}
impl Random {
    pub fn new(seed: u32) -> Self {
        let mut s = [0; 624];
        s[0] = 19650218;
        for i in 1..624 {
            s[i] = 1812433253u32
                .wrapping_mul(s[i - 1] ^ (s[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let mut i = 1;
        for _ in 0..624 {
            s[i] =
                (s[i] ^ ((s[i - 1] ^ (s[i - 1] >> 30)).wrapping_mul(1664525))).wrapping_add(seed);
            i += 1;
            if i == 624 {
                s[0] = s[623];
                i = 1;
            }
        }
        for _ in 0..623 {
            s[i] = (s[i] ^ ((s[i - 1] ^ (s[i - 1] >> 30)).wrapping_mul(1566083941)))
                .wrapping_sub(i as u32);
            i += 1;
            if i == 624 {
                s[0] = s[623];
                i = 1;
            }
        }
        s[0] = 0x80000000;
        Self { state: s, at: 624 }
    }
    fn word(&mut self) -> u32 {
        if self.at == 624 {
            for i in 0..624 {
                let y = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 == 1 { 0x9908b0df } else { 0 };
            }
            self.at = 0;
        }
        let mut y = self.state[self.at];
        self.at += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^ (y >> 18)
    }
    pub fn below(&mut self, n: u32) -> usize {
        let bits = u32::BITS - n.leading_zeros();
        loop {
            let v = self.word() >> (32 - bits);
            if v < n {
                return v as usize;
            }
        }
    }
    pub fn bytes(&mut self, size: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            let count = (size - out.len()).min(4);
            let n = self.word() >> ((4 - count) * 8);
            out.extend_from_slice(&n.to_le_bytes()[..count]);
        }
        out
    }
}
