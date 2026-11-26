use marmish_engine::{MissingRange, MissingReason};

fn main() {
    fn require_traits<T: Eq + Clone + std::fmt::Debug>() {}
    require_traits::<MissingRange>();

    let record = MissingRange {
        start: u64::MAX,
        end: Some(0_u64),
        reason: MissingReason::UncertainTail,
    };
    assert_eq!(record.clone(), record);
    let start: u64 = record.start;
    let end: Option<u64> = record.end;
    let reason: MissingReason = record.reason;
    assert_eq!(
        (start, end, reason),
        (u64::MAX, Some(0), MissingReason::UncertainTail)
    );
    println!("public fields, reversed interval, Clone and Eq");
}
