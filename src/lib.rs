#![cfg_attr(feature = "alloc_ref", feature(allocator_api, alloc_layout_extra))]
#![no_std]

#[cfg(any(test, fuzzing))]
#[macro_use]
extern crate std;

#[cfg(feature = "use_spin")]
extern crate spinning_top;

#[cfg(feature = "use_spin")]
use core::alloc::GlobalAlloc;
use core::alloc::Layout;
#[cfg(feature = "alloc_ref")]
use core::alloc::{AllocError, Allocator};
use core::mem::MaybeUninit;
#[cfg(feature = "use_spin")]
use core::ops::Deref;
use core::ptr::NonNull;
#[cfg(test)]
use hole::Hole;
use hole::HoleList;
#[cfg(feature = "use_spin")]
use spinning_top::Spinlock;

pub mod hole;
#[cfg(test)]
mod test;

/// A fixed size heap backed by a linked list of free memory blocks.
/*pub struct Heap {
    used: usize,
    holes: HoleList,
} */
pub struct SegmentedHeap<const SEGMENTS: usize> {
    used: usize,
    holes: [HoleList; SEGMENTS]
}

pub type Heap = SegmentedHeap<1>;

#[cfg(any(test, fuzzing))]
impl Heap {
    pub fn debug(&mut self) {
        println!(
            "bottom: {:?}, top: {:?}, size: {}, pending: {}",
            self.bottom(),
            self.top(),
            self.size(),
            self.holes[0].first.size,
        );
        self.holes[0].debug();
    }
}

unsafe impl Send for Heap {}


impl<const SEGMENTS: usize> SegmentedHeap<SEGMENTS> {
    /// Creates an empty heap. All allocate calls will return `None`.
    pub const fn empty() -> Self {
        Self {
            used: 0,
            holes: unsafe { core::mem::zeroed() },
        }
    }

    /// Allocates a chunk of the given size with the given alignment. Returns a pointer to the
    /// beginning of that chunk if it was successful. Else it returns `None`.
    /// This function scans the list of free memory blocks and uses the first block that is big
    /// enough. The runtime is in O(n) where n is the number of free blocks, but it should be
    /// reasonably fast for small allocations.
    pub fn allocate_first_fit(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let allocation = self.holes.iter_mut()
        // Applies the allocation function to each hole list and returns the first successful allocation
        .find_map(|list| list.allocate_first_fit(layout));
        
        if let Some((allocation, layout)) = allocation {
            self.used += layout.size();
            Some(allocation)
        } else {
            None
        }
    }

