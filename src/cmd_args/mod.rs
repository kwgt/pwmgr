/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! コマンドライン引数を取り扱うモジュール
//!

mod config;
mod logger;

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::BaseDirs;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::command::{
    add, edit, export, import, list, query, remove, search, sync, tags,
    CommandContext
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
static DEFAULT_CONFIG_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    BaseDirs::new()
        .unwrap()
        .config_local_dir()
        .join(env!("CARGO_PKG_NAME"))
        .to_path_buf()
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
    DEFAULT_CONFIG_PATH.join("config.toml")
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
/// デフォルトのログ出力先のパスを生成
///
/// # 戻り値
/// ログ出力先ディレクトリのパス情報
///
fn default_log_path() -> PathBuf {
    DEFAULT_DATA_PATH.join("log")
}

///
/// ログレベルを指し示す列挙子
///
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, Deserialize, Serialize)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "UPPERCASE")]
enum LogLevel {
    /// ログを記録しない
    #[serde(alias = "off", alias = "OFF")]
    #[value(alias = "off")]
    None,

    /// エラー情報以上のレベルを記録
    Error,

    /// 警告情報以上のレベルを記録
    Warn,

    /// 一般情報以上のレベルを記録
    Info,

    /// デバッグ情報以上のレベルを記録
    Debug,

    /// トレース情報以上のレベルを記録
    Trace,
}

// Intoトレイトの実装
impl Into<log::LevelFilter> for LogLevel {
    fn into(self) -> log::LevelFilter {
        match self {
            Self::None => log::LevelFilter::Off,
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }
}

// AsRefトレイトの実装
impl AsRef<str> for LogLevel {
    fn as_ref(&self) -> &str {
        match self {
            Self::None => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
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

    /// 記録するログレベルの指定
    #[arg(short = 'l', long = "log-level", value_name = "LEVEL",
        ignore_case = true)]
    log_level: Option<LogLevel>,

    /// ログの出力先の指定
    #[arg(short = 'L', long = "log-output", value_name = "PATH")]
    log_output: Option<PathBuf>,

    /// ログを標準出力にも同時出力するか否か
    #[arg(long = "log-tee")]
    log_tee: bool,

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
    /// ログレベルへのアクセサ
    ///
    /// # 戻り値
    /// 設定されたログレベルを返す
    fn log_level(&self) -> LogLevel {
        if let Some(level) = self.log_level {
            level
        } else {
            LogLevel::Info
        }
    }

    ///
    /// ログの出力先へのアクセサ
    ///
    /// # 戻り値
    /// ログの出力先として設定されたパス情報を返す。未設定の場合はデフォルトの
    /// パスを返す。
    ///
    fn log_output(&self) -> PathBuf {
        if let Some(path) = &self.log_output {
            path.clone()
        } else {
            default_log_path()
        }
    }

    ///
    /// ログの標準出力同時出力フラグへのアクセサ
    ///
    /// # 戻り値
    /// ログの標準出力同時出力が有効であればtrueを返す
    ///
    fn log_tee(&self) -> bool {
        self.log_tee
    }

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

                if self.log_level.is_none() {
                    if let Some(level) = config.log_level() {
                        self.log_level = Some(level);
                    }
                }

                if self.log_output.is_none() {
                    if let Some(path) = &config.log_output() {
                        self.log_output = Some(path.clone());
                    }
                }

                if self.editor.is_none() {
                    if let Some(editor) = &config.editor() {
                        self.editor = Some(editor.clone());
                    }
                }

                // コマンド毎のオプション情報へもコンフィギュレーションの内容を
                // 反映する。
                let opts: Option<&mut dyn ApplyConfig> = match
                    &mut self.command
                {
                    Some(Command::Query(opts)) => Some(opts),
                    Some(Command::Search(opts)) => Some(opts),
                    Some(Command::List(opts)) => Some(opts),
                    Some(Command::Tags(opts)) => Some(opts),
                    _ => None,
                };

