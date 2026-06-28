use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

const RANSOMWARE_LIVE_BASE: &str = "https://api.ransomware.live/v2";
const RANSOMLOOK_BASE: &str = "https://www.ransomlook.io/api";
const RANSOMNOTES_URL: &str = "https://www.ransomware.live/ransomnotes";

// === API レスポンス構造体 ===

#[derive(Debug, Deserialize)]
pub struct RansomwareLiveGroup {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    #[serde(default)]
    pub ttps: Option<serde_json::Value>,
    #[serde(default)]
    pub locations: Vec<Location>,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub fqdn: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub available: Option<bool>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default, rename = "type")]
    pub location_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RansomlookGroup {
    #[serde(default)]
    pub meta: Option<String>,
    #[serde(default)]
    pub profile: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct RansomlookMeta {
    #[serde(default)]
    pub tox: Option<String>,
    #[serde(default)]
    pub telegram: Option<String>,
    #[serde(default)]
    pub jabber: Option<String>,
    #[serde(default)]
    pub pgp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Victim {
    #[serde(default, alias = "post_title", alias = "victim")]
    pub name: Option<String>,
    #[serde(default, alias = "group")]
    pub group_name: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "post_url", alias = "claim_url")]
    pub post_url: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub screenshot: Option<String>,
    #[serde(default)]
    pub data_size: Option<String>,
    #[serde(default)]
    pub ransom: Option<String>,
    #[serde(default)]
    pub discovered: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
}

#[derive(Debug)]
pub struct RansomNote {
    pub group_name: String,
    pub filename: String,
    pub file_type: String,
    pub url: String,
}

// === メイン同期処理 ===

pub fn run_sync(conn: &Connection) -> Result<SyncStats, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTPクライアント作成エラー: {}", e))?;

    let mut stats = SyncStats::default();

    // 1. ransomware.live /groups → グループ名、locations取得
    println!("グループ情報を同期中...");
    let groups = fetch_groups_ransomware_live(&client)?;
    println!("  ransomware.live: {} グループ取得", groups.len());

    // グループをDBに保存
    let mut group_ids: HashMap<String, i64> = HashMap::new();
    let mut group_ids_lowercase: HashMap<String, i64> = HashMap::new();
    for group in &groups {
        let tools_json = group.tools.as_ref().map(|t| t.to_string());
        let ttps_json = group.ttps.as_ref().map(|t| t.to_string());
        let group_id = upsert_group(conn, &group.name, group.description.as_deref(), tools_json.as_deref(), ttps_json.as_deref())?;
        group_ids.insert(group.name.clone(), group_id);
        group_ids_lowercase.insert(group.name.to_lowercase(), group_id);

        // locations保存
        for loc in &group.locations {
            if let (Some(slug), Some(fqdn)) = (&loc.slug, &loc.fqdn) {
                let loc_type = loc.location_type.as_deref().unwrap_or("DLS");
                upsert_location(conn, group_id, loc_type, slug, fqdn, loc.title.as_deref(), loc.available.unwrap_or(false), loc.updated.as_deref())?;
                stats.urls += 1;
            }
        }
    }
    stats.groups = groups.len();

    // 2. ransomlook.io /group/{name} → 各グループのTOX、Telegram、PGP、profile補完
    println!("  ransomlook.io: 連絡先情報を補完中...");
    let total = groups.len();
    for (i, group) in groups.iter().enumerate() {
        if let Ok(ransomlook) = fetch_group_ransomlook(&client, &group.name) {
            if let Some(meta_str) = &ransomlook.meta {
                if let Ok(meta) = serde_json::from_str::<RansomlookMeta>(meta_str) {
                    update_group_contacts(conn, &group.name, meta.tox.as_deref(), meta.telegram.as_deref(), meta.jabber.as_deref(), meta.pgp.as_deref())?;
                }
            }
            if let Some(profile) = &ransomlook.profile {
                if !profile.is_empty() {
                    append_profile_to_description(conn, &group.name, profile)?;
                }
            }
        }
        // 100ms間隔
        thread::sleep(Duration::from_millis(100));
        print!("\r  ransomlook.io: 連絡先情報を補完中... {}/{}", i + 1, total);
    }
    println!();

    // 3. ransomware.live /recentvictims → 被害者情報取得 (60秒待機)
    println!("被害者情報を同期中...");
    println!("  レート制限対策: 60秒待機...");
    thread::sleep(Duration::from_secs(60));

    let victims = fetch_victims_ransomware_live(&client)?;
    println!("  ransomware.live: {} 件取得", victims.len());

    for victim in &victims {
        let group_name = victim.group_name.as_deref().unwrap_or("");
        if let Some(&group_id) = group_ids.get(group_name) {
            let name = victim.name.as_deref().unwrap_or("");
            if !name.is_empty() {
                upsert_victim(conn, group_id, victim)?;
                stats.victims += 1;
            }
        }
    }

    // 4. ransomware.live /ransomnotes ページをスクレイピング
    println!("ランサムノートを取得中...");
    let notes = fetch_ransom_notes(&client)?;
    println!("  ransomware.live: {} ノート取得", notes.len());

    for note in &notes {
        // 小文字で比較（ransomnotes pageは小文字、APIは大文字小文字混在）
        if let Some(&group_id) = group_ids_lowercase.get(&note.group_name.to_lowercase()) {
            upsert_ransom_note(conn, group_id, &note.filename, &note.file_type, &note.url)?;
            stats.notes += 1;
        }
    }

    Ok(stats)
}

