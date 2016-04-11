#![feature(unique)]
#![feature(const_fn)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

use hole::HoleList;

mod hole;
#[cfg(test)]
mod test;

/// A fixed size heap backed by a linked list of free memory blocks.
pub struct Heap {
    bottom: usize,
    top: usize,
    holes: HoleList,
}

impl Heap {
    /// Creates an empty heap. All allocate calls will return `None`.
    pub const fn empty() -> Heap {
        Heap {
            top: 0,
            bottom: 0,
            holes: HoleList::empty(),
        }
    }

    /// Creates a new heap with the given `bottom` and `top`. Both addresses must be valid and the
    /// memory in the `[heap_bottom, heap_top)` range must not be used for anything else. This
    /// function is unsafe because it can cause undefined behavior if the given addresses are
    /// invalid.
    pub unsafe fn new(heap_bottom: usize, heap_top: usize) -> Heap {
        Heap {
            bottom: heap_bottom,
            top: heap_top,
            holes: HoleList::new(heap_bottom, heap_top - heap_bottom),
        }
    }

    /// Allocates a chunk of the given size with the given alignment. Returns a pointer to the
    /// beginning of that chunk if it was successful. Else it returns `None`.
    /// This function scans the list of free memory blocks and uses the first block that is big
    /// enough. The runtime is in O(n) where n is the number of free blocks, but it should be
    /// reasonably fast for small allocations.
    pub fn allocate_first_fit(&mut self, mut size: usize, align: usize) -> Option<*mut u8> {
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }

        self.holes.allocate_first_fit(size, align)
    }

    /// Frees the given allocation. `ptr` must be a pointer returned
    /// by a call to the `allocate_first_fit` function with identical size and alignment. Undefined
    /// behavior may occur for invalid arguments, thus this function is unsafe.
    ///
    /// This function walks the list of free memory blocks and inserts the freed block at the
    /// correct place. If the freed block is adjacent to another free block, the blocks are merged
    /// again. This operation is in `O(n)` since the list needs to be sorted by address.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, mut size: usize, _align: usize) {
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }
        self.holes.deallocate(ptr, size);
    }

    /// Returns the bottom address of the heap.
    pub fn bottom(&self) -> usize {
        self.bottom
    }

    /// Returns the top address of the heap.
    pub fn top(&self) -> usize {
        self.top
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
