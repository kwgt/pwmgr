# pwmgr

サービスごとのユーザー名やパスワードを管理するCLIアプリケーションです。
エントリはredbデータベースに保存し、外部エディタでYAMLを編集して登録・更新します。
サービス名・別名・タグ・プロパティによる検索、YAML形式のバックアップ、
他ホストとのデータベース同期に対応しています。

## ビルド方法

Rust / Cargoと、利用するOS向けのリンカなどのビルド環境が必要です。
RustはRust 2024 Editionおよび依存クレートの要求バージョンに対応するものを
使用してください。

リポジトリのルートディレクトリで次のコマンドを実行します。

```sh
cargo build --release --locked
```

通常、実行ファイルは次の場所に生成されます。
Cargoの出力先を変更している場合は、その設定に従います。

| OS | 実行ファイル |
|:---|:---|
| Windows | `target/release/pwmgr.exe` |
| Linux / macOS | `target/release/pwmgr` |

以下の例では、実行ファイルを`PATH`の通ったディレクトリに配置し、
`pwmgr`で実行できるものとします。
配置せずに試す場合は、実行ファイルのパスを直接指定するか、
`cargo run --release --locked --`に続けて引数を渡してください。

```sh
cargo run --release --locked -- --help
cargo run --release --locked -- list
```

開発時のチェックとテストは次のコマンドで実行できます。

```sh
cargo check --locked
cargo test --locked
```

## 基本的な使い方

```sh
pwmgr [OPTIONS] <COMMAND> [COMMAND-OPTIONS]
```

グローバルオプションはサブコマンドの前に指定します。
全体のヘルプは`pwmgr --help`、個別のヘルプは
`pwmgr <COMMAND> --help`で確認できます。

まず、設定内容を確認し、必要に応じて設定ファイルを生成します。
設定ファイルがなくても既定値で実行できます。

```sh
pwmgr --show-options
pwmgr --save-default
```

エントリを追加するとエディタが開きます。内容を保存してエディタを終了すると、
入力内容の検証と登録が行われます。
既定のエディタはWindowsでは`notepad`、Linux / macOSでは`nano`です。

```sh
pwmgr add "Example Service"
pwmgr list
pwmgr query "Example Service"
pwmgr query --masked-mode "Example Service"
```

編集画面では、生成済みの`id`を変更せず、例えば次の項目を入力します。
`properties`には少なくとも1つの項目が必要です。

```yaml
service: "Example Service"
aliases: ["Old Service Name"]
tags: ["personal", "web"]
properties:
  username: "user@example.com"
  password!: "example-password"
```

プロパティ名の末尾に`!`を付けると秘匿項目になります。
`query`は既定では秘匿項目の値も表示します。
`--masked-mode`を指定するか、設定ファイルで`query.masked_mode = true`を
設定すると、秘匿項目の値を`***`に置き換えて表示します。

## グローバルオプション

| オプション | 内容 |
|:---|:---|
| `-c, --config PATH` | 読み込むTOML設定ファイルを指定 |
| `-d, --db-path PATH` | データベースファイルのパスを指定 |
| `-e, --editor EDITOR` | 編集に使用するエディタの実行ファイル名またはパスを指定 |
| `-l, --log-level LEVEL` | ログレベルを指定。既定は`INFO` |
| `-L, --log-output PATH` | ログの出力先ファイルまたはディレクトリを指定。`-`で標準出力 |
| `--log-tee` | ログを標準出力にも出力 |
| `--json-output` | `query` / `tags`の出力をJSONに変更 |
| `--show-options` | 適用後の設定内容を表示して終了 |
| `--save-default` | 既定の設定を設定ファイルへ保存して終了。既存ファイルは上書きを確認 |
| `-h, --help` | ヘルプを表示 |
| `-V, --version` | バージョンを表示 |

ログレベルは`off`（記録なし）、`error`、`warn`、`info`、`debug`、`trace`を
指定できます。CLIでは大文字・小文字を区別せず、記録なしは`NONE`でも指定できます。
ログ出力先がディレクトリの場合は、1本あたり2 MiBでローテーションし、
ローテーション済みのログを最大10本保持します。

