// The same native relation observer runs against both libraries.
use marmish_stream::{
    codec::Codec,
    copy_protocol::{HelperContext, Launch, Manifest},
    Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    Match {
        launch: Launch,
        manifest: Vec<u8>,
        force_version: Option<u32>,
        force_source: Option<Vec<u8>>,
    },
    Start {
        context: Vec<u8>,
        invalid_context: bool,
        launch: Launch,
        codec: Codec,
    },
    Build {
        context: Vec<u8>,
        invalid_context: bool,
        version: String,
        target: String,
        commit: [u8; 20],
    },
}

fn context(bytes: &[u8], invalid: bool) -> HelperContext {
    let mut context = HelperContext::decode(bytes).unwrap();
    if invalid {
        context.context_version = 0;
    }
    context
}

fn report(result: Result<()>) -> Value {
    match result {
        Ok(()) => json!({"ok":null}),
        Err(error) => json!({"error":[format!("{error:?}"),error.to_string()]}),
    }
}

fn main() {
    let _: fn(&Launch, &Manifest) -> bool = Launch::matches_manifest;
    let _: fn(&HelperContext, &Launch, Codec) -> Result<()> = HelperContext::validate_start;
    let _: fn(&HelperContext, &str, &str, [u8; 20]) -> Result<()> = HelperContext::validate_build;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        let output = match request {
            Request::Match {
                launch,
                manifest,
                force_version,
                force_source,
            } => {
                let mut manifest = Manifest::decode(&manifest).unwrap();
                if let Some(value) = force_version {
                    manifest.version = value;
                }
                if let Some(value) = force_source {
                    manifest.source = value;
                }
                json!(launch.matches_manifest(&manifest))
            }
            Request::Start {
                context: bytes,
                invalid_context,
                launch,
                codec,
            } => report(context(&bytes, invalid_context).validate_start(&launch, codec)),
            Request::Build {
                context: bytes,
                invalid_context,
                version,
                target,
                commit,
            } => report(context(&bytes, invalid_context).validate_build(&version, &target, commit)),
        };
        println!("{output}");
    }
}
