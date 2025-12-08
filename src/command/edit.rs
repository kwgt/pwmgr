/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! editサブコマンドの実装
//!

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde_yaml_ng;

use crate::cmd_args::{EditOpts, Options};
use crate::command::prompt::{Prompter, StdPrompter};
use crate::command::editor::{default_editor_launcher, rewrite_id_line};
use crate::database::EntryManager;
use crate::database::types::{Entry, ServiceId};
use super::CommandContext;

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct EditCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// 問い合わせ用のプロンプタ
    prompter: Arc<dyn Prompter>,

    /// エディタ起動手順
    editor_launcher: Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>,

    /// 対象ID文字列
    target_id: String,
}

impl EditCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &EditOpts) -> Result<Self> {
        let editor = opts.editor();
        let target_id = sub_opts.id();

        Ok(Self {
            manager: RefCell::new(opts.open()?),
            prompter: Arc::new(StdPrompter),
            editor_launcher: default_editor_launcher(editor),
            target_id,
        })
    }

    #[cfg(test)]
    ///
    /// テスト用に依存を差し替えたコンテキストを生成
    ///
    fn with_deps(
        manager: EntryManager,
        prompter: Arc<dyn Prompter>,
        editor_launcher: Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>,
        target_id: String,
    ) -> Self {
        Self {
            manager: RefCell::new(manager),
            prompter,
            editor_launcher,
            target_id,
        }
    }

    ///
    /// テンプレートを一時ファイルに書き出し、パスを返す
    ///
    fn write_entry(&self, entry: &Entry) -> Result<PathBuf> {
        let content = serde_yaml_ng::to_string(entry)
            .context("エントリのYAML化に失敗しました")?;
        let path = std::env::temp_dir()
            .join(format!("pwmgr-edit-{}.yml", entry.id().to_string()));
        fs::write(&path, content).context("エントリの書き出しに失敗しました")?;
        Ok(path)
    }
}

// CommandContextトレイトの実装
impl CommandContext for EditCommandContext {
    fn exec(&self) -> Result<()> {
        let id = ServiceId::from_string(&self.target_id)
            .map_err(|_| anyhow!("IDの形式が不正です: {}", self.target_id))?;

        let entry = self.manager.borrow_mut()
            .get(&id)?
            .ok_or_else(|| {
                anyhow!("指定されたIDのエントリが見つかりません: {}", id)
            })?;

        let path = self.write_entry(&entry)?;

        loop {
            (self.editor_launcher)(path.as_path())?;

            let content = fs::read_to_string(&path)
                .context("編集結果の読み込みに失敗しました")?;

            let entry_new: Entry = match serde_yaml_ng::from_str(&content) {
                Ok(entry) => entry,
                Err(err) => {
                    if self.prompter.ask_retry(
                        &format!("YAMLの解釈に失敗しました: {err}")
                    )? {
                        continue;
                    } else {
                        return Err(err.into());
                    }
                }
            };

            if entry_new.id() != id {
                if self.prompter.ask_retry(
                    "IDが変更されています。IDは変更しないでください。"
                )? {
                    let fixed = rewrite_id_line(&content, &id);
                    fs::write(&path, fixed)
                        .context("IDを書き戻す処理に失敗しました")?;
                    continue;
                } else {
                    return Err(anyhow!("IDが変更されました"));
                }
            }

            // 正規化して保存
            let entry_norm = Entry::new(
                id.clone(),
                entry_new.service(),
                entry_new.aliases(),
                entry_new.tags(),
                entry_new.properties(),
            );
            let mut entry_norm = entry_norm;
            entry_norm.set_removed(entry_new.is_removed());
            entry_norm.set_last_update_now();

            self.manager.borrow_mut().put(&entry_norm)?;
            break;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::prompt::test::QueuePrompter;
    use crate::database::EntryManager;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use ulid::Ulid;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-edit-test-{}.redb", Ulid::new()))
    }

    fn build_mgr_with_entry() -> (EntryManager, ServiceId) {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(path).unwrap();
        let id = ServiceId::new();

        let entry = Entry::new(
            id.clone(),
            "Alpha".to_string(),
            vec!["alp".into()],
            vec!["t1".into()],
            BTreeMap::from([("user".into(), "alice".into())]),
        );

        mgr.put(&entry).unwrap();
        (mgr, id)
    }

    ///
    /// 正常に編集して上書きできること
    ///
    #[test]
    fn edit_updates_entry() {
        let (mgr, id) = build_mgr_with_entry();
        let target_id = id.to_string();

        let editor_id = id.clone();
        let editor = Arc::new(move |path: &Path| -> Result<()> {
            let content = format!(
                concat!(
                    "id: \"{id}\"\n",
                    "service: \"Alpha\"\n",
                    "aliases:\n",
                    "  - beta\n",
                    "  - alpha\n",
                    "tags:\n",
                    "  - t2\n",
                    "  - t1\n",
                    "properties:\n",
                    "  user: alice2\n",
                ),
                id = editor_id
            );
            fs::write(path, content)?;
            Ok(())
        });

        let ctx = EditCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![])),
            editor,
            target_id,
        );

        ctx.exec().unwrap();

        let mut mgr = ctx.manager.borrow_mut();
        let entry = mgr.get(&id).unwrap().unwrap();

        // aliases/tags は Entry::new で正規化される
        assert_eq!(
            entry.aliases(),
            vec!["alpha".to_string(), "beta".to_string()]
        );

        assert_eq!(entry.tags(), vec!["t1".to_string(), "t2".to_string()]);
        assert_eq!(entry.properties().get("user").unwrap(), "alice2");
    }

    ///
    /// IDを変更した場合にリトライして元に戻して保存できること
    ///
    #[test]
    fn edit_retry_on_id_change() {
        let (mgr, id) = build_mgr_with_entry();
        let original_id = id.to_string();

        let attempts = Arc::new(AtomicUsize::new(0));
        let editor_id = original_id.clone();
        let editor_attempts = attempts.clone();

        let editor = Arc::new(move |path: &Path| -> Result<()> {
            let n = editor_attempts.fetch_add(1, Ordering::SeqCst);
            let id_to_write = if n == 0 {
                ServiceId::new().to_string() // 1回目は誤ったIDを書く
            } else {
                editor_id.clone() // 2回目以降は正しいIDを書く
            };

            let content = format!(
                concat!(
                    "id: \"{id}\"\n",
                    "service: \"Alpha\"\n",
                    "aliases: []\n",
                    "tags: []\n",
                    "properties: {{}}\n",
                ),
                id = id_to_write
            );
            fs::write(path, content)?;
            Ok(())
        });

        // ask_retry で1回だけ true を返し再編集を促す
        let ctx = EditCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![true])),
            editor,
            original_id.clone(),
        );

        // 1回目: ID誤り→書き戻し→2回目は元IDに戻るので成功
        ctx.exec().unwrap();
    }

    ///
    /// YAML解釈エラーで再編集を拒否するとエラーになること
    ///
    #[test]
    fn edit_yaml_error_without_retry() {
        let (mgr, id) = build_mgr_with_entry();

        let editor = Arc::new(|path: &Path| -> Result<()> {
            fs::write(path, "id: \"bad\nservice: \"svc\"")?;
            Ok(())
        });

        let ctx = EditCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![false])),
            editor,
            id.to_string(),
        );

        let res = ctx.exec();
        assert!(res.is_err());
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &EditOpts)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(EditCommandContext::new(opts, sub_opts)?))
}