                if let Some(opts) = opts {
                    opts.apply_config(&config);
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
                Command::Query(opts) => Some(opts),
                Command::Search(opts) => Some(opts),
                Command::Import(opts) => Some(opts),
                Command::Sync(opts) => Some(opts),
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
        println!("   log level:     {}", self.log_level().as_ref());
        println!("   log output:    {}", self.log_output().display());
        println!("   log tee:       {}", self.log_tee());
        println!("   editor:        {}", self.editor());

        // サブコマンドが指定されており、そのサブコマンドがオプションを持つなら
        // そのオプションも表示する。
        if let Some(command) = &self.command {
            let opts: Option<&dyn ShowOptions> = match command {
                Command::Query(opts) => Some(opts),
                Command::Search(opts) => Some(opts),
                Command::Edit(opts) => Some(opts),
                Command::List(opts) => Some(opts),
                Command::Tags(opts) => Some(opts),
                Command::Export(opts) => Some(opts),
                Command::Import(opts) => Some(opts),
                Command::Sync(opts) => Some(opts),
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
            Some(Command::Add(opts)) => add::build_context(self, opts),
            Some(Command::Edit(opts)) => edit::build_context(self, opts),
            Some(Command::List(opts)) => list::build_context(self, opts),
            Some(Command::Tags(opts)) => tags::build_context(self, opts),
            Some(Command::Export(opts)) => export::build_context(self, opts),
            Some(Command::Import(opts)) => import::build_context(self, opts),
            Some(Command::Remove(opts)) => remove::build_context(self, opts),
            Some(Command::Sync(opts)) => sync::build_context(self, opts),
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
    Add(AddOpts),

    /// 既存エントリの編集
    #[command(alias = "e")]
    Edit(EditOpts),

    /// 既存エントリのIDとサービス名の一覧
    #[command(alias = "l", visible_alias = "ls")]
    List(ListOpts),

    /// タグ一覧
    #[command(alias = "t")]
    Tags(TagsOpts),

    /// エントリの削除
    #[command(alias = "r", visible_alias = "rm")]
    Remove(RemoveOpts),

    /// バックアップ用YAMLの出力
    Export(ExportOpts),

    /// バックアップ用YAMLの取り込み
    Import(ImportOpts),

    /// 他ホストとのデータベース同期
    Sync(SyncOpts),
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
/// apply_config()実装を要求するトレイト
///
trait ApplyConfig {
    ///
    /// オプション設定へのコンフィギュレーションの反映
    ///
    fn apply_config(&mut self, config: &Config);
}

///
/// サブコマンドaddのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct AddOpts {
    /// 事前入力するサービス名（省略可）
    #[arg()]
    service_name: Option<String>,
}

impl AddOpts {
    ///
    /// サービス名（省略可）を返す
    ///
    pub(crate) fn service_name(&self) -> Option<String> {
        self.service_name.clone()
    }

    ///
    /// テスト用インスタンス生成関数
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(service_name: Option<String>) -> Self {
        Self { service_name }
    }
}

///
/// サブコマンドqueryのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct QueryOpts {
    /// 全てのプロパティを表示
    #[arg(short = 'f', long = "full")]
    full: bool,

    /// 秘匿項目をマスクして表示
    #[arg(short = 'M', long = "masked-mode", conflicts_with = "unmasked_mode")]
    masked_mode: bool,

    /// 秘匿項目をマスクせずに表示
    #[arg(short = 'U', long = "unmasked-mode", conflicts_with = "masked_mode")]
    unmasked_mode: bool,

    /// マッチモード
    #[arg(
        short = 'm',
        long = "match-mode",
        value_enum,
        value_name = "MODE",
        help = "マッチモード\n"
    )]
    match_mode: Option<MatchMode>,

    /// マスクモードのデフォルト値(config適用後に保持)
    #[arg(skip)]
    default_masked: Option<bool>,

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
        self.match_mode.unwrap_or(MatchMode::Contains)
    }

