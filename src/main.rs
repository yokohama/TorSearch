use std::fs;
use std::path::Path;

mod sync;

const DB_PATH: &str = "data/torsearch.db";
const SCHEMA_PATH: &str = "design/sqlite-ddl.sql";
const TEMPLATE_GROUP_DETAIL: &str = "design/templates/group_detail.md";
const TEMPLATE_VICTIM_DETAIL: &str = "design/templates/victim_detail.md";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "sync" => cmd_sync(),
        "groups" => cmd_groups(&args[2..]),
        "victims" => cmd_victims(&args[2..]),
        _ => print_usage(),
    }
}

fn print_usage() {
    println!("Usage: torsearch <command> [options]");
    println!();
    println!("Commands:");
    println!("  sync              APIからデータを同期 (初回はDB自動作成)");
    println!("  groups [-N] [id]  グループ一覧/詳細/検索");
    println!("  groups --by-tools     ツール別集計");
    println!("  groups --by-ttps      TTPs別集計");
    println!("  victims [-N] [id] 被害者一覧/詳細");
    println!("  victims --by-country  国別集計");
}

fn cmd_sync() {
    // DBがなければ自動でinit
    if !Path::new(DB_PATH).exists() {
        println!("データベースを初期化中...");

        let schema = match fs::read_to_string(SCHEMA_PATH) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("スキーマファイル読み込みエラー: {}", e);
                return;
            }
        };

        let conn = match rusqlite::Connection::open(DB_PATH) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("DB接続エラー: {}", e);
                return;
            }
        };

        if let Err(e) = conn.execute_batch(&schema) {
            eprintln!("スキーマ適用エラー: {}", e);
            return;
        }
        println!("完了: {}", DB_PATH);
        println!();
    }

    let conn = match rusqlite::Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("DB接続エラー: {}", e);
            return;
        }
    };

    match sync::run_sync(&conn) {
        Ok(stats) => {
            println!("同期完了: {} グループ, {} URL, {} 被害者, {} ノート",
                stats.groups, stats.urls, stats.victims, stats.notes);
        }
        Err(e) => {
            eprintln!("同期エラー: {}", e);
        }
    }
}

fn cmd_groups(args: &[String]) {
    let conn = match rusqlite::Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("DB接続エラー: {}", e);
            return;
        }
    };

    // --by-tools オプション
    if args.iter().any(|a| a == "--by-tools") {
        show_tools_summary(&conn);
        return;
    }

    // --by-ttps オプション
    if args.iter().any(|a| a == "--by-ttps") {
        show_ttps_summary(&conn);
        return;
    }

    let (limit, id, keyword) = parse_args(args, 20);

    if let Some(group_id) = id {
        show_group_detail(&conn, group_id);
        return;
    }

    let pattern = keyword.as_ref().map(|kw| format!("%{}%", kw)).unwrap_or_else(|| "%".to_string());

    let mut stmt = match conn.prepare(
        "SELECT
            g.id,
            g.name,
            COUNT(DISTINCT v.id) as victim_count,
            COALESCE(MAX(substr(v.discovered_at, 1, 10)), '-') as last_activity,
            COUNT(DISTINCT CASE WHEN gl.type = 'DLS' THEN gl.id END) as dls_count
         FROM groups g
         LEFT JOIN victims v ON g.id = v.group_id
         LEFT JOIN group_locations gl ON g.id = gl.group_id
         WHERE g.name LIKE ?1
         GROUP BY g.id
         ORDER BY last_activity DESC
         LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("クエリ準備エラー: {}", e);
            return;
        }
    };

    let rows = match stmt.query_map(rusqlite::params![pattern, limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("クエリ実行エラー: {}", e);
            return;
        }
    };

    println!("-------------------------------------------------------------------");
    println!(" ID  | Group              | Victims | Last Activity | DLS Sites");
    println!("-------------------------------------------------------------------");

    for row in rows {
        if let Ok((id, name, victims, date, dls)) = row {
            let name_display: String = name.chars().take(18).collect();
            println!(" {:<3} | {:<18} | {:<7} | {:<13} | {}", id, name_display, victims, date, dls);
        }
    }
    println!("-------------------------------------------------------------------");
}

