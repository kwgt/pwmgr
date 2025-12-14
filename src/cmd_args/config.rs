/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! コンフィギュレーション情報の定義
//!

use std::default::Default;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::{default_db_path, default_log_path};
use super::{LogLevel, MatchMode, DEFAULT_EDITOR};

///
/// コンフィギュレーションデータを集約する構造体
///
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Config {
    /// グローバルオプションに対する情報
    global: Option<GlobalInfo>,

    /// queryサブコマンド用の設定
    query: Option<QueryInfo>,

    /// searchサブコマンド用の設定
    search: Option<SearchInfo>,

    /// listサブコマンド用の設定
    list: Option<ListInfo>,

    /// tagsサブコマンド用の設定
    tags: Option<TagsInfo>,
}

impl Config {
    ///
    /// データベースファイルへのパスへのアクセサ
    ///
    /// # 戻り値
    /// データベースファイルパスが設定されている場合はパス情報を`Some()`でラップ
    /// して返す。
    ///
    pub(super) fn db_path(&self) -> Option<PathBuf> {
        self.global
            .as_ref()
            .and_then(|global| global.db_path.as_ref())
            .cloned()
    }

    ///
    /// ログレベルへのアクセサ
    ///
    pub(super) fn log_level(&self) -> Option<LogLevel> {
        self.global
            .as_ref()
            .and_then(|global| global.log_level)
    }

    ///
    /// ログ出力先へのアクセサ
    ///
    pub(super) fn log_output(&self) -> Option<PathBuf> {
        self.global
            .as_ref()
            .and_then(|global| global.log_output.as_ref())
            .cloned()
    }

    ///
    /// 使用するエディタ名へのアクセサ
    ///
    /// # 戻り値
    /// エディタ名が設定されている場合はパス情報を`Some()`でラップして返す。
    ///
    pub(super) fn editor(&self) -> Option<String> {
        self.global
            .as_ref()
            .and_then(|global| global.editor.as_ref())
            .cloned()
    }

    ///
    /// queryサブコマンドのマッチモードへのアクセサ
    ///
    pub(super) fn query_match_mode(&self) -> Option<MatchMode> {
        self.query
            .as_ref()
            .and_then(|query| query.match_mode.clone())
    }

    ///
    /// searchサブコマンドでサービス名を検索対象に含めるかのアクセサ
    ///
    pub(super) fn search_with_service_name(&self) -> Option<bool> {
        self.search
            .as_ref()
            .and_then(|search| search.with_service_name)
    }

    ///
    /// searchサブコマンドのマッチモードへのアクセサ
    ///
    pub(super) fn search_match_mode(&self) -> Option<MatchMode> {
        self.search
            .as_ref()
            .and_then(|search| search.match_mode.clone())
    }

    ///
    /// searchサブコマンドの検索対象プロパティへのアクセサ
    ///
    pub(super) fn search_target_properties(&self) -> Option<Vec<String>> {
        self.search
            .as_ref()
            .and_then(|search| search.target_properties.clone())
    }

    ///
    /// listサブコマンドでタグをAND解釈するか否かへのアクセサ
    ///
    pub(super) fn list_tag_and(&self) -> Option<bool> {
        self.list.as_ref().and_then(|list| list.tag_and)
    }

    ///
    /// listサブコマンドのソートモードへのアクセサ
    ///
    pub(super) fn list_sort_mode(&self) -> Option<ListSortMode> {
        self.list.as_ref().and_then(|list| list.sort_mode.clone())
    }

    ///
    /// listサブコマンドでソートを逆順にするか否かへのアクセサ
    ///
    pub(super) fn list_reverse_sort(&self) -> Option<bool> {
        self.list.as_ref().and_then(|list| list.reverse_sort)
    }

    ///
    /// listサブコマンドで削除済みエントリも表示するか否かへのアクセサ
    ///
    pub(super) fn list_with_removed(&self) -> Option<bool> {
        self.list.as_ref().and_then(|list| list.with_removed)
    }

    ///
    /// tagsサブコマンドで件数を表示するか否かへのアクセサ
    ///
    pub(super) fn tags_with_number(&self) -> Option<bool> {
        self.tags.as_ref().and_then(|tags| tags.with_number)
    }

