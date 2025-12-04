/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! サブコマンドの処理を提供するモジュール
//!

pub(crate) mod add;
pub(crate) mod edit;
pub(crate) mod editor;
pub(crate) mod export;
pub(crate) mod import;
pub(crate) mod list;
pub(crate) mod tags;
pub(crate) mod search;
pub(crate) mod query;
pub(crate) mod matcher;
pub(crate) mod prompt;
pub(crate) mod util;
pub(crate) mod remove;

use anyhow::Result;

///
/// コマンドコンテキスト集約するトレイト
///
pub(crate) trait CommandContext {
    ///
    /// サブコマンドの実行
    ///
    fn exec(&self) -> Result<()>;
}
