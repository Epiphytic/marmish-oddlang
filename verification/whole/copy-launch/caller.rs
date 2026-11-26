// Independent native consumer compiled against either package under test.
use marmish_stream::{
    Error,
    copy_protocol::{AttemptMode, CopyError, Direction, Launch, Phase, Stage, VERSION},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    fmt::Debug,
    io::{self, BufRead},
};

fn enum_table<T: TryFrom<u8, Error = Error> + Serialize + DeserializeOwned + Debug + Copy + Eq>(
    byte: fn(T) -> u8,
) -> Value {
    let mut accept = Vec::new();
    let mut reject = Vec::new();
    for n in 0..=255 {
        match T::try_from(n) {
            Ok(v) => {
                let wire = serde_json::to_value(v).unwrap();
                assert_eq!(serde_json::from_value::<T>(wire.clone()).unwrap(), v);
                assert_eq!(byte(v), n);
                accept.push(json!({"tag":n,"rust":format!("{v:?}"),"wire":wire}));
            }
            Err(e) => reject.push(json!([n, format!("{e:?}"), e.to_string()])),
        }
    }
    json!({"accept":accept,"reject":reject})
}
fn check(value: Launch) -> Value {
    // Full destructuring and field annotations catch absent/extra members or changed host types.
    let Launch {
        version,
        mode,
        transfer,
        client,
        server,
        direction,
        source,
        destination,
        overwrite,
        preserve_mode,
        preserve_mtime,
    } = &value;
    let _: (
        &u32,
        &AttemptMode,
        &[u8; 32],
        &[u8; 32],
        &[u8; 32],
        &Direction,
        &Vec<u8>,
        &Vec<u8>,
        &bool,
        &bool,
        &bool,
    ) = (
        version,
        mode,
        transfer,
        client,
        server,
        direction,
        source,
        destination,
        overwrite,
        preserve_mode,
        preserve_mtime,
    );
    assert!(value == value.clone());
    let valid: marmish_stream::Result<()> = value.validate();
    json!({"wire":serde_json::to_string(&value).unwrap(),"valid":valid.map_err(|e|(format!("{e:?}"),e.to_string()))})
}
fn main() {
    for line in io::stdin().lock().lines() {
        let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let v = match req["operation"].as_str().unwrap() {
            "enums" => json!({"version":VERSION,"enums":{
                "AttemptMode":enum_table::<AttemptMode>(|v|v as u8),"Direction":enum_table::<Direction>(|v|v as u8),
                "Phase":enum_table::<Phase>(|v|v as u8),"Stage":enum_table::<Stage>(|v|v as u8),
                "CopyError":enum_table::<CopyError>(|v|v as u8)}}),
            "decode" => match serde_json::from_str::<Launch>(req["text"].as_str().unwrap()) {
                Ok(v) => check(v),
                Err(e) => json!({"parseError":e.to_string()}),
            },
            "construct" => check(Launch {
                version: req["version"].as_u64().unwrap() as u32,
                mode: serde_json::from_value(req["mode"].clone()).unwrap(),
                transfer: [1; 32],
                client: [2; 32],
                server: [3; 32],
                direction: serde_json::from_value(req["direction"].clone()).unwrap(),
                source: serde_json::from_value(req["source"].clone()).unwrap(),
                destination: serde_json::from_value(req["destination"].clone()).unwrap(),
                overwrite: true,
                preserve_mode: false,
                preserve_mtime: true,
            }),
            _ => panic!("unrecognized caller operation"),
        };
        println!("{}", serde_json::to_string(&v).unwrap());
    }
}
