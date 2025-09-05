use anyhow::{anyhow, Result};
use postcard::{from_bytes_cobs, to_allocvec};
use serialport;
use std::io::{Read, Write};
use std::time::Duration;

use cobs;
use siger_core::{Msg, Request, Response, Frame, PROTO_V1};

pub trait RW: Read + Write {}
impl<T: Read + Write> RW for T {}

pub fn open(port: &str, baud: u32) -> anyhow::Result<Box<dyn serialport::SerialPort>> {
    Ok(serialport::new(port, baud)
        .timeout(Duration::from_millis(200))
        .open()?)
}

pub fn send_call(sp: &mut dyn serialport::SerialPort, id: u32, req: Request) -> anyhow::Result<Response> {
    send_recv(sp, id, Frame::One(req))
}

pub fn send_blob(sp: &mut dyn serialport::SerialPort, xid: u16, kind: siger_core::FragKind, bytes: &[u8]) -> anyhow::Result<()> {
    let total = bytes.len() as u32;
    let _: Response = send_recv(sp, 0xF000_0000 | xid as u32, Frame::FragBegin { id: xid, total_len: total, kind })?;
    const CHUNK: usize = 200; // fits in 256B postcard frame
    let mut off = 0u32;
    while (off as usize) < bytes.len() {
        let end = core::cmp::min(bytes.len(), off as usize + CHUNK);
        let last = end == bytes.len();
        let chunk = bytes[off as usize..end].to_vec();
        let _: Response = send_recv(sp, 0xF100_0000 | (xid as u32), Frame::FragPart { id: xid, offset: off, chunk, last })?;
        off = end as u32;
    }
    Ok(())
}

pub fn send_blob_and_recv_outbound(
    sp: &mut dyn serialport::SerialPort,
    xid: u16,
    kind: siger_core::FragKind,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    // Inbound frag (host -> device)
    send_blob(sp, xid, kind, bytes)?;

    // Expect outbound FragBegin
    let rb: Msg<Response> = recv_msg(sp, 8 * 1024)?;
    let (msg_id, total, kind2, frag_id) = match rb.msg {
        Response::FragBegin { id, total_len, kind } => (rb.id, total_len, kind, id),
        other => return Err(anyhow!("expected outbound FragBegin, got {:?}", other)),
    };
    if kind2 != kind { return Err(anyhow!("outbound kind mismatch")); }

    // Collect parts
    let mut out = Vec::with_capacity(total as usize);
    let mut expect_off = 0u32;
    loop {
        let rp: Msg<Response> = recv_msg(sp, 8 * 1024)?;
        if rp.id != msg_id { return Err(anyhow!("msg id changed mid-stream")); }
        match rp.msg {
            Response::FragPart { id, offset, chunk, last } => {
                if id != frag_id { return Err(anyhow!("frag id mismatch")); }
                if offset != expect_off { return Err(anyhow!("offset mismatch")); }
                out.extend_from_slice(&chunk);
                expect_off += chunk.len() as u32;
                if last { break; }
            }
            other => return Err(anyhow!("expected FragPart, got {:?}", other)),
        }
    }
    if out.len() as u32 != total { return Err(anyhow!("truncated outbound frag")); }
    Ok(out)
}

pub fn send_recv<T: serde::Serialize, R: for<'de> serde::Deserialize<'de>>(
    sp: &mut dyn serialport::SerialPort,
    id: u32,
    msg: T,
) -> anyhow::Result<R> {
    let m = Msg { v: PROTO_V1, id, msg };
    let buf = to_allocvec(&m)?;
    let mut framed = vec![0u8; cobs::max_encoding_length(buf.len()) + 1];
    let n = cobs::encode(&buf, &mut framed);
    framed.truncate(n);
    framed.push(0);
    sp.write_all(&framed)?;

    let mut rx: Vec<u8> = Vec::with_capacity(512);
    loop {
        let mut b = [0u8; 1];
        if sp.read_exact(&mut b).is_err() { continue; }
        if b[0] == 0 { break; }
        rx.push(b[0]);
    }
    let resp: Msg<R> = from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp.msg)
}

// Read a single Msg<Response> from the wire (COBS-delimited)
pub fn recv_msg(sp: &mut dyn serialport::SerialPort, max_len: usize) -> Result<Msg<Response>> {
    let mut rx: Vec<u8> = Vec::with_capacity(256);
    loop {
        let mut b = [0u8; 1];
        match sp.read_exact(&mut b) {
            Ok(()) => {
                rx.push(b[0]);
                if rx.len() > max_len {
                    return Err(anyhow!("frame too large (> {} bytes)", max_len));
                }
                if b[0] == 0 { break; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let resp: Msg<Response> = from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp)
}