#[derive(Default)]
pub struct SyncStats {
    pub groups: usize,
    pub urls: usize,
    pub victims: usize,
    pub notes: usize,
}

// === API 呼び出し ===

fn fetch_groups_ransomware_live(client: &reqwest::blocking::Client) -> Result<Vec<RansomwareLiveGroup>, String> {
    let url = format!("{}/groups", RANSOMWARE_LIVE_BASE);
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー: {}", e))?;
    let text = resp.text().map_err(|e| format!("レスポンス読み込みエラー: {}", e))?;

    // レート制限チェック
    if text.contains("per") && text.contains("minute") {
        return Err("レート制限: 1分後に再試行してください".to_string());
    }

    serde_json::from_str(&text).map_err(|e| format!("JSONパースエラー: {}", e))
}

fn fetch_group_ransomlook(client: &reqwest::blocking::Client, name: &str) -> Result<RansomlookGroup, String> {
    let url = format!("{}/group/{}", RANSOMLOOK_BASE, name);
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー: {}", e))?;
    let groups: Vec<RansomlookGroup> = resp.json().map_err(|e| format!("JSONパースエラー: {}", e))?;
    groups.into_iter().next().ok_or_else(|| "グループが見つかりません".to_string())
}

fn fetch_victims_ransomware_live(client: &reqwest::blocking::Client) -> Result<Vec<Victim>, String> {
    let url = format!("{}/recentvictims", RANSOMWARE_LIVE_BASE);
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー: {}", e))?;
    let text = resp.text().map_err(|e| format!("レスポンス読み込みエラー: {}", e))?;

    if text.contains("per") && text.contains("minute") {
        return Err("レート制限: 1分後に再試行してください".to_string());
    }

    serde_json::from_str(&text).map_err(|e| format!("JSONパースエラー: {}", e))
}

fn fetch_ransom_notes(client: &reqwest::blocking::Client) -> Result<Vec<RansomNote>, String> {
    let resp = client.get(RANSOMNOTES_URL).send().map_err(|e| format!("ページ取得エラー: {}", e))?;
    let html = resp.text().map_err(|e| format!("レスポンス読み込みエラー: {}", e))?;

    parse_ransom_notes_html(&html)
}