```sh
pwmgr --db-path ./database.redb list
pwmgr --editor vim add "Example Service"
pwmgr --json-output query "Example Service"
pwmgr --show-options query "Example Service"
```

## サブコマンド

`ID`には`list`や`search`に表示されるエントリID（ULID）を指定します。

| コマンド | 内容 | 主なオプション |
|:---|:---|:---|
| `add [SERVICE-NAME]` | YAMLテンプレートを編集して追加。サービス名を事前入力可能 | — |
| `edit ID` | 指定IDのエントリをエディタで編集 | — |
| `query KEY` | ID・サービス名・別名で検索し、エントリの内容を表示 | `-f, --full`、`-m, --match-mode MODE`、`-M, --masked-mode`、`-U, --unmasked-mode` |
| `search KEY` | 条件に一致するエントリのIDとサービス名を表示 | `-s, --service`、`-t, --tag TAG`、`-p, --property NAME`、`-m, --match-mode MODE`、`--sort-by MODE`、`-r, --reverse-sort` |
| `list` | エントリのIDとサービス名を一覧表示 | `-t, --tag TAG`、`--tag-and`、`--sort-by MODE`、`-r, --reverse-sort`、`--with-removed` |
| `tags [KEY]` | タグを一覧表示。KEYで絞り込み可能 | `-n, --number`、`-m, --match-mode MODE`、`--sort-by MODE`、`-r, --reverse-sort` |
| `remove ID` | 指定IDのエントリを削除 | `--hard` |
| `export` | バックアップ用YAMLを出力 | `-o, --output PATH` |
| `import [INPUT]` | バックアップ用YAMLを取り込み | `-m, --merge`、`-O, --overwrite`、`--dry-run` |
| `sync` | 他ホストのデータベースと同期 | `-s, --server [ADDR[:PORT]]`または`-c, --client ADDR[:PORT]` |

### 検索・一覧表示

マッチモードは`exact`（大文字・小文字を区別しない完全一致）、
`contains`（大文字・小文字を区別しない部分一致）、`regex`（正規表現）、
`fuzzy`（あいまい一致）です。
現在の実装では、`query` / `search` / `tags`の既定値は`contains`です。

`query --full`は別名やタグ、更新日時なども含めて表示します。
`--masked-mode`と`--unmasked-mode`は同時に指定できません。

`search`は通常、サービス名と別名を検索します。
`--property`で検索対象のプロパティを指定した場合は、その値を検索します。
さらに`--service`を指定するとサービス名・別名も検索対象に含めます。
`--tag`と`--property`はそれぞれ繰り返し指定できます。

`list`で複数のタグを指定すると、いずれかのタグを持つエントリを表示します。
`--tag-and`を指定すると、指定したすべてのタグを持つエントリに絞り込みます。

```sh
pwmgr query --full --match-mode exact "Example Service"
pwmgr search --match-mode regex "Example|Sample"
pwmgr search --property username "user@example.com"
pwmgr list --tag personal --tag web --tag-and
pwmgr list --sort-by last_update --reverse-sort
pwmgr tags --number --sort-by number_of_regist
```

`search` / `list`の`--sort-by`は`default`（ID昇順）、
`service_name`（サービス名昇順）、`last_update`（更新日時昇順）を指定できます。
更新日時未設定のエントリは末尾になります。
`tags`では`default`（タグ名昇順）、`number_of_regist`（登録件数降順）を指定します。
いずれも`--reverse-sort`で順序を反転できます。

### 削除・復元

`remove ID`は削除済みとしてマークするソフトリムーブです。
通常の検索・一覧表示・エクスポートから除外されます。
`list --with-removed`では削除済みのエントリも表示し、IDの末尾に`!`を付けます。
復元する場合は、末尾の`!`を除いたIDで`edit`を実行し、
YAMLの`removed`を削除するか`false`に変更します。

`remove --hard ID`はデータベースから完全に削除します。

### バックアップと取り込み

```sh
pwmgr export --output backup.yaml
pwmgr import --merge --dry-run backup.yaml
pwmgr import --merge backup.yaml
pwmgr import --merge --overwrite backup.yaml
```

