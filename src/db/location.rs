use rusqlite::Connection;

// === Structs ===

pub struct LocationRow {
    pub loc_type: String,
    pub slug: String,
    pub title: Option<String>,
    pub available: bool,
    pub last_checked_at: Option<String>,
}

// === Write ===

pub fn upsert(
    conn: &Connection,
    group_id: i64,
    loc_type: &str,
    slug: &str,
    fqdn: &str,
    title: Option<&str>,
    available: bool,
    last_checked: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO group_locations (group_id, type, slug, fqdn, title, available, last_checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(group_id, slug) DO UPDATE SET
            available = excluded.available,
            title = COALESCE(excluded.title, group_locations.title),
            last_checked_at = excluded.last_checked_at,
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![group_id, loc_type, slug, fqdn, title, available, last_checked],
    )
    .map_err(|e| format!("ロケーション挿入エラー: {}", e))?;
    Ok(())
}

// === Read ===

pub fn list_by_group(conn: &Connection, group_id: usize) -> Result<Vec<LocationRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT type, slug, title, available, last_checked_at FROM group_locations WHERE group_id = ?1 ORDER BY type, available DESC",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map([group_id], |row| {
            Ok(LocationRow {
                loc_type: row.get(0)?,
                slug: row.get(1)?,
                title: row.get(2)?,
                available: row.get(3)?,
                last_checked_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}
