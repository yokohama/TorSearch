-- TorSearch Database Schema
-- ランサムウェアグループ・被害者情報管理

-- グループ情報
CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    description TEXT,  -- グループ説明 (ransomware.live) + 関連記事URL (ransomlook profile)
    tools TEXT,        -- 使用ツール (JSON配列)
    ttps TEXT,         -- TTPs (JSON配列)
    tox_id TEXT,
    telegram TEXT,
    jabber TEXT,
    pgp TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- グループに紐づくサイトURL情報（1グループ:複数URL）
CREATE TABLE group_locations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL,
    type TEXT NOT NULL,  -- DLS, Chat, Files, Admin, API, Telegram
    slug TEXT NOT NULL,  -- 完全URL (http://xxx.onion/path)
    fqdn TEXT NOT NULL,  -- ドメインのみ (xxx.onion)
    title TEXT,          -- ページタイトル
    available BOOLEAN DEFAULT 0,
    last_checked_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE,
    UNIQUE(group_id, slug)
);

-- 被害者情報
CREATE TABLE victims (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL,
    post_title TEXT NOT NULL,        -- 被害者名
    country TEXT,                    -- 国コード (US, JP, etc.)
    activity TEXT,                   -- 業種
    description TEXT,
    post_url TEXT,                   -- 被害者ページ.onion URL
    website TEXT,                    -- 被害者の通常サイト
    screenshot_url TEXT,             -- スクリーンショットCDN URL
    data_size TEXT,                  -- 漏洩データサイズ
    ransom TEXT,                     -- 身代金額
    discovered_at DATETIME,
    published_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
);

-- ランサムノート
CREATE TABLE ransom_notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL,
    filename TEXT NOT NULL,          -- RESTORE-MY-FILES.txt等
    file_type TEXT NOT NULL,         -- txt, html, hta
    url TEXT NOT NULL,               -- ダウンロードURL
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE,
    UNIQUE(group_id, filename)
);

-- インデックス
CREATE INDEX idx_ransom_notes_group_id ON ransom_notes(group_id);
CREATE INDEX idx_group_locations_group_id ON group_locations(group_id);
CREATE INDEX idx_group_locations_type ON group_locations(type);
CREATE INDEX idx_group_locations_available ON group_locations(available);
CREATE INDEX idx_victims_group_id ON victims(group_id);
CREATE INDEX idx_victims_country ON victims(country);
CREATE INDEX idx_victims_discovered_at ON victims(discovered_at);

-- ビュー: アクティブなDLSサイト一覧
CREATE VIEW active_dls_sites AS
SELECT
    g.name AS group_name,
    gl.slug,
    gl.fqdn,
    gl.title,
    gl.last_checked_at
FROM group_locations gl
JOIN groups g ON gl.group_id = g.id
WHERE gl.type = 'DLS' AND gl.available = 1;

-- ビュー: 被害者一覧（日付フィルタはクエリ側で指定）
CREATE VIEW victims_with_group AS
SELECT
    v.id,
    v.post_title,
    g.name AS group_name,
    v.country,
    v.activity,
    v.post_url,
    v.website,
    v.discovered_at
FROM victims v
JOIN groups g ON v.group_id = g.id;

-- 使用例: SELECT * FROM victims_with_group WHERE discovered_at >= datetime('now', '-7 days');
