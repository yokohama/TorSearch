# TorSearch

ランサムウェアグループの.onionサイトと被害者情報を収集・監視するツール

## 使い方

### init

DBを初期化する。

```bash
$ torsearch init
データベースを初期化中...
完了: data/torsearch.db
```

- 処理: `design/sqlite-ddl.sql` を読み込んで `data/torsearch.db` に適用
- 冪等性: 既存DBがあればスキップ

### sync

APIからデータを取得しDBに保存する。

```bash
$ torsearch sync
グループ情報を同期中...
  ransomware.live: 351 グループ取得
  ransomlook.io: 連絡先情報を補完中... 351/351
被害者情報を同期中...
  ransomware.live: 100 件取得
同期完了: 351 グループ, 723 URL, 100 被害者
```

- 処理:
  1. ransomware.live `/groups` → グループ名、locations取得
  2. ransomlook.io `/group/{name}` → 各グループのTOX、Telegram、PGP、profile補完
  3. ransomware.live `/recentvictims` → 被害者情報取得
  4. ransomware.live `/ransomnotes` ページをスクレイピング → グループ別ノートURL取得
  5. DBに統合保存（UPSERT）
- レート制限対策:
  - ransomware.live: 2エンドポイント使用、間に60秒待機
  - ransomlook.io: 緩いが念のため100ms間隔

### 運用

DBからデータを参照する。結果表示はsearchsploit風。

#### groups

最近のグループ活動一覧を表示。

```bash
$ torsearch groups -20
-------------------------------------------------------------------
 ID  | Group              | Victims | Last Activity | DLS Sites
-------------------------------------------------------------------
 42  | lockbit            | 1234    | 2026-06-27    | 4
 15  | blackcat           | 567     | 2026-06-26    | 2
-------------------------------------------------------------------
```

- 処理: DBから最終活動日順にグループ取得
- 引数: `-N` 件数指定（デフォルト20）

```bash
$ torsearch groups lock
-------------------------------------------------------------------
 ID  | Group              | Victims | Last Activity | DLS Sites
-------------------------------------------------------------------
 42  | lockbit            | 1234    | 2026-06-27    | 4
 88  | lockbit3           | 89      | 2026-06-20    | 2
-------------------------------------------------------------------
```

- 処理: グループ名で部分一致検索

```bash
$ torsearch groups {id}
詳細表示。ＴＯＤＯ項目
```

- 処理: 指定されたidで詳細表示

#### victims

被害者一覧を表示。

```bash
$ torsearch victims -50
-------------------------------------------------------------------
 ID   | Victim                    | Group          | Country | Date
-------------------------------------------------------------------
 101  | Aptora                    | dragonforce    | US      | 2026-06-27
 99   | Benchmark Industrial      | play           | US      | 2026-06-26
-------------------------------------------------------------------
```

```bash
$ torsearch victims -50 {id}
詳細表示。ＴＯＤＯ項目
```

- 処理: 指定されたidで詳細表示

- 処理: DBから発見日順に被害者取得
- 引数: `-N` 件数指定（デフォルト20）
- オプション: `--by-country` 国別集計

```bash
$ torsearch victims --by-country
-------------------------------------------------------------------
 Country | Count
-------------------------------------------------------------------
 US      | 1234
 DE      | 456
-------------------------------------------------------------------
```


---

## API仕様

### ベースURL

| API | ベースURL | 認証 | レート制限 |
|-----|-----------|------|------------|
| ransomware.live v2 | `https://api.ransomware.live/v2` | 不要 | 1 req/min/endpoint |
| ransomware.live PRO | `https://api-pro.ransomware.live` | APIキー必要 | 無制限 (fair use) |
| ransomlook.io | `https://www.ransomlook.io/api` | 不要 | 緩い |

> **注意**: ransomware.live v1は非推奨 (Legacy compatibility only)。v2を使用すること。
> OpenCTIコネクタが2025年8月にAPI仕様変更で停止した事例あり。定期的な動作確認を推奨。

### エンドポイント比較

| 機能 | ransomware.live (v2) | ransomlook.io |
|------|----------------------|---------------|
| グループ一覧 | `/groups` | ❌ (グループ単体取得のみ) |
| グループ詳細 | `/group/{name}` | `/group/{name}` |
| グループ別被害者 | `/groupvictims/{name}` | ❌ |
| 最新被害者 | `/recentvictims` | `/posts?days=N` |
| 被害者検索 | `/searchvictims/{keyword}` | `/search?query=xxx` |
| 国別被害者 | `/countryvictims/{code}` | ❌ |
| 業種別被害者 | `/sectorvictims/{sector}` | ❌ |
| 年月別被害者 | `/victims/{year}/{month}` | ❌ |
| サイバー攻撃 | `/recentcyberattacks` | ❌ |
| 国別攻撃 | `/countrycyberattacks/{code}` | ❌ |
| YARA | `/yara/{group}` | ❌ |
| DBエクスポート | ❌ | `/export/{db_num}` (要APIキー) |

### 取得可能データ比較

#### 被害者情報 (victims)

