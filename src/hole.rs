use core::alloc::Layout;
use core::mem;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;

use super::align_up;

/// A sorted list of holes. It uses the the holes itself to store its nodes.
pub struct HoleList {
    first: Hole, // dummy
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

    /// Creates a `HoleList` that contains the given hole.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it
    /// creates a hole at the given `hole_addr`. This can cause undefined behavior if this address
    /// is invalid or if memory from the `[hole_addr, hole_addr+size)` range is used somewhere else.
    ///
    /// The pointer to `hole_addr` is automatically aligned.
    pub unsafe fn new(hole_addr: usize, hole_size: usize) -> HoleList {
        assert_eq!(size_of::<Hole>(), Self::min_size());

        let aligned_hole_addr = align_up(hole_addr, align_of::<Hole>());
        let ptr = aligned_hole_addr as *mut Hole;
        ptr.write(Hole {
            size: hole_size.saturating_sub(aligned_hole_addr - hole_addr),
            next: None,
        });

        HoleList {
            first: Hole {
                size: 0,
                next: Some(&mut *ptr),
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
        let size = align_up(size, mem::align_of::<Hole>());
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

        allocate_first_fit(&mut self.first, aligned_layout).map(|holeinfo| {
            (
                NonNull::new(holeinfo.addr as *mut u8).unwrap(),
                aligned_layout,
            )
        })
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
        deallocate(
            &mut self.first,
            ptr.as_ptr() as usize,
            aligned_layout.size(),
        );
        aligned_layout
    }

    /// Returns the minimal allocation size. Smaller allocations or deallocations are not allowed.
    pub fn min_size() -> usize {
        size_of::<usize>() * 2
    }

    /// Returns information about the first hole for test purposes.
    #[cfg(test)]
    pub fn first_hole(&self) -> Option<(usize, usize)> {
        self.first
            .next
            .as_ref()
            .map(|hole| ((*hole) as *const Hole as usize, hole.size))
    }
}

/// A block containing free memory. It points to the next hole and thus forms a linked list.
#[cfg(not(test))]
struct Hole {
    size: usize,
    next: Option<&'static mut Hole>,
}

#[cfg(test)]
pub struct Hole {
    pub size: usize,
    pub next: Option<&'static mut Hole>,
}

impl Hole {
    /// Returns basic information about the hole.
    fn info(&self) -> HoleInfo {
        HoleInfo {
            addr: self as *const _ as usize,
            size: self.size,
        }
    }
}

/// Basic information about a hole.
#[derive(Debug, Clone, Copy)]
struct HoleInfo {
    addr: usize,
    size: usize,
}

/// The result returned by `split_hole` and `allocate_first_fit`. Contains the address and size of
/// the allocation (in the `info` field), and the front and back padding.
struct Allocation {
    info: HoleInfo,
    front_padding: Option<HoleInfo>,
    back_padding: Option<HoleInfo>,
}

/// Splits the given hole into `(front_padding, hole, back_padding)` if it's big enough to allocate
/// `required_layout.size()` bytes with the `required_layout.align()`. Else `None` is returned.
/// Front padding occurs if the required alignment is higher than the hole's alignment. Back
/// padding occurs if the required size is smaller than the size of the aligned hole. All padding
/// must be at least `HoleList::min_size()` big or the hole is unusable.
fn split_hole(hole: HoleInfo, required_layout: Layout) -> Option<Allocation> {
    let required_size = required_layout.size();
    let required_align = required_layout.align();

    let (aligned_addr, front_padding) = if hole.addr == align_up(hole.addr, required_align) {
        // hole has already the required alignment
        (hole.addr, None)
    } else {
        // the required alignment causes some padding before the allocation
        let aligned_addr = align_up(hole.addr + HoleList::min_size(), required_align);
        (
            aligned_addr,
            Some(HoleInfo {
                addr: hole.addr,
                size: aligned_addr - hole.addr,
            }),
        )
    };

    let aligned_hole = {
        if aligned_addr + required_size > hole.addr + hole.size {
            // hole is too small
            return None;
        }
        HoleInfo {
            addr: aligned_addr,
            size: hole.size - (aligned_addr - hole.addr),
        }
    };

    let back_padding = if aligned_hole.size == required_size {
        // the aligned hole has exactly the size that's needed, no padding accrues
        None
    } else if aligned_hole.size - required_size < HoleList::min_size() {
        // we can't use this hole since its remains would form a new, too small hole
        return None;
    } else {
        // the hole is bigger than necessary, so there is some padding behind the allocation
        Some(HoleInfo {
            addr: aligned_hole.addr + required_size,
            size: aligned_hole.size - required_size,
        })
    };

    Some(Allocation {
        info: HoleInfo {
            addr: aligned_hole.addr,
            size: required_size,
        },
        front_padding: front_padding,
        back_padding: back_padding,
    })
}

/// Searches the list starting at the next hole of `previous` for a big enough hole. A hole is big
/// enough if it can hold an allocation of `layout.size()` bytes with the given `layout.align()`.
/// When a hole is used for an allocation, there may be some needed padding before and/or after
/// the allocation. The padding will then merge back to linked-list
/// This function uses the “first fit” strategy, so it breaks as soon as a big enough hole is
/// found (and returns it).
fn allocate_first_fit(mut previous: &mut Hole, layout: Layout) -> Result<HoleInfo, ()> {
    loop {
        let allocation: Option<Allocation> = previous
            .next
            .as_mut()
            .and_then(|current| split_hole(current.info(), layout.clone()));
        match allocation {
            Some(allocation) => {
                // link the front/back padding
                // Note that there must be no hole between following pair:
                // previous - front_padding
                // front_padding - back_padding
                // back_padding - previous.next
                previous.next = previous.next.as_mut().unwrap().next.take();
                if let Some(padding) = allocation.front_padding {
                    let ptr = padding.addr as *mut Hole;
                    unsafe {
                        ptr.write(Hole {
                            size: padding.size,
                            next: previous.next.take(),
                        })
                    }
                    previous.next = Some(unsafe { &mut *ptr });
                    previous = move_helper(previous).next.as_mut().unwrap();
                }
                if let Some(padding) = allocation.back_padding {
                    let ptr = padding.addr as *mut Hole;
                    unsafe {
                        ptr.write(Hole {
                            size: padding.size,
                            next: previous.next.take(),
                        })
                    }
                    previous.next = Some(unsafe { &mut *ptr });
                }
                return Ok(allocation.info);
            }
            None if previous.next.is_some() => {
                // try next hole
                previous = move_helper(previous).next.as_mut().unwrap();
            }
            None => {
                // this was the last hole, so no hole is big enough -> allocation not possible
                return Err(());
            }
        }
    }
}

/// Frees the allocation given by `(addr, size)`. It starts at the given hole and walks the list to
/// find the correct place (the list is sorted by address).
fn deallocate(mut hole: &mut Hole, addr: usize, mut size: usize) {
    loop {
        assert!(size >= HoleList::min_size());

        let hole_addr = if hole.size == 0 {
            // It's the dummy hole, which is the head of the HoleList. It's somewhere on the stack,
            // so it's address is not the address of the hole. We set the addr to 0 as it's always
            // the first hole.
            0
        } else {
            // tt's a real hole in memory and its address is the address of the hole
            hole as *mut _ as usize
        };

        // Each freed block must be handled by the previous hole in memory. Thus the freed
        // address must be always behind the current hole.
        assert!(
            hole_addr + hole.size <= addr,
            "invalid deallocation (probably a double free)"
        );

        // get information about the next block
        let next_hole_info = hole.next.as_ref().map(|next| next.info());

        match next_hole_info {
            Some(next) if hole_addr + hole.size == addr && addr + size == next.addr => {
                // block fills the gap between this hole and the next hole
                // before:  ___XXX____YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXXFFFFYYYYY____    where F is the freed block

                hole.size += size + next.size; // merge the F and Y blocks to this X block
                hole.next = hole.next.as_mut().unwrap().next.take(); // remove the Y block
            }
            _ if hole_addr + hole.size == addr => {
                // block is right behind this hole but there is used memory after it
                // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXXFFFF__YYYYY____    where F is the freed block

                // or: block is right behind this hole and this is the last hole
                // before:  ___XXX_______________    where X is this hole and Y the next hole
                // after:   ___XXXFFFF___________    where F is the freed block

                hole.size += size; // merge the F block to this X block
            }
            Some(next) if addr + size == next.addr => {
                // block is right before the next hole but there is used memory before it
                // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXX__FFFFYYYYY____    where F is the freed block

                hole.next = hole.next.as_mut().unwrap().next.take(); // remove the Y block
                size += next.size; // free the merged F/Y block in next iteration
                continue;
            }
            Some(next) if next.addr <= addr => {
                // block is behind the next hole, so we delegate it to the next hole
                // before:  ___XXX__YYYYY________    where X is this hole and Y the next hole
                // after:   ___XXX__YYYYY__FFFF__    where F is the freed block

                hole = move_helper(hole).next.as_mut().unwrap(); // start next iteration at next hole
                continue;
            }
            _ => {
                // block is between this and the next hole
                // before:  ___XXX________YYYYY_    where X is this hole and Y the next hole
                // after:   ___XXX__FFFF__YYYYY_    where F is the freed block

                // or: this is the last hole
                // before:  ___XXX_________    where X is this hole
                // after:   ___XXX__FFFF___    where F is the freed block

                let new_hole = Hole {
                    size: size,
                    next: hole.next.take(), // the reference to the Y block (if it exists)
                };
                // write the new hole to the freed memory
                debug_assert_eq!(addr % align_of::<Hole>(), 0);
                let ptr = addr as *mut Hole;
                unsafe { ptr.write(new_hole) };
                // add the F block as the next block of the X block
                hole.next = Some(unsafe { &mut *ptr });
            }
        }
        break;
    }
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
