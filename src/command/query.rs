/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! queryサブコマンドの実装
//!

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::cmd_args::{QueryOpts, Options};
use crate::database::types::{Entry, ServiceId};
use crate::database::EntryManager;
use super::{matcher::Matcher, CommandContext};

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct QueryCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// サブコマンドオプション
    opts: QueryOpts,

    /// JSON出力フラグ
    json_output: bool,
}

impl QueryCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &QueryOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            opts: sub_opts.clone(),
            json_output: opts.json(),
        })
    }

    ///
    /// キーがサービス名／別名にヒットするエントリを列挙する
    ///
    fn search_by_string(&self, matcher: &Matcher) -> Result<Vec<Entry>> {
        let mut results = Vec::new();

        let ids = {
            let mgr = self.manager.borrow();
            mgr.all_service()?
        };

        for id in ids {
            let entry_opt = {
                let mut mgr = self.manager.borrow_mut();
                mgr.get(&id)?
            };

            if let Some(entry) = entry_opt {
                let service_hit = matcher.is_match(&entry.service())?;
                let alias_hit = entry.aliases()
                    .iter()
                    .any(|alias| matcher.is_match(alias).unwrap_or(false));

                if service_hit || alias_hit {
                    results.push(entry);
                }
            }
        }

        Ok(results)
    }

    ///
    /// テキストでエントリを出力する
    ///
    fn print_entry(entry: &Entry) -> Result<()> {
        let mut buf = String::new();
        writeln!(&mut buf, "id: {}", entry.id())?;
        writeln!(&mut buf, "service: {}", entry.service())?;

        let props: BTreeMap<String, String> = entry.properties();
        writeln!(&mut buf, "properties:")?;
        if props.is_empty() {
            writeln!(&mut buf, "  (none)")?;
        } else {
            for (k, v) in props {
                writeln!(&mut buf, "  {k}: {v}")?;
            }
        }

        print!("{buf}");
        Ok(())
    }

    ///
    /// テキストでエントリを出力する
    ///
    fn print_full_entry(entry: &Entry) -> Result<()> {
        let mut buf = String::new();
        writeln!(&mut buf, "id: {}", entry.id())?;
        writeln!(&mut buf, "service: {}", entry.service())?;

        let aliases = entry.aliases();
        if aliases.is_empty() {
            writeln!(&mut buf, "aliases: (none)")?;
        } else {
            writeln!(&mut buf, "aliases: {}", aliases.join(", "))?;
        }

        let tags = entry.tags();
        if tags.is_empty() {
            writeln!(&mut buf, "tags: (none)")?;
        } else {
            writeln!(&mut buf, "tags: {}", tags.join(", "))?;
        }

        let props: BTreeMap<String, String> = entry.properties();
        writeln!(&mut buf, "properties:")?;
        if props.is_empty() {
            writeln!(&mut buf, "  (none)")?;
        } else {
            for (k, v) in props {
                writeln!(&mut buf, "  {k}: {v}")?;
            }
        }

        print!("{buf}");
        Ok(())
    }

    ///
    /// JSON出力用のエントリ表現を構築する
    ///
    fn to_display_entry(entry: &Entry) -> DisplayEntry {
        DisplayEntry {
            id: entry.id().to_string(),
            service: entry.service(),
            aliases: entry.aliases(),
            tags: entry.tags(),
            properties: entry.properties(),
        }
    }
}

// CommandContextトレイトの実装
impl CommandContext for QueryCommandContext {
    fn exec(&self) -> Result<()> {
        let key = self.opts.key();
        let matcher = Matcher::new(self.opts.match_mode(), key.clone())?;

        let mut hits: Vec<Entry> = Vec::new();

        // まずはULIDとして解釈できる場合にID検索を試みる
        if let Ok(id) = ServiceId::from_string(&key) {
            if let Some(entry) = self.manager.borrow_mut().get(&id)? {
                hits.push(entry);
            }
        }

        // IDで見つからない場合は文字列検索
        if hits.is_empty() {
            hits = self.search_by_string(&matcher)?;
        }

        if hits.is_empty() {
            return Err(anyhow!("該当するエントリが見つかりませんでした"));
        }

        if self.json_output {
            let display: Vec<DisplayEntry> = hits.iter()
                .map(Self::to_display_entry)
                .collect();
            let json = serde_json::to_string_pretty(&display)?;
            println!("{json}");
        } else {
            for (idx, entry) in hits.iter().enumerate() {
                if hits.len() != 0 {
                    println!("----")
                }

                if self.opts.is_full() {
                    Self::print_full_entry(entry)?;
                } else {
                    Self::print_entry(entry)?;
                }

                if idx + 1 != hits.len() {
                    println!();
                }
            }
        }

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &QueryOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(QueryCommandContext::new(opts, sub_opts)?))
}

