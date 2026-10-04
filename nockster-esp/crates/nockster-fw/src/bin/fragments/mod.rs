use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;
use nockster_core::FragKind;

// Maximum total size for fragment-assembled payloads (e.g. SignDraft).
// Keep this bounded to avoid unbounded heap growth on malformed hosts.
pub const MAX_TOTAL: usize = 64 * 1024;
pub const TX_CHUNK: usize = 512;

pub struct Inbound {
    pub id: u16,
    pub kind: FragKind,
    pub total_len: u32,
    pub next_off: u32,
    pub buf: Vec<u8>,
}

impl Inbound {
    pub fn new(id: u16, kind: FragKind, total_len: u32) -> Result<Self, ()> {
        let mut buf = Vec::new();
        buf.try_reserve_exact(total_len as usize).map_err(|_| ())?;
        Ok(Self {
            id,
            kind,
            total_len,
            next_off: 0,
            buf,
        })
    }
}

pub struct Outbound {
    pub msg_id: u32,
    pub id: u16,
    pub off: u32,
    pub data: Vec<u8>,
}

impl Outbound {
    pub fn new(msg_id: u32, id: u16, data: Vec<u8>) -> Self {
        Self {
            msg_id,
            id,
            off: 0,
            data,
        }
    }
}

#[allow(clippy::declare_interior_mutable_const)]
static INBOUND: Mutex<RefCell<Option<Inbound>>> = Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static OUTBOUND: Mutex<RefCell<Option<Outbound>>> = Mutex::new(RefCell::new(None));

#[inline]
pub fn begin_inbound(id: u16, kind: FragKind, total_len: u32) -> Result<(), ()> {
    if (total_len as usize) > MAX_TOTAL {
        return Err(());
    }

    let state = Inbound::new(id, kind, total_len)?;
    critical_section::with(|cs| {
        *INBOUND.borrow_ref_mut(cs) = Some(state);
    });
    Ok(())
}

#[inline]
pub fn take_inbound() -> Option<Inbound> {
    critical_section::with(|cs| INBOUND.borrow_ref_mut(cs).take())
}

#[inline]
pub fn put_inbound(state: Inbound) {
    critical_section::with(|cs| {
        *INBOUND.borrow_ref_mut(cs) = Some(state);
    });
}

#[inline]
pub fn set_outbound(msg_id: u32, id: u16, data: Vec<u8>) {
    critical_section::with(|cs| {
        *OUTBOUND.borrow_ref_mut(cs) = Some(Outbound::new(msg_id, id, data));
    });
}

#[inline]
pub fn take_outbound() -> Option<Outbound> {
    critical_section::with(|cs| OUTBOUND.borrow_ref_mut(cs).take())
}

#[inline]
pub fn put_outbound(state: Outbound) {
    critical_section::with(|cs| {
        *OUTBOUND.borrow_ref_mut(cs) = Some(state);
    });
}