fn parse_ransom_notes_html(html: &str) -> Result<Vec<RansomNote>, String> {
    let mut notes = Vec::new();

    // /ransomnote/{group}/{filename} パターンを探す
    let re_pattern = r#"href="/ransomnote/([^/]+)/([^"]+)""#;
    let re = regex::Regex::new(re_pattern).map_err(|e| format!("正規表現エラー: {}", e))?;

    for cap in re.captures_iter(html) {
        let group_name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let filename_encoded = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // URLデコード
        let filename = urlencoding::decode(filename_encoded).unwrap_or_else(|_| filename_encoded.into()).to_string();

        // ファイルタイプ抽出
        let file_type = if filename.ends_with(".txt") {
            "txt"
        } else if filename.ends_with(".html") {
            "html"
        } else if filename.ends_with(".hta") {
            "hta"
        } else {
            "unknown"
        };

        let url = format!("https://www.ransomware.live/ransomnote/{}/{}", group_name, filename_encoded);

        notes.push(RansomNote {
            group_name: group_name.to_string(),
            filename,
            file_type: file_type.to_string(),
            url,
        });
    }

    Ok(notes)
}

// === DB 操作 ===

fn upsert_group(conn: &Connection, name: &str, description: Option<&str>, tools: Option<&str>, ttps: Option<&str>) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO groups (name, description, tools, ttps) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET
            description = COALESCE(excluded.description, groups.description),
            tools = COALESCE(excluded.tools, groups.tools),
            ttps = COALESCE(excluded.ttps, groups.ttps),
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![name, description, tools, ttps],
    ).map_err(|e| format!("グループ挿入エラー: {}", e))?;

    let id: i64 = conn.query_row("SELECT id FROM groups WHERE name = ?1", [name], |row| row.get(0))
        .map_err(|e| format!("グループID取得エラー: {}", e))?;
    Ok(id)
}

fn update_group_contacts(conn: &Connection, name: &str, tox: Option<&str>, telegram: Option<&str>, jabber: Option<&str>, pgp: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET
            tox_id = COALESCE(?2, tox_id),
            telegram = COALESCE(?3, telegram),
            jabber = COALESCE(?4, jabber),
            pgp = COALESCE(?5, pgp),
            updated_at = CURRENT_TIMESTAMP
         WHERE name = ?1",
        rusqlite::params![name, tox, telegram, jabber, pgp],
    ).map_err(|e| format!("グループ連絡先更新エラー: {}", e))?;
    Ok(())
}

fn append_profile_to_description(conn: &Connection, name: &str, profile: &[String]) -> Result<(), String> {
    let profile_text = format!("\n\n## References\n{}", profile.iter().map(|u| format!("- {}", u)).collect::<Vec<_>>().join("\n"));
    conn.execute(
        "UPDATE groups SET description = COALESCE(description, '') || ?2, updated_at = CURRENT_TIMESTAMP WHERE name = ?1",
        rusqlite::params![name, profile_text],
    ).map_err(|e| format!("グループプロファイル追記エラー: {}", e))?;
    Ok(())
}

fn upsert_location(conn: &Connection, group_id: i64, loc_type: &str, slug: &str, fqdn: &str, title: Option<&str>, available: bool, last_checked: Option<&str>) -> Result<(), String> {
    conn.execute(
        "INSERT INTO group_locations (group_id, type, slug, fqdn, title, available, last_checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(group_id, slug) DO UPDATE SET
            available = excluded.available,
            title = COALESCE(excluded.title, group_locations.title),
            last_checked_at = excluded.last_checked_at,
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![group_id, loc_type, slug, fqdn, title, available, last_checked],
    ).map_err(|e| format!("ロケーション挿入エラー: {}", e))?;
    Ok(())
}

fn upsert_victim(conn: &Connection, group_id: i64, victim: &Victim) -> Result<(), String> {
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
    ).map_err(|e| format!("被害者挿入エラー: {}", e))?;
    Ok(())
}

fn upsert_ransom_note(conn: &Connection, group_id: i64, filename: &str, file_type: &str, url: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO ransom_notes (group_id, filename, file_type, url)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(group_id, filename) DO NOTHING",
        rusqlite::params![group_id, filename, file_type, url],
    ).map_err(|e| format!("ランサムノート挿入エラー: {}", e))?;
    Ok(())
}
