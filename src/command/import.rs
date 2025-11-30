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
    /// YAMLストリーミングからエントリを順次読み込む
    ///
    fn import_entries<R: Read>(&self, reader: R) -> Result<usize> {
        let de = serde_yaml_ng::Deserializer::from_reader(reader);
        let mut mgr = self.manager.borrow_mut();
        let mut count = 0usize;

        for doc in de {
            let entry_raw = Entry::deserialize(doc)?;
            let id = entry_raw.id();

            if !self.opts.is_overwrite() {
                if let Some(_) = mgr.get(&id)? {
                    return Err(anyhow!("既に存在するIDです: {}", id));
                }
            }

            if self.opts.is_dry_run() {
                continue;
            }

            // 正規化して登録
            let entry = Entry::new(
                id.clone(),
                entry_raw.service(),
                entry_raw.aliases(),
                entry_raw.tags(),
                entry_raw.properties(),
            );

            mgr.put(&entry)?;
            count += 1;
        }

        Ok(count)
    }
}

// CommandContextトレイトの実装
impl CommandContext for ImportCommandContext {
    fn exec(&self) -> Result<()> {
        // merge 未指定かつ dry-run でない場合は全削除して置き換え
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

            for id in ids {
                self.manager.borrow_mut().remove(&id)?;
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
            opts: ImportOpts::new_for_test(None, false, false, false),
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
            opts: ImportOpts::new_for_test(None, false, true, true),
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
