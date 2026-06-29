# TorSearch

.onionに散らばる、ランサムウェアグループの脅迫状や交渉チャット等、また被害者側の情報を収集・監視するツール

## Install

### 動作環境

- Linux (Ubuntu 22.04+, Kali Linux)
- macOS

### 必要なツール

- Rust (1.70+)
- Cargo

### インストール手順

```bash
# Rustのインストール（未インストールの場合）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# リポジトリのクローン
git clone https://github.com/xxx/TorSearch.git
cd TorSearch

# ビルド
cargo build --release

# 実行（初回はsyncでデータ取得）
./target/release/torsearch sync
```

## 使い方

### 初回＆都度データ更新

APIからデータを取得しDBに保存する。初回実行時はDBを自動作成。

```bash
$ torsearch sync
データベースを初期化中...
完了: data/torsearch.db

グループ情報を同期中...
  ransomware.live: 351 グループ取得
  ransomlook.io: 連絡先情報を補完中... 351/351
被害者情報を同期中...
  ransomware.live: 100 件取得
ランサムノートを取得中...
  ransomware.live: 339 ノート取得
同期完了: 351 グループ, 723 URL, 100 被害者, 339 ノート
```

**重要** コードが修正されてdb定義が変わった場合などは、 `torsearch.db` ファイルを削除して、再度、 `sync` しなおしてください。

### 運用

#### groups

- 最近のグループ活動一覧を表示。
  - 処理: DBから最終活動日順にグループ取得
  - 引数: `-N` 件数指定（デフォルト20）


```bash
$ torsearch groups -20

-------------------------------------------------------------------------------------------
 ID  | Group              | Victims | Last Activity | DLS | Notes | TOX | TG | JBR | PGP |
-------------------------------------------------------------------------------------------
 42  | lockbit            | 1234    | 2026-06-27    | 4   | 2     | -   | -  | -   | Y   |
 15  | blackcat           | 567     | 2026-06-26    | 2   | 1     | -   | -  | -   | -   |
-------------------------------------------------------------------------------------------
```

- 処理: グループ名で部分一致検索

```bash
$ torsearch groups lock

-------------------------------------------------------------------------------------------
 ID  | Group              | Victims | Last Activity | DLS | Notes | TOX | TG | JBR | PGP |
-------------------------------------------------------------------------------------------
 42  | lockbit            | 1234    | 2026-06-27    | 4   | 2     | -   | -  | -   | Y   |
 88  | lockbit3           | 89      | 2026-06-20    | 2   | 0     | -   | -  | -   | Y   |
-------------------------------------------------------------------------------------------
```

- 処理: ツール別にグループ数を集計

```bash
$ torsearch groups --by-tools

-------------------------------------------------------------------
 Tool                                              | Groups
-------------------------------------------------------------------
 Exfiltration: RClone                              | 45
 CredentialTheft: Mimikatz                         | 42
 LOLBAS: PsExec                                    | 38
-------------------------------------------------------------------
```

- 処理: TTPs別にグループ数を集計

```bash
$ torsearch groups --by-ttps

-------------------------------------------------------------------
 TTP                                               | Groups
-------------------------------------------------------------------
 T1486 Data Encrypted for Impact                   | 52
 T1490 Inhibit System Recovery                     | 48
 T1078 Valid Accounts                              | 45
-------------------------------------------------------------------
```

- グループの詳細を表示

```bash
$ torsearch groups {id}
```

```markdown
# akira

> ID: 38

## Description

Akira is a ransomware group first observed in March 2023...

## References
- https://www.sentinelone.com/labs/akira-ransomware-attacks-vpn-appliances
...
```

#### victims

- 被害者一覧を表示。

```bash
$ torsearch victims -50

-------------------------------------------------------------------
 ID   | Victim                    | Group          | Country | Date
-------------------------------------------------------------------
 101  | Aptora                    | dragonforce    | US      | 2026-06-27
 99   | Benchmark Industrial      | play           | US      | 2026-06-26
-------------------------------------------------------------------
```

- 被害者の詳細を表示。
  - 処理: DBから発見日順に被害者取得
  - 引数: `-N` 件数指定（デフォルト20）

```bash
$ torsearch victims {id}
```

```markdown
# Aptora

> ID: 101

## Basic Info

| Field | Value |
|-------|-------|
| Group | dragonforce |
| Country | US |
| Activity | Technology |
| Discovered | 2026-06-27 |

## Description

Aptora is a technology company...

## Links

| Type | URL |
|------|-----|
| Post URL | http://xxx.onion/... |
| Website | https://aptora.com |
```

- 被害国の一覧

```bash
$ torsearch victims --by-country

-------------------------------------
 Country    | Count | Last Discovered |
-------------------------------------
 US         | 1234  | 2026-06-29     |
 DE         | 456   | 2026-06-28     |
-------------------------------------
```

#### notes

- ランサムノート一覧を表示

```bash
$ torsearch notes -20

-------------------------------------------------
 ID  | Group              | URL                  |
-------------------------------------------------
 42  | lockbit3           | https://www.ranso... |
 15  | akira              | https://www.ranso... |
-------------------------------------------------
```

## 設計

- [API仕様](./design/api.md)
- [データベーススキーマ](./design/sqlite-ddl.sql)
- [帳票](./design/templates)
