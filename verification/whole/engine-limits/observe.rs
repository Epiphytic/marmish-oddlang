use std::os::fd::AsRawFd;

fn limits() -> libc::rlimit {
    let mut value = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut value) }, 0);
    value
}

fn main() {
    let initialize: fn() -> marmish_engine::Result<()> = marmish_engine::initialize_process_limits;
    let reason = marmish_engine::EngineError::WorkerUnavailable;
    assert_eq!(format!("{reason:?}"), "WorkerUnavailable");
    assert_eq!(reason.to_string(), "worker unavailable");
    let a = std::fs::File::open("/dev/null").unwrap();
    let b = std::fs::File::open("/dev/null").unwrap();
    let flags = |fd| unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert_eq!(unsafe { libc::fcntl(b.as_raw_fd(), libc::F_SETFD, 0) }, 0);
    let before_flags = [flags(a.as_raw_fd()), flags(b.as_raw_fd())];
    assert_eq!(before_flags, [libc::FD_CLOEXEC, 0]);
    let before = limits();
    initialize().unwrap();
    let first = limits();
    initialize().unwrap();
    let second = limits();
    assert_eq!((second.rlim_cur, second.rlim_max), (first.rlim_cur, first.rlim_max));
    assert_eq!(first.rlim_max, before.rlim_max);
    assert_eq!([flags(a.as_raw_fd()), flags(b.as_raw_fd())], before_flags);
    println!("{} {} {}", before.rlim_cur, first.rlim_cur, first.rlim_max);
}
