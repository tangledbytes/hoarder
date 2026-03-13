extern crate alloc;

use core::{
    alloc::Layout,
    mem::MaybeUninit,
    ops::{Deref, Index, IndexMut},
    ptr::NonNull,
};

use hoarder_common::error::{HoarderError, Result};

pub struct Array<T> {
    ptr: NonNull<T>,
    size: usize,
}

impl<T> Array<T>
where
    T: Copy,
{
    pub fn new(init: T, size: usize) -> Self {
        assert!(size > 0 && size < isize::MAX as usize);
        let layout = Layout::array::<T>(size).unwrap();

        let ptr = unsafe {
            let raw_ptr = alloc::alloc::alloc(layout) as *mut T;
            if raw_ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }

            // Initialize the memory
            for i in 0..size {
                core::ptr::write(raw_ptr.add(i), init);
            }

            NonNull::new_unchecked(raw_ptr)
        };
        Self { ptr, size }
    }
}

impl<T> Array<T> {
    pub fn from_fixed_iter(size: usize, mut iter: impl Iterator<Item = T>) -> Self {
        assert!(size < isize::MAX as usize);
        let layout = Layout::array::<T>(size).unwrap();

        let ptr = unsafe {
            let raw_ptr = alloc::alloc::alloc(layout) as *mut T;
            if raw_ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }

            // Initialize the memory
            for i in 0..size {
                core::ptr::write(raw_ptr.add(i), iter.next().unwrap());
            }

            NonNull::new_unchecked(raw_ptr)
        };

        Self { ptr, size }
    }

    pub fn len(&self) -> usize {
        self.size
    }
}

impl<T> Drop for Array<T> {
    fn drop(&mut self) {
        let layout = Layout::array::<T>(self.size).unwrap();
        unsafe {
            alloc::alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}

impl<T> Index<usize> for Array<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.size);
        unsafe { &*self.ptr.as_ptr().add(index) }
    }
}

impl<T> IndexMut<usize> for Array<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.size);
        unsafe { &mut *self.ptr.as_ptr().add(index) }
    }
}

impl<T> Deref for Array<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size) }
    }
}

pub struct RingBuffer<T> {
    ptr: NonNull<MaybeUninit<T>>,
    write_idx: usize,
    read_idx: usize,
    size: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0 && size < isize::MAX as usize);
        let layout = Layout::array::<MaybeUninit<T>>(size).unwrap();
        let ptr = unsafe {
            let raw_ptr = alloc::alloc::alloc(layout) as *mut MaybeUninit<T>;
            if raw_ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }
            NonNull::new_unchecked(raw_ptr)
        };
        Self {
            ptr,
            write_idx: 0,
            read_idx: 0,
            size,
            len: 0,
        }
    }

    pub fn from_fixed_iter(size: usize, mut iter: impl Iterator<Item = T>) -> Self {
        let mut buffer = Self::new(size);

        for _ in 0..size {
            let item = iter
                .next()
                .expect("Iterator yielded fewer elements than `size`");

            buffer.push(item).unwrap();
        }

        buffer
    }

    pub fn push(&mut self, data: T) -> Result<()> {
        if !self.is_full() {
            let a = unsafe { &mut *self.ptr.as_ptr().add(self.write_idx) };
            a.write(data);
            self.write_idx = (self.write_idx + 1) % self.size;
            self.len += 1;
            Ok(())
        } else {
            Err(HoarderError::PushError)
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if !self.is_empty() {
            let a = unsafe { &mut *self.ptr.as_ptr().add(self.read_idx) };
            self.read_idx = (self.read_idx + 1) % self.size;
            self.len -= 1;
            Some(unsafe { a.assume_init_read() })
        } else {
            None
        }
    }

    pub fn is_full(&self) -> bool {
        self.len == self.size
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn capacity(&self) -> usize {
        self.size
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop() {
            // pop() takes ownership and immediately drops the item
        }

        let layout = Layout::array::<MaybeUninit<T>>(self.size).unwrap();
        unsafe {
            alloc::alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    // ==========================================
    // Array Tests
    // ==========================================

    #[test]
    fn test_array_initialization() {
        let arr = Array::new(42, 5);
        assert_eq!(arr.len(), 5);
        for i in 0..5 {
            assert_eq!(arr[i], 42);
        }
    }

    #[test]
    fn test_array_mutation() {
        let mut arr = Array::new(0, 3);
        arr[0] = 10;
        arr[1] = 20;
        arr[2] = 30;

        assert_eq!(arr[0], 10);
        assert_eq!(arr[1], 20);
        assert_eq!(arr[2], 30);
    }

    #[test]
    fn test_array_deref_slice() {
        let arr = Array::new(7, 4);
        let slice: &[i32] = &*arr; // Test Deref
        assert_eq!(slice, &[7, 7, 7, 7]);
    }

    // ==========================================
    // RingBuffer Tests
    // ==========================================

    #[test]
    fn test_ring_buffer_basic_push_pop() {
        let mut rb = RingBuffer::new(3);
        assert!(rb.is_empty());
        assert_eq!(rb.capacity(), 3);
        assert_eq!(rb.len(), 0);

        assert!(rb.push(10).is_ok());
        assert!(rb.push(20).is_ok());

        assert_eq!(rb.len(), 2);
        assert!(!rb.is_empty());

        assert_eq!(rb.pop(), Some(10));
        assert_eq!(rb.pop(), Some(20));
        assert!(rb.is_empty());
        assert_eq!(rb.pop(), None); // Pop when empty
    }

    #[test]
    fn test_ring_buffer_full_capacity() {
        let mut rb = RingBuffer::new(2);

        assert!(rb.push(100).is_ok());
        assert!(rb.push(200).is_ok());
        assert!(rb.is_full());

        // Pushing to a full buffer should return an error
        assert!(rb.push(300).is_err());
        assert_eq!(rb.len(), 2);
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let mut rb = RingBuffer::new(3);

        rb.push(1).unwrap();
        rb.push(2).unwrap();
        rb.push(3).unwrap();

        assert_eq!(rb.pop(), Some(1));

        rb.push(4).unwrap();
        assert!(rb.is_full());

        assert_eq!(rb.pop(), Some(2));
        assert_eq!(rb.pop(), Some(3));
        assert_eq!(rb.pop(), Some(4));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_ring_buffer_memory_cleanup() {
        // This test ensures we don't leak memory when the buffer drops
        // or when items are overwritten/popped.
        // (Note: To actually verify this, run `cargo miri test`)
        let mut rb = RingBuffer::new(5);
        rb.push(String::from("hello")).unwrap();
        rb.push(String::from("world")).unwrap();
        let _ = rb.pop();
        // Buffer drops here. If Drop is not implemented correctly,
        // the remaining "world" string and the buffer allocation will leak.
    }
}
