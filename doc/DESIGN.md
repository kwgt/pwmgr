# 基本設計
## モジュール構成

```
  crate
   |
   +- cmd_args - コマンドライン引数処理モジュール
   |   |
   |   +- config - config.toml取り扱いモジュール
   |
   +- command - サブコマンド定義モジュール
   |   |
   |   +- matcher - Matcher列挙子定義を行うモジュール(共用モジュール)
   |   +- prompt - Promptトレイト定義を行うモジュール(共用モジュール)
   |   +- util - その他のユーティリティ定義モジュール(共用モジュール)
   |   |
   |   +- add - addサブコマンド定義モジュール
   |   +- edit - editサブコマンド定義モジュール
   |   +- export - exportサブコマンド定義モジュール
   |   +- import - importサブコマンド定義モジュール
   |   +- list - listサブコマンド定義モジュール
   |   +- remove - removeサブコマンド定義モジュール
   |   +- tags - tagsサブコマンド定義モジュール
   |   +- query - queryサブコマンド定義モジュール
   |   +- search - searchサブコマンド定義モジュール
   |
   +- database - データベース操作モジュール
       |
       +- types - データ型定義モジュール 

```

## データベース設計
データベースにはredbを用いる。データベース内には以下のテーブるを設ける。

| テーブル名 | キー | 値 |概要
|:---|:---
| entries | サービスID | エントリ情報 |サービスエントリを登録するテーブル
| tags | タグ文字列 | サービスID | タグとサービスIDの対応を保持するマルチマップテーブル |

### テーブル間の関係と整合性保持の指針

- `entries.tags` に記録されたタグと、`tags` テーブルの対応が常に一致するようにする。
- 追加/新規登録: `entries` への書き込みと同じトランザクションで、付与された各タグに対して `tags` に (tag, service_id) を追加する。
- 更新: 旧タグ集合と新タグ集合の差分を取り、削除されたタグは `tags` から対応を削除、新規タグは追加する。
- 削除: `entries` からエントリを削除するのと同じトランザクションで、そのエントリに紐づく全タグの対応を `tags` から削除する。
- 上記の整合更新は必ず1トランザクション内で完結させ、両テーブルに不整合が残らないようにする。


## 同期プロトコル
### 同期手順
TCPをベースとしてサーバとクライアントにロールを分ける。その上で以下の様に手順をまとめる。

 1. サーバ側で待ち受けを開始
 2. クライアントからサーバに接続
 3. サーバ側が持っているエントリ全てを一つずつクライアントに送信。クライアントは受信したエントリ毎に以下の評価を実施
   1. 受信したエントリのIDと同じエントリがクライアント側に無い場合はそのまま保存
   2. 受信したエントリのIDと同じエントリがクライアントにある場合は更新日時を比較
     1. 受信したクライアントの方が新しい場合はそのまま保存
     2. クライアント側が持っていたエントリの方が新しい場合は、受信したエントリを捨てそのIDを記録。
 4. サーバからエントリを送りきったらクライアントから以下のIDのエントリをサーバに送る。サーバはクライアントから送られたエントリを全て保存
   - 3.2.2で記録されたIDのエントリ
   - クライアントにしかなかったエントリ
 5. 終了

### パケット設計
Rustのenum/structをそのままシリアライズして用いる。`serde::{Serialize, Deserialize}`を派生し、バージョン交渉を`HELLO/HELLO_ACK`で行う。

