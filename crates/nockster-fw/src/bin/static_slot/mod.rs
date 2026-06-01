use core::cell::UnsafeCell;

pub struct StaticSlot<T> {
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for StaticSlot<T> {}

impl<T> StaticSlot<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    pub unsafe fn as_mut(&self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }
}