    ///
    /// 秘匿項目をマスクするか否か
    ///
    pub(crate) fn is_masked(&self) -> bool {
        if self.masked_mode {
            true
        } else if self.unmasked_mode {
            false
        } else {
            self.default_masked.unwrap_or(false)
        }
    }

    ///
    /// 全てのプロパティを出力するか否かのフラグ
    ///
    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(
        full: bool,
        match_mode: MatchMode,
        key: impl Into<String>,
    ) -> Self {
        Self {
            full,
            masked_mode: false,
            unmasked_mode: false,
            match_mode: Some(match_mode),
            default_masked: None,
            key: key.into(),
        }
    }

    ///
    /// テスト用のコンストラクタ（マスク指定付き）
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test_with_mask(
        full: bool,
        match_mode: MatchMode,
        key: impl Into<String>,
        masked_mode: bool,
        unmasked_mode: bool,
        default_masked: Option<bool>,
    ) -> Self {
        Self {
            full,
            masked_mode,
            unmasked_mode,
            match_mode: Some(match_mode),
            default_masked,
            key: key.into(),
        }
    }
}

// Validateトレイトの実装
impl Validate for QueryOpts {
    fn validate(&mut self) -> Result<()> {
        if self.masked_mode && self.unmasked_mode {
            return Err(anyhow!(
                "--masked-mode と --unmasked-mode は同時に指定できません"
            ));
        }

        Ok(())
    }
}

// ApplyConfigトレイトの実装
impl ApplyConfig for QueryOpts {
    fn apply_config(&mut self, config: &Config) {
        if self.match_mode.is_none() {
            if let Some(mode) = config.query_match_mode() {
                self.match_mode = Some(mode);
            }
        }

        if self.default_masked.is_none() {
            self.default_masked = config.query_masked_mode();
        }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for QueryOpts {
    fn show_options(&self) {
        println!("query command options");
        println!("   key:   {}", self.key());
        println!("   mode:  {:?}", self.match_mode());
        println!("   mask:  {}", self.is_masked());
    }
}

///
/// 検索キーを表す列挙型
///
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
/// ソートモードを表す列挙子
///
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum SortMode {
    /// デフォルト（ID順）
    Default,

    /// サービス名でソート
    ServiceName,

    /// 更新日時でソート
    LastUpdate,
}

///
/// タグ一覧のソートモードを表す列挙子
///
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum TagsSortMode {
    /// デフォルト（タグ名でソート）
    Default,

    /// 登録件数でソート
    NumberOfRegist,
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
    properties: Option<Vec<String>>,

    /// マッチモード
    #[arg(
        short = 'm',
        long = "match-mode",
        value_enum,
        value_name = "MODE",
        help = "マッチモード\n"
    )]
    match_mode: Option<MatchMode>,

    /// ソートモード
    #[arg(long = "sort-by", value_enum, value_name = "MODE")]
    sort_by: Option<SortMode>,

    /// ソート順を逆順にする
    #[arg(short = 'r', long = "reverse-sort")]
    reverse_sort: bool,

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
         self.service || self.target_properties().is_empty()
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
        self.properties.clone().unwrap_or_default()
    }

    ///
    /// マッチモードの取得
    ///
    pub(crate) fn match_mode(&self) -> MatchMode {
        self.match_mode.unwrap_or(MatchMode::Contains)
    }

    ///
    /// ソートモードの取得
    ///
    pub(crate) fn sort_mode(&self) -> SortMode {
        self.sort_by.unwrap_or(SortMode::Default)
    }

    ///
    /// ソートを逆順にするか
    ///
    pub(crate) fn reverse_sort(&self) -> bool {
        self.reverse_sort
    }

    ///
    /// 検索キーを取得
    ///
    pub(crate) fn key(&self) -> String {
        self.key_string.clone()
    }

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(
        service: bool,
        tags: Vec<String>,
        properties: Vec<String>,
        match_mode: MatchMode,
        sort_mode: SortMode,
        reverse_sort: bool,
        key: impl Into<String>,
    ) -> Self {
        Self {
            service,
            tags,
            properties: Some(properties),
            match_mode: Some(match_mode),
            sort_by: Some(sort_mode),
            reverse_sort,
            key_string: key.into(),
        }
    }
}

