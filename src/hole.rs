use core::alloc::Layout;
use core::convert::{TryFrom, TryInto};
use core::mem;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;

use crate::align_up_size;

use super::align_up;

/// A sorted list of holes. It uses the the holes itself to store its nodes.
pub struct HoleList {
    pub(crate) first: Hole, // dummy
}

pub struct Cursor {
    prev: NonNull<Hole>,
    hole: NonNull<Hole>,
}

enum Position<'a> {
    BeforeCurrent,
    BetweenCurrentNext {
        curr: &'a Hole,
        next: &'a Hole,
    },
    AfterBoth,
    AfterCurrentNoNext,
}

impl Cursor {
    fn next(mut self) -> Option<Self> {
        unsafe {
            if let Some(nhole) = self.hole.as_mut().next {
                Some(Cursor {
                    prev: self.hole,
                    hole: nhole,
                })
            } else {
                None
            }
        }
    }

    fn peek_next(&self) -> Option<&Hole> {
        unsafe {
            if let Some(nhole) = self.hole.as_ref().next {
                Some(nhole.as_ref())
            } else {
                None
            }
        }
    }

    fn current(&self) -> &Hole {
        unsafe {
            self.hole.as_ref()
        }
    }

    fn previous(&self) -> &Hole {
        unsafe {
            self.prev.as_ref()
        }
    }

