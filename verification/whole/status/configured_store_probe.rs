use rusqlite::{Connection, OpenFlags, ffi};
use std::io::Read;
use zeroize::Zeroizing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut material = Zeroizing::new([0u8; 67]);
    material[..2].copy_from_slice(b"x'");
    material[66] = b'\'';
    std::io::stdin().read_exact(&mut material[2..66])?;
    let path = std::env::args_os().nth(1).ok_or("expected database")?;
    let mode = std::env::args().nth(2);
    let insert = mode.as_deref() == Some("seed");
    let stale = mode.as_deref() == Some("stale");
    let access = if insert || stale {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let mut conn = Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
    // SAFETY: single live connection and fixed-length input buffer.
    if unsafe { ffi::sqlite3_key(conn.handle(), material.as_ptr().cast(), 67) } != ffi::SQLITE_OK {
        return Err("SQLCipher key setup failed".into());
    }
    conn.execute_batch("PRAGMA cipher_compatibility=4; PRAGMA cipher_page_size=4096;")?;
    if stale {
        conn.execute("UPDATE incoming SET created = 0", [])?;
        return Ok(());
    }
    if insert {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM command_engine_binding", [])?;
        let prior: Vec<u8> = tx.query_row(
            "SELECT settings FROM command_settings WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let mut settings: serde_json::Value = serde_json::from_slice(&prior)?;
        settings["active_per_principal"] = 2.into();
        tx.execute(
            "UPDATE command_settings SET settings=?1 WHERE id=1",
            [serde_json::to_vec(&settings)?],
        )?;
        let group = [81u8; 32];
        let sender = "24".repeat(32);
        for (tag, body_type, bytes) in [
            (1u8, "message", Some(b"fixture".as_slice())),
            (2, "receipt", Some(b"fixture".as_slice())),
            (3, "message", None),
        ] {
            tx.execute("INSERT INTO incoming(group_id,native_id,sender,application_id,envelope,digest,body_type,created) VALUES(?,?,?,?,?,?,?,?)",
                       rusqlite::params![group, [tag; 32], sender, format!("{now:012x}{tag:020x}"), bytes, "sample", body_type, now])?;
        }
        for (tag, state, bytes) in [
            (4, "prepared", Some(b"fixture".as_slice())),
            (5, "unsubmitted", Some(b"fixture".as_slice())),
            (6, "expired", None),
        ] {
            tx.execute("INSERT INTO outgoing(group_id,application_id,event_id,envelope,encoded,digest,state,created) VALUES(?,?,?,?,?,?,?,?)",
                       rusqlite::params![group, format!("{now:012x}{tag:020x}"), format!("{tag:064x}"), bytes, bytes, "sample", state, now])?;
        }
        tx.commit()?;
        return Ok(());
    }
    let read = |sql| -> rusqlite::Result<i64> { conn.query_row(sql, [], |r| r.get(0)) };
    let binding = conn
        .query_row(
            "SELECT version,hex(root),hex(key_id) FROM command_engine_binding",
            [],
            |r| {
                Ok((
                    r.get::<_, u32>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .ok();
    let settings: Vec<u8> = conn.query_row(
        "SELECT settings FROM command_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    println!(
        "{}",
        serde_json::json!({
            "version": conn.pragma_query_value(None, "user_version", |r| r.get::<_,u32>(0))?,
            "identity": conn.query_row("SELECT account FROM binding", [], |r| r.get::<_, String>(0))?,
            "conversations": read("SELECT count(*) FROM conversations")?,
            "received": read("SELECT count(*) FROM incoming WHERE body_type='message' AND envelope IS NOT NULL")?,
            "pending": read("SELECT count(*) FROM outgoing WHERE state IN ('unsubmitted','prepared','uncertain') AND envelope IS NOT NULL")?,
            "incoming": read("SELECT count(*) FROM incoming")?,
            "uncertain": read("SELECT count(*) FROM outgoing WHERE state='uncertain'")?,
            "floor": read("SELECT floor FROM replay_state WHERE singleton=1")?,
            "commandFloor": read("SELECT floor FROM command_replay_state WHERE id=1")?,
            "binding": binding,
            "settings": serde_json::from_slice::<serde_json::Value>(&settings)?
        })
    );
    Ok(())
}