    ///
    /// tagsサブコマンドのソートモードへのアクセサ
    ///
    pub(super) fn tags_sort_mode(&self) -> Option<TagsSortMode> {
        self.tags.as_ref().and_then(|tags| tags.sort_mode.clone())
    }

    ///
    /// tagsサブコマンドでソートを逆順にするか否かへのアクセサ
    ///
    pub(super) fn tags_reverse_sort(&self) -> Option<bool> {
        self.tags.as_ref().and_then(|tags| tags.reverse_sort)
    }

    ///
    /// tagsサブコマンドのマッチモードへのアクセサ
    ///
    pub(super) fn tags_match_mode(&self) -> Option<MatchMode> {
        self.tags.as_ref().and_then(|tags| tags.match_mode)
    }

    ///
    /// コンフィギュレーション情報の保存
    ///
    /// # 戻り値
    /// 保存に成功した場合は`Ok(())`を返す。失敗した場合はエラー情報を`Err()`で
    /// ラップして返す。
    ///
    #[allow(dead_code)]
    pub(super) fn save<P>(&self, path: P) -> Result<()>
    where 
        P: AsRef<Path>
    {
        if let Err(err) = std::fs::write(path, &toml::to_string(self)?) {
            Err(anyhow!("write config error: {}", err))
        } else {
            Ok(())
        }
    }
}

// Defaultトレイトの実装
impl Default for Config {
    fn default() -> Self {
        Self {
            global: Some(GlobalInfo {
                db_path: Some(default_db_path()),
                log_level: Some(LogLevel::Info),
                log_output: Some(default_log_path()),
                editor: Some(DEFAULT_EDITOR.to_string()),
            }),
            query: Some(QueryInfo {
                match_mode: Some(MatchMode::Contains),
            }),
            search: Some(SearchInfo {
                with_service_name: Some(false),
                match_mode: Some(MatchMode::Contains),
                target_properties: Some(vec![]),
            }),
            list: Some(ListInfo {
                tag_and: Some(false),
                sort_mode: Some(ListSortMode::Default),
                reverse_sort: Some(false),
                with_removed: Some(false),
            }),
            tags: Some(TagsInfo {
                with_number: Some(false),
                sort_mode: Some(TagsSortMode::Default),
                reverse_sort: Some(false),
                match_mode: Some(MatchMode::Contains),
            }),
        }
    }
}

///
/// グローバル設定を格納する構造体
///
#[derive(Debug, Deserialize, Serialize)]
struct GlobalInfo {
    /// データベースファイルへのパス
    db_path: Option<PathBuf>,

    /// ログレベル
    log_level: Option<LogLevel>,

    /// ログの出力先
    log_output: Option<PathBuf>,

    /// 使用するエディタ
    editor: Option<String>,
}