// Validateトレイトの実装
impl Validate for SearchOpts {
    fn validate(&mut self) -> Result<()> {
        Ok(())
    }
}

// ApplyConfigトレイトの実装
impl ApplyConfig for SearchOpts {
    fn apply_config(&mut self, config: &Config) {
        if !self.service {
            self.service = config.search_with_service_name().unwrap_or(false);
        }

        if self.match_mode.is_none() {
            self.match_mode = config.search_match_mode();
        }

        if self.properties.is_none() {
            self.properties = config.search_target_properties();
        }

        if self.sort_by.is_none() {
            self.sort_by = config.search_sort_mode();
        }

        if !self.reverse_sort {
            self.reverse_sort = config.search_reverse_sort().unwrap_or(false);
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
        println!("   sort mode:         {:?}", self.sort_mode());
        println!("   reverse sort:      {}", self.reverse_sort());
        println!("   search key:        {}", self.key());
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

    /// ソートモード
    #[arg(long = "sort-by", value_enum, value_name = "MODE")]
    sort_by: Option<SortMode>,

    /// ソート順を逆順にする
    #[arg(short = 'r', long = "reverse-sort")]
    reverse_sort: bool,

    /// 互換用: サービス名でソートする
    #[arg(short = 'N', long = "sort-by-service-name", hide = true)]
    sort_by_service_name_compat: bool,

    /// 互換用: 最終更新日時でソートする
    #[arg(short = 'L', long = "sort-by-last-update", hide = true)]
    sort_by_last_update_compat: bool,

    /// 削除済みエントリも表示する
    #[arg(long = "with-removed")]
    with_removed: bool,
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
    /// ソートモードの取得
    ///
    pub(crate) fn sort_mode(&self) -> SortMode {
        if let Some(mode) = self.sort_by {
            mode
        } else if self.sort_by_last_update_compat {
            SortMode::LastUpdate
        } else if self.sort_by_service_name_compat {
            SortMode::ServiceName
        } else {
            SortMode::Default
        }
    }

    ///
    /// ソートを逆順にするか
    ///
    pub(crate) fn reverse_sort(&self) -> bool {
        self.reverse_sort
    }

    ///
    /// 削除済みエントリも含めるか
    ///
    pub(crate) fn with_removed(&self) -> bool {
        self.with_removed
    }
}

// ApplyConfigトレイトの実装
impl ApplyConfig for ListOpts {
    fn apply_config(&mut self, config: &Config) {
        if !self.tag_and {
            self.tag_and = config.list_tag_and().unwrap_or(false);
        }

        if !self.reverse_sort {
            self.reverse_sort = config.list_reverse_sort().unwrap_or(false);
        }

        if !self.with_removed {
            self.with_removed = config.list_with_removed().unwrap_or(false);
        }

        if self.sort_by.is_none() {
            if self.sort_by_last_update_compat {
                self.sort_by = Some(SortMode::LastUpdate);
            } else if self.sort_by_service_name_compat {
                self.sort_by = Some(SortMode::ServiceName);
            }
        }

        if self.sort_by.is_none() {
            self.sort_by = config.list_sort_mode();
        }
    }
}

// ShowOptionsトレイトの実装
impl ShowOptions for ListOpts {
    fn show_options(&self) {
        println!("list command options");
        println!("   target_tags:   {:?}", self.tags);
        println!("   tag_and:       {}", self.is_tag_and());
        println!("   sort_mode:     {:?}", self.sort_mode());
        println!("   reverse_sort:  {}", self.reverse_sort());
        println!("   with_removed:  {}", self.with_removed());
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

    /// ソートモード
    #[arg(long = "sort-by", value_enum, value_name = "MODE")]
    sort_by: Option<TagsSortMode>,

    /// ソート結果を反転するフラグ
    #[arg(short = 'r', long = "reverse-sort")]
    reverse_sort: bool,

    /// 互換用: 件数でソートする
    #[arg(short = 'N', long = "sort-by-number", hide = true)]
    sort_by_number_compat: bool,

    /// マッチモード
    #[arg(
        short = 'm',
        long = "match-mode",
        value_enum,
        value_name = "MODE",
        help = "マッチモード\n"
    )]
    match_mode: Option<MatchMode>,

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
    /// ソートモードを返す
    ///
    pub(crate) fn sort_mode(&self) -> TagsSortMode {
        if let Some(mode) = self.sort_by {
            mode
        } else if self.sort_by_number_compat {
            TagsSortMode::NumberOfRegist
        } else {
            TagsSortMode::Default
        }
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
        self.match_mode.unwrap_or(MatchMode::Contains)
    }

    ///
    /// 絞り込みキー（省略可）を返す
    ///
    pub(crate) fn key(&self) -> Option<String> {
        self.key.clone()
    }

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(
        number: bool,
        sort_by: TagsSortMode,
        reverse_sort: bool,
        match_mode: MatchMode,
        key: Option<String>,
    ) -> Self {
        Self {
            number,
            sort_by: Some(sort_by),
            reverse_sort,
            sort_by_number_compat: false,
            match_mode: Some(match_mode),
            key,
        }
    }
}

// ApplyConfigトレイトの実装
impl ApplyConfig for TagsOpts {
    fn apply_config(&mut self, config: &Config) {
        if !self.number {
            self.number = config.tags_with_number().unwrap_or(false);
        }

        if !self.reverse_sort {
            self.reverse_sort = config.tags_reverse_sort().unwrap_or(false);
        }

        if self.sort_by.is_none() && self.sort_by_number_compat {
            self.sort_by = Some(TagsSortMode::NumberOfRegist);
        }

        if self.sort_by.is_none() {
            self.sort_by = config.tags_sort_mode();
        }

        if self.match_mode.is_none() {
            self.match_mode = config.tags_match_mode();
        }
    }
}

impl ShowOptions for TagsOpts {
    fn show_options(&self) {
        let key = self.key().unwrap_or_else(|| "(none)".to_string());

        println!("tags command options");
        println!("   number:          {}", self.number());
        println!("   sort_mode:       {:?}", self.sort_mode());
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

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
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

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
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
/// サブコマンドsyncのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct SyncOpts {
    /// サーバモードでの待ち受けアドレス（省略可）
    #[arg(
        short = 's',
        long = "server",
        value_name = "BIND-ADDR[:PORT]",
        num_args = 0..=1,
        default_missing_value = "0.0.0.0:2456",
        conflicts_with = "client_addr"
    )]
    server_addr: Option<String>,

