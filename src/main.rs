use std::fs;
use std::path::Path;

mod sync;

const DB_PATH: &str = "data/torsearch.db";
const SCHEMA_PATH: &str = "design/sqlite-ddl.sql";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "init" => cmd_init(),
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
    println!("  init              DBを初期化");
    println!("  sync              APIからデータを同期");
    println!("  groups [-N] [id]  グループ一覧/詳細");
    println!("  victims [-N] [id] 被害者一覧/詳細");
    println!("  victims --by-country  国別集計");
}

fn cmd_init() {
    if Path::new(DB_PATH).exists() {
        println!("データベースは既に存在します: {}", DB_PATH);
        return;
    }

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
}

fn cmd_sync() {
    if !Path::new(DB_PATH).exists() {
        eprintln!("エラー: データベースが存在しません。先に init を実行してください。");
        return;
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
    let result: Result<(String, Option<String>, Option<String>, Option<String>, Option<String>), _> = conn.query_row(
        "SELECT name, tox_id, telegram, jabber, pgp FROM groups WHERE id = ?1",
        [group_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    );

    let (name, tox, telegram, jabber, pgp) = match result {
        Ok(r) => r,
        Err(_) => {
            eprintln!("グループID {} が見つかりません", group_id);
            return;
        }
    };

    println!("=== Group: {} (ID: {}) ===", name, group_id);
    println!();

    if tox.is_some() || telegram.is_some() || jabber.is_some() || pgp.is_some() {
        println!("連絡先:");
        if let Some(t) = tox { println!("  TOX: {}", t); }
        if let Some(t) = telegram { println!("  Telegram: {}", t); }
        if let Some(j) = jabber { println!("  Jabber: {}", j); }
        if let Some(p) = pgp { println!("  PGP: {}...", p.chars().take(50).collect::<String>()); }
        println!();
    }

    // DLSサイト
    let mut stmt = conn.prepare(
        "SELECT slug, available FROM group_locations WHERE group_id = ?1 AND type = 'DLS'"
    ).unwrap();
    let sites: Vec<(String, bool)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    if !sites.is_empty() {
        println!("DLS Sites:");
        for (url, available) in sites {
            let status = if available { "UP" } else { "DOWN" };
            println!("  [{}] {}", status, url);
        }
        println!();
    }

    // ランサムノート
    let mut stmt = conn.prepare(
        "SELECT filename, url FROM ransom_notes WHERE group_id = ?1"
    ).unwrap();
    let notes: Vec<(String, String)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    if !notes.is_empty() {
        println!("Ransom Notes:");
        for (filename, _url) in notes {
            println!("  - {}", filename);
        }
        println!();
    }

    // 最近の被害者
    let mut stmt = conn.prepare(
        "SELECT post_title, country, substr(discovered_at, 1, 10) FROM victims WHERE group_id = ?1 ORDER BY discovered_at DESC LIMIT 5"
    ).unwrap();
    let victims: Vec<(String, Option<String>, Option<String>)> = stmt.query_map([group_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();

    if !victims.is_empty() {
        println!("Recent Victims (max 5):");
        for (name, country, date) in victims {
            let c = country.unwrap_or_else(|| "-".to_string());
            let d = date.unwrap_or_else(|| "-".to_string());
            println!("  {} [{}] {}", d, c, name);
        }
    }
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

    println!("=== Victim: {} (ID: {}) ===", name, victim_id);
    println!();
    println!("Group: {}", group);
    if let Some(c) = country { println!("Country: {}", c); }
    if let Some(a) = activity { println!("Activity: {}", a); }
    if let Some(d) = discovered { println!("Discovered: {}", d); }
    println!();

    if let Some(desc) = description {
        if !desc.is_empty() {
            println!("Description:");
            println!("  {}", desc.chars().take(200).collect::<String>());
            println!();
        }
    }

    if let Some(url) = post_url {
        println!("Post URL: {}", url);
    }
    if let Some(site) = website {
        println!("Website: {}", site);
    }
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