| データ | ransomware.live | ransomlook.io | 備考 |
|--------|:---------------:|:-------------:|------|
| 被害者名 | ✅ `post_title` | ✅ `post_title` | |
| グループ名 | ✅ `group_name` | ✅ `group_name` | |
| **国情報** | ✅ `country` | ❌ | ransomware.live独自 |
| **業種** | ✅ `activity` | ❌ | ransomware.live独自 |
| 発見日時 | ✅ `discovered` | ✅ `discovered` | |
| **被害者説明** | ✅ `description` | ✅ `description` | 両方あり |
| **被害者ページURL** | ✅ `post_url` (.onion) | ✅ `link` (相対パス) | ransomware.liveは直接.onion |
| **スクリーンショット** | ✅ `screenshot` (https) | ✅ `screen` (path) | ransomware.liveはCDN URL |
| **被害者サイト** | ✅ `website` | ❌ | ransomware.live独自 |
| **Infostealer情報** | ✅ `infostealer{}` | ❌ | 漏洩従業員数など |
| **データサイズ** | ⚠️ `data_size` (通常null) | ❌ | ransomware.live独自 |
| 身代金額 | ⚠️ `ransom` (通常null) | ❌ | |
| Magnetリンク | ❌ | ⚠️ `magnet` (通常null) | |

#### グループ情報 (groups)

| データ | ransomware.live | ransomlook.io | 備考 |
|--------|:---------------:|:-------------:|------|
| グループ名 | ✅ `name` | ✅ (配列のみ) | ransomlookはリストのみ |
| **TOX ID** | ❌ | ✅ `tox` | ransomlook独自 |
| **Telegram (連絡先)** | ❌ | ✅ `telegram` | ransomlook独自 |
| **Jabber** | ❌ | ✅ `jabber` | ransomlook独自 |
| **PGP鍵** | ❌ | ✅ `pgp` | ransomlook独自 |
| **プロファイル** | ❌ | ✅ `profile[]` | 関連記事URL |

#### サイトURL情報 (groups.locations[])

グループに紐づく.onionサイト情報。**各タイプ複数存在しうる**（ミラー、冗長化）

| タイプ | 説明 | ransomware.live | ransomlook.io |
|--------|------|:---------------:|:-------------:|
| **DLS** | Data Leak Site (被害者データ公開) | ✅ `type:"DLS"` | ⚠️ `chat:false` で推定 |
| **Chat** | 身代金交渉チャット | ✅ `type:"Chat"` | ✅ `chat:true` |
| **Files** | ファイル共有/ダウンロード | ✅ `type:"Files"` | ❌ |
| **Admin** | 管理者パネル | ✅ `type:"Admin"` | ❌ |
| **API** | APIエンドポイント | ✅ `type:"API"` | ❌ |
| **Telegram** | Telegramチャンネル | ✅ `type:"Telegram"` | ❌ |

各URLエントリのフィールド:

| フィールド | ransomware.live | ransomlook.io | 内容 |
|-----------|:---------------:|:-------------:|------|
| `slug` | ✅ | ✅ | 完全URL (`http://xxx.onion/path`) |
| `fqdn` | ✅ | ✅ | ドメインのみ (`xxx.onion`) |
| `title` | ✅ | ✅ | サイトタイトル |
| `available` | ✅ | ✅ | 稼働状況 |
| `updated` | ✅ | ✅ | 最終更新日時 (※DBスキーマ: `last_checked_at`) |
| `screen` | ❌ | ✅ | スクリーンショット (base64) |

### 精度向上ポイント

1. **被害者情報**: ransomware.liveから取得（国・業種・Webサイト・スクリーンショット）。ransomlook.ioの`/posts`にはdescriptionがないため統合不可。
2. **連絡先**: ransomlookのTOX/Telegram/PGP情報
3. **URL**: ransomware.liveの`post_url`で直接.onionリンク取得可能
4. **スクリーンショット**: ransomware.liveはCDN経由で取得容易

---

## データベーススキーマ

詳細は `sqlite.ddl` 参照

```
groups (グループ)
├── id, name
├── tox_id, telegram, jabber, pgp  -- 連絡先 (ransomlook)
├── profile                         -- 関連記事URL (JSON)
│
├─< group_locations (サイトURL) 1:N
│   ├── type            -- DLS, Chat, Files, Admin, API, Telegram
│   ├── slug            -- 完全URL
│   ├── fqdn            -- ドメインのみ
│   ├── title           -- ページタイトル
│   ├── available       -- 稼働状況
│   └── last_checked_at -- 最終確認日時 (API: updated)
│
├─< ransom_notes (ランサムノート) 1:N
│   ├── filename        -- RESTORE-MY-FILES.txt等
│   ├── file_type       -- txt, html, hta
│   └── url             -- ダウンロードURL
│
└─< victims (被害者) 1:N
    ├── post_title      -- 被害者名
    ├── country         -- 国コード
    ├── activity        -- 業種
    ├── description     -- 説明
    ├── post_url        -- 被害者ページ.onion URL
    ├── website         -- 被害者の通常サイト
    ├── screenshot_url  -- スクリーンショット
    ├── data_size       -- 漏洩データサイズ
    ├── ransom          -- 身代金額
    ├── discovered_at   -- 発見日時 (ransomware.live: discovered)
    └── published_at    -- 公開日時 (ransomlook: published)
```

---
