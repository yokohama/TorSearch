use rusqlite::Connection;

// === Structs ===

pub struct RansomNoteRow {
    pub filename: String,
    pub file_type: String,
    pub url: String,
}

// === Write ===

pub fn upsert(
    conn: &Connection,
    group_id: i64,
    filename: &str,
    file_type: &str,
    url: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO ransom_notes (group_id, filename, file_type, url)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(group_id, filename) DO NOTHING",
        rusqlite::params![group_id, filename, file_type, url],
    )
    .map_err(|e| format!("ランサムノート挿入エラー: {}", e))?;
    Ok(())
}

// === Read ===

pub fn list_by_group(conn: &Connection, group_id: usize) -> Result<Vec<RansomNoteRow>, String> {
    let mut stmt = conn
        .prepare("SELECT filename, file_type, url FROM ransom_notes WHERE group_id = ?1")
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map([group_id], |row| {
            Ok(RansomNoteRow {
                filename: row.get(0)?,
                file_type: row.get(1)?,
                url: row.get(2)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

pub struct NoteWithGroup {
    pub group_id: i64,
    pub group_name: String,
    pub url: String,
}

pub fn list(conn: &Connection, limit: usize) -> Result<Vec<NoteWithGroup>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.group_id, g.name, r.url
             FROM ransom_notes r
             JOIN groups g ON r.group_id = g.id
             ORDER BY r.id DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map([limit], |row| {
            Ok(NoteWithGroup {
                group_id: row.get(0)?,
                group_name: row.get(1)?,
                url: row.get(2)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}