#[derive(Serialize)]
struct DisplayEntry {
    id: String,
    service: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    properties: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_args::MatchMode;
    use crate::database::types::ServiceId;
    use crate::database::EntryManager;
    use ulid::Ulid;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-query-test-{}.redb", Ulid::new()))
    }

    fn build_mgr_with_entries() -> EntryManager {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();

        // service: "Alpha", aliases: ["alp"]
        let entry1 = Entry::new(
            ServiceId::new(),
            "Alpha".to_string(),
            vec!["alp".into()],
            vec!["t1".into()],
            BTreeMap::from([("user".into(), "alice".into())]),
        );

        // service: "Beta", aliases: ["bta"], typo-friendly
        let entry2 = Entry::new(
            ServiceId::new(),
            "Beta".to_string(),
            vec!["bta".into()],
            vec!["t2".into()],
            BTreeMap::from([("user".into(), "bob".into())]),
        );

        mgr.put(&entry1).unwrap();
        mgr.put(&entry2).unwrap();

        // スムーズにID検索確認できるようIDを返す
        mgr
    }

    fn build_ctx(mode: MatchMode, key: &str, json: bool) -> QueryCommandContext {
        let opts = QueryOpts::new_for_test(true, mode, key.to_string());
        QueryCommandContext {
            manager: RefCell::new(build_mgr_with_entries()),
            opts,
            json_output: json,
        }
    }

    ///
    /// containsモードでは部分一致でヒットすることを確認
    ///
    #[test]
    fn search_contains_hits_alias() {
        let ctx = build_ctx(MatchMode::Contains, "alp", false);
        let matcher = Matcher::new(
            ctx.opts.match_mode(),
            ctx.opts.key()
        ).unwrap();
        let hits = ctx.search_by_string(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Alpha".to_string());
    }

    ///
    /// exactモードでは完全一致のみヒットし、部分一致は除外されることを確認
    ///
    #[test]
    fn search_exact_requires_full_match() {
        // 大文字小文字は無視して完全一致する
        let ctx = build_ctx(MatchMode::Exact, "ALPHA", false);
        let matcher = Matcher::new(
            ctx.opts.match_mode(),
            ctx.opts.key()
        ).unwrap();

        let hits = ctx.search_by_string(&matcher).unwrap();
        assert_eq!(hits.len(), 1);

        // 部分一致やタイプミスはヒットしない
        let ctx_no_hit = build_ctx(MatchMode::Exact, "alp", false);
        let matcher = Matcher::new(
            ctx_no_hit.opts.match_mode(),
            ctx_no_hit.opts.key()
        ).unwrap();

        let hits = ctx_no_hit.search_by_string(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
    }

    ///
    /// 正規表現モードでサービス名にマッチすることを確認
    ///
    #[test]
    fn search_regex_hits() {
        let ctx = build_ctx(MatchMode::Regex, "^Be.*$", false);
        let matcher = Matcher::new(
            ctx.opts.match_mode(),
            ctx.opts.key()
        ).unwrap();
        let hits = ctx.search_by_string(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Beta".to_string());
    }

    ///
    /// ファジーモードで軽微なタイプミスを許容してヒットすることを確認
    ///
    #[test]
    fn search_fuzzy_hits_typo() {
        // "Btea" should fuzzy-match "Beta" with jaro-winkler >= 0.85
        let ctx = build_ctx(MatchMode::Fuzzy, "Btea", false);
        let matcher = Matcher::new(
            ctx.opts.match_mode(),
            ctx.opts.key()
        ).unwrap();
        let hits = ctx.search_by_string(&matcher).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].service(), "Beta".to_string());
    }
}
