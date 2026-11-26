use marmish_native::{journal, mdk_adapter};

fn into_mdk(
    conversation: journal::Conversation,
    sent: journal::SendStatus,
    received: journal::ReceivedMessage,
) -> (
    mdk_adapter::Conversation,
    mdk_adapter::SendStatus,
    mdk_adapter::ReceivedMessage,
) {
    (conversation, sent, received)
}

fn into_journal(
    conversation: mdk_adapter::Conversation,
    sent: mdk_adapter::SendStatus,
    received: mdk_adapter::ReceivedMessage,
) -> (
    journal::Conversation,
    journal::SendStatus,
    journal::ReceivedMessage,
) {
    (conversation, sent, received)
}

fn main() {
    let (mut conversation, mut sent, mut received) = into_mdk(
        journal::Conversation {
            session: String::from("s\n1"),
            group_id: String::from("group"),
            peer: String::from("北"),
            revoked: true,
        },
        journal::SendStatus {
            application_id: String::from("app"),
            app_event_id: String::from("event"),
            state: String::from("sent"),
        },
        journal::ReceivedMessage {
            cursor: u64::MAX,
            session: String::from("s\n1"),
            application_id: String::from("app"),
            sender: String::from("北"),
            data: vec![0_u8, 127, 255],
            invalidated: false,
        },
    );
    let cloned = into_journal(conversation.clone(), sent.clone(), received.clone());
    conversation.peer.clear();
    sent.state.push_str("-changed");
    received.data[0] = 1;

    let _: (&String, &String, &String, bool) = (
        &cloned.0.session,
        &cloned.0.group_id,
        &cloned.0.peer,
        cloned.0.revoked,
    );
    let _: (&String, &String, &String) = (
        &cloned.1.application_id,
        &cloned.1.app_event_id,
        &cloned.1.state,
    );
    let _: (u64, &String, &String, &String, &Vec<u8>, bool) = (
        cloned.2.cursor,
        &cloned.2.session,
        &cloned.2.application_id,
        &cloned.2.sender,
        &cloned.2.data,
        cloned.2.invalidated,
    );
    assert_eq!(
        (&*cloned.0.peer, &*cloned.1.state, cloned.2.data.as_slice()),
        ("北", "sent", &[0, 127, 255][..])
    );
    println!("{}", serde_json::to_string(&cloned.0).unwrap());
    println!("{}", serde_json::to_string(&cloned.1).unwrap());
    println!("{}", serde_json::to_string(&cloned.2).unwrap());
}
