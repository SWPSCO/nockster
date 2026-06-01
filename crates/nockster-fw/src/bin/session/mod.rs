use core::cell::RefCell;
use critical_section::Mutex;
use heapless::Vec as HVec;
use nockster_core::MAX_SEED_SLOTS;
use zeroize::Zeroize;

struct State {
    locked: bool,
    master_key: [u8; 32],
    master_key_set: bool,
    slots: HVec<[u8; 64], MAX_SEED_SLOTS>,
    active: usize,
}

impl State {
    const fn new() -> Self {
        Self {
            locked: true,
            master_key: [0; 32],
            master_key_set: false,
            slots: HVec::new(),
            active: 0,
        }
    }

    fn zeroize_seed_slots(&mut self) {
        for seed in self.slots.iter_mut() {
            seed.zeroize();
        }
        self.slots.clear();
    }
}

#[allow(clippy::declare_interior_mutable_const)]
static SESSION: Mutex<RefCell<State>> = Mutex::new(RefCell::new(State::new()));

#[inline]
pub fn is_locked() -> bool {
    critical_section::with(|cs| SESSION.borrow_ref(cs).locked)
}

#[inline]
pub fn set_locked(locked: bool) {
    critical_section::with(|cs| {
        SESSION.borrow_ref_mut(cs).locked = locked;
    });
}

#[inline]
pub fn has_seed() -> bool {
    critical_section::with(|cs| !SESSION.borrow_ref(cs).slots.is_empty())
}

#[inline]
pub fn master_key_copy() -> Option<[u8; 32]> {
    critical_section::with(|cs| {
        let state = SESSION.borrow_ref(cs);
        state.master_key_set.then_some(state.master_key)
    })
}

#[inline]
pub fn store_master_key(key: &[u8; 32]) {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        state.master_key.copy_from_slice(key);
        state.master_key_set = true;
    });
}

#[inline]
pub fn clear_master_key() {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        state.master_key.zeroize();
        state.master_key_set = false;
    });
}

#[inline]
pub fn set_seed(seed64: &[u8; 64]) {
    update_seed_store_from_slice(core::slice::from_ref(seed64));
}

#[inline]
pub fn update_seed_store_from_slice(seeds: &[[u8; 64]]) {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        state.zeroize_seed_slots();
        for seed in seeds {
            let _ = state.slots.push(*seed);
        }
        state.active = 0;
        state.locked = state.slots.is_empty();
    });
}

#[inline]
pub fn append_seed_slot(seed64: &[u8; 64]) {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        if state.slots.len() < MAX_SEED_SLOTS {
            let _ = state.slots.push(*seed64);
        }
    });
}

#[inline]
pub fn remove_seed_slot(index: usize) {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        if index >= state.slots.len() {
            return;
        }

        let len = state.slots.len();
        state.slots[index].zeroize();
        let mut i = index;
        while i + 1 < len {
            state.slots[i] = state.slots[i + 1];
            i += 1;
        }
        if let Some(mut removed) = state.slots.pop() {
            removed.zeroize();
        }
        if state.active >= state.slots.len() {
            state.active = state.slots.len().saturating_sub(1);
        }
        state.locked = state.slots.is_empty();
    });
}

#[inline]
pub fn wipe() {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        state.zeroize_seed_slots();
        state.active = 0;
        state.locked = true;
        state.master_key.zeroize();
        state.master_key_set = false;
    });
}

#[inline]
pub fn seed_slots_copy() -> HVec<[u8; 64], MAX_SEED_SLOTS> {
    critical_section::with(|cs| {
        let state = SESSION.borrow_ref(cs);
        let mut out = HVec::new();
        for seed in state.slots.iter() {
            let _ = out.push(*seed);
        }
        out
    })
}

#[inline]
pub fn get_seed_for_slot(slot: usize) -> Result<[u8; 64], ()> {
    critical_section::with(|cs| {
        let state = SESSION.borrow_ref(cs);
        if slot >= state.slots.len() {
            return Err(());
        }
        Ok(state.slots[slot])
    })
}

#[inline]
pub fn get_active_seed_copy() -> Result<[u8; 64], ()> {
    critical_section::with(|cs| {
        let state = SESSION.borrow_ref(cs);
        if state.slots.is_empty() {
            return Err(());
        }
        let idx = state.active.min(state.slots.len() - 1);
        Ok(state.slots[idx])
    })
}

#[inline]
pub fn set_active_slot(slot: usize) -> Result<(), ()> {
    critical_section::with(|cs| {
        let mut state = SESSION.borrow_ref_mut(cs);
        if slot >= state.slots.len() {
            return Err(());
        }
        state.active = slot;
        Ok(())
    })
}

#[inline]
pub fn active_slot_index() -> Result<usize, ()> {
    critical_section::with(|cs| {
        let state = SESSION.borrow_ref(cs);
        if state.slots.is_empty() {
            return Err(());
        }
        Ok(state.active.min(state.slots.len() - 1))
    })
}
