/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! addサブコマンドの実装

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use crate::cmd_args::{AddOpts, Options};
use crate::database::{
    types::{Entry, ServiceId},
    EntryManager,
};
use super::{
    editor::{default_editor_launcher, rewrite_id_line, EditorLauncher},
    prompt::{Prompter, StdPrompter},
    util::is_blank,
    CommandContext,
};

/// テンプレート（IDのみ置換する）
const ADD_TEMPLATE: &str = include_str!("templates/add_template.yml");

///
/// addサブコマンドのコンテキスト情報をパックした構造体
///
struct AddCommandContext {
    /// データベースオブジェクト
    manager: RefCell<EntryManager>,

    /// 問い合わせ用のプロンプタ
    prompter: Arc<dyn Prompter>,

    /// エディタ起動手順
    editor_launcher: Arc<EditorLauncher>,

    /// デフォルトサービス名（引数で指定された場合）
    default_service: Option<String>,
}

impl AddCommandContext {
    ///
    /// オブジェクトの生成
    ///
    fn new(opts: &Options, sub_opts: &AddOpts) -> Result<Self> {
        let editor = opts.editor();

        Ok(Self {
            manager: RefCell::new(opts.open()?),
            prompter: Arc::new(StdPrompter),
            editor_launcher: default_editor_launcher(editor),
            default_service: sub_opts.service_name(),
        })
    }

    ///
    /// テンプレートを一時ファイルに書き出し、パスを返す
    ///
    fn write_template(&self, id: &ServiceId) -> Result<PathBuf> {
        let content = ADD_TEMPLATE
            .replace("{{ID}}", &id.to_string())
            .replace("{{SERVICE}}", self.default_service.as_deref().unwrap_or(""));

        let path = std::env::temp_dir()
            .join(format!("pwmgr-add-{}.yml", id.to_string()));
        fs::write(&path, content).context("テンプレートの書き込みに失敗しました")?;
        Ok(path)
    }

    ///
    /// YAML上のID行を差し替える（見つからなければ先頭に挿入する）
    ///
    pub(super) fn rewrite_id_line(content: &str, id: &ServiceId) -> String {
        rewrite_id_line(content, id)
    }

    #[cfg(test)]
    ///
    /// テスト用に依存を差し替えたコンテキストを生成
    ///
    fn with_deps(
        manager: EntryManager,
        prompter: Arc<dyn Prompter>,
        editor_launcher: Arc<EditorLauncher>,
        default_service: Option<String>,
    ) -> Self {
        Self {
            manager: RefCell::new(manager),
            prompter,
            editor_launcher,
            default_service,
        }
    }
}

impl CommandContext for AddCommandContext {
    fn exec(&self) -> Result<()> {
        // 先にIDを割り当て、テンプレートへ埋め込む
        let id = ServiceId::new();
        let path = self.write_template(&id)?;

        loop {
            // エディタ起動
            (self.editor_launcher)(path.as_path())?;

            // 編集結果読み込み
            let content = fs::read_to_string(&path)
                .context("編集結果の読み込みに失敗しました")?;

            // YAML -> Entry
            let entry: Entry = match serde_yaml_ng::from_str(&content) {
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

            // IDが改変されていないか確認
            if entry.id() != id {
                if self.prompter.ask_retry(
                    "IDが変更されています。IDは変更しないでください。"
                )? {
                    let fixed = Self::rewrite_id_line(&content, &id);
                    fs::write(&path, fixed)
                        .context("IDを書き戻す処理に失敗しました")?;
                    continue;
                } else {
                    return Err(anyhow!("IDが変更されました"));
                }
            }

        // 正規化したエントリを登録
        // Entry::new() で別名・タグをソート＋重複排除して正規化してから登録する
        if is_blank(&entry.service()) {
            if self.prompter.ask_retry("サービス名が未入力です。再編集しますか？")? {
                continue;
                } else {
                    return Err(anyhow!("サービス名が未入力です"));
                }
            }

            if entry.properties().is_empty() {
                if self.prompter.ask_retry(
                    "プロパティが1件も登録されていません。再編集しますか？"
                )? {
                    continue;
                } else {
                    return Err(anyhow!("プロパティが未登録です"));
                }
            }

            let entry = Entry::new(
                id.clone(),
                entry.service(),
                entry.aliases(),
                entry.tags(),
                entry.properties(),
            );
            // 更新日時をセット
            let mut entry = entry;
            entry.set_last_update_now();

            self.manager.borrow_mut().put(&entry)?;
            break;
        }

        Ok(())
    }
}

///
/// コマンドコンテキストの生成
///
pub(crate) fn build_context(opts: &Options, sub_opts: &AddOpts) -> Result<Box<dyn CommandContext>> {
    Ok(Box::new(AddCommandContext::new(opts, sub_opts)?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::command::prompt::test::QueuePrompter;
    use crate::database::types::{Entry, ServiceId};
    use crate::database::EntryManager;
    use ulid::Ulid;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("pwmgr-test-{}.redb", Ulid::new()))
    }

    fn read_id_from_template(path: &Path) -> String {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("id:"))
            .map(|id| id.trim().trim_matches('"'))
            .unwrap()
            .to_string()
    }

