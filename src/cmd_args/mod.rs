/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! コマンドライン引数を取り扱うモジュール
//!

mod config;

use std::io::{BufReader, BufWriter};
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::BaseDirs;

use crate::command::{
    add, edit, export, import, list, query, search, tags, CommandContext
};
use crate::database::EntryManager;
use config::Config;

/// デフォルトのエディタ名
static DEFAULT_EDITOR: LazyLock<&'static str> = LazyLock::new(|| {
     // `#[cfg(target_os = ..)]`を並べる方法だと inactive-codeの警告が出るので
     // LazyLockによる実装にしている

    if cfg!(target_os = "windows") {
        "notepad"

    } else if cfg!(target_os = "linux") {
        "nano"

    } else if cfg!(target_os = "macos") {
        "nano"

    } else {
        panic!("not supported os");
    }
});

/// デフォルトのデータパス
static DEFAULT_DATA_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    BaseDirs::new()
        .unwrap()
        .data_local_dir()
        .join(env!("CARGO_PKG_NAME"))
        .to_path_buf()
});

///
/// デフォルトのコンフィグレーションファイルのパス情報を生成
///
/// # 戻り値
/// コンフィギュレーションファイルのパス情報
///
fn default_config_path() -> PathBuf {
    DEFAULT_DATA_PATH.join("config.toml")
}

///
/// デフォルトのデータベースファイルのパス情報を生成
///
/// # 戻り値
/// データベースファイルのパス情報
///
fn default_db_path() -> PathBuf {
    DEFAULT_DATA_PATH.join("database.redb")
}

///
/// グローバルオプション情報を格納する構造体
///
#[derive(Parser, Debug, Clone)]
#[command(
    name = "pwmgr",
    about = "パスワードマネージャ",
    version,
    long_about = None,
    subcommand_required = false,
    arg_required_else_help = true,
)]
pub struct Options {
    /// config.tomlを使用する場合のパス
    #[arg(short = 'c', long = "config")]
    config_path: Option<PathBuf>,

    /// データベースファイルのパス
    #[arg(short = 'd', long = "db-path")]
    db_path: Option<PathBuf>,

    /// 使用するエディタの名前
    #[arg(short = 'e', long = "editor")]
    editor: Option<String>,

    /// 出力形式をJSONに変更するか否かを表すフラグ
    #[arg(long = "json-output")]
    json: bool,

    /// 設定情報の表示
    #[arg(long = "show-options")]
    show_options: bool,

    /// デフォルト設定情報の保存
    #[arg(long = "save-default")]
    save_default: bool,

    /// 実行するサブコマンド
    #[command(subcommand)]
    command: Option<Command>,
}

impl Options {
    ///
    /// データベースパスへのアクセサ
    ///
    /// # 戻り値
    /// オプションで指定されたデータベースファイルへのパスを返す。オプションで未
    /// 定義の場合はデフォルトのパスを返す。
    ///
    pub(crate) fn db_path(&self) -> PathBuf {
        if let Some(path) = &self.db_path {
            path.clone()
        } else {
            default_db_path()
        }
    }

    ///
    /// データベースのオープン
    ///
    /// # 戻り値
    /// オープンに成功した場合はデータベースオブジェクトを`Ok()`でラップして返
    /// す。失敗した場合はエラー情報を`Err()`でラップして返す。
    ///
    pub(crate) fn open(&self) -> Result<EntryManager> {
        match EntryManager::open(self.db_path()) {
            Ok(mgr) => Ok(mgr),
            Err(err) => Err(
                anyhow!("open failed: {}", err).context("database open")
            )
        }
    }

