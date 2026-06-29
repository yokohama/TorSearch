use std::fs;
use std::path::Path;

mod db;
mod sync;

const DB_PATH: &str = "data/torsearch.db";
const SCHEMA_PATH: &str = "design/sqlite-ddl.sql";
const TEMPLATE_GROUP_DETAIL: &str = "design/templates/group_detail.md";
const TEMPLATE_VICTIM_DETAIL: &str = "design/templates/victim_detail.md";

fn print_table(columns: &[(&str, usize)], rows: &[Vec<String>]) {
    let total_width: usize = columns.iter().map(|(_, w)| w + 3).sum::<usize>() + 1;
    let separator = "-".repeat(total_width);

    println!("{}", separator);

    let header: String = columns.iter()
        .map(|(name, width)| format!(" {:<width$} |", name, width = width))
        .collect();
    println!("{}", header);

    println!("{}", separator);

    for row in rows {
        let line: String = row.iter()
            .zip(columns.iter())
            .map(|(val, (_, width))| {
                let display = truncate(val, *width);
                format!(" {:<width$} |", display, width = width)
            })
            .collect();
        println!("{}", line);
    }

    println!("{}", separator);
}

fn truncate(s: &str, max_width: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s.chars().take(max_width).collect()
    } else {
        let truncated: String = s.chars().take(max_width - 3).collect();
        format!("{}...", truncated)
    }
}

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
        "notes" => cmd_notes(&args[2..]),
        _ => print_usage(),
    }
}

fn print_usage() {
    println!("Usage: torsearch <command> [options]");
    println!();
    println!("Commands:");
    println!("  sync                  APIからデータを同期 (初回はDB自動作成)");
    println!("  groups [-N] [id]      グループ一覧/詳細/検索");
    println!("  groups --by-tools     ツール別集計");
    println!("  groups --by-ttps      TTPs別集計");
    println!("  victims [-N] [id]     被害者一覧/詳細");
    println!("  victims --by-country  国別集計");
    println!("  notes [-N]            ランサムノート一覧");
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

    let rows = match db::list(&conn, &pattern, limit) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let table_rows: Vec<Vec<String>> = rows.iter().map(|row| {
        vec![
            row.id.to_string(),
            row.name.clone(),
            row.victim_count.to_string(),
            row.last_activity.clone(),
            row.dls_count.to_string(),
            row.note_count.to_string(),
            if row.has_tox { "Y" } else { "-" }.to_string(),
            if row.has_telegram { "Y" } else { "-" }.to_string(),
            if row.has_jabber { "Y" } else { "-" }.to_string(),
            if row.has_pgp { "Y" } else { "-" }.to_string(),
        ]
    }).collect();

    print_table(
        &[("ID", 3), ("Group", 18), ("Victims", 7), ("Last Activity", 13), ("DLS", 3), ("Notes", 5), ("TOX", 3), ("TG", 2), ("JBR", 3), ("PGP", 3)],
        &table_rows,
    );
}

fn show_group_detail(conn: &rusqlite::Connection, group_id: usize) {
    let template = match fs::read_to_string(TEMPLATE_GROUP_DETAIL) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("テンプレート読み込みエラー: {}", e);
            return;
        }
    };

    let group = match db::get_by_id(&conn, group_id) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("グループID {} が見つかりません: {}", group_id, e);
            return;
        }
    };

    // Description
    let description_str = group.description.as_deref().unwrap_or("-").to_string();

    // Tools
    let tools_str = match group.tools.as_ref().and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(t).ok()) {
        Some(arr) if !arr.is_empty() => {
            let mut lines = Vec::new();
            for obj in arr {
                let Some(map) = obj.as_object() else { continue };
                for (category, items) in map {
                    let Some(arr) = items.as_array() else { continue };
                    let tools: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                    lines.push(format!("- **{}**: {}", category, tools.join(", ")));
                }
            }
            if lines.is_empty() { "-".to_string() } else { lines.join("\n") }
        }
        _ => "-".to_string(),
    };

    // TTPs
    let ttps_str = match group.ttps.as_ref().and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(t).ok()) {
        Some(arr) if !arr.is_empty() => {
            let mut lines = Vec::new();
            for tactic in arr {
                let tactic_name = tactic.get("tactic_name").and_then(|v| v.as_str()).unwrap_or("-");
                let tactic_id = tactic.get("tactic_id").and_then(|v| v.as_str()).unwrap_or("-");
                lines.push(format!("- **{} ({})**: ", tactic_name, tactic_id));
                let Some(techniques) = tactic.get("techniques").and_then(|v| v.as_array()) else { continue };
                let tech_names: Vec<&str> = techniques.iter()
                    .filter_map(|t| t.get("technique_name").and_then(|v| v.as_str()))
                    .collect();
                lines.push(format!("  {}", tech_names.join(", ")));
            }
            if lines.is_empty() { "-".to_string() } else { lines.join("\n") }
        }
        _ => "-".to_string(),
    };

    // Sites
    let sites = db::list_locations_by_group(&conn, group_id).unwrap_or_default();
    let sites_str = if !sites.is_empty() {
        sites.iter().map(|s| {
            let status = if s.available { "UP" } else { "DOWN" };
            let title_str = s.title.as_deref().unwrap_or("-");
            let checked_str = s.last_checked_at.as_ref().map(|c| c[..10.min(c.len())].to_string()).unwrap_or("-".to_string());
            format!("| {} | {} | {} | `{}` | {} |", s.loc_type, status, title_str, s.slug, checked_str)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - | - | - |".to_string()
    };

    // Ransom Notes
    let notes = db::list_ransom_notes_by_group(&conn, group_id).unwrap_or_default();
    let notes_str = if !notes.is_empty() {
        notes.iter().map(|n| {
            format!("| {} | {} | {} |", n.filename, n.file_type, n.url)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - |".to_string()
    };

    // Victims
    let victims = db::list_victims_by_group(&conn, group_id, 5).unwrap_or_default();
    let victims_str = if !victims.is_empty() {
        victims.iter().map(|v| {
            let c = v.country.as_deref().unwrap_or("-");
            let d = v.discovered_at.as_deref().unwrap_or("-");
            format!("| {} | {} | {} |", d, c, v.name)
        }).collect::<Vec<_>>().join("\n")
    } else {
        "| - | - | - |".to_string()
    };

    // Replace placeholders
    let output = template
        .replace("{{name}}", &group.name)
        .replace("{{id}}", &group_id.to_string())
        .replace("{{tox}}", group.tox.as_deref().unwrap_or("-"))
        .replace("{{telegram}}", group.telegram.as_deref().unwrap_or("-"))
        .replace("{{jabber}}", group.jabber.as_deref().unwrap_or("-"))
        .replace("{{pgp}}", &group.pgp.as_ref().map(|p| format!("{}...", p.chars().take(50).collect::<String>())).unwrap_or("-".to_string()))
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
        let rows = match db::count_by_country(&conn) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}", e);
                return;
            }
        };

        let table_rows: Vec<Vec<String>> = rows.iter().map(|row| {
            vec![
                row.country.clone(),
                row.count.to_string(),
                row.last_discovered.clone(),
            ]
        }).collect();

        print_table(
            &[("Country", 10), ("Count", 5), ("Last Discovered", 10)],
            &table_rows,
        );
        return;
    }

    let (limit, id, _) = parse_args(args, 20);

    if let Some(victim_id) = id {
        show_victim_detail(&conn, victim_id);
        return;
    }

    let rows = match db::list_victims(&conn, limit) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let table_rows: Vec<Vec<String>> = rows.iter().map(|row| {
        vec![
            row.id.to_string(),
            row.name.clone(),
            row.group_name.clone(),
            row.country.clone(),
            row.discovered_at.clone(),
        ]
    }).collect();

    print_table(
        &[("ID", 4), ("Victim", 25), ("Group", 14), ("Country", 7), ("Date", 10)],
        &table_rows,
    );
}

