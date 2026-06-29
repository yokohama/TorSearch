use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use crate::db;

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
    pub profile: Option<Vec<String>>,
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
        let group_id = db::upsert_group(conn, &group.name, group.description.as_deref(), tools_json.as_deref(), ttps_json.as_deref())?;
        group_ids.insert(group.name.clone(), group_id);
        group_ids_lowercase.insert(group.name.to_lowercase(), group_id);

        // locations保存
        for loc in &group.locations {
            if let (Some(slug), Some(fqdn)) = (&loc.slug, &loc.fqdn) {
                let loc_type = loc.location_type.as_deref().unwrap_or("DLS");
                db::upsert_location(conn, group_id, loc_type, slug, fqdn, loc.title.as_deref(), loc.available.unwrap_or(false), loc.updated.as_deref())?;
                stats.urls += 1;
            }
        }
    }
    stats.groups = groups.len();

    // 2. ransomlook.io /group/{name} → 各グループのTOX、Telegram、PGP、profile補完
    println!("  ransomlook.io: 連絡先情報を補完中...");
    let total = groups.len();
    for (i, group) in groups.iter().enumerate() {
        let Ok(ransomlook) = fetch_group_ransomlook(&client, &group.name) else { continue };

        let tox = ransomlook.tox.as_deref().filter(|s| !s.is_empty());
        let telegram = ransomlook.telegram.as_deref().filter(|s| !s.is_empty());
        let jabber = ransomlook.jabber.as_deref().filter(|s| !s.is_empty());
        let pgp = ransomlook.pgp.as_deref().filter(|s| !s.is_empty());
        if tox.is_some() || telegram.is_some() || jabber.is_some() || pgp.is_some() {
            db::update_contacts(conn, &group.name, tox, telegram, jabber, pgp)?;
        }
        if let Some(profile) = &ransomlook.profile {
            if !profile.is_empty() {
                db::append_profile_to_description(conn, &group.name, profile)?;
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
        if let Some(&group_id) = group_ids_lowercase.get(&group_name.to_lowercase()) {
            let name = victim.name.as_deref().unwrap_or("");
            if !name.is_empty() {
                let victim_data = db::VictimData {
                    name: victim.name.as_deref(),
                    country: victim.country.as_deref(),
                    activity: victim.activity.as_deref(),
                    description: victim.description.as_deref(),
                    post_url: victim.post_url.as_deref(),
                    website: victim.website.as_deref(),
                    screenshot: victim.screenshot.as_deref(),
                    data_size: victim.data_size.as_deref(),
                    ransom: victim.ransom.as_deref(),
                    discovered: victim.discovered.as_deref(),
                    published: victim.published.as_deref(),
                };
                db::upsert_victim(conn, group_id, &victim_data)?;
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
            db::upsert_ransom_note(conn, group_id, &note.filename, &note.file_type, &note.url)?;
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
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー [{}]: {:?}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTPエラー [{}]: {}", url, status));
    }

    let text = resp.text().map_err(|e| format!("レスポンス読み込みエラー [{}]: {:?}", url, e))?;

    // レート制限チェック
    if text.contains("per") && text.contains("minute") {
        return Err(format!("レート制限 [{}]: 1分後に再試行してください", url));
    }

    serde_json::from_str(&text).map_err(|e| format!("JSONパースエラー [{}]: {:?}\nレスポンス先頭200文字: {}", url, e, &text.chars().take(200).collect::<String>()))
}

fn fetch_group_ransomlook(client: &reqwest::blocking::Client, name: &str) -> Result<RansomlookGroup, String> {
    let url = format!("{}/group/{}", RANSOMLOOK_BASE, name);
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー [{}]: {:?}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTPエラー [{}]: {}", url, status));
    }

    let arr: Vec<serde_json::Value> = resp.json().map_err(|e| format!("JSONパースエラー [{}]: {:?}", url, e))?;
    let first = arr.into_iter().next().ok_or_else(|| format!("グループが見つかりません [{}]", url))?;
    serde_json::from_value(first).map_err(|e| format!("グループパースエラー [{}]: {:?}", url, e))
}

fn fetch_victims_ransomware_live(client: &reqwest::blocking::Client) -> Result<Vec<Victim>, String> {
    let url = format!("{}/recentvictims", RANSOMWARE_LIVE_BASE);
    let resp = client.get(&url).send().map_err(|e| format!("API呼び出しエラー [{}]: {:?}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTPエラー [{}]: {}", url, status));
    }

    let text = resp.text().map_err(|e| format!("レスポンス読み込みエラー [{}]: {:?}", url, e))?;

    if text.contains("per") && text.contains("minute") {
        return Err(format!("レート制限 [{}]: 1分後に再試行してください", url));
    }

    serde_json::from_str(&text).map_err(|e| format!("JSONパースエラー [{}]: {:?}\nレスポンス先頭200文字: {}", url, e, &text.chars().take(200).collect::<String>()))
}

fn fetch_ransom_notes(client: &reqwest::blocking::Client) -> Result<Vec<RansomNote>, String> {
    let resp = client.get(RANSOMNOTES_URL).send().map_err(|e| format!("ページ取得エラー [{}]: {:?}", RANSOMNOTES_URL, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTPエラー [{}]: {}", RANSOMNOTES_URL, status));
    }

    let html = resp.text().map_err(|e| format!("レスポンス読み込みエラー [{}]: {:?}", RANSOMNOTES_URL, e))?;

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