    // On success, it returns the new allocation, and the linked list has been updated
    // to accomodate any new holes and allocation. On error, it returns the cursor
    // unmodified, and has made no changes to the linked list of holes.
    fn split_current(self, required_layout: Layout) -> Result<(*mut u8, usize), Self> {
        let front_padding;
        let alloc_ptr;
        let alloc_size;
        let back_padding;

        // Here we create a scope, JUST to make sure that any created references do not
        // live to the point where we start doing pointer surgery below.
        {
            let hole_size = self.current().size;
            let hole_addr_u8 = self.hole.as_ptr().cast::<u8>();
            let required_size = required_layout.size();
            let required_align = required_layout.align();

            // Quick check: If the new item is larger than the current hole, it's never gunna
            // work. Go ahead and bail early to save ourselves some math.
            if hole_size < required_size {
                return Err(self);
            }

            // Attempt to fracture the current hole into the following parts:
            // ([front_padding], allocation, [back_padding])
            //
            // The paddings are optional, and only placed if required.
            //
            // First, figure out if front padding is necessary. This would be necessary if the new
            // allocation has a larger alignment requirement than the current hole, and we didn't get
            // lucky that the current position was well-aligned enough for the new item.
            let aligned_addr = if hole_addr_u8 == align_up(hole_addr_u8, required_align) {
                // hole has already the required alignment, no front padding is needed.
                front_padding = None;
                hole_addr_u8
            } else {
                // Unfortunately, we did not get lucky. Instead: Push the "starting location" FORWARD the size
                // of a hole node, to guarantee there is at least enough room for the hole header, and
                // potentially additional space.
                let new_start = hole_addr_u8.wrapping_add(HoleList::min_size());

                let aligned_addr = align_up(new_start, required_align);
                front_padding = Some(HoleInfo {
                    // Our new front padding will exist at the same location as the previous hole,
                    // it will just have a smaller size after we have chopped off the "tail" for
                    // the allocation.
                    addr: hole_addr_u8,
                    size: unsafe { aligned_addr.offset_from(hole_addr_u8) }
                        .try_into()
                        .unwrap(),
                });
                aligned_addr
            };

            // Okay, now that we found space, we need to see if the decisions we just made
            // ACTUALLY fit in the previous hole space
            let allocation_end = aligned_addr.wrapping_add(required_size);
            let hole_end = hole_addr_u8.wrapping_add(hole_size);

            if allocation_end > hole_end {
                // hole is too small
                return Err(self);
            }

            // Yes! We have successfully placed our allocation as well.
            alloc_ptr = aligned_addr;
            alloc_size = required_size;

            // Okay, time to move onto the back padding. Here, we are opportunistic -
            // if it fits, we sits. Otherwise we just skip adding the back padding, and
            // sort of assume that the allocation is actually a bit larger than it
            // actually needs to be.
            //
            // TODO(AJM): Write a test for this - does the code actually handle this
            // correctly? Do we need to check the delta is less than an aligned hole
            // size?
            let hole_layout = Layout::new::<Hole>();
            let back_padding_start = align_up(allocation_end, hole_layout.align());
            let back_padding_end = back_padding_start.wrapping_add(hole_layout.size());

            // Will the proposed new back padding actually fit in the old hole slot?
            back_padding = if back_padding_end <= hole_end {
                // Yes, it does!
                Some(HoleInfo {
                    addr: back_padding_start,
                    size: unsafe { hole_end.offset_from(back_padding_start) }
                            .try_into()
                            .unwrap(),
                })
            } else {
                // No, it does not.
                None
            };
        }

        ////////////////////////////////////////////////////////////////////////////
        // This is where we actually perform surgery on the linked list.
        ////////////////////////////////////////////////////////////////////////////
        let Cursor { mut prev, mut hole } = self;
        // Remove the current location from the previous node
        unsafe { prev.as_mut().next = None; }
        // Take the next node out of our current node
        let maybe_next_addr: Option<NonNull<Hole>> = unsafe { hole.as_mut().next.take() };

        // As of now, the old `Hole` is no more. We are about to replace it with one or more of
        // the front padding, the allocation, and the back padding.
        drop(hole);

        match (front_padding, back_padding) {
            (None, None) => {
                // No padding at all, how lucky! We still need to connect the PREVIOUS node
                // to the NEXT node, if there was one
                unsafe { prev.as_mut().next = maybe_next_addr; }
            },
            (None, Some(singlepad)) | (Some(singlepad), None) => unsafe {
                // We have front padding OR back padding, but not both.
                //
                // Replace the old node with the new single node. We need to stitch the new node
                // into the linked list. Start by writing the padding into the proper location
                let singlepad_ptr = singlepad.addr.cast::<Hole>();
                singlepad_ptr.write(Hole {
                    size: singlepad.size,
                    // If the old hole had a next pointer, the single padding now takes
                    // "ownership" of that link
                    next: maybe_next_addr,
                });

                // Then connect the OLD previous to the NEW single padding
                prev.as_mut().next = Some(NonNull::new_unchecked(singlepad_ptr));
            },
            (Some(frontpad), Some(backpad)) => unsafe {
                // We have front padding AND back padding.
                //
                // We need to stich them together as two nodes where there used to
                // only be one. Start with the back padding.
                let backpad_ptr = backpad.addr.cast::<Hole>();
                backpad_ptr.write(Hole {
                    size: backpad.size,
                    // If the old hole had a next pointer, the BACK padding now takes
                    // "ownership" of that link
                    next: maybe_next_addr,
                });

                // Now we emplace the front padding, and link it to both the back padding,
                // and the old previous
                let frontpad_ptr = frontpad.addr.cast::<Hole>();
                frontpad_ptr.write(Hole {
                    size: frontpad.size,
                    // We now connect the FRONT padding to the BACK padding
                    next: Some(NonNull::new_unchecked(backpad_ptr)),
                });

                // Then connect the OLD previous to the NEW FRONT padding
                prev.as_mut().next = Some(NonNull::new_unchecked(frontpad_ptr));
            }
        }

        // Well that went swimmingly! Hand off the allocation, with surgery performed successfully!
        Ok((alloc_ptr, alloc_size))
    }
}

impl HoleList {
    /// Creates an empty `HoleList`.
    #[cfg(not(feature = "const_mut_refs"))]
    pub fn empty() -> HoleList {
        HoleList {
            first: Hole {
                size: 0,
                next: None,
            },
        }
    }

    /// Creates an empty `HoleList`.
    #[cfg(feature = "const_mut_refs")]
    pub const fn empty() -> HoleList {
        HoleList {
            first: Hole {
                size: 0,
                next: None,
            },
        }
    }