    ///
    /// 使用するエディタの名前へのアクセサ
    ///
    /// # 戻り値
    /// オプションで指定された仕様で板の名前を返す。オプションで未定義の場合は環
    /// 境変数EDITORで指定されたエディタ名を、それも未定義の場合はデフォルトのエ
    /// ディタ名を返す(Windowの場合はnotepad、Linuxの場合はnano)。
    ///
    pub(crate) fn editor(&self) -> String {
        if let Some(editor) = &self.editor {
            editor.clone()

        } else if let Ok(editor) = std::env::var("EDITOR") {
            editor

        } else {
            DEFAULT_EDITOR.to_string()
        }
    }

    ///
    /// JSON出力の指定有無を返す
    ///
    pub(crate) fn json(&self) -> bool {
        self.json
    }

    ///
    /// コンフィギュレーションファイルの適用
    ///
    /// # 戻り値
    /// 処理に成功した場合は`Ok(())`を返す。
    ///
    /// # 注記
    /// config.tomlを読み込みオプション情報に反映する。
    ///
    fn apply_config(&mut self) -> Result<()> {
        let path = if let Some(path) = &self.config_path {
            // オプションでコンフィギュレーションファイルのパスが指定されて
            // いる場合、そのパスに何もなければエラー
            if !path.exists() {
                return Err(anyhow!("{} is not exists", path.display()));
            }

            // 指定されたパスを返す
            path.clone()

        } else {
            default_config_path()
        };

        // この時点でパスに何も無い場合はそのまま何もせず正常終了
        if !path.exists() {
            return Ok(());
        }

        // 指定されたパスにあるのがファイルでなければエラー
        if !path.is_file() {
            return Err(anyhow!("{} is not file", path.display()));
        }

        // そのパスからコンフィギュレーションを読み取る
        match config::load(&path) {
            // コンフィギュレーションファイルを読み取れた場合は内容をオプション
            // 情報に反映する。
            Ok(config) => {
                if self.db_path.is_none() {
                    if let Some(path) = &config.db_path() {
                        self.db_path = Some(path.clone());
                    }
                }


                if self.editor.is_none() {
                    if let Some(editor) = &config.editor() {
                        self.editor = Some(editor.clone());
                    }
                }

                Ok(())
            }

            // エラーが出たらそのままエラー
            Err(err) => Err(anyhow!("{}", err))
        }
    }

    ///
    /// オプション情報のバリデート
    ///
    /// # 戻り値
    /// オプション情報に矛盾が無い場合は`Ok(())`を返す。
    ///
    fn validate(&mut self) -> Result<()> {
        if self.show_options && self.save_default {
            return Err(anyhow!(
                "--show-options and --save-default can't be specified mutually"
            ));
        }

        if let Some(command) = &mut self.command {
            let opts: Option<&mut dyn Validate> = match command {
                Command::Search(opts) => Some(opts),
                _ => None
            };

            if let Some(opts) = opts {
                opts.validate()?;
            }
        }

        Ok(())
    }

    ///
    /// オプション設定内容の表示
    ///
    fn show_options(&self) {
        let config_path = if let Some(path) = &self.config_path {
            path.display().to_string()
        } else {
            let path = default_config_path();

            if path.exists() {
                path.display().to_string()
            } else {
                "(none)".to_string()
            }
        };

        println!("global options");
        println!("   config path:   {}", config_path);
        println!("   database path: {}", self.db_path().display());
        println!("   editor:        {}", self.editor());

        // サブコマンドが指定されており、そのサブコマンドがオプションを持つなら
        // そのオプションも表示する。
        if let Some(command) = &self.command {
            let opts: Option<&dyn ShowOptions> = match command {
                Command::Query(opts) => Some(opts),
                Command::Search(opts) => Some(opts),
                Command::Edit(opts) => Some(opts),
                Command::Export(opts) => Some(opts),
                Command::Import(opts) => Some(opts),
                _ => None,
            };

            if let Some(opts) = opts {
                println!("");
                opts.show_options();
            }
        }
    }