    /// クライアントモードで接続するアドレス
    #[arg(
        short = 'c',
        long = "client",
        value_name = "CONNECT-ADDR[:PORT]",
        conflicts_with = "server_addr"
    )]
    client_addr: Option<String>,
}

impl SyncOpts {
    ///
    /// アドレス文字列のバリデーション
    ///
    fn validate_addr(addr: &str) -> Result<()> {
        static ADDR_RE: LazyLock<Regex> = LazyLock::new(|| {
            // ホスト部(英数字/ドット/ハイフン/アスタリスク) + 任意のポート
            Regex::new(r"^[A-Za-z0-9*](?:[A-Za-z0-9.-]*[A-Za-z0-9])?(?::\d{1,5})?$")
                .expect("invalid regex")
        });

        if !ADDR_RE.is_match(addr) {
            return Err(anyhow!("invalid address format: {}", addr));
        }

        if let Some(idx) = addr.rfind(':') {
            if idx + 1 < addr.len() {
                let port_str = &addr[idx + 1..];
                let port: u32 = port_str
                    .parse()
                    .map_err(|_| anyhow!("port must be numeric: {}", port_str))?;
                if port == 0 || port > u16::MAX as u32 {
                    return Err(anyhow!("port must be in 1-65535: {}", port));
                }
            }
        }

        Ok(())
    }