    pub fn cursor(&mut self) -> Option<Cursor> {
        if let Some(hole) = self.first.next {
            Some(Cursor {
                hole,
                prev: NonNull::new(&mut self.first)?,
            })
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn debug(&mut self) {
        if let Some(cursor) = self.cursor() {
            let mut cursor = cursor;
            loop {
                println!(
                    "prev: {:?}[{}], hole: {:?}[{}]",
                    cursor.previous() as *const Hole,
                    cursor.previous().size,
                    cursor.current() as *const Hole,
                    cursor.current().size,
                );
                if let Some(c) = cursor.next() {
                    cursor = c;
                } else {
                    println!("Done!");
                    return;
                }
            }
        } else {
            println!("No holes");
        }
    }

    /// Creates a `HoleList` that contains the given hole.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it
    /// creates a hole at the given `hole_addr`. This can cause undefined behavior if this address
    /// is invalid or if memory from the `[hole_addr, hole_addr+size)` range is used somewhere else.
    ///
    /// The pointer to `hole_addr` is automatically aligned.
    pub unsafe fn new(hole_addr: *mut u8, hole_size: usize) -> HoleList {
        assert_eq!(size_of::<Hole>(), Self::min_size());

        let aligned_hole_addr = align_up(hole_addr, align_of::<Hole>());
        let ptr = aligned_hole_addr as *mut Hole;
        ptr.write(Hole {
            size: hole_size
                .saturating_sub(aligned_hole_addr.offset_from(hole_addr).try_into().unwrap()),
            next: None,
        });

        HoleList {
            first: Hole {
                size: 0,
                next: Some(NonNull::new_unchecked(ptr)),
            },
        }
    }

    /// Aligns the given layout for use with `HoleList`.
    ///
    /// Returns a layout with size increased to fit at least `HoleList::min_size` and proper
    /// alignment of a `Hole`.
    ///
    /// The [`allocate_first_fit`][HoleList::allocate_first_fit] and
    /// [`deallocate`][HoleList::deallocate] methods perform the required alignment
    /// themselves, so calling this function manually is not necessary.
    pub fn align_layout(layout: Layout) -> Layout {
        let mut size = layout.size();
        if size < Self::min_size() {
            size = Self::min_size();
        }
        let size = align_up_size(size, mem::align_of::<Hole>());
        let layout = Layout::from_size_align(size, layout.align()).unwrap();

        layout
    }

    /// Searches the list for a big enough hole.
    ///
    /// A hole is big enough if it can hold an allocation of `layout.size()` bytes with
    /// the given `layout.align()`. If such a hole is found in the list, a block of the
    /// required size is allocated from it. Then the start address of that
    /// block and the aligned layout are returned. The automatic layout alignment is required
    /// because the `HoleList` has some additional layout requirements for each memory block.
    ///
    /// This function uses the “first fit” strategy, so it uses the first hole that is big
    /// enough. Thus the runtime is in O(n) but it should be reasonably fast for small allocations.
    pub fn allocate_first_fit(&mut self, layout: Layout) -> Result<(NonNull<u8>, Layout), ()> {
        let aligned_layout = Self::align_layout(layout);
        let mut cursor = self.cursor().ok_or(())?;

        loop {
            match cursor.split_current(aligned_layout) {
                Ok((ptr, _len)) => {
                    return Ok((NonNull::new(ptr).ok_or(())?, aligned_layout));
                },
                Err(curs) => {
                    cursor = curs.next().ok_or(())?;
                },
            }
        }
    }

    /// Frees the allocation given by `ptr` and `layout`.
    ///
    /// `ptr` must be a pointer returned by a call to the [`allocate_first_fit`] function with
    /// identical layout. Undefined behavior may occur for invalid arguments.
    /// The function performs exactly the same layout adjustments as [`allocate_first_fit`] and
    /// returns the aligned layout.
    ///
    /// This function walks the list and inserts the given block at the correct place. If the freed
    /// block is adjacent to another free block, the blocks are merged again.
    /// This operation is in `O(n)` since the list needs to be sorted by address.
    ///
    /// [`allocate_first_fit`]: HoleList::allocate_first_fit
    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) -> Layout {
        let aligned_layout = Self::align_layout(layout);
        deallocate(self, ptr.as_ptr(), aligned_layout.size());
        aligned_layout
    }

    /// Returns the minimal allocation size. Smaller allocations or deallocations are not allowed.
    pub fn min_size() -> usize {
        size_of::<usize>() * 2
    }

    /// Returns information about the first hole for test purposes.
    #[cfg(test)]
    pub fn first_hole(&self) -> Option<(*const u8, usize)> {
        self.first
            .next
            .as_ref()
            .map(|hole| (hole.as_ptr() as *mut u8 as *const u8, unsafe { hole.as_ref().size }))
    }
}

/// A block containing free memory. It points to the next hole and thus forms a linked list.
pub(crate) struct Hole {
    pub size: usize,
    pub next: Option<NonNull<Hole>>,
}

impl Hole {
    /// Returns basic information about the hole.
    fn info(&mut self) -> HoleInfo {
        HoleInfo {
            addr: self as *mut _ as *mut u8,
            size: self.size,
        }
    }

