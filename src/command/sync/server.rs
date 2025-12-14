/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

//! サーバ側の同期処理

use std::net::TcpListener;

use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};

use crate::command::sync::{
    recv_packet, send_packet, NodeRole, SyncPacket, PROTOCOL_VERSION,
};
use crate::database::{TransactionReadable, TransactionWriter};

/*
 * サーバモードのエントリーポイント
 */
pub(super) fn run(addr: &str, writer: &mut TransactionWriter) -> Result<()> {
    /*
     * クライアントの接続待ち受け
     */
    let listener = TcpListener::bind(addr)
        .with_context(|| format!("bind {}", addr))?;

    let (mut stream, peer) = listener.accept().context("accept")?;
    info!("client connected: {}", peer);

    /*
     * Helloの受信と検証
     */
    let hello = match recv_packet(&mut stream)? {
        SyncPacket::Hello(h) => h,
        pkt => return Err(anyhow!("unexpected packet: {:?}", pkt)),
    };
    debug!(
        "recv Hello: proto={}, role={:?}, node={}",
        hello.protocol_version, hello.role, hello.node_id
    );

    if hello.protocol_version != PROTOCOL_VERSION {
        send_packet(&mut stream, SyncPacket::hello_ack(
            PROTOCOL_VERSION,
            false,
            Some("protocol version mismatch".into()),
        ))?;

        error!("protocol version mismatch: peer={}", hello.protocol_version);
        return Err(anyhow!("protocol version mismatch"));
    }

    if hello.role != NodeRole::Client {
        send_packet(&mut stream, SyncPacket::hello_ack(
            PROTOCOL_VERSION,
            false,
            Some("role mismatch".into()),
        ))?;

        error!("unexpected role from peer: {:?}", hello.role);
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
    info!("sent HelloAck: accept");

    /*
     * エントリ送信フェーズ（全件送信し、エントリごとにACKを受信）
     */
    let ids = writer.all_service()?;
    info!("server send phase start: {} entries", ids.len());
    let mut sent = 0u64;
    for id in ids {
        let entry = writer.get(&id)?
            .ok_or_else(|| anyhow!("missing entry during send"))?;
        debug!(
            "send entry to client: id={}, service={}",
            entry.id(),
            entry.service()
        );

        send_packet(&mut stream, SyncPacket::server_entry(entry))?;
        sent += 1;

        match recv_packet(&mut stream)? {
            SyncPacket::EntryAck(ack) => {
                if !ack.accepted {
                    let reason = ack.reason.unwrap_or_else(|| "rejected".into());
                    send_packet(&mut stream, SyncPacket::abort(reason.clone()))?;

                    error!("client rejected entry id={}: {}", ack.entry_id, reason);
                    return Err(anyhow!("client rejected entry: {}", reason));
                }
            }

            SyncPacket::Abort(abort) => {
                error!("client aborted during send: {}", abort.reason);
                return Err(anyhow!("client aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }

    send_packet(
        &mut stream,
        SyncPacket::server_entries_end(sent),
    )?;
    info!("server send phase end: {} entries sent", sent);

    /*
     * クライアントからの差分受信フェーズ
     */
    info!("server receive phase start");
    let mut received = 0u64;
    loop {
        match recv_packet(&mut stream)? {
            SyncPacket::ClientEntry(entry) => {
                debug!(
                    "recv entry from client: id={}, service={}",
                    entry.id(),
                    entry.service()
                );
                match writer.put(&entry) {
                    Ok(_) => {
                        send_packet(&mut stream,  SyncPacket::entry_ack(
                            entry.id(),
                            true,
                            None
                        ))?;
                        debug!(
                            "applied client entry: id={}, service={}",
                            entry.id(),
                            entry.service()
                        );
                    }

                    Err(err) => {
                        send_packet(&mut stream, SyncPacket::entry_ack(
                            entry.id(),
                            false,
                            Some(err.to_string()),
                        ))?;

                        send_packet(
                            &mut stream,
                            SyncPacket::abort("failed to apply client entry"),
                        )?;
                        error!(
                            "failed to apply client entry id={}: {}",
                            entry.id(),
                            err
                        );
                        return Err(anyhow!("apply client entry failed: {}", err));
                    }
                }

                received += 1;
            }

            SyncPacket::ClientEntriesEnd(end) => {
                if end.total_sent != received {
                    warn!(
                        "client sent {} entries, header says {}",
                        received, end.total_sent
                    );
                }
                break;
            }

            SyncPacket::Abort(abort) => {
                error!("client aborted during receive: {}", abort.reason);
                return Err(anyhow!("client aborted: {}", abort.reason));
            }

            other => return Err(anyhow!("unexpected packet: {:?}", other)),
        }
    }

    /*
     * 正常終了通知
     */
    send_packet(&mut stream, SyncPacket::finished())?;
    info!("server receive phase end: {} entries received", received);
    info!("server finished sync");

    /*
     * 終了
     */
    Ok(())
}
