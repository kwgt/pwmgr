/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

use anyhow::{Context, Result};
use strsim::jaro_winkler;

use crate::cmd_args::MatchMode;

///
/// 文字列照合方式を表現するマッチャ
///
#[derive(Clone)]
pub(crate) enum Matcher {
    /// 大文字小文字無視の完全一致
    Exact(String),

    /// 大文字小文字無視の部分一致
    Contains(String),

    /// 正規表現マッチ
    Regex(regex::Regex),

    /// Jaro-Winklerによるファジーマッチ
    Fuzzy(String),
}

impl Matcher {
    ///
    /// 指定されたモードとキーからマッチャを生成する
    ///
    pub(crate) fn new(mode: MatchMode, key: String) -> Result<Self> {
        match mode {
            MatchMode::Exact => Ok(Self::Exact(key.to_lowercase())),
            MatchMode::Contains => Ok(Self::Contains(key.to_lowercase())),
            MatchMode::Regex => Ok(Self::Regex(
                regex::Regex::new(&key)
                    .with_context(|| format!("正規表現の解釈に失敗しました: {key}"))?
            )),
            MatchMode::Fuzzy => Ok(Self::Fuzzy(key.to_lowercase())),
        }
    }

    ///
    /// 与えられた文字列がマッチするかを判定する
    ///
    pub(crate) fn is_match(&self, target: &str) -> Result<bool> {
        match self {
            Self::Exact(k) => Ok(target.to_lowercase() == *k),
            Self::Contains(k) => Ok(target.to_lowercase().contains(k)),
            Self::Regex(re) => Ok(re.is_match(target)),
            Self::Fuzzy(k) => {
                let score = jaro_winkler(k, &target.to_lowercase());
                Ok(score >= 0.85)
            }
        }
    }
}
