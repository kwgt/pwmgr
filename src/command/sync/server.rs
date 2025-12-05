/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//! サーバ側の同期処理

use std::cell::RefCell;
use std::net::TcpListener;

use anyhow::{anyhow, Context, Result};

use crate::command::sync::{
    recv_packet, send_packet, NodeRole, SyncPacket, PROTOCOL_VERSION,
};
use crate::database::EntryManager;

/*
 * サーバモードのエントリーポイント
 */
pub(super) fn run(addr: &str, manager: &RefCell<EntryManager>) -> Result<()> {
    /*
     * クライアントの接続待ち受け
     */
    let listener = TcpListener::bind(addr)
        .with_context(|| format!("bind {}", addr))?;
    let (mut stream, peer) = listener.accept().context("accept")?;
    eprintln!("client connected: {}", peer);

    /*
     * Helloの受信と検証
     */
    let hello = match recv_packet(&mut stream)? {
        SyncPacket::Hello(h) => h,
        pkt => return Err(anyhow!("unexpected packet: {:?}", pkt)),
    };

    if hello.protocol_version != PROTOCOL_VERSION {
        send_packet(&mut stream, SyncPacket::hello_ack(
            PROTOCOL_VERSION,
            false,
            Some("protocol version mismatch".into()),
        ))?;

        return Err(anyhow!("protocol version mismatch"));
    }

    if hello.role != NodeRole::Client {
        send_packet(&mut stream, SyncPacket::hello_ack(
            PROTOCOL_VERSION,
            false,
            Some("role mismatch".into()),
        ))?;

        return Err(anyhow!("unexpected role from peer"));
    }

    /*
     * HelloAckの送信
     */
    send_packet(&mut stream, SyncPacket::hello_ack(
        PROTOCOL_VERSION,
        true,
        None
    ))?;

    /*
     * エントリ送信フェーズ（全件送信し、エントリごとにACKを受信）
     */
    let ids = manager.borrow().all_service()?;
    let mut sent = 0u64;
    for id in ids {
        let entry = {
            let mut mgr = manager.borrow_mut();
            mgr.get(&id)?
                .ok_or_else(|| anyhow!("missing entry during send"))?
        };

        send_packet(&mut stream, SyncPacket::server_entry(entry))?;
        sent += 1;

        match recv_packet(&mut stream)? {
            SyncPacket::EntryAck(ack) => {
                if !ack.accepted {
                    let reason = ack.reason.unwrap_or_else(|| "rejected".into());
                    send_packet(&mut stream, SyncPacket::abort(reason.clone()))?;

                    return Err(anyhow!("client rejected entry: {}", reason));
                }
            }

            SyncPacket::Abort(abort) => {
                return Err(anyhow!("client aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }

    send_packet(
        &mut stream,
        SyncPacket::server_entries_end(sent),
    )?;

    /*
     * クライアントからの差分受信フェーズ
     */
    let mut received = 0u64;
    loop {
        match recv_packet(&mut stream)? {
            SyncPacket::ClientEntry(entry) => {
                let entry_id = entry.id().to_string();
                let res = {
                    let mut mgr = manager.borrow_mut();
                    mgr.put(&entry)
                };

                match res {
                    Ok(_) => {
                        send_packet(&mut stream,  SyncPacket::entry_ack(
                            entry_id,
                            true,
                            None
                        ))?;
                    }

                    Err(err) => {
                        send_packet(&mut stream, SyncPacket::entry_ack(
                            entry_id,
                            false,
                            Some(err.to_string()),
                        ))?;

                        send_packet(
                            &mut stream,
                            SyncPacket::abort("failed to apply client entry"),
                        )?;
                        return Err(anyhow!("apply client entry failed: {}", err));
                    }
                }

                received += 1;
            }

            SyncPacket::ClientEntriesEnd(end) => {
                if end.total_sent != received {
                    eprintln!(
                        "warning: client sent {} entries, header says {}",
                        received, end.total_sent
                    );
                }
                break;
            }

            SyncPacket::Abort(abort) => {
                return Err(anyhow!("client aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }

    /*
     * 正常終了通知
     */
    send_packet(&mut stream, SyncPacket::finished())?;
    Ok(())
}