```rust
#[derive(Serialize, Deserialize)]
pub enum SyncPacket {
    ///
    /// バージョン交渉と相手識別のための初回パケット
    ///
    Hello(Hello),

    ///
    /// Helloへの応答パケット（受け入れ可否を通知）
    ///
    HelloAck(HelloAck),

    ///
    /// サーバからクライアントへ送るエントリ本体
    ///
    ServerEntry(Entry),

    ///
    /// サーバ送信の終端と送信件数を示す
    ///
    ServerEntriesEnd(ServerEntriesEnd),

    ///
    /// クライアントからサーバへ送るエントリ本体
    ///
    ClientEntry(Entry),

    ///
    /// クライアント送信の終端と送信件数を示す
    ///
    ClientEntriesEnd(ClientEntriesEnd),
    ///
    /// エントリ適用の成否を送り返すACK
    ///
    EntryAck(EntryAck),

    ///
    /// 双方の同期完了を示す
    ///
    Finished,

    ///
    /// エラーやユーザ拒否による中断を示す
    ///
    Abort(Abort),
}

#[derive(Serialize, Deserialize)]
pub struct Hello {
    ///
    /// プロトコルバージョン（後方互換性確認用）
    ///
    pub protocol_version: u16,

    ///
    /// ノード識別子（ホストを一意に識別）
    ///
    pub node_id: String,

    ///
    /// ノードの役割（Server/Client）
    ///
    pub role: NodeRole,

    ///
    /// 相手との時計ずれ確認用の現在時刻（エポックミリ秒）
    ///
    pub now_epoch_ms: u64,
}

#[derive(Serialize, Deserialize)]
pub struct HelloAck {
    ///
    /// 合意したプロトコルバージョン
    ///
    pub protocol_version: u16,

    ///
    /// Helloを受理したか否か
    ///
    pub accepted: bool,

    ///
    /// 非受理時の理由
    ///
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum NodeRole {
    ///
    /// 待ち受け側（全件送信から開始）
    ///
    Server,

    ///
    /// 接続開始側（評価後に差分送信）
    ///
    Client,
}

#[derive(Serialize, Deserialize)]
pub struct ServerEntriesEnd {
    ///
    /// サーバが送信したエントリ件数
    ///
    pub total_sent: u64,
}

#[derive(Serialize, Deserialize)]
pub struct ClientEntriesEnd {
    ///
    /// クライアントが送信したエントリ件数
    ///
    pub total_sent: u64,
}

#[derive(Serialize, Deserialize)]
pub struct EntryAck {
    ///
    /// 対象エントリのID
    ///
    pub entry_id: String,

    ///
    /// 適用に成功したか否か
    ///
    pub accepted: bool,

    ///
    /// 拒否または失敗時の理由
    ///
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Abort {
    ///
    /// 中断理由メッセージ
    ///
    pub reason: String,
}
```

ここで`Entry`はデータベース格納形式と同じ構造体（`id`, `service`, `aliases`, `tags`, `properties`, `last_update`, `removed` を持ち、`last_update` に最終更新日時が格納される）を送受信する。

### パケット交換シーケンス
 1. クライアント→サーバ: `Hello`
 2. サーバ→クライアント: `HelloAck`（`accepted == false` の場合は接続終了）
 3. サーバ→クライアント: `ServerEntry` を全件送信、完了後に `ServerEntriesEnd`
 4. クライアント側で各エントリを評価
    - 既存に同一IDが無い場合: 受信を採用
    - 同一IDがあり `last_update` が新しい受信側を採用
    - `last_update` が同一で内容差分あり: サーバ側優先で採用するが、クライアントはユーザに確認プロンプトを表示し、拒否された場合は `Abort` を返してセッションを終了
    - クライアント側が新しい場合は受信エントリを捨て、そのIDを「送信候補」に記録
    - いずれの場合も適用または破棄の結果を `EntryAck` でサーバへ返す（適用失敗やユーザ拒否は `accepted == false` で理由を含める）
 5. クライアント→サーバ: 「送信候補」および「クライアントにのみ存在するエントリ」を `ClientEntry` (中身は `Entry`) として送信、完了後に `ClientEntriesEnd`
 6. サーバ側は受信した各エントリを適用し、その成否を `EntryAck` でクライアントへ返す（1トランザクションで`entries`と`tags`の整合性を保つ）
 7. 成功時はサーバ→クライアントへ `Finished` を送信。エラーやユーザ拒否時は `Abort` を送信し双方でセッションを終了する。

`ServerEntriesEnd` と `ClientEntriesEnd` の `total_sent` により、期待件数との差分チェックを行い、破損や途中切断を検出する。
