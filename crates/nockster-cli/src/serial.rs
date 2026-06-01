use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};
use nockster_core::{Frame, Msg, Request, Response, PROTO_V1};
use std::io::{self, ErrorKind, Read, Write};
use std::time::{Duration, Instant};

pub const NOCKSTER_USB_VID: u16 = 0x303a;
pub const NOCKSTER_USB_PID: u16 = 0x2001;

const HID_REPORT_ID: u8 = 1;
const HID_REPORT_LEN: usize = 64; // includes report-id prefix
const HID_REPORT_DATA_LEN: usize = HID_REPORT_LEN - 1;
const HID_PAYLOAD_MAX: usize = HID_REPORT_DATA_LEN - 1; // first byte is payload length

pub trait Link: Read + Write {
    fn timeout(&self) -> Duration;
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()>;
}

struct SerialLink {
    inner: Box<dyn serialport::SerialPort>,
}

impl SerialLink {
    fn open(port: &str, baud: u32) -> Result<Self> {
        let inner = serialport::new(port, baud)
            // Keep per-read timeouts short; higher-level calls implement overall deadlines.
            .timeout(Duration::from_millis(250))
            .open()
            .with_context(|| format!("open serial port {port}"))?;
        Ok(Self { inner })
    }
}

impl Read for SerialLink {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for SerialLink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Link for SerialLink {
    fn timeout(&self) -> Duration {
        self.inner.timeout()
    }

    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.inner
            .set_timeout(timeout)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}

struct HidLink {
    dev: HidDevice,
    timeout: Duration,
    rx_buf: Vec<u8>,
    rx_pos: usize,
}

impl HidLink {
    fn open(vid: u16, pid: u16) -> Result<Self> {
        let api = HidApi::new().context("init hidapi")?;
        let dev = api
            .open(vid, pid)
            .with_context(|| format!("open hid device vid=0x{vid:04x} pid=0x{pid:04x}"))?;
        Ok(Self {
            dev,
            timeout: Duration::from_millis(250),
            rx_buf: Vec::new(),
            rx_pos: 0,
        })
    }

    fn compact_rx(&mut self) {
        if self.rx_pos == 0 {
            return;
        }
        if self.rx_pos >= self.rx_buf.len() {
            self.rx_buf.clear();
            self.rx_pos = 0;
            return;
        }
        self.rx_buf.drain(..self.rx_pos);
        self.rx_pos = 0;
    }

    fn refill(&mut self) -> io::Result<()> {
        let mut report = [0u8; HID_REPORT_LEN];
        let timeout_ms: i32 = self.timeout.as_millis().min(i32::MAX as u128) as i32;
        let n = self
            .dev
            .read_timeout(&mut report, timeout_ms)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if n == 0 {
            return Err(io::Error::new(ErrorKind::TimedOut, "hid read timeout"));
        }
        if n < 3 {
            // Must include report-id + len + at least 1 data byte.
            return Ok(());
        }
        if report[0] != HID_REPORT_ID {
            return Ok(());
        }
        let claimed = report[1] as usize;
        let available = core::cmp::min(claimed, HID_PAYLOAD_MAX).min(n - 2);
        if available == 0 {
            return Ok(());
        }

        self.compact_rx();
        self.rx_buf.extend_from_slice(&report[2..2 + available]);
        Ok(())
    }
}

impl Read for HidLink {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Ensure we have at least one byte available, or return TimedOut.
        while self.rx_pos >= self.rx_buf.len() {
            // If we already consumed everything, reset the buffer.
            self.rx_buf.clear();
            self.rx_pos = 0;
            self.refill()?;
        }

        let available = self.rx_buf.len() - self.rx_pos;
        let take = available.min(buf.len());
        buf[..take].copy_from_slice(&self.rx_buf[self.rx_pos..self.rx_pos + take]);
        self.rx_pos += take;
        Ok(take)
    }
}

impl Write for HidLink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut off = 0usize;
        while off < buf.len() {
            let take = (buf.len() - off).min(HID_PAYLOAD_MAX);
            let mut report = [0u8; HID_REPORT_LEN];
            report[0] = HID_REPORT_ID;
            report[1] = take as u8;
            report[2..2 + take].copy_from_slice(&buf[off..off + take]);
            for b in report[2 + take..].iter_mut() {
                *b = 0;
            }
            self.dev
                .write(&report)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            off += take;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Link for HidLink {
    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.timeout = timeout;
        Ok(())
    }
}

