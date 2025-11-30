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

use super::default_db_path;
use super::DEFAULT_EDITOR;

///
/// コンフィギュレーションデータを集約する構造体
///
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Config {
    global: Option<GlobalInfo>,
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
                editor: Some(DEFAULT_EDITOR.to_string()),
            })
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