    #[test]
    /// SERVICE-NAME指定時にテンプレートへ事前入力されること
    fn template_prefills_service_name() {
        let mgr = build_manager();
        let ctx = AddCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![])),
            Arc::new(|_| Ok(())),
            Some("preset".to_string()),
        );

        let id = ServiceId::new();
        let path = ctx.write_template(&id).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("service: \"preset\""));
    }

    fn build_manager() -> EntryManager {
        let path = temp_db_path();
        EntryManager::open(path).unwrap()
    }

    /// 正常系: 編集内容が正しく登録され、別名/タグがソート+重複除去されること
    #[test]
    fn exec_registers_normalized_entry() {
        let mgr = build_manager();

        let editor = Arc::new(|path: &Path| -> Result<()> {
            let id = read_id_from_template(path);
            let yaml = format!(
                concat!(
                    "id: \"{id}\"\n",
                    "service: \"example\"\n",
                    "aliases:\n",
                    "  - b\n",
                    "  - a\n",
                    "  - a\n",
                    "tags:\n",
                    "  - tag2\n",
                    "  - tag1\n",
                    "  - tag1\n",
                    "properties:\n",
                    "  user: alice\n",
                ),
                id = id
            );
            fs::write(path, yaml)?;
            Ok(())
        });

        let ctx = AddCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![])),
            editor,
            None,
        );

        ctx.exec().unwrap();

        let mut mgr = ctx.manager.borrow_mut();
        let ids = mgr.all_service().unwrap();
        assert_eq!(ids.len(), 1);

        let entry: Entry = mgr.get(&ids[0]).unwrap().unwrap();
        assert_eq!(entry.service(), "example".to_string());
        assert_eq!(entry.aliases(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(entry.tags(), vec!["tag1".to_string(), "tag2".to_string()]);

        let mut props = BTreeMap::new();
        props.insert("user".to_string(), "alice".to_string());
        assert_eq!(entry.properties(), props);
    }

    /// IDを誤って変更した際にリトライして正しいIDで登録できること
    #[test]
    fn exec_retries_on_id_change() {
        let mgr = build_manager();
        let counter = AtomicUsize::new(0);
        let original_id = Arc::new(Mutex::new(None::<String>));

        let editor = {
            let original_id = Arc::clone(&original_id);

            Arc::new(move |path: &Path| -> Result<()> {
                let id_in_file = read_id_from_template(path);
                let mut stored = original_id.lock().unwrap();
                let base_id = stored
                    .get_or_insert_with(|| id_in_file.clone())
                    .clone();

                let turn = counter.fetch_add(1, Ordering::SeqCst);
                let wrong_id = ServiceId::new().to_string();
                let use_id = if turn == 0 { wrong_id } else { base_id };

                let yaml = format!(
                    concat!(
                        "id: \"{id}\"\n",
                        "service: \"svc\"\n",
                        "aliases: []\n",
                        "tags: []\n",
                        "properties:\n",
                        "  user: alice\n",
                    ),
                    id = use_id,
                );
                fs::write(path, yaml)?;
                Ok(())
            })
        };

        let ctx = AddCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![true])),
            editor,
            None,
        );

        ctx.exec().unwrap();

        let mgr = ctx.manager.borrow_mut();
        let ids = mgr.all_service().unwrap();
        assert_eq!(ids.len(), 1);
    }

    #[test]
    /// YAML解釈エラー時にリトライを拒否するとエラーで終了すること
    fn exec_fails_on_yaml_error_without_retry() {
        let mgr = build_manager();
        let editor = Arc::new(|path: &Path| -> Result<()> {
            fs::write(path, "id: \"bad\nservice: \"svc\"")?;
            Ok(())
        });

        let ctx = AddCommandContext::with_deps(
            mgr,
            Arc::new(QueuePrompter::new(vec![false])),
            editor,
            None,
        );

        let result = ctx.exec();
        assert!(result.is_err());
    }
}
