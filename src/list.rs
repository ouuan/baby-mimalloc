use core::ptr::{null_mut, NonNull};

pub struct LinkedList<T> {
    first: *mut T,
    last: *mut T,
}

pub trait LinkedListItem {
    fn prev(&self) -> *mut Self;

    fn next(&self) -> *mut Self;

    fn set_prev(&mut self, prev: *mut Self);

    fn set_next(&mut self, next: *mut Self);
}

impl<T> LinkedList<T> {
    pub const fn new() -> Self {
        Self {
            first: null_mut(),
            last: null_mut(),
        }
    }

    pub const fn first(&self) -> *mut T {
        self.first
    }
}

impl<T: LinkedListItem> LinkedList<T> {
    /// Push a new element at the beginning of the list.
    pub unsafe fn push_front(&mut self, mut el: NonNull<T>) {
        el.as_mut().set_next(self.first);
        el.as_mut().set_prev(null_mut());

        if let Some(first) = self.first.as_mut() {
            first.set_prev(el.as_ptr());
        } else {
            self.last = el.as_ptr();
        }

        self.first = el.as_ptr();
    }

    /// Push a new element at the end of the list.
    ///
    /// Returns whether the first element of the list is updated.
    pub unsafe fn push_back(&mut self, mut el: NonNull<T>) -> bool {
        el.as_mut().set_prev(self.last);
        el.as_mut().set_next(null_mut());

        let result = if let Some(last) = self.last.as_mut() {
            last.set_next(el.as_ptr());
            false
        } else {
            self.first = el.as_ptr();
            true
        };

        self.last = el.as_ptr();

        result
    }

    /// Remove an element from the list. The element must be in the list.
    ///
    /// Returns whether the first element of the list is updated.
    pub unsafe fn remove(&mut self, mut el: NonNull<T>) -> bool {
        if let Some(prev) = el.as_ref().prev().as_mut() {
            prev.set_next(el.as_ref().next());
        }
        if let Some(next) = el.as_ref().next().as_mut() {
            next.set_prev(el.as_ref().prev());
        }
        let first_updated = el.as_ptr() == self.first;
        if first_updated {
            self.first = el.as_ref().next();
        }
        if el.as_ptr() == self.last {
            self.last = el.as_ref().prev();
        }
        el.as_mut().set_prev(null_mut());
        el.as_mut().set_next(null_mut());
        first_updated
    }

    /// Check if an element is in the list. The element must not be in another list.
    pub fn contains(&self, el: &T) -> bool {
        !el.next().is_null() || !el.prev().is_null() || el as *const _ == self.first
    }
}

macro_rules! impl_list_item {
    ($name: ident) => {
        impl crate::list::LinkedListItem for $name {
            fn prev(&self) -> *mut Self {
                self.prev
            }

            fn next(&self) -> *mut Self {
                self.next
            }

            fn set_prev(&mut self, prev: *mut Self) {
                self.prev = prev
            }

            fn set_next(&mut self, next: *mut Self) {
                self.next = next
            }
        }
    };
}

pub(crate) use impl_list_item;
