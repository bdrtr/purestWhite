//! A value on the heap whose allocation can fail, so that moving it moves a pointer: the parse
//! runs on an 8 KB task stack, and a paragraph or the XML parser are too big to move around it.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

pub(crate) struct Boxed<T>(Box<[T; 1]>);

impl<T> Boxed<T> {
    /// `None` when out of memory.
    pub(crate) fn try_new(value: T) -> Option<Self> {
        let mut one = Vec::new();
        one.try_reserve_exact(1).ok()?;
        one.push(value);
        one.into_boxed_slice().try_into().ok().map(Self)
    }
}

impl<T> Deref for Boxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        let [value] = &*self.0;
        value
    }
}

impl<T> DerefMut for Boxed<T> {
    fn deref_mut(&mut self) -> &mut T {
        let [value] = &mut *self.0;
        value
    }
}
