use marmish_engine::MissingReason;
use serde_json::{Value, json};
use std::io::{self, Read};

fn main() {
    use MissingReason::{Expired, Limit, StorageFailure, UncertainTail};
    let changed = std::env::args().nth(1).as_deref() == Some("mutation");
    let values = if changed {
        [StorageFailure, Limit, Expired, UncertainTail]
    } else {
        [Limit, StorageFailure, Expired, UncertainTail]
    };
    let order: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(ordinal, reason)| {
            assert_eq!(reason.clone(), reason);
            assert_eq!(reason.clone() as usize, ordinal);
            json!({"native": format!("{reason:?}"), "wire": serde_json::to_string(&reason).unwrap()})
        })
        .collect();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let observations: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let wire = case["wire"].as_str().unwrap();
            let result = match serde_json::from_str::<MissingReason>(wire) {
                Ok(value) => json!({"encoded":serde_json::to_string(&value).unwrap()}),
                Err(error) => json!({
                    "diagnostic": error.to_string(), "line": error.line(), "column": error.column()
                }),
            };
            json!({"id":case["id"],"result":result})
        })
        .collect();
    println!("{}", json!({"variants":order,"observations":observations}));
}
