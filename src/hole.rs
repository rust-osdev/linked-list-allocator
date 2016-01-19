use core::ptr::Unique;
use core::mem::{self, size_of};
use core::intrinsics;

use super::align_up;

pub struct Hole {
    pub size: usize,
    pub next: Option<Unique<Hole>>,
}

impl Hole {
    // Returns the first hole that is big enough starting at the **next** hole. The reason is that
    // it is implemented as a single linked list (we need to update the previous pointer). So even
    // if _this_ hole would be large enough, it won't be used.
    pub fn get_first_fit(&mut self, size: usize, align: usize) -> Option<Unique<Hole>> {
        assert!(size % size_of::<usize>() == 0);
        // align must be a power of two
        assert!(unsafe { intrinsics::ctpop(align) } == 1); // exactly one bit set

        // take the next hole and set `self.next` to None
        match self.next.take() {
            None => None,
            Some(mut next) => {
                let next_addr = *next as usize;
                let start_addr = align_up(next_addr, align);

                // the needed padding for the desired alignment
                let padding = start_addr - next_addr;
                assert!(padding == 0 || padding >= size_of::<usize>() * 2); // TODO
                let next_real_size = unsafe { next.get() }.size - padding;

                if next_real_size == size {
                    let next_next: Option<Unique<_>> = unsafe { next.get_mut() }.next.take();
                    self.next = next_next;
                    Some(next)
                } else if next_real_size > size {
                    let next_next: Option<Unique<_>> = unsafe { next.get_mut() }.next.take();
                    let new_hole = Hole {
                        size: next_real_size - size,
                        next: next_next,
                    };
                    unsafe {
                        let mut new_hole_ptr = Unique::new((start_addr + size) as *mut Hole);
                        mem::forget(mem::replace(new_hole_ptr.get_mut(), new_hole));
                        self.next = Some(new_hole_ptr);
                    }
                    Some(next)
                } else {
                    let ret = unsafe { next.get_mut().get_first_fit(size, align) };
                    self.next = Some(next);
                    ret
                }
            }
        }
    }

    pub fn add_hole(&mut self, mut hole: Unique<Hole>) {
        unsafe {
            if hole.get().size == 0 {
                return;
            }
            assert!(hole.get().size % size_of::<usize>() == 0);
            assert!(hole.get().next.is_none());
        }

        let hole_addr = *hole as usize;

        if self.next.as_mut().map_or(false, |n| hole_addr < **n as usize) {
            // hole is before start of next hole or this is the last hole
            let self_addr = self as *mut _ as usize;

            if hole_addr == self_addr + self.size {
                // new hole is right behind this hole, so we can just increase this's size
                self.size += unsafe { hole.get().size };
            } else {
                // insert the hole behind this hole
                unsafe { hole.get_mut() }.next = self.next.take();
                self.next = Some(hole);
            }
        } else {
            // hole is behind next hole
            assert!(self.next.is_some());
            let next = self.next.as_mut().unwrap();
            assert!(hole_addr > **next as usize);

            // insert it behind next hole
            unsafe { next.get_mut().add_hole(hole) };
        }
    }
}