    ///
    /// サブコマンドのコマンドコンテキストの生成
    ///
    pub(crate) fn build_context(&self) -> Result<Box<dyn CommandContext>> {
        match &self.command {
            Some(Command::Query(opts)) => query::build_context(self, opts),
            Some(Command::Search(opts)) => search::build_context(self, opts),
            Some(Command::Add) => add::build_context(self),
            Some(Command::Edit(opts)) => edit::build_context(self, opts),
            Some(Command::List(opts)) => list::build_context(self, opts),
            Some(Command::Tags(opts)) => tags::build_context(self, opts),
            Some(Command::Export(opts)) => export::build_context(self, opts),
            Some(Command::Import(opts)) => import::build_context(self, opts),
            None => Err(anyhow!("command not specified")),
        }
    }
}

///
/// サブコマンドの定義
///
#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// サービス名/過去名/IDによる検索
    #[command(alias = "q")]
    Query(QueryOpts),

    /// サービスの検索
    #[command(alias = "s")]
    Search(SearchOpts),

    /// エントリの追加
    #[command(alias = "a")]
    Add,

    /// 既存エントリの編集
    #[command(alias = "e")]
    Edit(EditOpts),

    /// 既存エントリのIDとサービス名の一覧
    #[command(alias = "l", visible_alias = "ls")]
    List(ListOpts),

    /// タグ一覧
    #[command(alias = "t")]
    Tags(TagsOpts),

    /// バックアップ用YAMLの出力
    Export(ExportOpts),

    /// バックアップ用YAMLの取り込み
    Import(ImportOpts),
}

///
/// show_options()実装を要求するトレイト
///
trait ShowOptions {
    ///
    /// オプション設定内容の表示
    ///
    fn show_options(&self);
}

///
/// validate()実装を要求するトレイト
///
trait Validate {
    ///
    /// オプション設定内容の表示
    ///
    fn validate(&mut self) -> Result<()>;
}

///
/// サブコマンドqueryのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct QueryOpts {
    /// 全てのプロパティを表示
    #[arg(short = 'f', long = "full")]
    full: bool,

    /// マッチモード（exact / contains / regex / fuzzy）
    #[arg(short = 'm', long = "match-mode", value_enum,
        default_value_t = MatchMode::Contains, value_name = "MODE")]
    match_mode: MatchMode,

    /// 検索のためのキー(サービス名/過去名/ID)
    #[arg()]
    key: String,
}

impl QueryOpts {
    ///
    /// 検索キーへのアクセサ
    ///
    /// # 戻り値
    /// キー文字列を返す
    ///
    pub(crate) fn key(&self) -> String {
        self.key.clone()
    }

    ///
    /// マッチモードへのアクセサ
    ///
    pub(crate) fn match_mode(&self) -> MatchMode {
        self.match_mode.clone()
    }

    ///
    /// 全てのプロパティを出力するか否かのフラグ
    ///
    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    #[cfg(test)]
    ///
    /// テスト用のコンストラクタ
    ///
    pub(crate) fn new_for_test(
        full: bool,
        match_mode: MatchMode,
        key: impl Into<String>,
    ) -> Self {
        Self {
            full,
            match_mode,
            key: key.into(),
        }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for QueryOpts {
    fn show_options(&self) {
        println!("query command options");
        println!("   key:   {}", self.key());
        println!("   mode:  {:?}", self.match_mode());
    }
}

///
/// 検索キーを表す列挙型
///
#[derive(Clone, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub(crate) enum MatchMode {
    /// 完全一致（大文字小文字無視）
    Exact,

    /// 部分一致（大文字小文字無視）
    Contains,

    /// 正規表現マッチ
    Regex,

    /// ファジーマッチ（閾値は実装側で固定）
    Fuzzy,
}

///
/// サブコマンドsearchのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct SearchOpts {
    /// サービス名を検索対象とするか否かを表すフラグ
    #[arg(long = "service", short = 's')]
    service: bool,

    /// 絞り込みを行うタグ(複数指定可)
    #[arg(long = "tag", short = 't', value_name = "TAG")]
    tags: Vec<String>,

