use marmish_native::transport::ArtiDirs;
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

fn main() {
    let mut dirs = ArtiDirs {
        state: PathBuf::from(OsString::from_vec(vec![b'.', b'.', b'/', 255])),
        cache: PathBuf::from("relative/../cache"),
    };
    let snapshot = dirs.clone();
    dirs.state.push("child");
    dirs.cache.push("temp");
    let _: (&PathBuf, &PathBuf) = (&snapshot.state, &snapshot.cache);
    assert_eq!(snapshot.state.as_os_str().as_bytes(), b"../\xff");
    assert_eq!(snapshot.cache.as_os_str().as_bytes(), b"relative/../cache");
    println!("{snapshot:?}");
}