fn show_group_detail(conn: &rusqlite::Connection, group_id: usize) {
    let template = match fs::read_to_string(TEMPLATE_GROUP_DETAIL) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("テンプレート読み込みエラー: {}", e);
            return;
        }
    };

    let result: Result<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>), _> = conn.query_row(
        "SELECT name, tox_id, telegram, jabber, pgp, description, tools, ttps FROM groups WHERE id = ?1",
        [group_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
    );

    let (name, tox, telegram, jabber, pgp, description, tools, ttps) = match result {
        Ok(r) => r,
        Err(_) => {
            eprintln!("グループID {} が見つかりません", group_id);
            return;
        }
    };

    // Description
    let description_str = description.as_deref().unwrap_or("-").to_string();

    // Tools
    let tools_str = if let Some(ref t) = tools {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(t) {
            if !arr.is_empty() {
                let mut lines = Vec::new();
                for obj in arr {
                    if let Some(map) = obj.as_object() {
                        for (category, items) in map {
                            if let Some(arr) = items.as_array() {
                                let tools: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                                lines.push(format!("- **{}**: {}", category, tools.join(", ")));
                            }
                        }
                    }
                }
                if lines.is_empty() { "-".to_string() } else { lines.join("\n") }
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        }
    } else {
        "-".to_string()
    };

    // TTPs
    let ttps_str = if let Some(ref t) = ttps {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(t) {
            if !arr.is_empty() {
                let mut lines = Vec::new();
                for tactic in arr {
                    let tactic_name = tactic.get("tactic_name").and_then(|v| v.as_str()).unwrap_or("-");
                    let tactic_id = tactic.get("tactic_id").and_then(|v| v.as_str()).unwrap_or("-");
                    lines.push(format!("- **{} ({})**: ", tactic_name, tactic_id));
                    if let Some(techniques) = tactic.get("techniques").and_then(|v| v.as_array()) {
                        let tech_names: Vec<&str> = techniques.iter()
                            .filter_map(|t| t.get("technique_name").and_then(|v| v.as_str()))
                            .collect();
                        lines.push(format!("  {}", tech_names.join(", ")));
                    }
                }
                if lines.is_empty() { "-".to_string() } else { lines.join("\n") }
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        }
    } else {
        "-".to_string()
    };

    // Sites
    let mut stmt = conn.prepare(
        "SELECT type, slug, title, available, last_checked_at FROM group_locations WHERE group_id = ?1 ORDER BY type, available DESC"
    ).unwrap();
    let sites: Vec<(String, String, Option<String>, bool, Option<String>)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    let sites_str = if !sites.is_empty() {
        sites.iter().map(|(loc_type, url, title, available, last_checked)| {
            let status = if *available { "UP" } else { "DOWN" };
            let title_str = title.as_deref().unwrap_or("-");
            let checked_str = last_checked.as_ref().map(|c| c[..10.min(c.len())].to_string()).unwrap_or("-".to_string());
            format!("| {} | {} | {} | `{}` | {} |", loc_type, status, title_str, url, checked_str)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - | - | - |".to_string()
    };

    // Ransom Notes
    let mut stmt = conn.prepare(
        "SELECT filename, file_type, url FROM ransom_notes WHERE group_id = ?1"
    ).unwrap();
    let notes: Vec<(String, String, String)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    let notes_str = if !notes.is_empty() {
        notes.iter().map(|(filename, file_type, url)| {
            format!("| {} | {} | {} |", filename, file_type, url)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - |".to_string()
    };

    // Victims
    let mut stmt = conn.prepare(
        "SELECT post_title, country, substr(discovered_at, 1, 10) FROM victims WHERE group_id = ?1 ORDER BY discovered_at DESC LIMIT 5"
    ).unwrap();
    let victims: Vec<(String, Option<String>, Option<String>)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    let victims_str = if !victims.is_empty() {
        victims.iter().map(|(name, country, date)| {
            let c = country.as_deref().unwrap_or("-");
            let d = date.as_deref().unwrap_or("-");
            format!("| {} | {} | {} |", d, c, name)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - |".to_string()
    };

    // Replace placeholders
    let output = template
        .replace("{{name}}", &name)
        .replace("{{id}}", &group_id.to_string())
        .replace("{{tox}}", tox.as_deref().unwrap_or("-"))
        .replace("{{telegram}}", telegram.as_deref().unwrap_or("-"))
        .replace("{{jabber}}", jabber.as_deref().unwrap_or("-"))
        .replace("{{pgp}}", &pgp.as_ref().map(|p| format!("{}...", p.chars().take(50).collect::<String>())).unwrap_or("-".to_string()))
        .replace("{{description}}", &description_str)
        .replace("{{tools}}", &tools_str)
        .replace("{{ttps}}", &ttps_str)
        .replace("{{sites}}", &sites_str)
        .replace("{{ransom_notes}}", &notes_str)
        .replace("{{victims}}", &victims_str);

    print!("{}", output);
}

fn cmd_victims(args: &[String]) {
    let conn = match rusqlite::Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("DB接続エラー: {}", e);
            return;
        }
    };

    // --by-country オプション
    if args.iter().any(|a| a == "--by-country") {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(country, 'Unknown') as c, COUNT(*) as cnt
             FROM victims
             GROUP BY c
             ORDER BY cnt DESC"
        ).unwrap();

        let rows: Vec<(String, i64)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).unwrap().filter_map(|r| r.ok()).collect();

        println!("-------------------------------------------------------------------");
        println!(" Country | Count");
        println!("-------------------------------------------------------------------");
        for (country, count) in rows {
            println!(" {:<7} | {}", country, count);
        }
        println!("-------------------------------------------------------------------");
        return;
    }

    let (limit, id, _) = parse_args(args, 20);

    if let Some(victim_id) = id {
        show_victim_detail(&conn, victim_id);
        return;
    }

    let mut stmt = conn.prepare(
        "SELECT
            v.id,
            v.post_title,
            g.name,
            COALESCE(v.country, '-'),
            COALESCE(substr(v.discovered_at, 1, 10), '-')
         FROM victims v
         JOIN groups g ON v.group_id = g.id
         ORDER BY v.discovered_at DESC
         LIMIT ?1"
    ).unwrap();

    let rows: Vec<(i64, String, String, String, String)> = stmt.query_map([limit], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    println!("-------------------------------------------------------------------");
    println!(" ID   | Victim                    | Group          | Country | Date");
    println!("-------------------------------------------------------------------");

    for (id, victim, group, country, date) in rows {
        let victim_display: String = victim.chars().take(25).collect();
        let group_display: String = group.chars().take(14).collect();
        println!(" {:<4} | {:<25} | {:<14} | {:<7} | {}", id, victim_display, group_display, country, date);
    }
    println!("-------------------------------------------------------------------");
}

fn show_victim_detail(conn: &rusqlite::Connection, victim_id: usize) {
    let template = match fs::read_to_string(TEMPLATE_VICTIM_DETAIL) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("テンプレート読み込みエラー: {}", e);
            return;
        }
    };

    let result: Result<(String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>), _> = conn.query_row(
        "SELECT v.post_title, g.name, v.country, v.activity, v.description, v.post_url, v.website, v.discovered_at
         FROM victims v
         JOIN groups g ON v.group_id = g.id
         WHERE v.id = ?1",
        [victim_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
    );

    let (name, group, country, activity, description, post_url, website, discovered) = match result {
        Ok(r) => r,
        Err(_) => {
            eprintln!("被害者ID {} が見つかりません", victim_id);
            return;
        }
    };

    let output = template
        .replace("{{name}}", &name)
        .replace("{{id}}", &victim_id.to_string())
        .replace("{{group}}", &group)
        .replace("{{country}}", country.as_deref().unwrap_or("-"))
        .replace("{{activity}}", activity.as_deref().unwrap_or("-"))
        .replace("{{discovered}}", discovered.as_deref().unwrap_or("-"))
        .replace("{{description}}", description.as_deref().unwrap_or("-"))
        .replace("{{post_url}}", post_url.as_deref().unwrap_or("-"))
        .replace("{{website}}", website.as_deref().unwrap_or("-"));

    print!("{}", output);
}

fn parse_args(args: &[String], default_limit: usize) -> (usize, Option<usize>, Option<String>) {
    let mut limit = default_limit;
    let mut id = None;
    let mut keyword = None;

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            if let Ok(n) = arg[1..].parse::<usize>() {
                limit = n;
            }
        } else if let Ok(n) = arg.parse::<usize>() {
            id = Some(n);
        } else if !arg.starts_with('-') {
            keyword = Some(arg.clone());
        }
    }

    (limit, id, keyword)
}

fn show_tools_summary(conn: &rusqlite::Connection) {
    let mut stmt = conn.prepare("SELECT tools FROM groups WHERE tools IS NOT NULL AND tools != '[]'").unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().filter_map(|r| r.ok()).collect();

    let mut tool_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for tools_json in rows {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&tools_json) {
            for obj in arr {
                if let Some(map) = obj.as_object() {
                    for (category, items) in map {
                        if let Some(tools) = items.as_array() {
                            for tool in tools {
                                if let Some(name) = tool.as_str() {
                                    let key = format!("{}: {}", category, name);
                                    *tool_counts.entry(key).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut sorted: Vec<_> = tool_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    println!("-------------------------------------------------------------------");
    println!(" Tool                                              | Groups");
    println!("-------------------------------------------------------------------");
    for (tool, count) in sorted.iter().take(30) {
        let tool_display: String = tool.chars().take(50).collect();
        println!(" {:<50} | {}", tool_display, count);
    }
    println!("-------------------------------------------------------------------");
}

fn show_ttps_summary(conn: &rusqlite::Connection) {
    let mut stmt = conn.prepare("SELECT ttps FROM groups WHERE ttps IS NOT NULL AND ttps != '[]'").unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().filter_map(|r| r.ok()).collect();

    let mut ttp_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for ttps_json in rows {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&ttps_json) {
            for tactic in arr {
                if let Some(techniques) = tactic.get("techniques").and_then(|v| v.as_array()) {
                    for tech in techniques {
                        let id = tech.get("technique_id").and_then(|v| v.as_str()).unwrap_or("-");
                        let name = tech.get("technique_name").and_then(|v| v.as_str()).unwrap_or("-");
                        let key = format!("{} {}", id, name);
                        *ttp_counts.entry(key).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let mut sorted: Vec<_> = ttp_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    println!("-------------------------------------------------------------------");
    println!(" TTP                                               | Groups");
    println!("-------------------------------------------------------------------");
    for (ttp, count) in sorted.iter().take(30) {
        let ttp_display: String = ttp.chars().take(50).collect();
        println!(" {:<50} | {}", ttp_display, count);
    }
    println!("-------------------------------------------------------------------");
}
