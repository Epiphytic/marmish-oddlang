use marmish_vault::{Error, KeyRef, Label, Metadata, Namespace, Purpose};
use std::collections::HashSet;

fn rejected(error: Error) -> String {
    format!("{error:?}/{}/{}", error.category(), error)
}
fn key(version: u8, purpose: Purpose, text: &str) {
    match KeyRef::new(version, purpose, text) {
        Ok(value) => {
            assert_eq!(
                (value.version(), value.purpose(), value.id()),
                (version, purpose, text)
            );
            assert!(HashSet::from([value.clone()]).contains(&value));
            println!("ok");
        }
        Err(error) => println!("{}", rejected(error)),
    }
}
fn label(text: &str) {
    match Label::new(text) {
        Ok(value) => {
            assert_eq!(value.as_str(), text);
            assert_eq!(value.clone(), value);
            println!("ok");
        }
        Err(error) => println!("{}", rejected(error)),
    }
}

fn main() {
    let _: fn(Namespace) -> u8 = Namespace::version;
    let _: fn(Purpose) -> &'static str = Purpose::as_str;
    let _: fn(&str) -> Result<Purpose, Error> = Purpose::parse;
    let _: fn(u8, Purpose, &str) -> Result<KeyRef, Error> = KeyRef::new;
    let _: fn(&str) -> Result<Label, Error> = Label::new;
    let _: fn(&KeyRef) -> &str = KeyRef::id;
    let _: fn(&KeyRef) -> u8 = KeyRef::version;
    let _: fn(&KeyRef) -> Purpose = KeyRef::purpose;
    let _: fn(&Label) -> &str = Label::as_str;
    for value in [Namespace::Legacy, Namespace::MarmishV2] {
        let copy = value;
        assert_eq!(copy, value);
        println!("{value:?}/{}", value.version());
    }
    for text in [
        "nostr",
        "sqlcipher",
        "Nostr",
        "",
        " nostr",
        "sqlcipher ",
        "nostr\0",
        "sqlciphér",
    ] {
        match Purpose::parse(text) {
            Ok(purpose) => println!("{purpose:?}/{}", purpose.as_str()),
            Err(error) => println!("{}", rejected(error)),
        }
    }
    for error in [
        Error::Unavailable,
        Error::Uninitialized,
        Error::LockedOrDenied,
        Error::Missing,
        Error::Exists,
        Error::Ambiguous,
        Error::Malformed,
        Error::BackendFailure,
    ] {
        assert!(std::error::Error::source(&error).is_none());
        println!("{}", rejected(error));
    }
    for version in 0u8..=255 {
        key(version, Purpose::Nostr, &"a".repeat(64));
    }
    for size in [0, 1, 63, 64, 65] {
        key(2, Purpose::Sqlcipher, &"f".repeat(size));
    }
    for ch in 0..=127 {
        key(
            2,
            Purpose::Sqlcipher,
            &format!("{}{}", "a".repeat(63), char::from(ch)),
        );
    }
    key(2, Purpose::Sqlcipher, &format!("{}é", "a".repeat(62)));
    key(0, Purpose::Sqlcipher, "BAD");
    for size in [0, 1, 126, 127, 128, 129] {
        label(&"a".repeat(size));
    }
    for text in [
        "é".repeat(64),
        "🦀".repeat(32),
        format!("{}🦀", "a".repeat(125)),
        format!("{}é", "a".repeat(126)),
        format!("{}é", "a".repeat(127)),
        format!("{}\u{1f}", "a".repeat(128)),
        "\u{200b}".to_owned(),
        "\u{feff}".to_owned(),
    ] {
        label(&text);
    }
    for scalar in (0..=0xa0).chain(0x2027..=0x202f).chain(0x2065..=0x206a) {
        label(&format!("A{}B", char::from_u32(scalar).unwrap()));
    }
    let record = Metadata {
        key_ref: KeyRef::new(2, Purpose::Sqlcipher, &"a".repeat(64)).unwrap(),
        label: Label::new("local-index").unwrap(),
    };
    assert_eq!(record, record.clone());
    assert_eq!(record.key_ref.purpose(), Purpose::Sqlcipher);
    assert_eq!(record.label.as_str(), "local-index");
    println!("{record:?}");
}