    /// Frees the given allocation. `ptr` must be a pointer returned
    /// by a call to the `allocate_first_fit` function with identical size and alignment.
    ///
    /// This function walks the list of free memory blocks and inserts the freed block at the
    /// correct place. If the freed block is adjacent to another free block, the blocks are merged
    /// again. This operation is in `O(n)` since the list needs to be sorted by address.
    ///
    /// # Safety
    ///
    /// `ptr` must be a pointer returned by a call to the [`allocate_first_fit`] function with
    /// identical layout. Undefined behavior may occur for invalid arguments. Panics when none of the hole lists contain the passed pointer.
    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) {
        for hole_list in self.holes.iter_mut() {
            // If the pointer is in the hole list's memory range, deallocate from that list and exit
            if ptr.as_ptr() >= hole_list.bottom && ptr.as_ptr() < hole_list.top {
                self.used -= hole_list.deallocate(ptr, layout).size();
                return;
            }
        }
        panic!("The pointer for deallocation does not match any of the allocator's memory ranges.");
    }

    /// Returns the size of the heap.
    ///
    /// This is the size the heap is using for allocations, not necessarily the
    /// total amount of bytes given to the heap. To determine the exact memory
    /// boundaries, use [`bottom`][Self::bottom] and [`top`][Self::top].
    pub fn size(&self) -> usize {
        unsafe {
            self.holes
            .iter()
            .map(|hole_list| hole_list.top.offset_from(hole_list.bottom) as usize)
            .sum()
        }
    }

    /// Returns the size of the used part of the heap
    pub fn used(&self) -> usize {
        self.used
    }

    /// Returns the size of the free part of the heap
    pub fn free(&self) -> usize {
        self.size() - self.used
    }

    /// Initializes an empty heap
    ///
    /// The `heap_bottom` pointer is automatically aligned, so the [`bottom()`][Self::bottom]
    /// method might return a pointer that is larger than `heap_bottom` after construction.
    ///
    /// The given `heap_size`s must be large enough to store the required
    /// metadata, otherwise this function will panic. Depending on the
    /// alignment of the `hole_addr` pointer, the minimum size is between
    /// `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    ///
    /// The usable size for allocations will be truncated to the nearest
    /// alignment of `align_of::<usize>`. Any extra bytes left at the end
    /// will be reclaimed once sufficient additional space is given to
    /// [`extend`][Heap::extend].
    ///
    /// # Safety
    ///
    /// This function must be called at most once and must only be used on an
    /// empty heap.
    ///
    /// The bottom addresses must be valid and the memory in the
    /// `[heap_bottom, heap_bottom + heap_size)` range must not be used for anything else and must not overlap.
    /// This function is unsafe because it can cause undefined behavior if the given addresses
    /// are invalid.
    ///
    /// The provided memory range must be valid for the `'static` lifetime.
    pub unsafe fn init_from_pointers(&mut self, heaps: [(*mut u8, usize); SEGMENTS]) {
        self.used = 0;
        for (index, (heap_bottom, heap_size)) in heaps.iter().enumerate() {
            self.holes[index] = HoleList::new(*heap_bottom, *heap_size)
        }
    }

    /// Initialize an empty heap with provided memory.
    ///
    /// The caller is responsible for procuring a region of raw memory that may be utilized by the
    /// allocator. This might be done via any method such as (unsafely) taking a region from the
    /// program's memory, from a mutable static, or by allocating and leaking such memory from
    /// another allocator.
    ///
    /// The latter approach may be especially useful if the underlying allocator does not perform
    /// deallocation (e.g. a simple bump allocator). Then the overlaid linked-list-allocator can
    /// provide memory reclamation.
    ///
    /// The usable size for allocations will be truncated to the nearest
    /// alignment of `align_of::<usize>`. Any extra bytes left at the end
    /// will be reclaimed once sufficient additional space is given to
    /// [`extend`][Heap::extend].
    ///
    /// # Panics
    ///
    /// This method panics if the heap is already initialized.
    ///
    /// It also panics when the length of the given `mem` slice is not large enough to
    /// store the required metadata. Depending on the alignment of the slice, the minimum
    /// size is between `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    pub fn init_from_slices(&mut self, mut regions: [&'static mut [MaybeUninit<u8>]; SEGMENTS]) {
        assert!(
            self.holes.iter().all(|h| h.bottom.is_null()),
            "The heap has already been initialized."
        );
        for (index, region) in regions.iter_mut().enumerate() {
            // SAFETY: All initialization requires the bottom address to be valid, which implies it
            // must not be 0. Initially the address is 0. The assertion above ensures that no
            // initialization had been called before.
            // The given address and size is valid according to the safety invariants of the mutable
            // reference handed to us by the caller.
            self.holes[index] = unsafe {HoleList::new((*region).as_mut_ptr().cast(), region.len())};
        }
    }
}

impl Heap {
    /// Initializes an empty heap
    ///
    /// The `heap_bottom` pointer is automatically aligned, so the [`bottom()`][Self::bottom]
    /// method might return a pointer that is larger than `heap_bottom` after construction.
    ///
    /// The given `heap_size` must be large enough to store the required
    /// metadata, otherwise this function will panic. Depending on the
    /// alignment of the `hole_addr` pointer, the minimum size is between
    /// `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    ///
    /// The usable size for allocations will be truncated to the nearest
    /// alignment of `align_of::<usize>`. Any extra bytes left at the end
    /// will be reclaimed once sufficient additional space is given to
    /// [`extend`][Heap::extend].
    ///
    /// # Safety
    ///
    /// This function must be called at most once and must only be used on an
    /// empty heap.
    ///
    /// The bottom address must be valid and the memory in the
    /// `[heap_bottom, heap_bottom + heap_size)` range must not be used for anything else.
    /// This function is unsafe because it can cause undefined behavior if the given address
    /// is invalid.
    ///
    /// The provided memory range must be valid for the `'static` lifetime.
    pub unsafe fn init(&mut self, heap_bottom: *mut u8, heap_size: usize) {
        self.used = 0;
        self.holes[0] = HoleList::new(heap_bottom, heap_size);
    }

