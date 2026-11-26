#[cfg(test)]
mod authorized_submit_offline {
    use super::*;

    const REMOTE: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const ENDPOINT: &str = "ws://offline.invalid/relay";

    fn connect(path: &Path) -> rusqlite::Connection {
        let db = rusqlite::Connection::open(path).unwrap();
        db.pragma_update(None, "key", "offline-disposable-fixture-key")
            .unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .unwrap();
        db
    }

    fn write_grant(path: &Path, trusted: bool, role: &str) {
        let remotes = if trusted { vec![REMOTE] } else { Vec::new() };
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path.join(ROOM_CONFIGURATION))
            .unwrap();
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "role": role, "trusted_remotes": remotes, "relays": [ENDPOINT], "tor": "auto"
            }),
        )
        .unwrap();
    }

    #[test]
    fn local_room_selection_and_commit_receipt() {
        let root = PathBuf::from(std::env::var_os("MARMISH_OFFLINE_SUBMIT_FIXTURE").unwrap());
        let file = root.join("journal.db");
        let local = [0x22; 32];
        let local_id = hex::encode(local);
        let session = hex::encode([0x33; 16]);
        let (requests, _receiver) = mpsc::channel(1);
        let prepared = RoomPrepareQueue {
            root: root.clone(),
            endpoint: ENDPOINT.into(),
            local: local_id.clone(),
            requests,
            states: Arc::new(Mutex::new(BTreeMap::new())),
        };
        let wire_id = |value: u8| {
            let mut id = [value; 16];
            id[..6].copy_from_slice(&epoch_seconds().unwrap().to_be_bytes()[2..]);
            hex::encode(id)
        };
        let mut request = SubmitRequest {
            operation: SUBMIT_OPERATION.into(),
            remote: REMOTE.into(),
            relays: vec![ENDPOINT.into()],
            session: session.clone(),
            request: wire_id(0x44),
            payload: SubmitPayload {
                chunks: vec!["L3Vzci9iaW4vdHJ1ZQ==".into()],
                invocation: wire_id(0x55),
                bytes: String::new(),
            },
        };
        let mut db = connect(&file);
        db.execute_batch(
            "CREATE TABLE local_room_prepare_v1 (
            local_account TEXT, remote_account TEXT, relay TEXT, session TEXT,
            native_group BLOB, welcome_id BLOB, ready INTEGER)",
        )
        .unwrap();
        db.execute(
            "INSERT INTO local_room_prepare_v1 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            rusqlite::params![local_id, REMOTE, ENDPOINT, session, &[1u8][..], &[2u8][..]],
        )
        .unwrap();
        write_grant(&root, true, "server");
        assert!(matches!(
            select_submit_room(&db, &prepared, &request),
            Err("command_unauthorized")
        ));
        write_grant(&root, false, "client");
        assert!(matches!(
            select_submit_room(&db, &prepared, &request),
            Err("command_unauthorized")
        ));
        write_grant(&root, true, "client");
        request.relays.clear();
        assert!(matches!(
            select_submit_room(&db, &prepared, &request),
            Err("command_relay_binding")
        ));
        request.relays.push(ENDPOINT.into());
        request.session = hex::encode([0x38; 16]);
        assert!(matches!(
            select_submit_room(&db, &prepared, &request),
            Err("command_room_pending")
        ));
        request.session = session.clone();
        let select = |db: &rusqlite::Connection, request: &SubmitRequest| {
            select_submit_room(db, &prepared, request).unwrap()
        };
        // Native membership and Welcome verification are supplied by the production caller.
        // This fixture exercises the authenticated retained-room selection and local commit boundary.
        let retained = select(&db, &request);
        let handle =
            persist_verified_submit(&mut db, local, retained, &request, "offline").unwrap();
        let expected = format!(
            "{{\"body\":{{\"argv\":[\"L3Vzci9iaW4vdHJ1ZQ==\"],\"invocation\":\"{}\",\"stdin\":\"\"}},\"client\":\"{local_id}\",\"id\":\"{}\",\"op\":\"submit\",\"protocol\":\"marmish-command/1\",\"server\":\"{REMOTE}\",\"session\":\"{session}\",\"type\":\"request\"}}",
            request.payload.invocation, request.request,
        );
        let digest = hex::encode(::sha2::Sha256::digest(expected.as_bytes()));
        assert_eq!(handle.digest, digest);
        assert_eq!(handle.invocation, request.payload.invocation);
        assert_eq!(handle.remote, REMOTE);
        let verify_row = |db: &rusqlite::Connection| {
            let (frame, count): (Vec<u8>, i64) = db.query_row(
                "SELECT frame, (SELECT count(*) FROM oddlang_queue_frames_v1) FROM oddlang_queue_frames_v1",
                [], |row| Ok((row.get(0)?, row.get(1)?)),
            ).unwrap();
            assert_eq!(frame, expected.as_bytes());
            assert_eq!(count, 1);
        };
        verify_row(&db);
        drop(db);
        let mut db = connect(&file);
        let retained = select(&db, &request);
        let replay =
            persist_verified_submit(&mut db, local, retained, &request, "offline").unwrap();
        assert_eq!(
            serde_json::to_vec(&handle).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        request.payload.bytes = "AA==".into();
        let retained = select(&db, &request);
        assert!(matches!(
            persist_verified_submit(&mut db, local, retained, &request, "offline"),
            Err("command_request_conflict")
        ));
        write_grant(&root, false, "client");
        assert!(matches!(
            select_submit_room(&db, &prepared, &request),
            Err("command_unauthorized")
        ));
        verify_row(&db);
    }
}
