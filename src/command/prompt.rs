/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

use std::io::{self, Write};

use anyhow::Result;

///
/// 対話的な問い合わせを扱うためのトレイト
///
pub(crate) trait Prompter: Send + Sync {
    ///
    /// 再編集の要否を問い合わせる
    ///
    fn ask_retry(&self, msg: &str) -> Result<bool>;

    ///
    /// yes/no の確認を行う
    ///
    /// # 引数
    /// * `msg` - 確認メッセージ
    /// * `default` - 入力が空のときに採用するデフォルト値（trueならYes）
    /// * `label` - プロンプト表示の先頭につけるラベル（省略可）
    ///
    fn confirm(&self, msg: &str, default: bool, label: Option<&str>) -> Result<bool>;
}

///
/// 標準入出力を用いたプロンプト実装
///
#[derive(Default)]
pub(crate) struct StdPrompter;

impl Prompter for StdPrompter {
    fn ask_retry(&self, msg: &str) -> Result<bool> {
        self.confirm(
            msg,
            false, // デフォルトは No
            Some("再編集しますか？"),
        )
    }

    fn confirm(&self, msg: &str, default: bool, label: Option<&str>) -> Result<bool> {
        eprintln!("{}", msg);
        let prompt = label.unwrap_or("続行しますか？");
        eprint!(
            "{} [{}]: ",
            prompt,
            if default { "Y/n" } else { "y/N" }
        );
        io::stdout().flush().ok();

        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let ans = buf.trim().to_lowercase();

        if ans.is_empty() {
            return Ok(default);
        }

        Ok(ans == "y" || ans == "yes")
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::sync::Mutex;

    use anyhow::Result;

    use super::Prompter;

    /// 簡易キュー型プロンプタ（テスト用）
    pub(crate) struct QueuePrompter {
        answers: Mutex<Vec<bool>>,
    }

    impl QueuePrompter {
        pub(crate) fn new(answers: Vec<bool>) -> Self {
            Self {
                answers: Mutex::new(answers),
            }
        }

        fn pop(&self, default: bool) -> bool {
            self.answers
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(default)
        }
    }

    impl Prompter for QueuePrompter {
        fn ask_retry(&self, _msg: &str) -> Result<bool> {
            Ok(self.pop(false))
        }

        fn confirm(&self, _msg: &str, default: bool, _label: Option<&str>) -> Result<bool> {
            Ok(self.pop(default))
        }
    }
}
