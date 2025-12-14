/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//! クライアント側の同期処理

use std::collections::HashSet;
use std::net::TcpStream;

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use log::{debug, error, info};
use ulid::Ulid;

use crate::command::prompt::Prompter;
use crate::command::sync::{
    recv_packet, send_packet, NodeRole, SyncPacket, PROTOCOL_VERSION,
};
use crate::database::{TransactionReadable, TransactionWriter};
use crate::database::types::{Entry, ServiceId};

/*
 * クライアントモードのエントリーポイント
 */
pub(super) fn run(
    addr: &str,
    writer: &mut TransactionWriter,
    prompter: &dyn Prompter,
) -> Result<()> {
    /*
     * サーバへ接続
     */
    info!("client: connect to {}", addr);
    let mut stream = TcpStream::connect(addr)
        .with_context(|| format!("connect {}", addr))?;

    let node_id = Ulid::new().to_string();

    /*
     * Helloの送信
     */
    send_packet(&mut stream, SyncPacket::hello(
        PROTOCOL_VERSION,
        node_id.clone(),
        NodeRole::Client,
        Local::now().timestamp_millis() as u64,
    ))?;
    debug!(
        "client: sent Hello proto={}, node={}",
        PROTOCOL_VERSION, node_id
    );

    /*
     * HelloAckの受信と確認
     */
    let ack = match recv_packet(&mut stream)? {
        SyncPacket::HelloAck(ack) => ack,
        pkt => return Err(anyhow!("unexpected packet: {:?}", pkt)),
    };

    if !ack.accepted || ack.protocol_version != PROTOCOL_VERSION {
        error!(
            "client: server rejected: {:?}",
            ack.reason.as_ref().map(|s| s.as_str()).unwrap_or("unknown")
        );
        return Err(anyhow!(
            "server rejected: {:?}",
            ack.reason.unwrap_or_else(|| "unknown".into())
        ));
    }
    info!("client: HelloAck accepted");

    /*
     * サーバからの全件受信フェーズ
     */
    info!("client: receive phase start");
    let mut send_candidates: HashSet<String> = HashSet::new();
    let mut remaining_local: HashSet<String> = writer
        .all_service()?
        .into_iter()
        .map(|id| id.to_string())
        .collect();
    let mut received = 0u64;

    loop {
        match recv_packet(&mut stream)? {
            SyncPacket::ServerEntry(entry) => {
                let entry_id = entry.id().to_string();
                remaining_local.remove(&entry_id);

                let decision = decide_entry(writer, &entry, prompter)?;
                match decision {
                    EntryDecision::AdoptRemote => {
                        writer.put(&entry)?;
                        send_ack(&mut stream, &entry_id, true, None)?;
                        debug!(
                            "client: adopt remote entry id={}, service={}",
                            entry.id(),
                            entry.service()
                        );
                    }
                    EntryDecision::KeepLocal => {
                        send_candidates.insert(entry_id.clone());
                        send_ack(&mut stream, &entry_id, true, None)?;
                        debug!(
                            "client: keep local entry id={}, service={}",
                            entry.id(),
                            entry.service()
                        );
                    }
                    EntryDecision::Abort(msg) => {
                        send_ack(&mut stream, &entry_id, false, Some(msg.clone()))?;
                        send_packet(&mut stream, SyncPacket::abort(msg),)?;
                        error!(
                            "client: abort on conflict id={}, service={}",
                            entry.id(),
                            entry.service()
                        );
                        return Err(anyhow!("aborted by user"));
                    }
                }

                received += 1;
            }

            SyncPacket::ServerEntriesEnd(_end) => {
                break;
            }

            SyncPacket::Abort(abort) => {
                return Err(anyhow!("server aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }
    info!("client: receive phase end ({} entries)", received);

    /*
     * クライアント側の差分送信フェーズ
     */
    // サーバから届かなかったローカル専用エントリも送信対象にする
    for id in remaining_local {
        send_candidates.insert(id);
    }

    let mut sent = 0u64;
    info!("client: send phase start ({} candidates)", send_candidates.len());
    for id_str in send_candidates {
        let entry = {
            let id = ServiceId::from_string(&id_str)?;
            writer.get(&id)?
                .ok_or_else(|| anyhow!("missing local entry {}", id_str))?
        };

        send_packet(&mut stream, SyncPacket::client_entry(entry))?;
        sent += 1;

        match recv_packet(&mut stream)? {
            SyncPacket::EntryAck(ack) => {
                if !ack.accepted {
                    let reason = ack.reason.unwrap_or_else(|| "rejected".into());
                    send_packet(&mut stream, SyncPacket::abort(reason.clone()))?;
                    error!("client: server rejected entry id={}", ack.entry_id);
                    return Err(anyhow!("server rejected entry: {}", reason));
                }
                debug!("client: entry ack id={}", ack.entry_id);
            }

            SyncPacket::Abort(abort) => {
                error!("client: server aborted during send: {}", abort.reason);
                return Err(anyhow!("server aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }

    send_packet(&mut stream, SyncPacket::client_entries_end(sent))?;
    info!("client: send phase end ({} entries)", sent);

    /*
     * 終了待ち
     */
    match recv_packet(&mut stream)? {
        SyncPacket::Finished => {
            info!("client: sync finished");
            Ok(())
        }
        SyncPacket::Abort(abort) => {
            error!("client: server aborted: {}", abort.reason);
            Err(anyhow!("server aborted: {}", abort.reason))
        }
        other => Err(anyhow!("unexpected packet: {:?}", other)),
    }
}

/// エントリ比較の結果
enum EntryDecision {
    /// 受信エントリを採用
    AdoptRemote,
    /// ローカルの方が新しいので保持（送信候補にする）
    KeepLocal,
    /// 同時刻差分でユーザが拒否したため中断
    Abort(String),
}

/*
 * 受信エントリをどう扱うか判定する
 */
fn decide_entry(
    writer: &TransactionWriter,
    incoming: &Entry,
    prompter: &dyn Prompter,
) -> Result<EntryDecision> {
    let id = incoming.id();
    let local_entry = writer.get(&id)?;

    if local_entry.is_none() {
        return Ok(EntryDecision::AdoptRemote);
    }

    let local_entry = local_entry.unwrap();

    let incoming_ts = incoming.last_update();
    let local_ts = local_entry.last_update();

    // 同一時刻の扱い
    if incoming_ts == local_ts {
        if is_same_entry(&local_entry, incoming) {
            return Ok(EntryDecision::KeepLocal);
        }

        // サーバ優先だがユーザ確認を挟む
        let ok = prompter.confirm(
            "同一時刻の更新が競合しました。サーバ側を採用しますか？",
            false,
            Some("競合"),
        )?;
        if ok {
            return Ok(EntryDecision::AdoptRemote);
        } else {
            return Ok(EntryDecision::Abort(
                "user rejected conflict resolution".into(),
            ));
        }
    }

    // タイムスタンプ比較（Noneは常に古い扱い）
    match (incoming_ts, local_ts) {
        (Some(r), Some(l)) if r > l => Ok(EntryDecision::AdoptRemote),
        (Some(_), Some(_)) => Ok(EntryDecision::KeepLocal),
        (Some(_), None) => Ok(EntryDecision::AdoptRemote),
        (None, Some(_)) => Ok(EntryDecision::KeepLocal),
        (None, None) => Ok(EntryDecision::KeepLocal),
    }
}

/*
 * エントリ内容が同一かどうか比較する（timestamp除く）
 */
fn is_same_entry(a: &Entry, b: &Entry) -> bool {
    a.id() == b.id()
        && a.service() == b.service()
        && a.aliases() == b.aliases()
        && a.tags() == b.tags()
        && a.properties() == b.properties()
        && a.is_removed() == b.is_removed()
}

/*
 * ACK送信ヘルパ
 */
fn send_ack(
    stream: &mut TcpStream,
    entry_id: &str,
    accepted: bool,
    reason: Option<String>,
) -> Result<()> {
    send_packet(stream, SyncPacket::entry_ack(entry_id, accepted, reason))
}