`export`は`--output`を省略すると標準出力へ出力します。
バックアップYAMLにはパスワードを含む値が平文で出力されます。

`import`は`INPUT`を省略すると標準入力から読み込みます。
**`--merge`を省略すると、既存の全エントリを削除して入力データに置き換えます。**
`--merge`を指定した場合は既存エントリを保持して追加し、IDの重複はエラーになります。
`--merge --overwrite`では同じIDの更新日時を比較し、新しいエントリを残します。
`--dry-run`は`--merge`と組み合わせ、書き込みを行わずに取り込み内容を検証します。

### 他ホストとの同期

サーバ側で待ち受けを開始してから、クライアント側から接続します。

```sh
pwmgr sync --server 192.168.1.10:2456
```

```sh
pwmgr sync --client 192.168.1.10:2456
```

ポートの既定値は`2456`です。
`pwmgr sync --server`のようにアドレスを省略すると、`0.0.0.0:2456`で待ち受けます。
更新日時を比較して双方のエントリを同期し、同一日時で内容が異なる場合は
クライアント側でサーバの内容を採用するか確認します。
同期はTCP上のMessagePackで行います。

## 設定ファイル

設定ファイルはTOML形式です。
`global`、`query`、`search`、`list`、`tags`の各テーブルで既定値を指定できます。
各テーブルとキーは省略可能です。

```sh
pwmgr --save-default
pwmgr --config ./config.toml list
```

`--save-default`は現在のCLI指定値ではなく、アプリケーションの既定値を保存します。
`--config`で明示的に指定するファイルは、事前に存在している必要があります。

代表的な設定例です。`masked_mode = true`は秘匿項目をマスク表示する例です。
`db_path`と`log_output`は、必要に応じて利用環境のパスを指定してください。

```toml
[global]
log_level = "INFO"
# db_path = "/path/to/database.redb"
# log_output = "/path/to/log"
# editor = "vim"

[query]
match_mode = "contains"
masked_mode = true

[search]
with_service_name = false
match_mode = "contains"
target_properties = []
sort_mode = "default"
reverse_sort = false

[list]
tag_and = false
sort_mode = "default"
reverse_sort = false
with_removed = false

[tags]
match_mode = "contains"
with_number = false
sort_mode = "default"
reverse_sort = false
```

TOMLの`log_level`には`NONE`、`ERROR`、`WARN`、`INFO`、`DEBUG`、`TRACE`を指定します。
Windowsのパスは、例えば`db_path = 'C:\Users\user\pwmgr\database.redb'`のように
シングルクォートで囲むと、バックスラッシュをそのまま記述できます。
独自のデータベースパスを指定する場合は、親ディレクトリを事前に作成してください。

設定はCLI指定が設定ファイルより優先されます。
エディタはCLI、設定ファイル、環境変数`EDITOR`、OSごとの既定値の順に選択されます。
`reverse_sort`などの有効化フラグは、設定ファイルで`true`にした場合、
CLIで省略しても有効になるため、無効にするには設定ファイルを変更してください。

### 既定の保存先

| OS | 設定ファイル | データベース |
|:---|:---|:---|
| Windows | `%LOCALAPPDATA%\pwmgr\config.toml` | `%LOCALAPPDATA%\pwmgr\database.redb` |
| Linux | `~/.config/pwmgr/config.toml` | `~/.local/share/pwmgr/database.redb` |
| macOS | `~/Library/Application Support/pwmgr/config.toml` | `~/Library/Application Support/pwmgr/database.redb` |

Linuxでは`XDG_CONFIG_HOME` / `XDG_DATA_HOME`が設定されている場合、
それぞれの配下の`pwmgr`ディレクトリを使用します。
ログの既定出力先は、データベースと同じディレクトリの下の`log`です。
実際に使用されるパスやサブコマンドの設定は、`--show-options`で確認できます。

## 関連ドキュメント

- [要求仕様](doc/REQUIREMENTS.md)
- [基本設計・同期プロトコル](doc/DESIGN.md)