    /// 検索対象とするプロパティのリスト(複数指定可)
    #[arg(long = "property", short = 'p', value_name = "PROPERTY_NAME")]
    properties: Vec<String>,

    /// マッチモード（exact / contains / regex / fuzzy）
    #[arg(short = 'm', long = "match-mode", value_enum,
        default_value_t = MatchMode::Contains, value_name = "MODE")]
    match_mode: MatchMode,

    /// 検索のためのキー
    #[arg()]
    key_string: String,
}

impl SearchOpts {
    ///
    /// サービス名を検索対象とするか否かを表すフラグへのアクセサ
    ///
    /// # 戻り値
    /// サービス名を検索対象とする場合は`true`を返す。
    ///
    pub(crate) fn is_include_service(&self) -> bool {
        self.service || self.properties.len() == 0
    }

    ///
    /// 絞り込み対象のタグのリストへのアクセサ
    ///
    /// # 戻り値
    /// 絞り込み対象のタグのリストをVec<String>で返す
    ///
    pub(crate) fn target_tags(&self) -> Vec<String> {
        self.tags.clone()
    }

    ///
    /// 検索対象とするプロパティ名のリストへのアクセサ
    ///
    /// # 戻り値
    /// 検索対象とするプロパティ名のリストをVec<String>で返す
    ///
    pub(crate) fn target_properties(&self) -> Vec<String> {
        self.properties.clone()
    }

    ///
    /// マッチモードの取得
    ///
    pub(crate) fn match_mode(&self) -> MatchMode {
        self.match_mode.clone()
    }

    ///
    /// 検索キーを取得
    ///
    pub(crate) fn key(&self) -> String {
        self.key_string.clone()
    }

    #[cfg(test)]
    ///
    /// テスト用のコンストラクタ
    ///
    pub(crate) fn new_for_test(
        service: bool,
        tags: Vec<String>,
        properties: Vec<String>,
        match_mode: MatchMode,
        key: impl Into<String>,
    ) -> Self {
        Self {
            service,
            tags,
            properties,
            match_mode,
            key_string: key.into(),
        }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for SearchOpts {
    fn show_options(&self) {
        println!("search command options");
        println!("   include service:   {}", self.is_include_service());
        println!("   target tags:       {:?}", self.target_tags());
        println!("   target properties: {:?}", self.target_properties());
        println!("   match mode:        {:?}", self.match_mode());
        println!("   search key:        {}", self.key());
    }
}

// Validateトレイトの実装
impl Validate for SearchOpts {
    fn validate(&mut self) -> Result<()> {
        Ok(())
    }
}

///
/// サブコマンドeditのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct EditOpts {
    /// 編集対象のサービスID
    #[arg()]
    id: String,
}

impl EditOpts {
    ///
    /// 編集エントリのID文字列へのアクセサ
    ///
    /// # 戻り値
    /// ID文字列を返す
    ///
    pub(crate) fn id(&self) -> String {
        self.id.clone()
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for EditOpts {
    fn show_options(&self) {
        println!("edit command options");
        println!("   target_id:   {}", self.id());
    }
}

///
/// サブコマンドlistのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct ListOpts {
    /// リストアップ対象タグ(複数指定可)
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tags: Vec<String>,

    /// 複数タグ指定時にAND条件で絞り込む（未指定時はOR）
    #[arg(long = "tag-and")]
    tag_and: bool,

    /// サービス名でソートする（デフォルトはID）
    #[arg(short = 'N', long = "sort-by-service-name")]
    sort_by_service_name: bool,

    /// ソート順を逆順にする
    #[arg(short = 'r', long = "reverse-sort")]
    reverse_sort: bool,
}

impl ListOpts {
    ///
    /// タグフィルタの一覧を取得
    ///
    /// # 戻り値
    /// タグリストを返す
    pub(crate) fn target_tags(&self) -> Vec<String> {
        self.tags.clone()
    }

