/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

use std::cell::RefCell;

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::cmd_args::{Options, TagsOpts};
use crate::command::matcher::Matcher;
use crate::database::EntryManager;
use super::CommandContext;

#[derive(Serialize, Clone)]
struct TagInfo {
    tag: String,
    count: usize,
}

///
/// tagsサブコマンドのコンテキスト情報
///
struct TagsCommandContext {
    /// データベースマネージャ
    manager: RefCell<EntryManager>,
    /// コマンドオプション
    opts: TagsOpts,
    /// JSON出力フラグ
    json_output: bool,
}

impl TagsCommandContext {
    ///
    /// コンテキスト生成
    ///
    fn new(opts: &Options, sub_opts: &TagsOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            opts: sub_opts.clone(),
            json_output: opts.json(),
        })
    }

    ///
    /// 全タグの一覧を収集
    ///
    fn collect_tags(&self) -> Result<Vec<TagInfo>> {
        let list = self.manager.borrow_mut().all_tags()?;
        Ok(list
            .into_iter()
            .map(|(tag, count)| TagInfo { tag, count })
            .collect())
    }

    ///
    /// キー指定がある場合はマッチモードに従ってフィルタ
    ///
    fn filter(&self, tags: Vec<TagInfo>) -> Result<Vec<TagInfo>> {
        if let Some(key) = self.opts.key() {
            let matcher = Matcher::new(self.opts.match_mode(), key)?;
            let filtered = tags
                .into_iter()
                .filter(|t| matcher.is_match(&t.tag).unwrap_or(false))
                .collect();
            Ok(filtered)
        } else {
            Ok(tags)
        }
    }

    ///
    /// オプションに従ってソート
    ///
    fn sort(&self, mut tags: Vec<TagInfo>) -> Vec<TagInfo> {
        if self.opts.sort_by_number() {
            tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        } else {
            tags.sort_by(|a, b| a.tag.cmp(&b.tag));
        }

        if self.opts.reverse_sort() {
            tags.reverse();
        }

        tags
    }

    ///
    /// 出力（JSON/テキスト）
    ///
    fn print(&self, tags: &[TagInfo]) -> Result<()> {
        if self.json_output {
            let json = serde_json::to_string_pretty(tags)?;
            println!("{json}");
        } else {
            for t in tags {
                if self.opts.number() {
                    println!("{}\t{}", t.tag, t.count);
                } else {
                    println!("{}", t.tag);
                }
            }
        }
        Ok(())
    }
}

impl CommandContext for TagsCommandContext {
    fn exec(&self) -> Result<()> {
        let tags = self.collect_tags()?;
        if tags.is_empty() {
            return Err(anyhow!("付与されたタグはありません"));
        }

        let tags = self.filter(tags)?;
        if tags.is_empty() {
            return Err(anyhow!("付与されたタグはありません"));
        }

        let tags = self.sort(tags);
        self.print(&tags)
    }
}

pub(crate) fn build_context(opts: &Options, sub_opts: &TagsOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(TagsCommandContext::new(opts, sub_opts)?))
}
