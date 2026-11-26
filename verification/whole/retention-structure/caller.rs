use marmish_core::Retention;

fn main() {
    println!("default={:?}", Retention::default().seconds());
    println!("advertised={}", Retention::MAX_SECONDS);
    for input in [
        None,
        Some(0),
        Some(1),
        Some(604_800),
        Some(604_801),
        Some(31_536_000),
        Some(u64::MAX),
    ] {
        match Retention::from_seconds(input) {
            Ok(value) => println!("{input:?} => {:?}", value.seconds()),
            Err(error) => println!("{input:?} => {error:?}: {error}"),
        }
    }
}
