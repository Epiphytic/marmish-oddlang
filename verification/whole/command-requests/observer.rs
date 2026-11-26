// Each isolated consumer supplies its own codec and adapter at build time.
mod codec;
mod peer;
use std::io::{self, BufRead};

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(reference)]
mod foundation;

fn main() {
    for line in io::stdin().lock().lines() {
        let (op, raw): (String, String) = serde_json::from_str(&line.unwrap()).unwrap();
        let output = peer::cycle(&op, &hex::decode(raw).unwrap()).ok().map(hex::encode);
        println!("{}", serde_json::to_string(&output).unwrap());
    }
}
