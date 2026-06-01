use core::cell::RefCell;
use critical_section::Mutex;
use heapless::Vec as HVec;
use usbd_hid::hid_class::HIDClass;

pub const PLAIN_BUF_LEN: usize = 4096;
pub const ENC_BUF_LEN: usize = cobs::max_encoding_length(PLAIN_BUF_LEN) + 1;
const TX_QUEUE_LEN: usize = ENC_BUF_LEN * 3;

const REPORT_ID: u8 = 1;
pub const REPORT_TOTAL_LEN: usize = 64; // includes report-id prefix
const REPORT_DATA_LEN: usize = REPORT_TOTAL_LEN - 1; // excluding report-id
const PAYLOAD_MAX: usize = REPORT_DATA_LEN - 1; // first byte is payload length
pub const MAX_TX_REPORTS_PER_TICK: usize = 4;
pub const MAX_RX_REPORTS_PER_TICK: usize = 8;

// Vendor-defined HID report:
// report = [report_id=1][len][data...]
// - len is the number of valid bytes in data (0..=PAYLOAD_MAX)
// - data is a byte-stream containing postcard+COBS frames delimited by 0x00
pub const REPORT_DESCRIPTOR: &[u8] = &[
    0x06,
    0x00,
    0xFF, // USAGE_PAGE (Vendor Defined 0xFF00)
    0x09,
    0x01, // USAGE (0x01)
    0xA1,
    0x01, // COLLECTION (Application)
    0x85,
    REPORT_ID, // REPORT_ID (1)
    0x15,
    0x00, // LOGICAL_MINIMUM (0)
    0x26,
    0xFF,
    0x00, // LOGICAL_MAXIMUM (255)
    0x75,
    0x08, // REPORT_SIZE (8)
    0x95,
    REPORT_DATA_LEN as u8, // REPORT_COUNT (63)
    0x09,
    0x01, // USAGE (0x01)
    0x81,
    0x02, // INPUT (Data,Var,Abs)
    0x09,
    0x01, // USAGE (0x01)
    0x91,
    0x02, // OUTPUT (Data,Var,Abs)
    0xC0, // END_COLLECTION
];

struct TxQueue {
    buf: HVec<u8, TX_QUEUE_LEN>,
    pos: usize,
}

#[allow(clippy::declare_interior_mutable_const)]
static TX_QUEUE: Mutex<RefCell<Option<TxQueue>>> = Mutex::new(RefCell::new(None));

#[inline]
pub fn service_tx<B: usb_device::bus::UsbBus>(hid: &mut HIDClass<'_, B>) {
    for _ in 0..MAX_TX_REPORTS_PER_TICK {
        let mut report = [0u8; REPORT_TOTAL_LEN];
        let Some(take) = prepare_next_report(&mut report) else {
            return;
        };

        match hid.push_raw_input(&report) {
            Ok(_) => mark_report_sent(take),
            Err(usb_device::UsbError::WouldBlock | usb_device::UsbError::InvalidState) => return,
            Err(_) => {
                drop_tx_queue();
                return;
            }
        }
    }
}

#[inline]
fn prepare_next_report(report: &mut [u8; REPORT_TOTAL_LEN]) -> Option<usize> {
    critical_section::with(|cs| {
        let mut slot = TX_QUEUE.borrow_ref_mut(cs);
        if let Some(queue) = slot.as_mut() {
            if queue.pos >= queue.buf.len() {
                *slot = None;
                return None;
            }

            report[0] = REPORT_ID;
            let remaining = queue.buf.len() - queue.pos;
            let take = remaining.min(PAYLOAD_MAX);
            report[1] = take as u8;
            report[2..2 + take].copy_from_slice(&queue.buf[queue.pos..queue.pos + take]);
            for b in report[2 + take..].iter_mut() {
                *b = 0;
            }
            Some(take)
        } else {
            None
        }
    })
}

#[inline]
fn mark_report_sent(take: usize) {
    critical_section::with(|cs| {
        let mut slot = TX_QUEUE.borrow_ref_mut(cs);
        if let Some(queue) = slot.as_mut() {
            queue.pos = queue.pos.saturating_add(take).min(queue.buf.len());
            if queue.pos >= queue.buf.len() {
                *slot = None;
            }
        }
    });
}

#[inline]
fn drop_tx_queue() {
    critical_section::with(|cs| {
        *TX_QUEUE.borrow_ref_mut(cs) = None;
    });
}

#[inline]
fn compact(queue: &mut TxQueue) {
    if queue.pos == 0 {
        return;
    }
    if queue.pos >= queue.buf.len() {
        queue.buf.clear();
        queue.pos = 0;
        return;
    }

    let remaining = queue.buf.len() - queue.pos;
    for i in 0..remaining {
        queue.buf[i] = queue.buf[queue.pos + i];
    }
    queue.buf.truncate(remaining);
    queue.pos = 0;
}

#[inline]
pub fn write<B: usb_device::bus::UsbBus>(hid: &mut HIDClass<'_, B>, buf: &[u8]) -> bool {
    if buf.len() > TX_QUEUE_LEN {
        return false;
    }

    service_tx(hid);
    if enqueue(buf) {
        service_tx(hid);
        return true;
    }

    service_tx(hid);
    let enqueued = enqueue(buf);
    service_tx(hid);
    enqueued
}

#[inline]
fn enqueue(buf: &[u8]) -> bool {
    critical_section::with(|cs| {
        let mut slot = TX_QUEUE.borrow_ref_mut(cs);
        let queue = slot.get_or_insert_with(|| TxQueue {
            buf: HVec::new(),
            pos: 0,
        });
        compact(queue);

        if queue.buf.len().saturating_add(buf.len()) <= TX_QUEUE_LEN {
            let _ = queue.buf.extend_from_slice(buf);
            true
        } else {
            false
        }
    })
}

#[inline]
pub fn debug<B: usb_device::bus::UsbBus>(hid: &mut HIDClass<'_, B>, msg: &[u8]) {
    #[cfg(debug_assertions)]
    {
        write(hid, msg);
    }
    let _ = (hid, msg);
}

#[inline]
pub fn output_payload(report: &[u8; REPORT_TOTAL_LEN], len: usize) -> Option<&[u8]> {
    if len < 2 || report[0] != REPORT_ID {
        return None;
    }
    let payload_len = report[1] as usize;
    let available = core::cmp::min(payload_len, len.saturating_sub(2)).min(PAYLOAD_MAX);
    Some(&report[2..2 + available])
}
