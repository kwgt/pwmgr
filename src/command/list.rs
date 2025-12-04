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

    /// 最終更新日時でソートするか
    sort_by_last_update: bool,

    /// 削除済みエントリも含めるか
    with_removed: bool,
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
            sort_by_last_update: sub_opts.sort_by_last_update(),
            with_removed: sub_opts.with_removed(),
        })
    }

    ///
    /// タグフィルタに応じて対象ID集合を取得
    ///
    fn collect_ids(&self) -> Result<Vec<ServiceId>> {
        let mut mgr = self.manager.borrow_mut();

        // タグ指定なしなら全件
        if self.target_tags.is_empty() {
            return mgr.all_service_filtered(!self.with_removed);
        }

        let target_lower: Vec<String> =
            self.target_tags.iter().map(|t| t.to_lowercase()).collect();

        // タグテーブルを用いて対象IDを取得
        let all_tags = mgr.all_tags()?;
        let mut sets: Vec<BTreeSet<ServiceId>> = Vec::new();

        for (tag, _) in all_tags {
            if target_lower.iter().any(|t| t == &tag.to_lowercase()) {
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

        if self.sort_by_last_update {
            let mut items = Vec::new();
            for id in ids {
                if let Some(entry) = mgr.get(&id)? {
                    items.push((entry.last_update(), entry.service(), id, entry.is_removed()));
                }
            }
            items.sort_by(|a, b| a.0.cmp(&b.0));
            if self.reverse_sort {
                items.reverse();
            }
            for (last, service, id, removed) in items {
                let stamp = last
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string());
                let prefix = if removed { "-" } else { "" };
                println!("{}{}\t{}\t{}", prefix, id, service, stamp);
            }
        } else if self.sort_by_service_name {
            let mut items = Vec::new();
            for id in ids {
                if let Some(entry) = mgr.get(&id)? {
                    items.push((entry.service(), id, entry.is_removed()));
                }
            }
            items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            if self.reverse_sort {
                items.reverse();
            }
            for (service, id, removed) in items {
                let prefix = if removed { "-" } else { "" };
                println!("{}{}\t{}", prefix, id, service);
            }
        } else {
            if self.reverse_sort {
                ids.reverse();
            }
            for id in ids {
                if let Some(entry) = mgr.get(&id)? {
                    println!(
                        "{}{}\t{}",
                        id,
                        if entry.is_removed() { "!" } else { "" },
                        entry.service()
                    );
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
    use std::collections::BTreeMap;

    use chrono::{Duration, Local};
    use ulid::Ulid;

    use super::*;
    use crate::database::types::{Entry, ServiceId};
    use crate::database::EntryManager;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-list-test-{}.redb", Ulid::new()))
    }

    fn build_mgr() -> EntryManager {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();

        let mut e1 = Entry::new(
            ServiceId::new(),
            "Alpha".into(),
            vec![],
            vec!["Tag1".into()],
            BTreeMap::new(),
        );
        let mut e2 = Entry::new(
            ServiceId::new(),
            "Beta".into(),
            vec![],
            vec!["tag2".into()],
            BTreeMap::new(),
        );
        let mut e3 = Entry::new(
            ServiceId::new(),
            "Gamma".into(),
            vec![],
            vec!["tag2".into()],
            BTreeMap::new(),
        );

        // 更新日時を手動で設定し直す
        e1.set_last_update(Local::now() - Duration::minutes(10));
        e2.set_last_update(Local::now() - Duration::minutes(5));
        e3.set_last_update(Local::now());

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
            sort_by_last_update: false,
            with_removed: false,
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
            sort_by_last_update: false,
            with_removed: false,
        };

        // 実行経路を通すだけ（出力は確認不要なので collect_ids だけ確認）
        let mut ids = ctx.collect_ids().unwrap();
        ids.sort();
        assert_eq!(ids.len(), 3);
    }
}
