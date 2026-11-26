use marmish_native::identity::BackendChoice;

fn preserve<T: Clone + Copy + PartialEq + Eq>(value: T) -> T {
    let copied = value;
    assert!(copied == value);
    value.clone()
}

fn name(value: BackendChoice) -> &'static str {
    match value {
        BackendChoice::Auto => "Auto",
        BackendChoice::Keychain => "Keychain",
        BackendChoice::File => "File",
    }
}

fn main() {
    let values = [
        BackendChoice::Auto,
        BackendChoice::Keychain,
        BackendChoice::File,
    ];
    for (index, value) in values.into_iter().enumerate() {
        let cloned = preserve(value);
        for (other, candidate) in values.into_iter().enumerate() {
            assert!((cloned == candidate) == (index == other));
            assert!(
                (std::mem::discriminant(&cloned) == std::mem::discriminant(&candidate))
                    == (index == other)
            );
        }
        println!(
            "{}|{}|{}|{}",
            name(cloned),
            cloned as isize,
            cloned as u8,
            cloned as i128
        );
    }
}