fn parse_hex_u16(mut s: &str) -> Option<u16> {
    s = s.trim();
    if let Some(rest) = s.strip_prefix("0x") {
        s = rest;
    }
    u16::from_str_radix(s, 16).ok()
}

fn parse_hid_selector(port: &str) -> Option<(u16, u16)> {
    let p = port.trim();
    if p.eq_ignore_ascii_case("hid") {
        return Some((NOCKSTER_USB_VID, NOCKSTER_USB_PID));
    }
    let lower = p.to_ascii_lowercase();
    let rest = lower.strip_prefix("hid:")?;
    let mut it = rest.split(':');
    let vid = parse_hex_u16(it.next()?)?;
    let pid = parse_hex_u16(it.next()?)?;
    Some((vid, pid))
}

pub fn open(port: &str, baud: u32) -> Result<Box<dyn Link>> {
    if let Some((vid, pid)) = parse_hid_selector(port) {
        return Ok(Box::new(HidLink::open(vid, pid)?));
    }
    Ok(Box::new(SerialLink::open(port, baud)?))
}

pub fn send_call(sp: &mut dyn Link, id: u32, req: Request) -> anyhow::Result<Response> {
    let req = Msg {
        v: PROTO_V1,
        id,
        msg: Frame::One(req),
    };
    Ok(round_trip_frame(sp, &req)?.msg)
}

pub fn send_blob(
    sp: &mut dyn Link,
    xid: u16,
    kind: nockster_core::FragKind,
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
    sp: &mut dyn Link,
    xid: u16,
    kind: nockster_core::FragKind,
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
    let mut outbound_begin: Option<(u32, u32, nockster_core::FragKind, u16)> = None;
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
pub fn recv_msg(sp: &mut dyn Link, max_len: usize) -> Result<Msg<Response>> {
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
    let resp: Msg<Response> = postcard::from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp)
}

/// send one postcard-COBS Msg<Frame> and receive one Msg<Response>
/// - writes the request (already COBS-framed by postcard, includes trailing 0x00)
/// - reads until a full COBS frame (ends with 0x00) or overall deadline expires
pub fn round_trip_frame(sp: &mut dyn Link, req: &Msg<Frame>) -> Result<Msg<Response>> {
    // Long enough for user confirmation and PBKDF2 on-device.
    round_trip_frame_with_deadline(sp, req, Duration::from_secs(120))
}

/// Same as above, but with a caller-specified overall timeout.
pub fn round_trip_frame_with_deadline(
    sp: &mut dyn Link,
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
    let mut bad_cobs_frames: usize = 0;
    let mut last_cobs_err: Option<String> = None;
    let resp = loop {
        let mut rx = Vec::<u8>::with_capacity(256);

        // Read exactly one COBS-delimited frame (ends with 0x00).
        loop {
            if start.elapsed() > max_wait {
                if bad_cobs_frames > 0 {
                    if let Some(err) = last_cobs_err.as_deref() {
                        bail!(
                            "serial read: timed out waiting for response ({} ms); ignored {} malformed COBS frame(s) (last decode error: {})",
                            max_wait.as_millis(),
                            bad_cobs_frames,
                            err
                        );
                    }
                    bail!(
                        "serial read: timed out waiting for response ({} ms); ignored {} malformed COBS frame(s)",
                        max_wait.as_millis(),
                        bad_cobs_frames
                    );
                }
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
        let resp: Msg<Response> = match postcard::from_bytes_cobs(&mut frame) {
            Ok(v) => v,
            Err(e) => {
                bad_cobs_frames = bad_cobs_frames.saturating_add(1);
                last_cobs_err = Some(e.to_string());
                // Keep waiting; a stray empty/garbled frame can happen during
                // resets or when multiple host tools are racing the port.
                continue;
            }
        };
        if resp.id != req.id {
            // Stray response from a previous request; keep waiting for ours.
            continue;
        }
        if resp.v != PROTO_V1 {
            return Err(anyhow!("bad protocol version {}", resp.v));
        }
        break resp;
    };

    // best-effort restore
    sp.set_timeout(old_to).ok();
    Ok(resp)
}
