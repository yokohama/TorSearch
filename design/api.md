# API仕様

## ベースURL

| API | ベースURL | 認証 | レート制限 |
|-----|-----------|------|------------|
| ransomware.live v2 | `https://api.ransomware.live/v2` | 不要 | 1 req/min/endpoint |
| ransomware.live PRO | `https://api-pro.ransomware.live` | APIキー必要 | 無制限 (fair use) |
| ransomlook.io | `https://www.ransomlook.io/api` | 不要 | 緩い |

> **注意**: ransomware.live v1は非推奨 (Legacy compatibility only)。v2を使用すること。
> OpenCTIコネクタが2025年8月にAPI仕様変更で停止した事例あり。定期的な動作確認を推奨。

## sync処理

1. ransomware.live `/groups` → グループ名、locations、description、tools、ttps取得
2. ransomlook.io `/group/{name}` → 各グループのTOX、Telegram、PGP補完、profileをdescriptionに追記
3. ransomware.live `/recentvictims` → 被害者情報取得
4. ransomware.live `/ransomnotes` ページをスクレイピング → グループ別ノートURL取得
5. DBに統合保存（UPSERT）

### レート制限対策

- ransomware.live: 2エンドポイント使用、間に60秒待機
- ransomlook.io: 緩いが念のため100ms間隔

## エンドポイント比較

| 機能 | ransomware.live (v2) | ransomlook.io |
|------|----------------------|---------------|
| グループ一覧 | `/groups` | - (グループ単体取得のみ) |
| グループ詳細 | `/group/{name}` | `/group/{name}` |
| グループ別被害者 | `/groupvictims/{name}` | - |
| 最新被害者 | `/recentvictims` | `/posts?days=N` |
| 被害者検索 | `/searchvictims/{keyword}` | `/search?query=xxx` |
| 国別被害者 | `/countryvictims/{code}` | - |
| 業種別被害者 | `/sectorvictims/{sector}` | - |
| 年月別被害者 | `/victims/{year}/{month}` | - |
| サイバー攻撃 | `/recentcyberattacks` | - |
| 国別攻撃 | `/countrycyberattacks/{code}` | - |
| YARA | `/yara/{group}` | - |
| DBエクスポート | - | `/export/{db_num}` (要APIキー) |

## 取得可能データ比較

### 被害者情報 (victims)

| データ | ransomware.live | ransomlook.io | 備考 |
|--------|:---------------:|:-------------:|------|
| 被害者名 | `post_title` | `post_title` | |
| グループ名 | `group_name` | `group_name` | |
| 国情報 | `country` | - | ransomware.live独自 |
| 業種 | `activity` | - | ransomware.live独自 |
| 発見日時 | `discovered` | `discovered` | |
| 被害者説明 | `description` | `description` | 両方あり |
| 被害者ページURL | `post_url` (.onion) | `link` (相対パス) | ransomware.liveは直接.onion |
| スクリーンショット | `screenshot` (https) | `screen` (path) | ransomware.liveはCDN URL |
| 被害者サイト | `website` | - | ransomware.live独自 |
| Infostealer情報 | `infostealer{}` | - | 漏洩従業員数など |
| データサイズ | `data_size` (通常null) | - | ransomware.live独自 |
| 身代金額 | `ransom` (通常null) | - | |
| Magnetリンク | - | `magnet` (通常null) | |

### グループ情報 (groups)

| データ | ransomware.live | ransomlook.io | 備考 |
|--------|:---------------:|:-------------:|------|
| グループ名 | `name` | (配列のみ) | ransomlookはリストのみ |
| 説明 | `description` | - | ransomware.live独自 |
| 使用ツール | `tools[]` | - | ransomware.live独自 (JSON配列) |
| TTPs | `ttps[]` | - | ransomware.live独自 (JSON配列) |
| TOX ID | - | `tox` | ransomlook独自 |
| Telegram (連絡先) | - | `telegram` | ransomlook独自 |
| Jabber | - | `jabber` | ransomlook独自 |
| PGP鍵 | - | `pgp` | ransomlook独自 |
| プロファイル | - | `profile[]` | 関連記事URL → descriptionに統合 |

### サイトURL情報 (groups.locations[])

グループに紐づく.onionサイト情報。各タイプ複数存在しうる（ミラー、冗長化）

| タイプ | 説明 | ransomware.live | ransomlook.io |
|--------|------|:---------------:|:-------------:|
| DLS | Data Leak Site (被害者データ公開) | `type:"DLS"` | `chat:false` で推定 |
| Chat | 身代金交渉チャット | `type:"Chat"` | `chat:true` |
| Files | ファイル共有/ダウンロード | `type:"Files"` | - |
| Admin | 管理者パネル | `type:"Admin"` | - |
| API | APIエンドポイント | `type:"API"` | - |
| Telegram | Telegramチャンネル | `type:"Telegram"` | - |

各URLエントリのフィールド:

| フィールド | ransomware.live | ransomlook.io | 内容 |
|-----------|:---------------:|:-------------:|------|
| `slug` | o | o | 完全URL (`http://xxx.onion/path`) |
| `fqdn` | o | o | ドメインのみ (`xxx.onion`) |
| `title` | o | o | サイトタイトル |
| `available` | o | o | 稼働状況 |
| `updated` | o | o | 最終更新日時 (※DBスキーマ: `last_checked_at`) |
| `screen` | - | o | スクリーンショット (base64) |

## 精度向上ポイント

1. **被害者情報**: ransomware.liveから取得（国・業種・Webサイト・スクリーンショット）。ransomlook.ioの`/posts`にはdescriptionがないため統合不可。
2. **連絡先**: ransomlookのTOX/Telegram/PGP情報
3. **URL**: ransomware.liveの`post_url`で直接.onionリンク取得可能
4. **スクリーンショット**: ransomware.liveはCDN経由で取得容易
