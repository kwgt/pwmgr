/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! searchサブコマンドの実装
//!

use std::cell::RefCell;

use anyhow::{anyhow, Result};

use crate::cmd_args::{SearchOpts, Options};
use crate::command::matcher::Matcher;
use crate::database::types::Entry;
use crate::database::{EntryManager, TransactionReader, TransactionReadable};
use super::CommandContext;

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct SearchCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// サブコマンドオプション
    opts: SearchOpts,
}

impl SearchCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &SearchOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            opts: sub_opts.clone(),
        })
    }

    ///
    /// タグフィルタを適用する（AND/ORは将来拡張を想定し、現状OR相当で処理）
    ///
    fn tag_filter(entry: &Entry, tags: &[String]) -> bool {
        if tags.is_empty() {
            return true;
        }

        tags.iter().any(|t| entry.tags().contains(t))
    }

    ///
    /// サービス名/別名でマッチするか
    ///
    fn service_or_alias_hit(entry: &Entry, matcher: &Matcher) -> Result<bool> {
        if matcher.is_match(&entry.service())? {
            return Ok(true);
        }

        Ok(entry.aliases()
            .iter()
            .any(|alias| matcher.is_match(alias).unwrap_or(false)))
    }

    ///
    /// プロパティでマッチするか
    ///
    fn property_hit(entry: &Entry, matcher: &Matcher, target_props: &[String]) -> Result<bool> {
        if target_props.is_empty() {
            return Ok(false);
        }

        for (k, v) in entry.properties() {
            if target_props.contains(&k) && matcher.is_match(&v)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    ///
    /// ヒット一覧を収集する
    ///
    fn collect_hits_with_reader(
        &self,
        matcher: &Matcher,
        reader: &TransactionReader,
    ) -> Result<Vec<Entry>> {
        let include_service = self.opts.is_include_service();
        let target_props = self.opts.target_properties();
        let target_tags = self.opts.target_tags();

        let ids = reader.all_service_filtered(true)?;

        let mut hits = Vec::new();

        for id in ids {
            if let Some(entry) = reader.get(&id)? {
                if !Self::tag_filter(&entry, &target_tags) {
                    continue;
                }

                let mut hit = false;
                if include_service {
                    hit |= Self::service_or_alias_hit(&entry, matcher)?;
                }

                if !hit {
                    hit |= Self::property_hit(&entry, matcher, &target_props)?;
                }

                if hit {
                    hits.push(entry);
                }
            }
        }

        Ok(hits)
    }

    ///
    /// ヒット一覧を収集する（トランザクションラッパ）
    ///
    fn collect_hits(&self, matcher: &Matcher) -> Result<Vec<Entry>> {
        self.manager
            .borrow()
            .with_read_transaction(|reader| {
                self.collect_hits_with_reader(matcher, reader)
            })
    }

    ///
    /// テキストでエントリを出力する
    ///
    fn print_entry(entry: &Entry) -> Result<()> {
        println!("{}\t{}", entry.id(),  entry.service());
        Ok(())
    }
}

// CommandContextトレイトの実装
impl CommandContext for SearchCommandContext {
    fn exec(&self) -> Result<()> {
        let matcher = Matcher::new(self.opts.match_mode(), self.opts.key())?;
        let hits = self.collect_hits(&matcher)?;

        if hits.is_empty() {
            return Err(anyhow!("該当するエントリが見つかりませんでした"));
        }

        for entry in hits.iter() {
            Self::print_entry(entry)?;
        }

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &SearchOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(SearchCommandContext::new(opts, sub_opts)?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;
    use crate::cmd_args::MatchMode;
    use crate::database::types::ServiceId;
    use crate::database::EntryManager;
    use ulid::Ulid;

    ///
    /// テンポラリDBファイルのパスを生成する
    ///
    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-search-test-{}.redb", Ulid::new()))
    }

    ///
    /// テスト用のエントリを投入したエントリマネージャを生成する
    ///
    fn build_mgr_with_entries() -> EntryManager {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();

        let e1 = Entry::new(
            ServiceId::new(),
            "Alpha".to_string(),
            vec!["alp".into()],
            vec!["t1".into()],
            BTreeMap::from([("user".into(), "alice".into())]),
        );

        let e2 = Entry::new(
            ServiceId::new(),
            "Beta".to_string(),
            vec!["bta".into()],
            vec!["t2".into()],
            BTreeMap::from([("user".into(), "bob".into())]),
        );

        mgr.put(&e1).unwrap();
        mgr.put(&e2).unwrap();
        mgr
    }

    ///
    /// テスト用のコンテキストを構築する
    ///
    fn build_ctx(opts: SearchOpts) -> SearchCommandContext {
        SearchCommandContext {
            manager: RefCell::new(build_mgr_with_entries()),
            opts,
        }
    }

    ///
    /// サービス名/別名がcontainsモードでヒットすることを確認
    ///
    #[test]
    fn search_hits_service_contains() {
        let opts = SearchOpts::new_for_test(
            true,
            vec![],
            vec![],
            MatchMode::Contains,
            "alp",
        );
        let ctx = build_ctx(opts);
        let matcher = Matcher::new(ctx.opts.match_mode(), ctx.opts.key()).unwrap();
        let hits = ctx.collect_hits(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Alpha".to_string());
    }

    ///
    /// プロパティ指定のみでヒットすることを確認
    ///
    #[test]
    fn search_hits_property_only() {
        let opts = SearchOpts::new_for_test(
            false,                  // service対象外
            vec![],                 // tagsなし
            vec!["user".into()],    // userプロパティのみ対象
            MatchMode::Exact,
            "bob",
        );
        let ctx = build_ctx(opts);
        let matcher = Matcher::new(ctx.opts.match_mode(), ctx.opts.key()).unwrap();
        let hits = ctx.collect_hits(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Beta".to_string());
    }

    ///
    /// タグフィルタで対象外となる場合ヒットしないことを確認
    ///
    #[test]
    fn search_respects_tag_filter() {
        let opts = SearchOpts::new_for_test(
            true,
            vec!["t3".into()], // どのエントリにも存在しないタグ
            vec![],
            MatchMode::Exact,
            "Alpha",
        );
        let ctx = build_ctx(opts);
        let matcher = Matcher::new(ctx.opts.match_mode(), ctx.opts.key()).unwrap();
        let hits = ctx.collect_hits(&matcher).unwrap();
        assert_eq!(hits.len(), 0);
    }

    ///
    /// 正規表現モードでサービス名にマッチすることを確認
    ///
    #[test]
    fn search_regex_hits() {
        let opts = SearchOpts::new_for_test(
            true,
            vec![],
            vec![],
            MatchMode::Regex,
            "^Be.*$",
        );
        let ctx = build_ctx(opts);
        let matcher = Matcher::new(ctx.opts.match_mode(), ctx.opts.key()).unwrap();
        let hits = ctx.collect_hits(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Beta".to_string());
    }

    ///
    /// ファジーモードで軽微なタイプミスを許容してヒットすることを確認
    ///
    #[test]
    fn search_fuzzy_hits_typo() {
        let opts = SearchOpts::new_for_test(
            true,
            vec![],
            vec![],
            MatchMode::Fuzzy,
            "Btea",
        );
        let ctx = build_ctx(opts);
        let matcher = Matcher::new(ctx.opts.match_mode(), ctx.opts.key()).unwrap();
        let hits = ctx.collect_hits(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Beta".to_string());
    }
}
