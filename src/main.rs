/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! プログラムのエントリポイント
//!

mod cmd_args;
pub(crate) mod command;
pub(crate) mod database;

use std::sync::Arc;

use anyhow::Result;
use cmd_args::Options;

///
/// プログラムのエントリポイント
///
fn main() {
    /*
     * マンドラインオプションのパース
     */
    let opts = match cmd_args::parse() {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("error: {}", err);
            std::process::exit(1);
        }
    };

    if let Err(err) = run(opts) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}

///
/// プログラムの実行関数
///
/// # 引数
/// * `opts` - オプション情報をパックしたオブジェクト
///
/// # 戻り値
/// 処理に失敗した場合はエラー情報を`Err()`でラップして返す。
///
fn run(opts: Arc<Options>) -> Result<()> {
    opts.build_context()?.exec()?;
    Ok(())
}
