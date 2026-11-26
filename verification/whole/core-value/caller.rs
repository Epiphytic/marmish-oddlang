use marmish_core::{ConfigError, Retention};
use std::error::Error;
use std::fmt::Debug;

const LIMIT: u64 = 365 * 24 * 60 * 60;
const DEFAULT: u64 = 24 * 60 * 60;

fn named_error(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::EmptyRelaySet => "EmptyRelaySet",
        ConfigError::InvalidUrl => "InvalidUrl",
        ConfigError::DisallowedScheme => "DisallowedScheme",
        ConfigError::DisallowedComponents => "DisallowedComponents",
        ConfigError::DisallowedHost => "DisallowedHost",
        ConfigError::LoopbackDenied => "LoopbackDenied",
        ConfigError::RetentionOutOfRange => "RetentionOutOfRange",
    }
}

fn check(input: Option<u64>) {
    let accepted = input.map_or(true, |value| value != 0 && value <= LIMIT);
    match Retention::from_seconds(input) {
        Ok(retention) => {
            assert!(accepted, "accepted out-of-range input {input:?}");
            assert_eq!(retention.seconds(), input);
            println!("OBS\t{input:?}\tok\t{:?}", retention.seconds());
        }
        Err(error) => {
            assert!(!accepted, "rejected allowed input {input:?}");
            assert_eq!(error, ConfigError::RetentionOutOfRange);
            println!("OBS\t{input:?}\terror\t{}", named_error(&error));
        }
    }
}

#[test]
fn retention_public_surface() {
    fn traits<T: Clone + Copy + Debug + Eq + Default>() {}
    traits::<Retention>();
    let default = Retention::default();
    let disabled = Retention::from_seconds(None).unwrap();
    assert_eq!(default.seconds(), Some(DEFAULT));
    assert_eq!(default, default.clone());
    assert_eq!(default, Retention::from_seconds(Some(DEFAULT)).unwrap());
    assert_ne!(default, disabled);
    assert_eq!(format!("{default:?}"), "Retention(Some(86400))");
    assert_eq!(format!("{disabled:?}"), "Retention(None)");
    assert_eq!(Retention::MAX_SECONDS, LIMIT);
    println!(
        "OBS\tapi\t{}\t{default:?}\t{disabled:?}",
        Retention::MAX_SECONDS
    );
}

#[test]
fn independent_boundaries() {
    for input in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(DEFAULT - 1),
        Some(DEFAULT),
        Some(DEFAULT + 1),
        Some(LIMIT - 1),
        Some(LIMIT),
        Some(LIMIT + 1),
        Some(LIMIT + 2),
        Some(u32::MAX.into()),
        Some(u64::MAX - 1),
        Some(u64::MAX),
    ] {
        check(input);
    }
}

#[test]
fn seeded_samples() {
    let mut state = 0x8ab4_f2a2_dc11_0773_u64;
    for _ in 0..256 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        check(Some(state));
        check(Some(1 + state % LIMIT));
        check(Some(LIMIT - 64 + state % 129));
    }
}

#[test]
fn exact_error_messages() {
    for (ordinal, (variant, expected)) in [
        (
            ConfigError::EmptyRelaySet,
            "an explicit nonempty relay set is required",
        ),
        (ConfigError::InvalidUrl, "invalid relay URL"),
        (
            ConfigError::DisallowedScheme,
            "relay scheme must be ws or wss",
        ),
        (
            ConfigError::DisallowedComponents,
            "relay URL contains unsupported components",
        ),
        (ConfigError::DisallowedHost, "relay host is not permitted"),
        (
            ConfigError::LoopbackDenied,
            "loopback relays require explicit development opt-in",
        ),
        (
            ConfigError::RetentionOutOfRange,
            "retention must be between one second and 365 days",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let label = named_error(&variant);
        assert_eq!(format!("{variant:?}"), label);
        assert_eq!(variant.to_string(), expected);
        assert!((&variant as &dyn Error).source().is_none());
        let display = variant.to_string();
        assert_eq!(variant as usize, ordinal);
        println!("OBS\t{label}\t{display}\t{ordinal}");
    }
}
