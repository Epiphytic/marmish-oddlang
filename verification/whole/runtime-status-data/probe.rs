use marmish_native::mdk_adapter::RuntimeStatus;

fn main() {
    let identity = "snow\n雪\u{0}";
    let values = [
        ("", 0, 0, 0),
        (identity, 1, 255, 256),
        ("limit", usize::MAX, usize::MAX - 1, usize::MAX / 2),
    ];
    for (identity, conversations, received, pending) in values {
        let status = RuntimeStatus {
            identity: identity.into(),
            conversations,
            received,
            pending,
        };
        let _: (&String, usize, usize, usize) = (
            &status.identity, status.conversations, status.received, status.pending,
        );
        println!("{}", serde_json::to_string(&status).unwrap());
        let check = serde_json::json!({
            "identity": identity, "conversations": conversations,
            "received": received, "pending": pending,
        });
        assert_eq!(serde_json::to_value(&status).unwrap(), check);
    }
    println!("usize:{}", usize::BITS);
}
