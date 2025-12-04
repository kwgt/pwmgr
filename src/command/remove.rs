/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//! removeサブコマンドの実装

use std::cell::RefCell;

use anyhow::{anyhow, Result};

use crate::cmd_args::{Options, RemoveOpts};
use crate::database::EntryManager;
use crate::database::types::ServiceId;
use super::CommandContext;

///
/// removeサブコマンドのコンテキスト情報をパックした構造体
///
struct RemoveCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// 対象ID
    id: String,

    /// ハード削除フラグ
    hard: bool,
}

impl RemoveCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &RemoveOpts) -> Result<Self> {
        Ok(Self {
            manager: RefCell::new(opts.open()?),
            id: sub_opts.id(),
            hard: sub_opts.is_hard(),
        })
    }
}

impl CommandContext for RemoveCommandContext {
    fn exec(&self) -> Result<()> {
        let id = ServiceId::from_string(&self.id)
            .map_err(|_| anyhow!("IDの形式が不正です: {}", self.id))?;

        if self.hard {
            self.manager.borrow_mut().remove(&id)?;
            println!("removed (hard): {}", id);
        } else {
            let mut mgr = self.manager.borrow_mut();
            if let Some(mut entry) = mgr.get(&id)? {
                entry.set_removed(true);
                entry.set_last_update_now();
                mgr.put(&entry)?;
                println!("removed (soft): {}", id);
            } else {
                return Err(anyhow!("指定されたIDのエントリが見つかりません: {}", id));
            }
        }

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &RemoveOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(RemoveCommandContext::new(opts, sub_opts)?))
}
