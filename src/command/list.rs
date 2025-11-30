/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! listサブコマンドの実装
//!

use std::cell::RefCell;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::cmd_args::{Options, ListOpts};
use crate::database::{EntryManager, types::ServiceId};
use super::CommandContext;

///
/// listサブコマンドのコンテキスト情報をパックした構造体
///
struct ListCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// フィルタ対象タグ
    target_tags: Vec<String>,

    /// タグをAND条件で解釈するか
    tag_and: bool,
}

impl ListCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &ListOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            target_tags: sub_opts.target_tags(),
            tag_and: sub_opts.is_tag_and(),
        })
    }

    ///
    /// タグフィルタに応じて対象ID集合を取得
    ///
    fn collect_ids(&self) -> Result<Vec<ServiceId>> {
        let mgr = self.manager.borrow();

        if self.target_tags.is_empty() {
            return mgr.all_service();
        }

        if self.tag_and {
            // AND条件: 最初のタグ集合をベースに積集合を計算
            let mut iter = self.target_tags.iter();
            let first = iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("no tags"))?;
            let mut set: BTreeSet<ServiceId> =
                mgr.tagged_service(first)?
                    .into_iter()
                    .collect();

            for tag in iter {
                let ids: BTreeSet<ServiceId> =
                    mgr.tagged_service(tag)?
                        .into_iter()
                        .collect();
                set = set.intersection(&ids).cloned().collect();
                if set.is_empty() {
                    break;
                }
            }

            Ok(set.into_iter().collect())
        } else {
            // OR条件: 各タグの集合を和集合にする
            let mut set = BTreeSet::new();

            for tag in &self.target_tags {
                for id in mgr.tagged_service(tag)? {
                    set.insert(id);
                }
            }

            Ok(set.into_iter().collect())
        }
    }
}

// CommandContextトレイトの実装
impl CommandContext for ListCommandContext {
    fn exec(&self) -> Result<()> {
        // 対象IDを取得
        let mut ids = self.collect_ids()?;
        ids.sort();

        let mut mgr = self.manager.borrow_mut();

        for id in ids {
            if let Some(entry) = mgr.get(&id)? {
                println!("{}\t{}", id.to_string(), entry.service());
            }
        }

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &ListOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(ListCommandContext::new(opts, sub_opts)?))
}
