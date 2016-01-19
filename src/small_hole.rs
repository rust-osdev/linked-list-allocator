use core::ptr::Unique;
use core::mem::size_of;
use core::intrinsics;

// A hole with size == size_of::<usize>()
pub struct SmallHole {
    pub next: Option<Unique<SmallHole>>,
}

impl SmallHole {
    // Returns the first hole that has the desired alignment starting at the **next** hole. The
    // reason is that it is implemented as a single linked list (we need to update the previous
    // pointer). So even if _this_ hole would be large enough, it won't be used.
    pub fn get_first_fit(&mut self, align: usize) -> Option<Unique<SmallHole>> {
        // align must be a power of two
        assert!(unsafe { intrinsics::ctpop(align) } == 1); // exactly one bit set

        // take the next hole and set `self.next` to None
        match self.next.take() {
            None => None,
            Some(mut next) => {
                let next_addr = *next as usize;

                if next_addr % align == 0 {
                    let next_next: Option<Unique<_>> = unsafe { next.get_mut() }.next.take();
                    self.next = next_next;
                    Some(next)
                } else {
                    let ret = unsafe { next.get_mut().get_first_fit(align) };
                    self.next = Some(next);
                    ret
                }
            }
        }
    }

    pub fn add_hole(&mut self, mut hole: Unique<SmallHole>) {
        unsafe {
            assert!(hole.get().next.is_none());
        }

        let hole_addr = *hole as usize;

        if self.next.as_mut().map_or(false, |n| hole_addr < **n as usize) {
            // hole is before start of next hole or this is the last hole
            let self_addr = self as *mut _ as usize;

            if hole_addr == self_addr + size_of::<usize>() {
                // New hole is right behind this hole, so we want to increase this's size.
                // But this forms a normal sized hole, so we need to remove this block from the
                // small list
                unimplemented!();
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