    ///
    /// 複数タグをAND条件で解釈するか
    ///
    pub(crate) fn is_tag_and(&self) -> bool {
        self.tag_and
    }

    ///
    /// サービス名でソートするか
    ///
    pub(crate) fn sort_by_service_name(&self) -> bool {
        self.sort_by_service_name
    }

    ///
    /// ソートを逆順にするか
    ///
    pub(crate) fn reverse_sort(&self) -> bool {
        self.reverse_sort
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for ListOpts {
    fn show_options(&self) {
        println!("list command options");
        println!("   target_tags:   {:?}", self.tags);
        println!("   tag_and:       {}", self.tag_and);
        println!("   sort_by_name:  {}", self.sort_by_service_name);
        println!("   reverse_sort:  {}", self.reverse_sort);
    }
}

///
/// サブコマンドtagsのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct TagsOpts {
    /// 件数を表示するフラグ
    #[arg(short = 'n', long = "number")]
    number: bool,

    /// 件数でソートするフラグ（デフォルトは降順、--reverse-sortで反転）
    #[arg(short = 's', long = "sort-by-number")]
    sort_by_number: bool,

    /// ソート結果を反転するフラグ
    #[arg(short = 'r', long = "reverse-sort")]
    reverse_sort: bool,

    /// マッチモード（exact / contains / regex / fuzzy）
    #[arg(short = 'm', long = "match-mode", value_enum,
        default_value_t = MatchMode::Exact, value_name = "MODE")]
    match_mode: MatchMode,

    /// 絞り込み用のキー（省略時は全タグ）
    #[arg()]
    key: Option<String>,
}

impl TagsOpts {
    ///
    /// 件数表示の有無を返す
    ///
    pub(crate) fn number(&self) -> bool {
        self.number
    }

    ///
    /// 件数ソートの有無を返す
    ///
    pub(crate) fn sort_by_number(&self) -> bool {
        self.sort_by_number
    }

    ///
    /// ソートの逆順指定の有無を返す
    ///
    pub(crate) fn reverse_sort(&self) -> bool {
        self.reverse_sort
    }

    ///
    /// マッチモードを返す
    ///
    pub(crate) fn match_mode(&self) -> MatchMode {
        self.match_mode.clone()
    }

    ///
    /// 絞り込みキー（省略可）を返す
    ///
    pub(crate) fn key(&self) -> Option<String> {
        self.key.clone()
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        number: bool,
        sort_by_number: bool,
        reverse_sort: bool,
        match_mode: MatchMode,
        key: Option<String>,
    ) -> Self {
        Self {
            number,
            sort_by_number,
            reverse_sort,
            match_mode,
            key,
        }
    }
}

impl ShowOptions for TagsOpts {
    fn show_options(&self) {
        let key = self.key().unwrap_or_else(|| "(none)".to_string());

        println!("tags command options");
        println!("   number:          {}", self.number());
        println!("   sort_by_number:  {}", self.sort_by_number());
        println!("   reverse_sort:    {}", self.reverse_sort());
        println!("   match_mode:      {:?}", self.match_mode());
        println!("   key:             {}", key);
    }
}

///
/// サブコマンドexportのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct ExportOpts {
    /// 出力ファイル名(デフォルトは標準出力)
    #[arg(long = "output", short = 'o', value_name = "PATH")]
    output: Option<PathBuf>,
}

impl ExportOpts {
    ///
    /// 出力先のライターオブジェクトへのアクセサ
    ///
    /// # 戻り値
    /// 出力先のオープン済みのライターオブジェクト
    ///
    pub(crate) fn output(&self) -> Result<BufWriter<impl std::io::Write>> {
        let io: Box<dyn std::io::Write> = if let Some(file) = &self.output {
            Box::new(File::create(file)?)
        } else {
            Box::new(std::io::stdout())
        };

        Ok(BufWriter::new(io))
    }

