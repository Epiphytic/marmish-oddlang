#[cfg(test)]
mod release_route_probe {
    use super::*;
    #[test]
    fn distinguishes_prepare_and_submit() {
        let (requests, mut receiver) = mpsc::channel(1);
        let queue = RoomPrepareQueue {
            root: PathBuf::new(),
            endpoint: "ws://offline.invalid".into(),
            local: "00".repeat(32),
            requests,
            states: Arc::new(Mutex::new(BTreeMap::new())),
        };
        // Invalid principal makes the control fail before any credential or network I/O.
        let mut packet = serde_json::json!({"operation":"command_prepare","relays":[],"remote":""});
        let reply = queue.route(&serde_json::to_vec(&packet).unwrap()).unwrap();
        let observed: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(observed["command"]["category"], "command_unauthorized");
        packet["operation"] = "command_submit".into();
        assert!(queue.route(&serde_json::to_vec(&packet).unwrap()).is_none());
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }
}
