use anyhow::{anyhow, bail, Context, Result};
use cobs;
use postcard::{from_bytes_cobs, to_allocvec};
use serialport::SerialPort;
use siger_core::{Frame, Msg, Request, Response, PROTO_V1};
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

pub trait RW: Read + Write {}
impl<T: Read + Write> RW for T {}

pub fn open(port: &str, baud: u32) -> anyhow::Result<Box<dyn serialport::SerialPort>> {
    Ok(serialport::new(port, baud)
        .timeout(Duration::from_millis(30000))
        .open()?)
}

pub fn send_call(
    sp: &mut dyn serialport::SerialPort,
    id: u32,
    req: Request,
) -> anyhow::Result<Response> {
    send_recv(sp, id, Frame::One(req))
}

pub fn send_blob(
    sp: &mut dyn serialport::SerialPort,
    xid: u16,
    kind: siger_core::FragKind,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let total = bytes.len() as u32;
    let _: Response = send_recv(
        sp,
        0xF000_0000 | xid as u32,
        Frame::FragBegin {
            id: xid,
            total_len: total,
            kind,
        },
    )?;
    const CHUNK: usize = 200; // fits in 256B postcard frame
    let mut off = 0u32;
    while (off as usize) < bytes.len() {
        let end = core::cmp::min(bytes.len(), off as usize + CHUNK);
        let last = end == bytes.len();
        let chunk = bytes[off as usize..end].to_vec();
        let _: Response = send_recv(
            sp,
            0xF100_0000 | (xid as u32),
            Frame::FragPart {
                id: xid,
                offset: off,
                chunk,
                last,
            },
        )?;
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
        Response::FragBegin {
            id,
            total_len,
            kind,
        } => (rb.id, total_len, kind, id),
        other => return Err(anyhow!("expected outbound FragBegin, got {:?}", other)),
    };
    if kind2 != kind {
        return Err(anyhow!("outbound kind mismatch"));
    }

    // Collect parts
    let mut out = Vec::with_capacity(total as usize);
    let mut expect_off = 0u32;
    loop {
        let rp: Msg<Response> = recv_msg(sp, 8 * 1024)?;
        if rp.id != msg_id {
            return Err(anyhow!("msg id changed mid-stream"));
        }
        match rp.msg {
            Response::FragPart {
                id,
                offset,
                chunk,
                last,
            } => {
                if id != frag_id {
                    return Err(anyhow!("frag id mismatch"));
                }
                if offset != expect_off {
                    return Err(anyhow!("offset mismatch"));
                }
                out.extend_from_slice(&chunk);
                expect_off += chunk.len() as u32;
                if last {
                    break;
                }
            }
            other => return Err(anyhow!("expected FragPart, got {:?}", other)),
        }
    }
    if out.len() as u32 != total {
        return Err(anyhow!("truncated outbound frag"));
    }
    Ok(out)
}

pub fn send_recv<T: serde::Serialize, R: for<'de> serde::Deserialize<'de>>(
    sp: &mut dyn serialport::SerialPort,
    id: u32,
    msg: T,
) -> anyhow::Result<R> {
    let m = Msg {
        v: PROTO_V1,
        id,
        msg,
    };
    let buf = to_allocvec(&m)?;
    let mut framed = vec![0u8; cobs::max_encoding_length(buf.len()) + 1];
    let n = cobs::encode(&buf, &mut framed);
    framed.truncate(n);
    framed.push(0);
    sp.write_all(&framed)?;
    sp.flush()?;  // Force USB serial buffer to flush

    let mut rx: Vec<u8> = Vec::with_capacity(512);
    loop {
        let mut b = [0u8; 1];
        if sp.read_exact(&mut b).is_err() {
            continue;
        }
        if b[0] == 0 {
            break;
        }
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
                if b[0] == 0 {
                    break;
                }
            }
            Err(ref e) if e.kind() == ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let resp: Msg<Response> = from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp)
}

/// send one postcard-COBS Msg<Frame> and receive one Msg<Response>
/// - writes the request (already COBS-framed by postcard, includes trailing 0x00)
/// - reads until a full COBS frame (ends with 0x00) or overall deadline expires
pub fn round_trip_frame(sp: &mut dyn SerialPort, req: &Msg<Frame>) -> Result<Msg<Response>> {
    round_trip_frame_with_deadline(sp, req, Duration::from_secs(30))
}

/// Same as above, but with a caller-specified overall timeout.
pub fn round_trip_frame_with_deadline(
    sp: &mut dyn SerialPort,
    req: &Msg<Frame>,
    max_wait: Duration,
) -> Result<Msg<Response>> {
    // (A) Clear any stale bytes that might be sitting in the RX buffer.
    // We do this by doing non-fatal reads with a tiny timeout for ~50ms.
    let old_to = sp.timeout();
    sp.set_timeout(Duration::from_millis(50)).ok();
    let mut drain = [0u8; 256];
    for _ in 0..4 {
        match sp.read(&mut drain) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut => break,
            Err(_) => break,
        }
    }
    // restore caller’s timeout (if any). If you manage the timeout elsewhere, you can skip this.
    if let t = old_to {
        sp.set_timeout(t).ok();
    } else {
        // give a short per-read timeout so TimedOut is frequent and cheap
        sp.set_timeout(Duration::from_millis(150)).ok();
    }

    // (B) Encode + send request
    let buf = postcard::to_allocvec_cobs(req).context("encode COBS request")?;
    sp.write_all(&buf).context("write request")?;
    sp.flush().ok();

    // (C) Read until a complete COBS frame (ends with 0x00) or deadline.
    let start = Instant::now();
    let mut rx = Vec::<u8>::with_capacity(256);
    let mut tmp = [0u8; 256];

    loop {
        // overall deadline?
        if start.elapsed() > max_wait {
            bail!(
                "serial read: timed out waiting for COBS frame ({} ms)",
                max_wait.as_millis()
            );
        }

        match sp.read(&mut tmp) {
            Ok(n) if n > 0 => {
                rx.extend_from_slice(&tmp[..n]);
                // postcard COBS frames end with 0x00
                if rx.iter().any(|&b| b == 0) {
                    // keep everything up to and including the first 0x00 in case esp pipelined multiple frames
                    if let Some(pos) = rx.iter().position(|&b| b == 0) {
                        let mut frame = rx[..=pos].to_vec();
                        let resp: Msg<Response> = postcard::from_bytes_cobs(&mut frame)
                            .context("decode COBS response")?;
                        return Ok(resp);
                    }
                }
                if rx.len() > (1 << 20) {
                    bail!("response too large");
                }
            }
            Ok(_) => {
                // n == 0: nothing this tick; loop
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                // per-read timeout: harmless; keep waiting until overall deadline
                continue;
            }
            Err(e) => return Err(anyhow!("serial read: {e}")),
        }
    }
}