    /// Initialize an empty heap with provided memory.
    ///
    /// The caller is responsible for procuring a region of raw memory that may be utilized by the
    /// allocator. This might be done via any method such as (unsafely) taking a region from the
    /// program's memory, from a mutable static, or by allocating and leaking such memory from
    /// another allocator.
    ///
    /// The latter approach may be especially useful if the underlying allocator does not perform
    /// deallocation (e.g. a simple bump allocator). Then the overlaid linked-list-allocator can
    /// provide memory reclamation.
    ///
    /// The usable size for allocations will be truncated to the nearest
    /// alignment of `align_of::<usize>`. Any extra bytes left at the end
    /// will be reclaimed once sufficient additional space is given to
    /// [`extend`][Heap::extend].
    ///
    /// # Panics
    ///
    /// This method panics if the heap is already initialized.
    ///
    /// It also panics when the length of the given `mem` slice is not large enough to
    /// store the required metadata. Depending on the alignment of the slice, the minimum
    /// size is between `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    pub fn init_from_slice(&mut self, mem: &'static mut [MaybeUninit<u8>]) {
        assert!(
            self.bottom().is_null(),
            "The heap has already been initialized."
        );
        let size = mem.len();
        let address = mem.as_mut_ptr().cast();
        // SAFETY: All initialization requires the bottom address to be valid, which implies it
        // must not be 0. Initially the address is 0. The assertion above ensures that no
        // initialization had been called before.
        // The given address and size is valid according to the safety invariants of the mutable
        // reference handed to us by the caller.
        unsafe { self.init(address, size) }
    }

    /// Creates a new heap with the given `bottom` and `size`.
    ///
    /// The `heap_bottom` pointer is automatically aligned, so the [`bottom()`][Self::bottom]
    /// method might return a pointer that is larger than `heap_bottom` after construction.
    ///
    /// The given `heap_size` must be large enough to store the required
    /// metadata, otherwise this function will panic. Depending on the
    /// alignment of the `hole_addr` pointer, the minimum size is between
    /// `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    ///
    /// The usable size for allocations will be truncated to the nearest
    /// alignment of `align_of::<usize>`. Any extra bytes left at the end
    /// will be reclaimed once sufficient additional space is given to
    /// [`extend`][Heap::extend].
    ///
    /// # Safety
    ///
    /// The bottom address must be valid and the memory in the
    /// `[heap_bottom, heap_bottom + heap_size)` range must not be used for anything else.
    /// This function is unsafe because it can cause undefined behavior if the given address
    /// is invalid.
    ///
    /// The provided memory range must be valid for the `'static` lifetime.
    pub unsafe fn new(heap_bottom: *mut u8, heap_size: usize) -> Heap {
        Heap {
            used: 0,
            holes: [HoleList::new(heap_bottom, heap_size)],
        }
    }

    /// Creates a new heap from a slice of raw memory.
    ///
    /// This is a convenience function that has the same effect as calling
    /// [`init_from_slice`] on an empty heap. All the requirements of `init_from_slice`
    /// apply to this function as well.
    pub fn from_slice(mem: &'static mut [MaybeUninit<u8>]) -> Heap {
        let size = mem.len();
        let address = mem.as_mut_ptr().cast();
        // SAFETY: The given address and size is valid according to the safety invariants of the
        // mutable reference handed to us by the caller.
        unsafe { Self::new(address, size) }
    }

    /// Returns the bottom address of the heap.
    ///
    /// The bottom pointer is automatically aligned, so the returned pointer
    /// might be larger than the bottom pointer used for initialization.
    pub fn bottom(&self) -> *mut u8 {
        self.holes[0].bottom
    }

    /// Return the top address of the heap.
    ///
    /// Note: The heap may choose to not use bytes at the end for allocations
    /// until there is enough room for metadata, but it still retains ownership
    /// over memory from [`bottom`][Self::bottom] to the address returned.
    pub fn top(&self) -> *mut u8 {
        unsafe { self.holes[0].top.add(self.holes[0].pending_extend as usize) }
    }

    /// Extends the size of the heap by creating a new hole at the end.
    ///
    /// Small extensions are not guaranteed to grow the usable size of
    /// the heap. In order to grow the Heap most effectively, extend by
    /// at least `2 * size_of::<usize>`, keeping the amount a multiple of
    /// `size_of::<usize>`.
    ///
    /// Calling this method on an uninitialized Heap will panic.
    ///
    /// # Safety
    ///
    /// The amount of data given in `by` MUST exist directly after the original
    /// range of data provided when constructing the [Heap]. The additional data
    /// must have the same lifetime of the original range of data.
    ///
    /// Even if this operation doesn't increase the [usable size][`Self::size`]
    /// by exactly `by` bytes, those bytes are still owned by the Heap for
    /// later use.
    pub unsafe fn extend(&mut self, by: usize) {
        self.holes[0].extend(by);
    }
}

