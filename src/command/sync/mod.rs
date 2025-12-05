/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//!
//! syncサブコマンドの実装
//!

pub(crate) mod client;
pub(crate) mod server;

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cmd_args::{Options, SyncMode, SyncOpts};
use crate::command::prompt::{Prompter, StdPrompter};
use crate::command::CommandContext;
use crate::database::types::Entry;
use crate::database::EntryManager;

/// プロトコルバージョン
const PROTOCOL_VERSION: u16 = 1;

///
/// プロトコルで用いるパケット
///
#[derive(Debug, Serialize, Deserialize)]
enum SyncPacket {
    ///
    /// バージョン交渉と相手識別のための初回パケット
    ///
    Hello(Hello),

    ///
    /// Helloへの応答パケット（受け入れ可否を通知）
    ///
    HelloAck(HelloAck),

    ///
    /// サーバからクライアントへ送るエントリ本体
    ///
    ServerEntry(Entry),

    ///
    /// サーバ送信の終端と送信件数を示す
    ///
    ServerEntriesEnd(ServerEntriesEnd),

    ///
    /// クライアントからサーバへ送るエントリ本体
    ///
    ClientEntry(Entry),

    ///
    /// クライアント送信の終端と送信件数を示す
    ///
    ClientEntriesEnd(ClientEntriesEnd),

    ///
    /// エントリ適用の成否を送り返すACK
    ///
    EntryAck(EntryAck),

    ///
    /// 双方の同期完了を示す
    ///
    Finished,

    ///
    /// エラーやユーザ拒否による中断を示す
    ///
    Abort(Abort),
}

impl SyncPacket {
    ///
    /// Helloパケットの生成
    ///
    fn hello(
        protocol_version: u16,
        node_id: String,
        role: NodeRole,
        now_epoch_ms: u64,
    ) -> Self {
        Self::Hello(Hello {
            protocol_version,
            node_id,
            role,
            now_epoch_ms,
        })
    }

    ///
    /// HelloAckパケットの生成
    ///
    fn hello_ack(
        protocol_version: u16,
        accepted: bool,
        reason: Option<String>,
    ) -> Self {
        Self::HelloAck(HelloAck {
            protocol_version,
            accepted,
            reason,
        })
    }

    ///
    /// サーバ送信エントリパケットの生成
    ///
    fn server_entry(entry: Entry) -> Self {
        Self::ServerEntry(entry)
    }

    ///
    /// サーバ送信終端パケットの生成
    ///
    fn server_entries_end(total_sent: u64) -> Self {
        Self::ServerEntriesEnd(ServerEntriesEnd { total_sent })
    }

    ///
    /// クライアント送信エントリパケットの生成
    ///
    fn client_entry(entry: Entry) -> Self {
        Self::ClientEntry(entry)
    }

    ///
    /// クライアント送信終端パケットの生成
    ///
    fn client_entries_end(total_sent: u64) -> Self {
        Self::ClientEntriesEnd(ClientEntriesEnd { total_sent })
    }

    ///
    /// エントリACKパケットの生成
    ///
    fn entry_ack(
        entry_id: impl Into<String>,
        accepted: bool,
        reason: Option<String>,
    ) -> Self {
        Self::EntryAck(EntryAck {
            entry_id: entry_id.into(),
            accepted,
            reason,
        })
    }

    ///
    /// Finishedパケットの生成
    ///
    fn finished() -> Self {
        Self::Finished
    }

    ///
    /// Abortパケットの生成
    ///
    fn abort(reason: impl Into<String>) -> Self {
        Self::Abort(Abort {
            reason: reason.into(),
        })
    }
}

///
/// Helloパケット
///
#[derive(Debug, Serialize, Deserialize)]
struct Hello {
    ///
    /// プロトコルバージョン（後方互換性確認用）
    ///
    protocol_version: u16,

    ///
    /// ノード識別子（ホストを一意に識別）
    ///
    node_id: String,

