use rusqlite::Connection;

// === Structs ===

pub struct VictimRow {
    pub id: i64,
    pub name: String,
    pub group_name: String,
    pub country: String,
    pub discovered_at: String,
}

pub struct VictimDetail {
    pub name: String,
    pub group_name: String,
    pub country: Option<String>,
    pub activity: Option<String>,
    pub description: Option<String>,
    pub post_url: Option<String>,
    pub website: Option<String>,
    pub discovered_at: Option<String>,
}

pub struct CountryCount {
    pub country: String,
    pub count: i64,
    pub last_discovered: String,
}

// === Write ===

pub struct VictimData<'a> {
    pub name: Option<&'a str>,
    pub country: Option<&'a str>,
    pub activity: Option<&'a str>,
    pub description: Option<&'a str>,
    pub post_url: Option<&'a str>,
    pub website: Option<&'a str>,
    pub screenshot: Option<&'a str>,
    pub data_size: Option<&'a str>,
    pub ransom: Option<&'a str>,
    pub discovered: Option<&'a str>,
    pub published: Option<&'a str>,
}

pub fn upsert(conn: &Connection, group_id: i64, victim: &VictimData) -> Result<(), String> {
    conn.execute(
        "INSERT INTO victims (
            group_id, post_title, country, activity, description,
            post_url, website, screenshot_url, data_size, ransom,
            discovered_at, published_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT DO NOTHING",
        rusqlite::params![
            group_id,
            victim.name,
            victim.country,
            victim.activity,
            victim.description,
            victim.post_url,
            victim.website,
            victim.screenshot,
            victim.data_size,
            victim.ransom,
            victim.discovered,
            victim.published,
        ],
    )
    .map_err(|e| format!("被害者挿入エラー: {}", e))?;
    Ok(())
}

// === Read ===

pub fn list(conn: &Connection, limit: usize) -> Result<Vec<VictimRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
                v.id,
                v.post_title,
                g.name,
                COALESCE(v.country, '-'),
                COALESCE(substr(v.discovered_at, 1, 10), '-')
             FROM victims v
             JOIN groups g ON v.group_id = g.id
             ORDER BY v.discovered_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map([limit], |row| {
            Ok(VictimRow {
                id: row.get(0)?,
                name: row.get(1)?,
                group_name: row.get(2)?,
                country: row.get(3)?,
                discovered_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

pub fn get_by_id(conn: &Connection, id: usize) -> Result<VictimDetail, String> {
    conn.query_row(
        "SELECT 
           v.post_title, 
           g.name, 
           v.country, 
           v.activity, 
           v.description, 
           v.post_url, 
           v.website, 
           v.discovered_at
         FROM victims v
         JOIN groups g ON v.group_id = g.id
         WHERE v.id = ?1",
        [id],
        |row| {
            Ok(VictimDetail {
                name: row.get(0)?,
                group_name: row.get(1)?,
                country: row.get(2)?,
                activity: row.get(3)?,
                description: row.get(4)?,
                post_url: row.get(5)?,
                website: row.get(6)?,
                discovered_at: row.get(7)?,
            })
        },
    )
    .map_err(|e| format!("被害者取得エラー: {}", e))
}

pub fn count_by_country(conn: &Connection) -> Result<Vec<CountryCount>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(NULLIF(country, ''), 'Unknown') as c, COUNT(*) as cnt,
                    COALESCE(MAX(substr(discovered_at, 1, 10)), '-') as last_date
             FROM victims
             GROUP BY c
             ORDER BY cnt DESC",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CountryCount {
                country: row.get(0)?,
                count: row.get(1)?,
                last_discovered: row.get(2)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

pub struct VictimSummary {
    pub name: String,
    pub country: Option<String>,
    pub discovered_at: Option<String>,
}

pub fn list_by_group(conn: &Connection, group_id: usize, limit: usize) -> Result<Vec<VictimSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
               post_title,
               country,
               substr(discovered_at, 1, 10)
             FROM victims WHERE group_id = ?1 ORDER BY discovered_at DESC LIMIT ?2",
        )
        .map_err(|e| format!("クエリ準備エラー: {}", e))?;

    let rows = stmt
        .query_map(rusqlite::params![group_id, limit], |row| {
            Ok(VictimSummary {
                name: row.get(0)?,
                country: row.get(1)?,
                discovered_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("クエリ実行エラー: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}
