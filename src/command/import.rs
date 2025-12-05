/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! importサブコマンドの実装
//!

use anyhow::{anyhow, Result};

use crate::cmd_args::{ImportOpts, Options};
use crate::database::types::Entry;
use crate::database::EntryManager;
use crate::command::prompt::Prompter;
use super::CommandContext;
use std::cell::RefCell;
use std::io::Read;
use serde::Deserialize;

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct ImportCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// サブコマンドオプション
    opts: ImportOpts,

    /// プロンプタ
    prompter: Box<dyn Prompter>,
}

impl ImportCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &ImportOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            opts: sub_opts.clone(),
            prompter: Box::new(crate::command::prompt::StdPrompter),
        })
    }

    ///
    /// import用にエントリを正規化する（タグ/エイリアス整形、last_update保持）
    ///
    fn normalize_entry(entry_raw: Entry) -> Entry {
        let mut entry = Entry::new(
            entry_raw.id(),
            entry_raw.service(),
            entry_raw.aliases(),
            entry_raw.tags(),
            entry_raw.properties(),
        );

        if let Some(dt) = entry_raw.last_update() {
            entry.set_last_update(dt);
        }

        if entry_raw.is_removed() {
            entry.set_removed(true);
        }

        entry
    }

    ///
    /// YAMLストリーミングからエントリを順次読み込み、トランザクション内で処理する
    ///
    fn import_entries<R: Read>(&self, reader: R) -> Result<usize> {
        let mut deserializer = serde_yaml_ng::Deserializer::from_reader(reader);
        let merge = self.opts.is_merge();
        let overwrite = self.opts.is_overwrite();
        let dry_run = self.opts.is_dry_run();

        // 置換モードでの削除対象リストを事前取得（読み取り）
        let existing_ids = if !merge && !dry_run {
            self.manager.borrow().all_service()?
        } else {
            Vec::new()
        };

        let mut imported = 0usize;

        self.manager.borrow().with_write_transaction(|writer| {
            // 置換モード: 先に全削除
            if !merge && !dry_run {
                for id in existing_ids.iter() {
                    writer.remove(id)?;
                }
            }

            for doc in deserializer.by_ref() {
                let entry_raw = Entry::deserialize(doc)?;
                let entry = Self::normalize_entry(entry_raw);
                let id = entry.id();

                if let Some(existing) = writer.get(&id)? {
                    if !overwrite {
                        return Err(anyhow!("既に存在するIDです: {}", id));
                    }

                    // 上書き時は更新日時を比較して新しい方を残す
                    let new_is_newer = match (entry.last_update(), existing.last_update()) {
                        (Some(new), Some(old)) => new > old,
                        (Some(_), None) => true,
                        _ => false,
                    };

                    if dry_run {
                        continue;
                    }

                    if new_is_newer {
                        eprintln!("overwrite (newer) id {}", id);
                        writer.put(&entry)?;
                        imported += 1;
                    } else {
                        eprintln!("skip overwrite: existing newer id {}", id);
                    }
                } else {
                    if dry_run {
                        continue;
                    }

                    writer.put(&entry)?;
                    imported += 1;
                }
            }

            Ok(())
        })?;

        Ok(imported)
    }
}

// CommandContextトレイトの実装
impl CommandContext for ImportCommandContext {
    fn exec(&self) -> Result<()> {
        // merge 未指定かつ dry-run でない場合は全削除して置き換え前に確認
        if !self.opts.is_merge() && !self.opts.is_dry_run() {
            let ids = self.manager.borrow().all_service()?;
            if ids.len() > 0 {
                let msg = format!(
                    "既存データ({}件)を上書きします。よろしいですか？",
                    ids.len(),
                );
                if !self.prompter.confirm(&msg, false, None)? {
                    return Err(anyhow!("インポートを中止しました"));
                }
            }
        }

        let imported = self.import_entries(self.opts.input()?)?;
        println!("imported {} entries", imported);
        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &ImportOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(ImportCommandContext::new(opts, sub_opts)?))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::collections::BTreeMap;

    use ulid::Ulid;

    use crate::cmd_args::ImportOpts;
    use crate::database::EntryManager;
    use crate::database::types::ServiceId;
    use crate::command::prompt::test::QueuePrompter;
    use super::*;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-import-test-{}.redb", Ulid::new()))
    }

    fn make_opts() -> ImportOpts {
        ImportOpts::new_for_test(None, false, true, false)
    }


    ///
    /// 複数ドキュメント（---区切り）の読み込みでエントリが登録されること
    ///
    #[test]
    fn import_multi_docs() {
        let path = temp_db_path();
        let mgr = EntryManager::open(path).unwrap();

        let yaml = r#"---
id: "01J1M8Z6Y1Y1Y1Y1Y1Y1Y1Y1Y1"
service: "Alpha"
aliases: []
tags: []
properties:
  user: alice
---
id: "01J1M8Z6Y2Y2Y2Y2Y2Y2Y2Y2Y2"
service: "Beta"
aliases: []
tags: []
properties:
  user: bob
"#;

        let ctx = ImportCommandContext {
            manager: RefCell::new(mgr),
            opts: make_opts(),
            prompter: Box::new(QueuePrompter::new(vec![true])),
        };

        let imported = ctx.import_entries(Cursor::new(yaml)).unwrap();
        assert_eq!(imported, 2);

        let mgr = ctx.manager.borrow_mut();
        let ids = mgr.all_service().unwrap();
        assert_eq!(ids.len(), 2);
    }

    ///
    /// overwrite=false で既存IDがある場合はエラーになること
    ///
    #[test]
    fn import_duplicate_id_errors_without_overwrite() {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();

        let yaml = r#"---
id: "01J1M8Z6Y1Y1Y1Y1Y1Y1Y1Y1Y1"
service: "Alpha"
aliases: []
tags: []
properties: {}
"#;

        // 先に登録
        let entry = Entry::new(
            ServiceId::from_string("01J1M8Z6Y1Y1Y1Y1Y1Y1Y1Y1Y1").unwrap(),
            "Alpha".to_string(),
            vec![],
            vec![],
            BTreeMap::new(),
        );
        mgr.put(&entry).unwrap();

        let ctx = ImportCommandContext {
            manager: RefCell::new(mgr),
            opts: ImportOpts::new_for_test(None, true, false, false),
            prompter: Box::new(QueuePrompter::new(vec![true])),
        };

        let res = ctx.import_entries(Cursor::new(yaml));
        assert!(res.is_err());
    }

    ///
    /// dry-run の場合は登録されないこと
    ///
    #[test]
    fn import_dry_run_does_not_write() {
        let path = temp_db_path();
        let mgr = EntryManager::open(path).unwrap();

        let yaml = r#"---
id: "01J1M8Z6Y1Y1Y1Y1Y1Y1Y1Y1Y1"
service: "Alpha"
aliases: []
tags: []
properties: {}
"#;

        let ctx = ImportCommandContext {
            manager: RefCell::new(mgr),
            opts: ImportOpts::new_for_test(None, true, true, true),
            prompter: Box::new(QueuePrompter::new(vec![true])),
        };

        let imported = ctx.import_entries(Cursor::new(yaml)).unwrap();
        assert_eq!(imported, 0);
        // テーブルが未作成の場合は all_service() がエラーになるため、エラーか空であることを許容
        match ctx.manager.borrow().all_service() {
            Ok(ids) => assert!(ids.is_empty()),
            Err(_) => {}
        }
    }
}
