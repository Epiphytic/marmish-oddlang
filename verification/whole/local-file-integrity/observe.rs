// This fixture is compiled only in test harnesses, after generation.
#[cfg(test)]
mod local_file_observations {
    use super::{Error, observed_open, observed_read, observed_verify};
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File},
        os::{
            fd::AsRawFd,
            unix::fs::{PermissionsExt, symlink},
        },
    };

    fn classify<T>(value: Result<T, Error>) -> &'static str {
        match value {
            Ok(_) => "accepted",
            Err(Error::Invalid("unsafe_install_path")) => "descriptor",
            Err(Error::Invalid("install_asset")) => "metadata",
            Err(Error::Invalid("install_digest")) => "digest",
            Err(Error::Invalid("install_manifest")) => "bounded",
            Err(Error::Io(err)) if err.raw_os_error() == Some(libc::ELOOP) => "symlink-io",
            Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => "missing-io",
            Err(error) => panic!("unexpected typed error: {error:?}"),
        }
    }

    fn check_fd(file: &File) {
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
        let access = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        assert!(access >= 0 && access & libc::O_ACCMODE == libc::O_RDONLY);
    }

    #[test]
    fn descriptor_guard_order() {
        let directory =
            std::env::temp_dir().join(format!("descriptor-observation-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        struct RemoveDirectory(std::path::PathBuf);
        impl Drop for RemoveDirectory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let cleanup = RemoveDirectory(directory);
        let root = cleanup.0.as_path();
        let file = root.join("plain");
        let data: Vec<_> = (0..(2 * 65536 + 3)).map(|n| (n % 251) as u8).collect();
        fs::write(&file, &data).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let total = data.len() as u64;
        let digest: [u8; 32] = Sha256::digest(&data).into();
        let bad = [0u8; 32];
        let missing = root.join("absent");
        let link = root.join("link");
        symlink(&file, &link).unwrap();
        let empty = root.join("empty");
        fs::write(&empty, []).unwrap();

        let handle = observed_open(&file, total).unwrap();
        check_fd(&handle);
        assert_eq!(observed_read(&file, total).unwrap(), data);
        assert!(observed_read(&empty, 0).unwrap().is_empty());

        let cases = [
            (
                "wrong-extent-before-hash",
                classify(observed_verify(&file, total + 1, 0o600, &bad, true)),
                "metadata",
            ),
            (
                "larger-file-before-metadata",
                classify(observed_verify(&file, total - 1, 0o600, &bad, true)),
                "descriptor",
            ),
            (
                "larger-read",
                classify(observed_read(&file, total - 1)),
                "descriptor",
            ),
            (
                "non-regular",
                classify(observed_open(root, total)),
                "descriptor",
            ),
            (
                "symlink",
                classify(observed_open(&link, total)),
                "symlink-io",
            ),
            (
                "missing",
                classify(observed_open(&missing, total)),
                "missing-io",
            ),
            (
                "bad-mode-before-digest",
                classify(observed_verify(&file, total, 0o644, &bad, true)),
                "metadata",
            ),
            (
                "ignored-mode",
                classify(observed_verify(&file, total, 0o644, &digest, false)),
                "accepted",
            ),
            (
                "ignored-mode-bad-hash",
                classify(observed_verify(&file, total, 0o644, &bad, false)),
                "digest",
            ),
            (
                "correct-hash-mode",
                classify(observed_verify(&file, total, 0o600, &digest, true)),
                "accepted",
            ),
        ];
        for (name, actual, expected) in cases {
            assert_eq!(actual, expected, "{name}");
            println!("FILE_CASE={name}:{actual}");
        }
        for (count, name) in [(1, "oneLink"), (2, "twoLinks"), (3, "threeLinks")] {
            if count > 1 {
                fs::hard_link(&file, root.join(format!("equal-{count}"))).unwrap();
            }
            println!("LINK_CASE={name}:{}", classify(observed_open(&file, total)));
        }
    }
}