    fn addr_u8(&self) -> *const u8 {
        self as *const Hole as *const u8
    }
}

/// Basic information about a hole.
#[derive(Debug, Clone, Copy)]
struct HoleInfo {
    addr: *mut u8,
    size: usize,
}

/// The result returned by `split_hole` and `allocate_first_fit`. Contains the address and size of
/// the allocation (in the `info` field), and the front and back padding.
struct Allocation {
    info: HoleInfo,
    front_padding: Option<HoleInfo>,
    back_padding: Option<HoleInfo>,
}

unsafe fn make_hole(addr: *mut u8, size: usize) -> NonNull<Hole> {
    let hole_addr = addr.cast::<Hole>();
    debug_assert_eq!(addr as usize % align_of::<Hole>(), 0);
    hole_addr.write(Hole {size, next: None});
    NonNull::new_unchecked(hole_addr)
}

impl Cursor {
    fn try_insert_back(self, mut node: NonNull<Hole>) -> Result<Self, Self> {
        // Covers the case where the new hole exists BEFORE the current pointer,
        // which only happens when previous is the stub pointer
        if node < self.hole {
            let node_u8 = node.as_ptr().cast::<u8>();
            let node_size = unsafe { node.as_ref().size };
            let hole_u8 = self.hole.as_ptr().cast::<u8>();

            assert!(node_u8.wrapping_add(node_size) <= hole_u8);
            assert_eq!(self.previous().size, 0);

            let Cursor { mut prev, hole } = self;
            unsafe {
                prev.as_mut().next = Some(node);
                node.as_mut().next = Some(hole);
            }
            Ok(Cursor { prev, hole: node })
        } else {
            Err(self)
        }
    }

    fn try_insert_after(&mut self, mut node: NonNull<Hole>) -> Result<(), ()> {
        if self.hole < node {
            let node_u8 = node.as_ptr().cast::<u8>();
            let node_size = unsafe { node.as_ref().size };
            let hole_u8 = self.hole.as_ptr().cast::<u8>();
            let hole_size = self.current().size;

            // Does hole overlap node?
            assert!(hole_u8.wrapping_add(hole_size) <= node_u8);

            // If we have a next, does the node overlap next?
            if let Some(next) = self.current().next.as_ref() {
                let node_u8 = node_u8 as *const u8;
                assert!(node_u8.wrapping_add(node_size) <= next.as_ptr().cast::<u8>())
            }

            // All good! Let's insert that after.
            unsafe {
                let maybe_next = self.hole.as_mut().next.replace(node);
                node.as_mut().next = maybe_next;
            }
            Ok(())
        } else {
            Err(())
        }
    }


