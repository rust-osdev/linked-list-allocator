#![feature(unique)]
#![feature(const_fn)]
#![feature(alloc, allocator_api)]
#![no_std]

extern crate alloc;

#[cfg(test)]
#[macro_use]
extern crate std;

extern crate spin;

use hole::{Hole, HoleList};
use core::mem;
use core::ops::Deref;
use alloc::allocator::{Alloc, Layout, AllocErr};
use spin::Mutex;

mod hole;
#[cfg(test)]
mod test;

/// A fixed size heap backed by a linked list of free memory blocks.
pub struct Heap {
    bottom: usize,
    size: usize,
    holes: HoleList,
}

impl Heap {
    /// Creates an empty heap. All allocate calls will return `None`.
    pub const fn empty() -> Heap {
        Heap {
            bottom: 0,
            size: 0,
            holes: HoleList::empty(),
        }
    }

    /// Initializes an empty heap
    ///
    /// # Unsafety
    ///
    /// This function must be called at most once and must only be used on an
    /// empty heap.
    pub unsafe fn init(&mut self, heap_bottom: usize, heap_size: usize) {
        self.bottom = heap_bottom;
        self.size = heap_size;
        self.holes = HoleList::new(heap_bottom, heap_size);
    }

    /// Creates a new heap with the given `bottom` and `size`. The bottom address must be valid
    /// and the memory in the `[heap_bottom, heap_bottom + heap_size)` range must not be used for
    /// anything else. This function is unsafe because it can cause undefined behavior if the
    /// given address is invalid.
    pub unsafe fn new(heap_bottom: usize, heap_size: usize) -> Heap {
        Heap {
            bottom: heap_bottom,
            size: heap_size,
            holes: HoleList::new(heap_bottom, heap_size),
        }
    }

    /// Allocates a chunk of the given size with the given alignment. Returns a pointer to the
    /// beginning of that chunk if it was successful. Else it returns `None`.
    /// This function scans the list of free memory blocks and uses the first block that is big
    /// enough. The runtime is in O(n) where n is the number of free blocks, but it should be
    /// reasonably fast for small allocations.
    pub fn allocate_first_fit(&mut self, layout: Layout) -> Result<*mut u8, AllocErr> {
        let mut size = layout.size();
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }
        let size = align_up(size, mem::align_of::<Hole>());
        let layout = Layout::from_size_align(size, layout.align()).unwrap();

        self.holes.allocate_first_fit(layout)
    }

    /// Frees the given allocation. `ptr` must be a pointer returned
    /// by a call to the `allocate_first_fit` function with identical size and alignment. Undefined
    /// behavior may occur for invalid arguments, thus this function is unsafe.
    ///
    /// This function walks the list of free memory blocks and inserts the freed block at the
    /// correct place. If the freed block is adjacent to another free block, the blocks are merged
    /// again. This operation is in `O(n)` since the list needs to be sorted by address.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let mut size = layout.size();
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }
        let size = align_up(size, mem::align_of::<Hole>());
        let layout = Layout::from_size_align(size, layout.align()).unwrap();

        self.holes.deallocate(ptr, layout);
    }

    /// Returns the bottom address of the heap.
    pub fn bottom(&self) -> usize {
        self.bottom
    }

    /// Returns the size of the heap.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Return the top address of the heap
    pub fn top(&self) -> usize {
        self.bottom + self.size
    }

    /// Extends the size of the heap by creating a new hole at the end
    ///
    /// # Unsafety
    ///
    /// The new extended area must be valid
    pub unsafe fn extend(&mut self, by: usize) {
        let top = self.top();
        let layout = Layout::from_size_align(by, 1).unwrap();
        self.holes.deallocate(top as *mut u8, layout);
        self.size += by;
    }
}

unsafe impl Alloc for Heap {
    unsafe fn alloc(&mut self, layout: Layout) -> Result<*mut u8, AllocErr> {
        self.allocate_first_fit(layout)
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        self.deallocate(ptr, layout)
    }

    fn oom(&mut self, _: AllocErr) -> ! {
        panic!("Out of memory");
    }
}

pub struct LockedHeap(Mutex<Heap>);

impl LockedHeap {
    /// Creates an empty heap. All allocate calls will return `None`.
    pub const fn empty() -> LockedHeap {
        LockedHeap(Mutex::new(Heap::empty()))
    }

    /// Creates a new heap with the given `bottom` and `size`. The bottom address must be valid
    /// and the memory in the `[heap_bottom, heap_bottom + heap_size)` range must not be used for
    /// anything else. This function is unsafe because it can cause undefined behavior if the
    /// given address is invalid.
    pub unsafe fn new(heap_bottom: usize, heap_size: usize) -> LockedHeap {
        LockedHeap(Mutex::new(Heap {
            bottom: heap_bottom,
            size: heap_size,
            holes: HoleList::new(heap_bottom, heap_size),
        }))
    }
}

impl Deref for LockedHeap {
    type Target = Mutex<Heap>;

    fn deref(&self) -> &Mutex<Heap> {
        &self.0
    }
}

unsafe impl<'a> Alloc for &'a LockedHeap {
    unsafe fn alloc(&mut self, layout: Layout) -> Result<*mut u8, AllocErr> {
        self.0.lock().allocate_first_fit(layout)
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        self.0.lock().deallocate(ptr, layout)
    }

    fn oom(&mut self, _: AllocErr) -> ! {
        panic!("Out of memory");
    }
}

/// Align downwards. Returns the greatest x with alignment `align`
/// so that x <= addr. The alignment must be a power of 2.
pub fn align_down(addr: usize, align: usize) -> usize {
    if align.is_power_of_two() {
        addr & !(align - 1)
    } else if align == 0 {
        addr
    } else {
        panic!("`align` must be a power of 2");
    }
}

/// Align upwards. Returns the smallest x with alignment `align`
/// so that x >= addr. The alignment must be a power of 2.
pub fn align_up(addr: usize, align: usize) -> usize {
    align_down(addr + align - 1, align)
}
