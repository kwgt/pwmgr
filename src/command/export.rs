/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! exportサブコマンドの実装
//!

use std::cell::RefCell;
use std::io::Write;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::cmd_args::{ExportOpts, Options};
use crate::database::types::Entry;
use crate::database::EntryManager;
use super::CommandContext;

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct ExportCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// サブコマンドオプション
    opts: ExportOpts,
}

impl ExportCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &ExportOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            opts: sub_opts.clone(),
        })
    }

    ///
    /// 全エントリを収集する
    ///
    fn collect_entries(&self) -> Result<Vec<Entry>> {
        let ids = self.manager.borrow().all_service()?;
        let mut entries = Vec::new();

        for id in ids {
            if let Some(entry) = self.manager.borrow_mut().get(&id)? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

// CommandContextトレイトの実装
impl CommandContext for ExportCommandContext {
    fn exec(&self) -> Result<()> {
        let mut writer = self.opts.output()?;

        let entries = self.collect_entries()?;
        if entries.is_empty() {
            return Err(anyhow!("エクスポート対象のエントリがありません"));
        }

        let mut serializer = serde_yaml_ng::Serializer::new(&mut writer);
        for entry in entries {
            entry.serialize(&mut serializer)
                .context("YAMLへのシリアライズに失敗しました")?;
        }
        writer.flush().ok();

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &ExportOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(ExportCommandContext::new(opts, sub_opts)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::types::{Entry, ServiceId};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use ulid::Ulid;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-export-test-{}.redb", Ulid::new()))
    }

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
    /// エントリをYAMLで標準出力（バッファ）に書き出せることを確認
    ///
    #[test]
    fn export_to_writer() {
        let mgr = build_mgr_with_entries();

        // BufWriterを差し替えるため opts.output() 相当を再現
        let mut buf: Vec<u8> = Vec::new();
        let opts = ExportOpts::new_for_test(None);

        let ctx = ExportCommandContext {
            manager: RefCell::new(mgr),
            opts: opts.clone(),
        };

        let entries = ctx.collect_entries().unwrap();
        let mut serializer = serde_yaml_ng::Serializer::new(&mut buf);
        for entry in entries {
            entry.serialize(&mut serializer).unwrap();
        }

        let as_str = String::from_utf8(buf).unwrap();
        assert!(as_str.contains("Alpha"));
        assert!(as_str.contains("Beta"));
        assert!(as_str.contains("---")); // 複数ドキュメント区切り
    }

    ///
    /// エントリが空の場合はエラーになることを確認
    ///
    #[test]
    fn export_empty_outputs_empty_array() {
        let path = temp_db_path();
        let mgr = EntryManager::open(path).unwrap();

        let opts = ExportOpts::new_for_test(None);

        let ctx = ExportCommandContext {
            manager: RefCell::new(mgr),
            opts: opts.clone(),
        };

        let res = ctx.exec();
        assert!(res.is_err());
    }
}
