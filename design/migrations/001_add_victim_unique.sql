-- Migration: Add UNIQUE constraint to victims table
-- 重複データを削除してからUNIQUE制約を追加

-- 1. 重複を除いた一時テーブル作成
CREATE TABLE victims_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL,
    post_title TEXT NOT NULL,
    country TEXT,
    activity TEXT,
    description TEXT,
    post_url TEXT,
    website TEXT,
    screenshot_url TEXT,
    data_size TEXT,
    ransom TEXT,
    discovered_at DATETIME,
    published_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE,
    UNIQUE(group_id, post_url)
);

-- 2. 重複を除いてデータ移行（同じgroup_id, post_urlは最新のみ）
INSERT INTO victims_new (
    group_id, post_title, country, activity, description,
    post_url, website, screenshot_url, data_size, ransom,
    discovered_at, published_at, created_at, updated_at
)
SELECT
    group_id, post_title, country, activity, description,
    post_url, website, screenshot_url, data_size, ransom,
    discovered_at, published_at, created_at, updated_at
FROM victims
WHERE id IN (
    SELECT MAX(id) FROM victims GROUP BY group_id, post_url
);

-- 3. 旧テーブル削除
DROP TABLE victims;

-- 4. リネーム
ALTER TABLE victims_new RENAME TO victims;

-- 5. インデックス再作成
CREATE INDEX idx_victims_group_id ON victims(group_id);
CREATE INDEX idx_victims_country ON victims(country);
CREATE INDEX idx_victims_discovered_at ON victims(discovered_at);