///
/// コンフィギュレーション情報の読み込み
///
pub(super) fn load<P>(path: P) -> Result<Config>
where 
    P: AsRef<Path>
{
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

///
/// queryサブコマンドの設定情報
///
#[derive(Debug, Deserialize, Serialize)]
struct QueryInfo {
    /// マッチモード
    match_mode: Option<MatchMode>,
}

///
/// searchサブコマンドの設定情報
///
#[derive(Debug, Deserialize, Serialize)]
struct SearchInfo {
    /// サービス名を検索対象に含めるか
    with_service_name: Option<bool>,

    /// マッチモード
    match_mode: Option<MatchMode>,

    /// 検索対象とするプロパティ名のリスト
    target_properties: Option<Vec<String>>,
}

///
/// listサブコマンドの設定情報
///
#[derive(Debug, Deserialize, Serialize)]
struct ListInfo {
    /// 複数タグ指定時にAND評価を行うか
    tag_and: Option<bool>,

    /// ソートモード
    sort_mode: Option<ListSortMode>,

    /// ソート順を逆順にするか
    reverse_sort: Option<bool>,

    /// 削除済みエントリも表示するか
    with_removed: Option<bool>,
}

///
/// listサブコマンドのソートモードを表す列挙子
///
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ListSortMode {
    /// デフォルト(エントリIDソート)
    Default,

    /// サービス名でソート
    ServiceName,

    /// 更新日時でソート
    LastUpdate,
}

///
/// tagsサブコマンドの設定情報
///
#[derive(Debug, Deserialize, Serialize)]
struct TagsInfo {
    /// 件数も表示するか
    with_number: Option<bool>,

    /// ソートモード
    sort_mode: Option<TagsSortMode>,

    /// ソートを逆順にするか
    reverse_sort: Option<bool>,

    /// マッチモード
    match_mode: Option<MatchMode>,
}

///
/// tagsサブコマンドのソートモードを表す列挙子
///
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum TagsSortMode {
    /// デフォルト(タグ名でソート)
    Default,

    /// 登録件数でソート
    NumberOfRegist,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{default_db_path, default_log_path, LogLevel, DEFAULT_EDITOR};

    #[test]
    fn default_config_values() {
        let config = Config::default();

        assert_eq!(config.db_path(), Some(default_db_path()));
        assert_eq!(config.log_level(), Some(LogLevel::Info));
        assert_eq!(config.log_output(), Some(default_log_path()));
        assert_eq!(config.editor(), Some(DEFAULT_EDITOR.to_string()));

        assert_eq!(
            config.query_match_mode(),
            Some(MatchMode::Contains)
        );

        assert_eq!(
            config.search_with_service_name(),
            Some(false)
        );
        assert_eq!(
            config.search_match_mode(),
            Some(MatchMode::Contains)
        );
        assert_eq!(
            config.search_target_properties(),
            Some(vec![])
        );

        assert_eq!(config.list_tag_and(), Some(false));
        assert_eq!(
            config.list_sort_mode(),
            Some(ListSortMode::Default)
        );
        assert_eq!(config.list_reverse_sort(), Some(false));
        assert_eq!(config.list_with_removed(), Some(false));

        assert_eq!(config.tags_with_number(), Some(false));
        assert_eq!(
            config.tags_sort_mode(),
            Some(TagsSortMode::Default)
        );
        assert_eq!(config.tags_reverse_sort(), Some(false));
        assert_eq!(config.tags_match_mode(), Some(MatchMode::Contains));
    }

    #[test]
    fn deserialize_config_from_toml() {
        let toml = r#"
[global]
db_path = "./db.redb"
log_level = "off"
log_output = "./logs"
editor = "vim"

[query]
match_mode = "regex"

[search]
with_service_name = true
match_mode = "exact"
target_properties = ["user", "pass"]

[list]
tag_and = true
sort_mode = "last_update"
reverse_sort = true
with_removed = true

[tags]
with_number = true
sort_mode = "number_of_regist"
reverse_sort = true
match_mode = "fuzzy"
"#;

        let config: Config = toml::from_str(toml).expect("toml parse failed");

        assert_eq!(
            config.db_path(),
            Some(PathBuf::from("./db.redb"))
        );
        assert_eq!(config.log_level(), Some(LogLevel::None));
        assert_eq!(config.log_output(), Some(PathBuf::from("./logs")));
        assert_eq!(config.editor(), Some("vim".to_string()));

        assert_eq!(
            config.query_match_mode(),
            Some(MatchMode::Regex)
        );

        assert_eq!(
            config.search_with_service_name(),
            Some(true)
        );
        assert_eq!(config.search_match_mode(), Some(MatchMode::Exact));
        assert_eq!(
            config.search_target_properties(),
            Some(vec!["user".to_string(), "pass".to_string()])
        );

        assert_eq!(config.list_tag_and(), Some(true));
        assert_eq!(
            config.list_sort_mode(),
            Some(ListSortMode::LastUpdate)
        );
        assert_eq!(config.list_reverse_sort(), Some(true));
        assert_eq!(config.list_with_removed(), Some(true));

        assert_eq!(config.tags_with_number(), Some(true));
        assert_eq!(
            config.tags_sort_mode(),
            Some(TagsSortMode::NumberOfRegist)
        );
        assert_eq!(config.tags_reverse_sort(), Some(true));
        assert_eq!(config.tags_match_mode(), Some(MatchMode::Fuzzy));
    }
}
