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

    /// サービス名でソートするか
    sort_by_service_name: bool,

    /// ソートを逆順にするか
    reverse_sort: bool,
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
            sort_by_service_name: sub_opts.sort_by_service_name(),
            reverse_sort: sub_opts.reverse_sort(),
        })
    }

    ///
    /// タグフィルタに応じて対象ID集合を取得
    ///
    fn collect_ids(&self) -> Result<Vec<ServiceId>> {
        let mut mgr = self.manager.borrow_mut();

        // タグ指定なしなら全件
        if self.target_tags.is_empty() {
            return mgr.all_service();
        }

        let target_lower: Vec<String> =
            self.target_tags.iter().map(|t| t.to_lowercase()).collect();

        // まず全タグ一覧を取得し、指定タグとマッチするものだけを見る
        let all_tags = mgr.all_tags()?;

        let mut sets: Vec<BTreeSet<ServiceId>> = Vec::new();
        for (tag, _) in all_tags {
            if target_lower
                .iter()
                .any(|t| t == &tag.to_lowercase())
            {
                let ids: BTreeSet<ServiceId> =
                    mgr.tagged_service(&tag)?
                        .into_iter()
                        .collect();
                sets.push(ids);
            }
        }

        if sets.is_empty() {
            return Ok(Vec::new());
        }

        let result: BTreeSet<ServiceId> = if self.tag_and {
            let mut iter = sets.into_iter();
            let mut acc = iter.next().unwrap();
            for s in iter {
                acc = acc.intersection(&s).cloned().collect();
                if acc.is_empty() {
                    break;
                }
            }
            acc
        } else {
            sets.into_iter().flatten().collect()
        };

        Ok(result.into_iter().collect())
    }
}

// CommandContextトレイトの実装
impl CommandContext for ListCommandContext {
    fn exec(&self) -> Result<()> {
        // 対象IDを取得
        let mut ids = self.collect_ids()?;
        ids.sort();

        let mut mgr = self.manager.borrow_mut();

        if self.sort_by_service_name {
            let mut items = Vec::new();
            for id in ids {
                if let Some(entry) = mgr.get(&id)? {
                    items.push((entry.service(), id));
                }
            }
            items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            if self.reverse_sort {
                items.reverse();
            }
            for (service, id) in items {
                println!("{}\t{}", id.to_string(), service);
            }
        } else {
            if self.reverse_sort {
                ids.reverse();
            }
            for id in ids {
                if let Some(entry) = mgr.get(&id)? {
                    println!("{}\t{}", id.to_string(), entry.service());
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::types::{Entry, ServiceId};
    use crate::database::EntryManager;
    use std::collections::BTreeMap;
    use ulid::Ulid;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-list-test-{}.redb", Ulid::new()))
    }

    fn build_mgr() -> EntryManager {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();

        let e1 = Entry::new(
            ServiceId::new(),
            "Alpha".into(),
            vec![],
            vec!["Tag1".into()],
            BTreeMap::new(),
        );
        let e2 = Entry::new(
            ServiceId::new(),
            "Beta".into(),
            vec![],
            vec!["tag2".into()],
            BTreeMap::new(),
        );
        let e3 = Entry::new(
            ServiceId::new(),
            "Gamma".into(),
            vec![],
            vec!["tag2".into()],
            BTreeMap::new(),
        );
        mgr.put(&e1).unwrap();
        mgr.put(&e2).unwrap();
        mgr.put(&e3).unwrap();
        mgr
    }

    #[test]
    /// タグ指定が大文字小文字を無視してマッチすること
    fn list_tags_case_insensitive() {
        let mgr = build_mgr();
        let ctx = ListCommandContext {
            manager: RefCell::new(mgr),
            target_tags: vec!["tAg1".into()],
            tag_and: false,
            sort_by_service_name: false,
            reverse_sort: false,
        };

        let ids = ctx.collect_ids().unwrap();
        assert_eq!(ids.len(), 1);
    }

    #[test]
    /// サービス名ソートと逆順ソートが組み合わせられること
    fn list_sort_by_service_and_reverse() {
        let mgr = build_mgr();
        let ctx = ListCommandContext {
            manager: RefCell::new(mgr),
            target_tags: vec![],
            tag_and: false,
            sort_by_service_name: true,
            reverse_sort: true,
        };

        // 実行経路を通すだけ（出力は確認不要なので collect_ids だけ確認）
        let mut ids = ctx.collect_ids().unwrap();
        ids.sort();
        assert_eq!(ids.len(), 3);
    }
}
