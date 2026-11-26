use marmish_engine::{Stream, SubmitDisposition};

fn declared<T: Clone + std::fmt::Debug + PartialEq + Eq>() {}

fn stream(value: Stream) -> &'static str {
    match value {
        Stream::Stdout => "Stdout",
        Stream::Stderr => "Stderr",
    }
}

fn disposition(value: SubmitDisposition) -> &'static str {
    match value {
        SubmitDisposition::Accepted => "Accepted",
        SubmitDisposition::Existing => "Existing",
    }
}

fn main() {
    declared::<Stream>();
    declared::<SubmitDisposition>();
    for value in [Stream::Stdout, Stream::Stderr] {
        assert_eq!(value, value.clone());
        println!("Stream:{}:{value:?}:{}", stream(value.clone()), value as isize);
    }
    assert_ne!(Stream::Stdout, Stream::Stderr);
    for value in [SubmitDisposition::Accepted, SubmitDisposition::Existing] {
        assert_eq!(value, value.clone());
        println!(
            "SubmitDisposition:{}:{value:?}:{}",
            disposition(value.clone()),
            value.clone() as isize
        );
    }
    assert_ne!(SubmitDisposition::Accepted, SubmitDisposition::Existing);
}
