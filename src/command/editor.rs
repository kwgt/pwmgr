/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use regex::Regex;

use crate::database::types::ServiceId;

pub(crate) type EditorLauncher = dyn Fn(&Path) -> Result<()> + Send + Sync;

///
/// デフォルトのエディタ起動ロジックを生成する
///
pub(crate) fn default_editor_launcher(editor: String) -> Arc<EditorLauncher> {
    Arc::new(move |path: &Path| {
        let status = Command::new(&editor)
            .arg(path)
            .status()
            .with_context(|| format!("エディタ {} の起動に失敗しました", editor))?;

        if !status.success() {
            return Err(anyhow!("エディタが異常終了しました (exit={})", status));
        }

        Ok(())
    })
}

///
/// YAML上のID行を差し替える（見つからなければ先頭に挿入する）
///
pub(crate) fn rewrite_id_line(content: &str, id: &ServiceId) -> String {
    let re = Regex::new(r"(?m)^id\s*:.*$")
        .expect("regex compile failed");
    let replacement = format!("id: \"{}\"", id.to_string());

    if re.is_match(content) {
        re.replace(content, replacement).to_string()
    } else {
        format!("{replacement}\n{content}")
    }
}