    ///
    /// 同期の動作モード
    ///
    pub(crate) fn mode(&self) -> Result<SyncMode> {
        if let Some(addr) = &self.server_addr {
            Ok(SyncMode::Server(addr.clone()))
        } else if let Some(addr) = &self.client_addr {
            Ok(SyncMode::Client(addr.clone()))
        } else {
            Err(anyhow!("either --server or --client must be specified"))
        }
    }
}

///
/// syncモードを表す列挙
///
#[derive(Clone, Debug)]
pub(crate) enum SyncMode {
    /// サーバとして待ち受け
    Server(String),

    /// クライアントとして接続
    Client(String),
}

// ShowOptionsトレイトの実装
impl ShowOptions for SyncOpts {
    fn show_options(&self) {
        println!("sync command options");
        match self.mode() {
            Ok(SyncMode::Server(addr)) => println!("   mode: server @ {}", addr),
            Ok(SyncMode::Client(addr)) => println!("   mode: client -> {}", addr),
            Err(err) => println!("   mode: invalid ({})", err),
        }
    }
}

// Validateトレイトの実装
impl Validate for SyncOpts {
    fn validate(&mut self) -> Result<()> {
        match (self.server_addr.as_ref(), self.client_addr.as_ref()) {
            (Some(_), Some(_)) => Err(anyhow!("--server と --client は同時に指定できません")),
            (None, None) => Err(anyhow!("--server か --client のどちらかを指定してください")),
            (Some(addr), None) if addr.trim().is_empty() => {
                Err(anyhow!("--server で空のアドレスは指定できません"))
            }
            (None, Some(addr)) if addr.trim().is_empty() => {
                Err(anyhow!("--client で空のアドレスは指定できません"))
            }
            (Some(addr), None) => {
                Self::validate_addr(addr)?;
                Ok(())
            }
            (None, Some(addr)) => {
                Self::validate_addr(addr)?;
                Ok(())
            }
        }
    }
}