#[cfg(all(feature = "alloc_ref", feature = "use_spin"))]
unsafe impl Allocator for LockedHeap {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(layout.dangling_ptr(), 0));
        }
        match self.0.lock().allocate_first_fit(layout) {
            Some(ptr) => Ok(NonNull::slice_from_raw_parts(ptr, layout.size())),
            None => Err(AllocError),
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            self.0.lock().deallocate(ptr, layout);
        }
    }
}

#[cfg(feature = "use_spin")]
pub struct LockedHeap(Spinlock<Heap>);

#[cfg(feature = "use_spin")]
impl LockedHeap {
    pub const fn empty() -> LockedHeap {
        LockedHeap(Spinlock::new(Heap::empty()))
    }

    /// Creates a new heap with the given `bottom` and `size`.
    ///
    /// The `heap_bottom` pointer is automatically aligned, so the [`bottom()`][Heap::bottom]
    /// method might return a pointer that is larger than `heap_bottom` after construction.
    ///
    /// The given `heap_size` must be large enough to store the required
    /// metadata, otherwise this function will panic. Depending on the
    /// alignment of the `hole_addr` pointer, the minimum size is between
    /// `2 * size_of::<usize>` and `3 * size_of::<usize>`.
    ///
    /// # Safety
    ///
    /// The bottom address must be valid and the memory in the
    /// `[heap_bottom, heap_bottom + heap_size)` range must not be used for anything else.
    /// This function is unsafe because it can cause undefined behavior if the given address
    /// is invalid.
    ///
    /// The provided memory range must be valid for the `'static` lifetime.
    pub unsafe fn new(heap_bottom: *mut u8, heap_size: usize) -> LockedHeap {
        LockedHeap(Spinlock::new(Heap {
            used: 0,
            holes: [HoleList::new(heap_bottom, heap_size)],
        }))
    }
}

#[cfg(feature = "use_spin")]
impl Deref for LockedHeap {
    type Target = Spinlock<Heap>;

    fn deref(&self) -> &Spinlock<Heap> {
        &self.0
    }
}

#[cfg(feature = "use_spin")]
unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0
            .lock()
            .allocate_first_fit(layout)
            .map_or(core::ptr::null_mut(), |allocation| allocation.as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0
            .lock()
            .deallocate(NonNull::new_unchecked(ptr), layout)
    }
}

/// Align downwards. Returns the greatest x with alignment `align`
/// so that x <= addr. The alignment must be a power of 2.
pub fn align_down_size(size: usize, align: usize) -> usize {
    if align.is_power_of_two() {
        size & !(align - 1)
    } else if align == 0 {
        size
    } else {
        panic!("`align` must be a power of 2");
    }
}

pub fn align_up_size(size: usize, align: usize) -> usize {
    align_down_size(size + align - 1, align)
}

/// Align upwards. Returns the smallest x with alignment `align`
/// so that x >= addr. The alignment must be a power of 2.
pub fn align_up(addr: *mut u8, align: usize) -> *mut u8 {
    let offset = addr.align_offset(align);
    addr.wrapping_add(offset)
}
