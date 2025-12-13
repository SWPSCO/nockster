use anyhow::{anyhow, bail, Context, Result};
use postcard::from_bytes_cobs;
use serialport::SerialPort;
use siger_core::{Frame, Msg, Request, Response, PROTO_V1};
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

pub trait RW: Read + Write {}
impl<T: Read + Write> RW for T {}

pub fn open(port: &str, baud: u32) -> anyhow::Result<Box<dyn serialport::SerialPort>> {
    Ok(serialport::new(port, baud)
        // Keep per-read timeouts short; higher-level calls implement overall deadlines.
        .timeout(Duration::from_millis(250))
        .open()?)
}

pub fn send_call(
    sp: &mut dyn serialport::SerialPort,
    id: u32,
    req: Request,
) -> anyhow::Result<Response> {
    let req = Msg {
        v: PROTO_V1,
        id,
        msg: Frame::One(req),
    };
    Ok(round_trip_frame(sp, &req)?.msg)
}

pub fn send_blob(
    sp: &mut dyn serialport::SerialPort,
    xid: u16,
    kind: siger_core::FragKind,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let total = bytes.len() as u32;
    let req = Msg {
        v: PROTO_V1,
        id: 0xF000_0000 | xid as u32,
        msg: Frame::FragBegin {
            id: xid,
            total_len: total,
            kind,
        },
    };
    let begin_resp: Msg<Response> = round_trip_frame(sp, &req)?;
    if let Response::Err { code } = begin_resp.msg {
        return Err(anyhow!("device returned error code {code}"));
    }
    const CHUNK: usize = 200; // fits in 256B postcard frame
    let mut off = 0u32;
    while (off as usize) < bytes.len() {
        let end = core::cmp::min(bytes.len(), off as usize + CHUNK);
        let last = end == bytes.len();
        let chunk = bytes[off as usize..end].to_vec();
        let req = Msg {
            v: PROTO_V1,
            id: 0xF100_0000 | (xid as u32),
            msg: Frame::FragPart {
                id: xid,
                offset: off,
                chunk,
                last,
            },
        };
        let _: Response = round_trip_frame(sp, &req)?.msg;
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
    //
    // Note: the firmware may reply with `Response::FragBegin` directly to the
    // final `FragPart` (kicking off the outbound stream immediately). We
    // capture that and treat it as the outbound begin.
    let total = bytes.len() as u32;
    let req = Msg {
        v: PROTO_V1,
        id: 0xF000_0000 | xid as u32,
        msg: Frame::FragBegin {
            id: xid,
            total_len: total,
            kind,
        },
    };
    let _: Response = round_trip_frame(sp, &req)?.msg;

    const CHUNK: usize = 200; // fits in 256B postcard frame
    let mut off = 0u32;
    let mut outbound_begin: Option<(u32, u32, siger_core::FragKind, u16)> = None;
    while (off as usize) < bytes.len() {
        let end = core::cmp::min(bytes.len(), off as usize + CHUNK);
        let last = end == bytes.len();
        let chunk = bytes[off as usize..end].to_vec();
        let req = Msg {
            v: PROTO_V1,
            id: 0xF100_0000 | (xid as u32),
            msg: Frame::FragPart {
                id: xid,
                offset: off,
                chunk,
                last,
            },
        };
        let resp: Msg<Response> = round_trip_frame(sp, &req)?;
        if let Response::Err { code } = resp.msg {
            return Err(anyhow!("device returned error code {code}"));
        }
        if last {
            if let Response::FragBegin {
                id,
                total_len,
                kind: k,
            } = resp.msg
            {
                outbound_begin = Some((resp.id, total_len, k, id));
            }
        }
        off = end as u32;
    }

    let (msg_id, total, kind2, frag_id) = if let Some(v) = outbound_begin {
        v
    } else {
        // Expect outbound FragBegin as a separate message.
        let rb: Msg<Response> = recv_msg(sp, 8 * 1024)?;
        match rb.msg {
            Response::FragBegin {
                id,
                total_len,
                kind,
            } => (rb.id, total_len, kind, id),
            other => return Err(anyhow!("expected outbound FragBegin, got {:?}", other)),
        }
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
    // Long enough for user confirmation and PBKDF2 on-device.
    round_trip_frame_with_deadline(sp, req, Duration::from_secs(120))
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
    sp.set_timeout(Duration::from_millis(20)).ok();
    let mut drain = [0u8; 256];
    for _ in 0..4 {
        match sp.read(&mut drain) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut => break,
            Err(_) => break,
        }
    }
    // Give a short per-read timeout so TimedOut is frequent and cheap, and we
    // can enforce the overall deadline precisely.
    sp.set_timeout(Duration::from_millis(200)).ok();

    // (B) Encode + send request
    let buf = postcard::to_allocvec_cobs(req).context("encode COBS request")?;
    sp.write_all(&buf).context("write request")?;

    // (C) Read until a complete COBS frame (ends with 0x00) or deadline.
    let start = Instant::now();
    let resp = loop {
        let mut rx = Vec::<u8>::with_capacity(256);

        // Read exactly one COBS-delimited frame (ends with 0x00).
        loop {
            if start.elapsed() > max_wait {
                bail!(
                    "serial read: timed out waiting for response ({} ms)",
                    max_wait.as_millis()
                );
            }

            let mut b = [0u8; 1];
            match sp.read_exact(&mut b) {
                Ok(()) => {
                    rx.push(b[0]);
                    if rx.len() > (1 << 20) {
                        bail!("response too large");
                    }
                    if b[0] == 0 {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                Err(e) => return Err(anyhow!("serial read: {e}")),
            }
        }

        let mut frame = rx;
        let resp: Msg<Response> =
            postcard::from_bytes_cobs(&mut frame).context("decode COBS response")?;
        if resp.v != PROTO_V1 {
            return Err(anyhow!("bad protocol version {}", resp.v));
        }
        if resp.id != req.id {
            // Stray response from a previous request; keep waiting for ours.
            continue;
        }
        break resp;
    };

    // best-effort restore
    sp.set_timeout(old_to).ok();
    Ok(resp)
}
