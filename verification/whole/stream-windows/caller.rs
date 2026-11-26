use marmish_stream::Result;
use marmish_stream::windows::{ReceiveWindow, SendWindow};
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn hex(bytes: Vec<u8>) -> Value {
    json!(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}
fn note<T>(r: Result<T>, encode: impl FnOnce(T) -> Value) -> Value {
    match r {
        Ok(v) => encode(v),
        Err(e) => json!({"error":format!("{e:?}")}),
    }
}
fn send(w: &mut SendWindow, s: &Value) -> Value {
    let n = || s["offset"].as_u64().unwrap();
    let limit = || usize::try_from(s["limit"].as_u64().unwrap()).unwrap();
    match s["op"].as_str().unwrap() {
        "push" => note(w.push(&bytes(&s["bytes"])), |v| json!(v)),
        "read" => note(w.read(n(), limit()), hex),
        "ack" => note(w.acknowledge(n()), |()| json!("ok")),
        "finish" => note(w.finish(n()), |()| json!("ok")),
        "state" => {
            json!({"produced":w.produced(),"acknowledged":w.acknowledged(),"buffered":w.buffered(),"credit":w.credit(),"eof":w.eof(),"drained":w.drained()})
        }
        _ => unreachable!(),
    }
}
fn recv(w: &mut ReceiveWindow, s: &Value) -> Value {
    let n = || s["offset"].as_u64().unwrap();
    let limit = || usize::try_from(s["limit"].as_u64().unwrap()).unwrap();
    match s["op"].as_str().unwrap() {
        "accept" => note(w.accept(n(), &bytes(&s["bytes"])), |v| json!(v)),
        "pending" => note(w.pending(limit()), hex),
        "deliver" => note(w.delivered_through(n()), |()| json!("ok")),
        "finish" => note(w.finish(n()), |()| json!("ok")),
        "state" => {
            json!({"accepted":w.accepted(),"delivered":w.delivered(),"buffered":w.buffered(),"credit":w.credit(),"eof":w.eof(),"drained":w.drained()})
        }
        _ => unreachable!(),
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let trace: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut outbound = SendWindow::default();
        let mut inbound = ReceiveWindow::default();
        let responses: Vec<_> = trace["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| match trace["operation"].as_str().unwrap() {
                "send_window" => send(&mut outbound, s),
                "receive_window" => recv(&mut inbound, s),
                _ => unreachable!(),
            })
            .collect();
        println!("{}", json!([trace["name"], responses]));
    }
}
