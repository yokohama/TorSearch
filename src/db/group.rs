use rusqlite::Connection;

// === Structs ===

/// グループ一覧表示用
pub struct GroupRow {
    pub id: i64,
    pub name: String,
    pub victim_count: i64,
    pub last_activity: String,
    pub dls_count: i64,
    pub note_count: i64,
    pub has_tox: bool,
    pub has_telegram: bool,
    pub has_jabber: bool,
    pub has_pgp: bool,
}

/// グループ詳細表示用
pub struct GroupDetail {
    pub name: String,
    pub tox: Option<String>,
    pub telegram: Option<String>,
    pub jabber: Option<String>,
    pub pgp: Option<String>,
    pub description: Option<String>,
    pub tools: Option<String>,
    pub ttps: Option<String>,
}

// === Write ===

pub fn upsert(
    conn: &Connection,
    name: &str,
    description: Option<&str>,
    tools: Option<&str>,
    ttps: Option<&str>,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO groups (name, description, tools, ttps) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET
            description = COALESCE(excluded.description, groups.description),
            tools = COALESCE(excluded.tools, groups.tools),
            ttps = COALESCE(excluded.ttps, groups.ttps),
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![name, description, tools, ttps],
    )
    .map_err(|e| format!("グループ挿入エラー: {}", e))?;

    let id: i64 =
        conn.query_row("SELECT id FROM groups WHERE name = ?1", [name], |row| {
            row.get(0)
        })
        .map_err(|e| format!("グループID取得エラー: {}", e))?;
    Ok(id)
}

// === Read ===

pub fn list(
    conn: &Connection,
    pattern: &str,
    limit: usize,
) -> Result<Vec<GroupRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
                g.id,
                g.name,
                COUNT(DISTINCT v.id) as victim_count,
                COALESCE(MAX(substr(v.discovered_at, 1, 10)), '-') as last_activity,
                COUNT(DISTINCT CASE WHEN gl.type = 'DLS' THEN gl.id END) as dls_count,
                COUNT(DISTINCT rn.id) as note_count,
                g.tox_id IS NOT NULL as has_tox,
                g.telegram IS NOT NULL as has_telegram,
                g.jabber IS NOT NULL as has_jabber,
                g.pgp IS NOT NULL as has_pgp
             FROM groups g
             LEFT JOIN victims v ON g.id = v.group_id
             LEFT JOIN group_locations gl ON g.id = gl.group_id
             LEFT JOIN ransom_notes rn ON g.id = rn.group_id
             WHERE g.name LIKE ?1
             GROUP BY g.id
             ORDER BY last_activity DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok(GroupRow {
                id: row.get(0)?,
                name: row.get(1)?,
                victim_count: row.get(2)?,
                last_activity: row.get(3)?,
                dls_count: row.get(4)?,
                note_count: row.get(5)?,
                has_tox: row.get(6)?,
                has_telegram: row.get(7)?,
                has_jabber: row.get(8)?,
                has_pgp: row.get(9)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

pub fn get_by_id(conn: &Connection, id: usize) -> Result<GroupDetail, String> {
    conn.query_row(
        "SELECT name, tox_id, telegram, jabber, pgp, description, tools, ttps FROM groups WHERE id = ?1",
        [id],
        |row| {
            Ok(GroupDetail {
                name: row.get(0)?,
                tox: row.get(1)?,
                telegram: row.get(2)?,
                jabber: row.get(3)?,
                pgp: row.get(4)?,
                description: row.get(5)?,
                tools: row.get(6)?,
                ttps: row.get(7)?,
            })
        },
    )
    .map_err(|e| format!("グループ取得エラー: {}", e))
}

pub fn get_tools_json(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT tools FROM groups
               WHERE tools IS NOT NULL AND tools != '[]'",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn get_ttps_json(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ttps FROM groups
              WHERE ttps IS NOT NULL AND ttps != '[]'",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn update_contacts(
    conn: &Connection,
    name: &str,
    tox: Option<&str>,
    telegram: Option<&str>,
    jabber: Option<&str>,
    pgp: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET
            tox_id = COALESCE(?2, tox_id),
            telegram = COALESCE(?3, telegram),
            jabber = COALESCE(?4, jabber),
            pgp = COALESCE(?5, pgp),
            updated_at = CURRENT_TIMESTAMP
         WHERE name = ?1",
        rusqlite::params![name, tox, telegram, jabber, pgp],
    )
    .map_err(|e| format!("グループ連絡先更新エラー: {}", e))?;
    Ok(())
}

pub fn append_profile_to_description(
    conn: &Connection,
    name: &str,
    profile: &[String],
) -> Result<(), String> {
    let profile_text = format!(
        "\n\n## References\n{}",
        profile
            .iter()
            .map(|u| format!("- {}", u))
            .collect::<Vec<_>>()
            .join("\n")
    );
    conn.execute(
        "UPDATE groups SET description = COALESCE(description, '') || ?2, updated_at = CURRENT_TIMESTAMP WHERE name = ?1",
        rusqlite::params![name, profile_text],
    )
    .map_err(|e| format!("グループプロファイル追記エラー: {}", e))?;
    Ok(())
}
