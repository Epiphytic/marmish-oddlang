use rusqlite::{Connection, OpenFlags, ffi};
use std::io::Read;
use zeroize::Zeroizing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut material = Zeroizing::new([0u8; 67]);
    material[..2].copy_from_slice(b"x'");
    material[66] = b'\'';
    std::io::stdin().read_exact(&mut material[2..66])?;
    let path = std::env::args_os().nth(1).ok_or("expected database")?;
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
    // SAFETY: single live connection and fixed-length input buffer.
    if unsafe { ffi::sqlite3_key(conn.handle(), material.as_ptr().cast(), 67) } != ffi::SQLITE_OK {
        return Err("SQLCipher key setup failed".into());
    }
    conn.execute_batch("PRAGMA cipher_compatibility=4; PRAGMA cipher_page_size=4096;")?;
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    assert_eq!(integrity, "ok");
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let mut schema = conn.prepare("SELECT type,name,sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY type,name")?;
    let mut objects = Vec::new();
    for row in schema.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
        let (kind, name, sql) = row?;
        let count = if kind == "table" {
            let identifier = name.replace('"', "\"\"");
            conn.query_row(&format!("SELECT count(*) FROM \"{identifier}\""), [], |r| r.get::<_, i64>(0))?
        } else { 0 };
        objects.push(serde_json::json!({"type":kind,"name":name,"declaration":sql,"rows":count}));
    }
    let measurements = if std::env::args().nth(2).as_deref() == Some("journal") {
        let query = |sql| -> rusqlite::Result<u32> { conn.query_row(sql, [], |row| row.get(0)) };
        let binding: String = conn.query_row("SELECT account FROM binding", [], |row| row.get(0))?;
        let baseline: Vec<u8> = conn.query_row("SELECT settings FROM command_settings WHERE id=1", [], |row| row.get(0))?;
        let settings: serde_json::Value = serde_json::from_slice(&baseline)?;
        serde_json::json!({"identity":binding,"conversations":query("SELECT count(*) FROM conversations")?,
            "received":query("SELECT count(*) FROM incoming WHERE body_type = 'message' AND envelope IS NOT NULL")?,
            "pending":query("SELECT count(*) FROM outgoing WHERE state IN ('unsubmitted','prepared','uncertain') AND envelope IS NOT NULL")?,
            "baselineConfig": settings})
    } else {
        serde_json::Value::Null
    };
    println!("{}", serde_json::json!({"objects":objects,"version":version,"measurements":measurements}));
    Ok(())
}