    // Merge the current node with an IMMEDIATELY FOLLOWING next node
    fn try_merge_next_two(self) {
        let Cursor { prev: _, mut hole } = self;

        for _ in 0..2 {
            // Is there a next node?
            let mut next = if let Some(next) = unsafe { hole.as_mut() }.next.as_ref() {
                *next
            } else {
                return;
            };

            // Can we directly merge these? e.g. are they touching?
            //
            // TODO(AJM): Should "touching" also include deltas <= node size?
            let hole_u8 = hole.as_ptr().cast::<u8>();
            let hole_sz = unsafe { hole.as_ref().size };
            let next_u8 = next.as_ptr().cast::<u8>();

            let touching = hole_u8.wrapping_add(hole_sz) == next_u8;

            if touching {
                let next_sz;
                let next_next;
                unsafe {
                    let next_mut = next.as_mut();
                    next_sz = next_mut.size;
                    next_next = next_mut.next.take();
                }
                unsafe {
                    let hole_mut = hole.as_mut();
                    hole_mut.next = next_next;
                    hole_mut.size += next_sz;
                }
                // Okay, we just merged the next item. DON'T move the cursor, as we can
                // just try to merge the next_next, which is now our next.
            } else {
                // Welp, not touching, can't merge. Move to the next node.
                hole = next;
            }
        }

    }
}

/// Frees the allocation given by `(addr, size)`. It starts at the given hole and walks the list to
/// find the correct place (the list is sorted by address).
fn deallocate(list: &mut HoleList, addr: *mut u8, size: usize) {
    // Start off by just making this allocation a hole.
    let hole = unsafe { make_hole(addr, size) };

    let cursor = if let Some(cursor) = list.cursor() {
        cursor
    } else {
        // Oh hey, there are no holes at all. That means this just becomes the only hole!
        list.first.next = Some(hole);
        return;
    };

    // First, check if we can just insert this node at the top of the list. If the
    // insertion succeeded, then our cursor now points to the NEW node, behind the
    // previous location the cursor was pointing to.
    let cursor = match cursor.try_insert_back(hole) {
        Ok(cursor) => {
            // Yup! It lives at the front of the list. Hooray!
            cursor
        },
        Err(mut cursor) => {
            // Nope. It lives somewhere else. Advance the list until we find its home
            while let Err(()) = cursor.try_insert_after(hole) {
                cursor = cursor.next().unwrap();
            }
            cursor
        },
    };

    // We now need to merge up to two times to combine the current node with the next
    // two nodes.
    cursor.try_merge_next_two();
}


/// Identity function to ease moving of references.
///
/// By default, references are reborrowed instead of moved (equivalent to `&mut *reference`). This
/// function forces a move.
///
/// for more information, see section “id Forces References To Move” in:
/// https://bluss.github.io/rust/fun/2015/10/11/stuff-the-identity-function-does/
fn move_helper<T>(x: T) -> T {
    x
}

#[cfg(test)]
pub mod test {
    use super::*;
    use core::alloc::Layout;
    use std::mem::{align_of, size_of, MaybeUninit};
    use std::prelude::v1::*;
    use crate::Heap;

    #[repr(align(128))]
    struct Chonk<const N: usize> {
        data: [MaybeUninit<u8>; N]
    }

    impl<const N: usize> Chonk<N> {
        pub fn new() -> Self {
            Self {
                data: [MaybeUninit::uninit(); N],
            }
        }
    }

    fn new_heap() -> Heap {
        const HEAP_SIZE: usize = 1000;
        let heap_space = Box::leak(Box::new(Chonk::<HEAP_SIZE>::new()));
        let data = &mut heap_space.data;
        let assumed_location = data.as_mut_ptr().cast();

        let heap = Heap::from_slice(data);
        assert!(heap.bottom == assumed_location);
        assert!(heap.size == HEAP_SIZE);
        heap
    }

    #[test]
    fn cursor() {
        let mut heap = new_heap();
        let curs = heap.holes_mut().cursor().unwrap();
        // This is the "dummy" node
        assert_eq!(curs.previous().size, 0);
        // This is the "full" heap
        assert_eq!(curs.current().size, 1000);
        // There is no other hole
        assert!(curs.peek_next().is_none());

        let reqd = Layout::from_size_align(256, 1).unwrap();
        let _ = curs.split_current(reqd).map_err(drop).unwrap();
    }

    #[test]
    fn aff() {
        let mut heap = new_heap();
        let reqd = Layout::from_size_align(256, 1).unwrap();
        let _ = heap.allocate_first_fit(reqd).unwrap();
    }
}
