#[path = "claim/wire.rs"]
mod command_binding;
#[path = "relations/relations.rs"]
mod relations;
#[path = "server/wire.rs"]
mod requests;
#[path = "client/wire.rs"]
mod responses;

use std::io::{self, BufRead};

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
}

macro_rules! binding {
    ($codec:ident, $op:expr, $bytes:expr, $( $name:literal => $decode:ident ),+) => {
        match $op {
            $( $name => {
                let frame = $codec::$decode($bytes).expect("valid frozen packet");
                command_binding::WireBinding { client:frame.client, server:frame.server, session:frame.session }
            } ),+,
            _ => panic!("unknown operation"),
        }
    };
}

fn main() {
    for line in io::stdin().lock().lines() {
        let [role, op, raw, local, peer, session]: [String; 6] =
            serde_json::from_str(&line.unwrap()).unwrap();
        let local = hex::decode(local).unwrap().try_into().unwrap();
        let peer = hex::decode(peer).unwrap().try_into().unwrap();
        let session = hex::decode(session).unwrap().try_into().unwrap();
        let bytes = hex::decode(raw).unwrap();
        let result = match role.as_str() {
            "client" => binding!(responses, op.as_str(), &bytes, "submit"=>decode_submit,"status"=>decode_status,
                "poll"=>decode_poll,"cancel"=>decode_cancel,"finish"=>decode_finish).check_client_room(session, local, peer),
            "server" => binding!(requests, op.as_str(), &bytes, "submit"=>decode_submit,"status"=>decode_status,
                "poll"=>decode_poll,"cancel"=>decode_cancel,"finish"=>decode_finish).check_server_room(session, local, peer),
            _ => panic!("unexpected role"),
        };
        println!(
            "{}",
            match result {
                Ok(()) => "ok",
                Err(Error::Invalid(reason)) => reason,
            }
        );
    }
}
