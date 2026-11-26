use marmish_native::diagnostics::{enable, event, stage};

fn main() {
    let _: fn() = enable;
    let _: fn(&'static str, &'static str) = event;
    let _: fn(&mut &'static str, &'static str) = stage;
    let mut current = "initial";
    match std::env::args().nth(1).as_deref() {
        Some("disabled") => {
            event("hidden", "unused");
            stage(&mut current, "before");
        }
        Some("sequence") => {
            stage(&mut current, "before");
            enable();
            event("quoted \"\\", "unicode 雪\n\t\u{1}");
            stage(&mut current, "after");
            enable();
            event("", "");
        }
        Some("cross-thread") => {
            std::thread::spawn(enable).join().unwrap();
            std::thread::spawn(|| event("joined", "visible")).join().unwrap();
        }
        Some("closed-sink") => {
            enable();
            event("write", "fails");
            stage(&mut current, "after-error");
        }
        _ => panic!("unknown scenario"),
    }
    println!("{current}");
}