// Validateトレイトの実装
impl Validate for ImportOpts {
    fn validate(&mut self) -> Result<()> {
        if self.dry_run && !self.merge {
            return Err(anyhow!("--dry-run は --merge 指定時のみ指定できます"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn config_from_toml(src: &str) -> Config {
        toml::from_str(src).expect("toml parse failed")
    }

    #[test]
    fn search_apply_config_fill_defaults() {
        let cfg = config_from_toml(
            r#"
[search]
with_service_name = true
match_mode = "regex"
target_properties = ["user", "pass"]
sort_mode = "service_name"
reverse_sort = true
"#,
        );

        let mut opts = SearchOpts {
            service: false,
            tags: vec![],
            properties: None,
            match_mode: None,
            sort_by: None,
            reverse_sort: false,
            key_string: "dummy".into(),
        };

        opts.apply_config(&cfg);

        assert!(opts.is_include_service());
        assert_eq!(opts.match_mode(), MatchMode::Regex);
        assert_eq!(
            opts.target_properties(),
            vec!["user".to_string(), "pass".to_string()]
        );
        assert_eq!(opts.sort_mode(), SortMode::ServiceName);
        assert!(opts.reverse_sort());
    }

    #[test]
    fn list_apply_config_sort_and_flags() {
        let cfg = config_from_toml(
            r#"
[list]
tag_and = true
sort_mode = "last_update"
reverse_sort = true
with_removed = true
"#,
        );

        let mut opts = ListOpts {
            tags: vec![],
            tag_and: false,
            reverse_sort: false,
            sort_by: None,
            sort_by_service_name_compat: false,
            sort_by_last_update_compat: false,
            with_removed: false,
        };

        opts.apply_config(&cfg);

        assert!(opts.is_tag_and());
        assert_eq!(opts.sort_mode(), SortMode::LastUpdate);
        assert!(opts.reverse_sort());
        assert!(opts.with_removed());
    }

    #[test]
    fn tags_apply_config_sort_and_flags() {
        let cfg = config_from_toml(
            r#"
[tags]
with_number = true
sort_mode = "number_of_regist"
reverse_sort = true
"#,
        );

        let mut opts = TagsOpts {
            number: false,
            reverse_sort: false,
            sort_by: None,
            sort_by_number_compat: false,
            match_mode: Some(MatchMode::Exact),
            key: None,
        };

        opts.apply_config(&cfg);

        assert!(opts.number());
        assert_eq!(opts.sort_mode(), TagsSortMode::NumberOfRegist);
        assert!(opts.reverse_sort());
        assert_eq!(opts.match_mode(), MatchMode::Exact);
    }

    #[test]
    fn query_validate_rejects_conflicting_mask_flags() {
        let mut opts = QueryOpts::new_for_test_with_mask(
            true,
            MatchMode::Exact,
            "dummy",
            true,
            true,
            None,
        );

        assert!(opts.validate().is_err());
    }

    #[test]
    fn confirm_overwrite_yes_and_no() {
        let mut output = Vec::new();
        let mut yes = Cursor::new(b"y\n");
        let mut no = Cursor::new(b"n\n");
        let path = Path::new("dummy");

        assert!(confirm_overwrite_with_io(path, &mut yes, &mut output).unwrap());
        assert!(!confirm_overwrite_with_io(path, &mut no, &mut output).unwrap());
    }
}

///
/// サブコマンドremoveのオプション
///
#[derive(Clone, Args, Debug)]
pub(crate) struct RemoveOpts {
    /// ハードリムーブフラグ
    #[arg(long = "hard")]
    hard: bool,

    /// 削除対象のID
    #[arg()]
    id: String,
}

impl RemoveOpts {
    ///
    /// ハードリムーブか否かを表すフラグを返す
    ///
    pub(crate) fn is_hard(&self) -> bool {
        self.hard
    }

    ///
    /// 削除対象IDへのアクセサ
    ///
    pub(crate) fn id(&self) -> String {
        self.id.clone()
    }

    ///
    /// テスト用のコンストラクタ
    ///
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(id: impl Into<String>, hard: bool) -> Self {
        Self { id: id.into(), hard }
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
     * ログ機能の初期化
     */
    logger::init(&opts)?;

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

        if path.exists() {
            if !confirm_overwrite(&path)? {
                println!("write default config is canceled.");
                std::process::exit(0);
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Config::default().save(&path)?;
        println!("write default config to {}", path.display().to_string());
        std::process::exit(0);
    }

    /*
     * 設定情報の返却
     */
    Ok(Arc::new(opts))
}

///
/// config.tomlの上書き可否を標準入出力で問い合わせる
///
/// # 引数
/// * `path` - 対象となるパス
///
/// # 戻り値
/// 上書きを許可する場合は`true`、拒否された場合は`false`を返す。
///
fn confirm_overwrite(path: &Path) -> Result<bool> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    let mut input = stdin.lock();
    let mut output = stdout.lock();

    confirm_overwrite_with_io(path, &mut input, &mut output)
}

///
/// 任意の入出力を使ってconfig.tomlの上書き可否を問い合わせる
///
/// # 引数
/// * `path` - 対象となるパス
/// * `input` - 入力ストリーム（質問への回答を受け取る）
/// * `output` - 出力ストリーム（質問を表示する）
///
/// # 戻り値
/// 上書きを許可する場合は`true`、拒否された場合は`false`を返す。
///
fn confirm_overwrite_with_io<R, W>(path: &Path, input: &mut R, output: &mut W,)
    -> Result<bool>
where
    R: BufRead,
    W: Write,
{
    write!(
        output,
        "{} は既に存在します。上書きしますか？ [y/N]: ",
        path.display()
    )?;
    output.flush()?;

    let mut buf = String::new();
    input.read_line(&mut buf)?;

    let ans = buf.trim().to_lowercase();
    Ok(ans == "y" || ans == "yes")
}
