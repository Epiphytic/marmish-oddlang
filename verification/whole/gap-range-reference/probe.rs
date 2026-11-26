#[cfg(with_range)]
use marmish_engine::MissingRange;
use marmish_engine::MissingReason;
use serde_json::{Value, json};
use std::io::{self, Read};

fn outcome<T>(decoded: Result<T, serde_json::Error>, encode: impl Fn(T) -> String) -> Value {
    match decoded {
        Ok(value) => json!({"encoded": encode(value)}),
        Err(error) => {
            json!({"diagnostic": error.to_string(), "line": error.line(), "column": error.column()})
        }
    }
}

fn check_public_cases() {
    use MissingReason::*;
    let cases = [Limit, StorageFailure, Expired, UncertainTail];
    for (index, reason) in cases.into_iter().enumerate() {
        let expected = match &reason {
            Limit => "Limit",
            StorageFailure => "StorageFailure",
            Expired => "Expired",
            UncertainTail => "UncertainTail",
        };
        assert_eq!(reason.clone(), reason);
        assert_eq!(format!("{reason:?}"), expected);
        assert_eq!(reason as usize, index);
    }
}

fn main() {
    check_public_cases();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let rows: Vec<_> = cases
        .iter()
        .map(|case| {
            let wire = case["wire"].as_str().unwrap();
            let result = match case["type"].as_str().unwrap() {
                #[cfg(with_range)]
                "range" => outcome(serde_json::from_str::<MissingRange>(wire), |v| {
                    serde_json::to_string(&v).unwrap()
                }),
                "reason" => outcome(serde_json::from_str::<MissingReason>(wire), |v| {
                    serde_json::to_string(&v).unwrap()
                }),
                other => panic!("unexpected test type {other}"),
            };
            json!({"id": case["id"], "result": result})
        })
        .collect();
    println!("{}", serde_json::to_string(&rows).unwrap());
}