fn show_victim_detail(conn: &rusqlite::Connection, victim_id: usize) {
    let template = match fs::read_to_string(TEMPLATE_VICTIM_DETAIL) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("テンプレート読み込みエラー: {}", e);
            return;
        }
    };

    let victim = match db::get_victim_by_id(&conn, victim_id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("被害者ID {} が見つかりません: {}", victim_id, e);
            return;
        }
    };

    let output = template
        .replace("{{name}}", &victim.name)
        .replace("{{id}}", &victim_id.to_string())
        .replace("{{group}}", &victim.group_name)
        .replace("{{country}}", victim.country.as_deref().unwrap_or("-"))
        .replace("{{activity}}", victim.activity.as_deref().unwrap_or("-"))
        .replace("{{discovered}}", victim.discovered_at.as_deref().unwrap_or("-"))
        .replace("{{description}}", victim.description.as_deref().unwrap_or("-"))
        .replace("{{post_url}}", victim.post_url.as_deref().unwrap_or("-"))
        .replace("{{website}}", victim.website.as_deref().unwrap_or("-"));

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
    let rows = match db::get_tools_json(conn) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut tool_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for tools_json in rows {
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&tools_json) else { continue };

        for obj in arr {
            let Some(map) = obj.as_object() else { continue };
            for (category, items) in map {
                let Some(tools) = items.as_array() else { continue };
                for tool in tools {
                    let Some(name) = tool.as_str() else { continue };
                    let key = format!("{}: {}", category, name);
                    *tool_counts.entry(key).or_insert(0) += 1;
                }
            }
        }
    }

    let mut sorted: Vec<_> = tool_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let table_rows: Vec<Vec<String>> = sorted.iter()
        .take(30)
        .map(|(tool, count)| vec![tool.clone(), count.to_string()])
        .collect();

    print_table(
        &[("Tool", 50), ("Groups", 6)],
        &table_rows,
    );
}

fn show_ttps_summary(conn: &rusqlite::Connection) {
    let rows = match db::get_ttps_json(conn) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut ttp_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for ttps_json in rows {
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&ttps_json) else { continue };

        for tactic in arr {
            let Some(techniques) = tactic.get("techniques").and_then(|v| v.as_array()) else { continue };
            for tech in techniques {
                let id = tech.get("technique_id").and_then(|v| v.as_str()).unwrap_or("-");
                let name = tech.get("technique_name").and_then(|v| v.as_str()).unwrap_or("-");
                let key = format!("{} {}", id, name);
                *ttp_counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    let mut sorted: Vec<_> = ttp_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let table_rows: Vec<Vec<String>> = sorted.iter()
        .take(30)
        .map(|(ttp, count)| vec![ttp.clone(), count.to_string()])
        .collect();

    print_table(
        &[("TTP", 50), ("Groups", 6)],
        &table_rows,
    );
}

fn cmd_notes(args: &[String]) {
    let conn = match rusqlite::Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("DB接続エラー: {}", e);
            return;
        }
    };

    let (limit, _, _) = parse_args(args, 50);

    let rows = match db::list_ransom_notes(&conn, limit) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let table_rows: Vec<Vec<String>> = rows.iter().map(|row| {
        vec![
            row.group_id.to_string(),
            row.group_name.clone(),
            row.url.clone(),
        ]
    }).collect();

    print_table(
        &[("ID", 3), ("Group", 18), ("URL", 80)],
        &table_rows,
    );
}