    ///
    /// ノードの役割（Server/Client）
    ///
    role: NodeRole,

    ///
    /// 相手との時計ずれ確認用の現在時刻（エポックミリ秒）
    ///
    now_epoch_ms: u64,
}

///
/// HelloAckパケット
///
#[derive(Debug, Serialize, Deserialize)]
struct HelloAck {
    ///
    /// 合意したプロトコルバージョン
    ///
    protocol_version: u16,

    ///
    /// Helloを受理したか否か
    ///
    accepted: bool,

    ///
    /// 非受理時の理由
    ///
    reason: Option<String>,
}

///
/// ノードの役割
///
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum NodeRole {
    ///
    /// 待ち受け側（全件送信から開始）
    ///
    Server,

    ///
    /// 接続開始側（評価後に差分送信）
    ///
    Client,
}

///
/// サーバ送信の終端を示す
///
#[derive(Debug, Serialize, Deserialize)]
struct ServerEntriesEnd {
    ///
    /// サーバが送信したエントリ件数
    ///
    total_sent: u64,
}

///
/// クライアント送信の終端を示す
///
#[derive(Debug, Serialize, Deserialize)]
struct ClientEntriesEnd {
    ///
    /// クライアントが送信したエントリ件数
    ///
    total_sent: u64,
}

///
/// エントリ適用ACK
///
#[derive(Debug, Serialize, Deserialize)]
struct EntryAck {
    ///
    /// 対象エントリのID
    ///
    entry_id: String,

    ///
    /// 適用に成功したか否か
    ///
    accepted: bool,

    ///
    /// 拒否または失敗時の理由
    ///
    reason: Option<String>,
}

///
/// セッション中断パケット
///
#[derive(Debug, Serialize, Deserialize)]
struct Abort {
    ///
    /// 中断理由メッセージ
    ///
    reason: String,
}

///
/// パケット送信（長さプレフィックス + MessagePack）
///
fn send_packet(stream: &mut TcpStream, packet: SyncPacket)
    -> Result<()>
{
    let buf = rmp_serde::to_vec_named(&packet)
        .context("serialize packet")?;
    let len = buf.len() as u32;
    stream.write_all(&len.to_be_bytes()).context("write length")?;
    stream.write_all(&buf).context("write packet")?;
    stream.flush().ok();
    Ok(())
}

///
/// パケット受信（長さプレフィックス + MessagePack）
///
fn recv_packet(stream: &mut TcpStream) -> Result<SyncPacket> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).context("read length")?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).context("read packet")?;

    rmp_serde::from_slice(&buf).context("deserialize packet")
}

///
/// syncコマンドコンテキスト
///
pub(crate) struct SyncCommandContext {
    /// 動作モード
    mode: SyncMode,

    /// エントリーマネージャインスタンス
    manager: RefCell<EntryManager>,

    /// プロンプターコンテキスト
    prompter: Arc<dyn Prompter>,
}

impl SyncCommandContext {
    ///
    /// オブジェクトの生成
    ///
    pub(crate) fn new(opts: &Options, sub_opts: &SyncOpts) -> Result<Self> {
        Ok(Self {
            mode: sub_opts.mode()?,
            manager: RefCell::new(opts.open()?),
            prompter: Arc::new(StdPrompter),
        })
    }
}

impl CommandContext for SyncCommandContext {
    ///
    /// syncコマンドの実行
    ///
    fn exec(&self) -> Result<()> {
        match &self.mode {
            SyncMode::Server(addr) => {
                server::run(addr, &self.manager)
            }

            SyncMode::Client(addr) => {
                client::run(addr, &self.manager, self.prompter.as_ref())
            }
        }
    }
}

///
/// コマンドコンテキストのビルダ
///
pub(crate) fn build_context(opts: &Options, sub_opts: &SyncOpts,)
    -> Result<Box<dyn CommandContext>>
{
    Ok(Box::new(SyncCommandContext::new(opts, sub_opts)?))
}
