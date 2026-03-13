extern crate alloc;

use alloc::vec::Vec;
use core::{alloc::Layout, ptr::NonNull};

use hoarder_common::{
    error::{HoarderError, Result},
};

use crate::collections::{RingBuffer, Array};

#[derive(Debug)]
pub struct AlignedBuffers {
    ptr: NonNull<u8>,
    pub buf_size: usize,
    pub count: usize,
    pub alignment: usize,
    total_size: usize,
}

impl AlignedBuffers {
    pub fn new(count: usize, buf_size: usize, alignment: usize) -> Result<Self> {
        let total = count.checked_mul(buf_size);

        assert!(total.is_some());
        assert!(alignment.is_power_of_two());
        assert!(
            buf_size % alignment == 0,
            "buf_size must be a multiple of alignment"
        );

        let total = unsafe { total.unwrap_unchecked() };
        let layout = unsafe { Layout::from_size_align_unchecked(total, alignment) };

        // SAFETY: alloc returns a pointer or null; we check it
        let raw = unsafe { alloc::alloc::alloc(layout) };
        let ptr = NonNull::new(raw).ok_or_else(|| HoarderError::MemAllocFail)?;

        Ok(Self {
            ptr,
            total_size: total,
            buf_size,
            alignment,
            count,
        })
    }

    pub fn buf_ptr(&self, index: usize) -> *const u8 {
        assert!(index < self.count);
        unsafe { self.ptr.as_ptr().add(index * self.buf_size) }
    }

    pub fn buf_ptr_mut(&mut self, index: usize) -> *mut u8 {
        assert!(index < self.count);
        unsafe { self.ptr.as_ptr().add(index * self.buf_size) }
    }
}

impl Drop for AlignedBuffers {
    fn drop(&mut self) {
        // We validated layout in new(); use unchecked here to avoid panic in Drop.
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.total_size, self.alignment);
            alloc::alloc::dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenId {
    pub index: u32,
    pub generation: u32,
}

pub struct GenAlloc {
    generation: Array<u32>,
    free_list: RingBuffer<u32>,
}

impl GenAlloc {
    /// creates a new geneational allocator with the given
    /// capacity. It uses the `mask` to limit the given
    /// capacity.
    pub fn new(capacity: u32, mask: u32) -> Self {
        let mask = if mask == 0 { u32::MAX } else { mask };

        let capacity = capacity & mask;
        let generation = Array::new(0, capacity as usize);
        let free_list = RingBuffer::from_fixed_iter(capacity as _, (0..capacity).rev());
        Self {
            generation,
            free_list,
        }
    }

    pub fn alloc(&mut self) -> Option<GenId> {
        let idx = self.free_list.pop()?;
        let generation = self.generation[idx as usize];
        Some(GenId {
            index: idx,
            generation,
        })
    }

    pub fn free(&mut self, id: GenId) -> bool {
        if self.is_valid(id) {
            self.generation[id.index as usize] += 1;
            self.free_list.push(id.index);
            true
        } else {
            false
        }
    }

    pub fn is_valid(&self, id: GenId) -> bool {
        let idx = id.index as usize;
        idx < self.generation.len() && id.generation == self.generation[idx]
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct BufferHandle(pub GenId);

pub struct BufferPool<const BUF_SIZE: usize = 0x1000, const ALIGN: usize = 0x1000> {
    pool: AlignedBuffers,
    alloc: GenAlloc,
}

impl<const BUF_SIZE: usize, const ALIGN: usize> BufferPool<BUF_SIZE, ALIGN> {
    pub fn new(count: u32) -> Self {
        let pool = AlignedBuffers::new(count as usize, BUF_SIZE, ALIGN).unwrap();
        Self {
            pool,
            alloc: GenAlloc::new(count, 0),
        }
    }

    pub const fn pool(&self) -> &AlignedBuffers {
        &self.pool
    }

    #[inline(always)]
    pub fn alloc(&mut self) -> Option<BufferHandle> {
        let alloc = self.alloc.alloc()?;
        Some(BufferHandle(alloc))
    }

    #[inline(always)]
    pub fn get(&self, handle: BufferHandle) -> Option<&[u8]> {
        if self.alloc.is_valid(handle.0) {
            Some(unsafe {
                core::slice::from_raw_parts(self.pool.buf_ptr(handle.0.index as usize), BUF_SIZE)
            })
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self, handle: BufferHandle) -> Option<&mut [u8]> {
        if self.alloc.is_valid(handle.0) {
            Some(unsafe {
                core::slice::from_raw_parts_mut(
                    self.pool.buf_ptr_mut(handle.0.index as usize),
                    BUF_SIZE,
                )
            })
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn free(&mut self, handle: BufferHandle) -> bool {
        self.alloc.free(handle.0)
    }
}

pub struct ObjectPool<T, const ID: u8> {
    data: Vec<Option<T>>,
    alloc: GenAlloc,
}

impl<T, const ID: u8> ObjectPool<T, ID> {
    const MASK: u32 = 0x00FF_FFFF;

    pub fn new(capacity: u32) -> Self {
        let mut data = Vec::with_capacity(capacity as usize);
        data.resize_with(capacity as usize, || None::<T>);

        let alloc = GenAlloc::new(capacity, Self::MASK);
        Self { data, alloc }
    }

    pub const fn capacity(&self) -> u32 {
        self.data.capacity() as _
    }

    pub fn spawn(&mut self, object: T) -> Option<ObjectHandle> {
        let alloc = self.alloc.alloc()?;
        assert!(self.data[alloc.index as usize].is_none());

        self.data[alloc.index as usize] = Some(object);
        Some(ObjectHandle::new(ID, alloc))
    }

    pub fn get(&self, handle: ObjectHandle) -> Option<&T> {
        let index = handle.index();
        if self.alloc.is_valid(GenId {
            index,
            generation: handle.generation(),
        }) {
            self.data[index as usize].as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, handle: ObjectHandle) -> Option<&mut T> {
        let index = handle.index();
        if self.alloc.is_valid(GenId {
            index,
            generation: handle.generation(),
        }) {
            self.data[index as usize].as_mut()
        } else {
            None
        }
    }

    pub fn despawn(&mut self, handle: ObjectHandle) -> bool {
        let index = handle.index();
        if self.alloc.free(GenId {
            index,
            generation: handle.generation(),
        }) {
            self.data[index as usize] = None;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct ObjectHandle(u64);

impl ObjectHandle {
    const ID_SHIFT: u64 = 56;
    const INDEX_SHIFT: u64 = 32;
    const POOL_MASK: u32 = ObjectPool::<(), 0>::MASK;

    const fn new(pool_id: u8, gen_id: GenId) -> Self {
        let n = ((pool_id as u64) << Self::ID_SHIFT)
            | ((gen_id.index as u64) << Self::INDEX_SHIFT)
            | (gen_id.generation as u64);

        Self(n)
    }

    pub const fn pool_id(self) -> u8 {
        (self.0 >> Self::ID_SHIFT) as u8
    }

    pub const fn index(self) -> u32 {
        (self.0 >> Self::INDEX_SHIFT) as u32 & Self::POOL_MASK
    }

    pub const fn generation(self) -> u32 {
        self.0 as u32
    }
}

impl From<u64> for ObjectHandle {
    fn from(value: u64) -> Self {
        ObjectHandle(value)
    }
}

impl Into<u64> for ObjectHandle {
    fn into(self) -> u64 {
        self.0
    }
}