    #[cfg(test)]
    ///
    /// テスト用のコンストラクタ
    ///
    pub(crate) fn new_for_test(output: Option<PathBuf>) -> Self {
        Self { output }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for ExportOpts {
    fn show_options(&self) {
        let export_to = if let Some(path) = &self.output {
            &path.display().to_string()
        } else {
            "(stdout)"
        };

        println!("export command options");
        println!("   export to:  {}", export_to);
    }
}

///
/// サブコマンドimportのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct ImportOpts {
    /// マージフラグ
    #[arg(short = 'm', long = "merge", short = 'm')]
    merge: bool,

    /// オーバライトフラグ
    #[arg(short = 'O', long = "overwrite", short = 'O')]
    overwrite: bool,

    /// Dry-Runフラグ
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// 入力ファイル名(指定なしで標準入力)
    #[arg()]
    input_path: Option<PathBuf>,
}

impl ImportOpts {
    ///
    /// 入力元のリーダーオブジェクトへのアクセサ
    ///
    /// # 戻り値
    /// 入力元のオープン済みのライターオブジェクト
    ///
    pub(crate) fn input(&self) -> Result<BufReader<impl std::io::Read>> {
        let io: Box<dyn std::io::Read> = if let Some(file) = &self.input_path {
            Box::new(File::open(file)?)
        } else {
            Box::new(std::io::stdin())
        };

        Ok(BufReader::new(io))
    }

    ///
    /// マージを行うか否かのフラグへのアクセサ
    ///
    /// # 戻り値
    /// マージを行う場合は`true`を返す。
    ///
    pub(crate) fn is_merge(&self) -> bool {
        self.merge
    }

    ///
    /// 重複するサービスが存在する場合に上書きするか否かを表すフラグへのアクセサ
    ///
    /// # 戻り値
    /// 上書を行う場合は`true`を返す。
    ///
    pub(crate) fn is_overwrite(&self) -> bool {
        self.overwrite
    }

    ///
    /// ドライランを行うか否かのフラグへのアクセサ
    ///
    /// # 戻り値
    /// ドライランを行う場合は`true`を返す。
    ///
    pub(crate) fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    #[cfg(test)]
    ///
    /// テスト用のコンストラクタ
    ///
    pub(crate) fn new_for_test(
        input: Option<PathBuf>,
        merge: bool,
        overwrite: bool,
        dry_run: bool,
    ) -> Self {
        Self {
            input_path: input,
            merge,
            overwrite,
            dry_run,
        }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for ImportOpts {
    fn show_options(&self) {
        let import_from = if let Some(path) = &self.input_path {
            &path.display().to_string()
        } else {
            "(stdin)"
        };

        println!("import command options");
        println!("   import from:  {}", import_from);
        println!("   is mearge:    {}", self.is_merge());
        println!("   is overwrite: {}", self.is_overwrite());
        println!("   is dry-run: {}", self.is_dry_run());
    }
}
///
/// コマンドライン引数のパース処理
///
/// # 戻り値
/// オプション情報をまとめたオブジェクトを返す。
///
pub(crate) fn parse() -> Result<Arc<Options>> {
    let mut opts = Options::parse();

    /*
     * デフォルトデータパスの作成
     */
    std::fs::create_dir_all(DEFAULT_DATA_PATH.clone())?;

    /*
     * コンフィギュレーションファイルの適用
     */
    opts.apply_config()?;

    /*
     * 設定情報のバリデーション
     */
    opts.validate()?;

    /*
     * 設定情報の表示
     */
    if opts.show_options {
        opts.show_options();
        std::process::exit(0);
    }

    /*
     * デフォルト設定の保存
     */
    if opts.save_default {
        let path = if let Some(path) = &opts.config_path {
            path.clone()
        } else {
            default_config_path()
        };

        Config::default().save(&path)?;
        println!("write default config to {}", path.display().to_string());
        std::process::exit(0);
    }

    /*
     * 設定情報の返却
     */
    Ok(Arc::new(opts))
}
