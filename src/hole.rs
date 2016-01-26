use core::ptr::Unique;
use core::mem::{self, size_of};

#[cfg(not(test))]
macro_rules! println {
    ($fmt:expr) => {  };
    ($fmt:expr, $($arg:tt)*) => {  };
}

use super::align_up;

pub struct HoleList {
    first: Hole, // dummy
}

impl HoleList {
    pub const fn empty() -> HoleList {
        HoleList {
            first: Hole {
                size: 0,
                next: None,
            },
        }
    }

    pub unsafe fn new(ptr: *mut Hole, size: usize) -> HoleList {
        assert!(size_of::<Hole>() == Self::min_size());

        mem::forget(mem::replace(&mut *ptr,
                                 Hole {
                                     size: size,
                                     next: None,
                                 }));

        HoleList {
            first: Hole {
                size: 0,
                next: Some(Unique::new(ptr)),
            },
        }
    }

    pub fn allocate_first_fit(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        println!("allocate {} bytes (align {})", size, align);
        assert!(size >= Self::min_size());

        if let Some((start_addr, front_padding, back_padding)) =
               allocate_first_fit(&mut self.first, size, align) {
            if let Some((padding_addr, padding_size)) = front_padding {
                self.deallocate(padding_addr as *mut u8, padding_size)
            }
            if let Some((padding_addr, padding_size)) = back_padding {
                self.deallocate(padding_addr as *mut u8, padding_size)
            }
            Some(start_addr as *mut u8)
        } else {
            None
        }
    }

    pub fn deallocate(&mut self, ptr: *mut u8, size: usize) {
        println!("deallocate {:p} ({} bytes)", ptr, size);
        assert!(size >= Self::min_size());

        deallocate(&mut self.first, ptr as usize, size)
    }

    pub fn min_size() -> usize {
        size_of::<usize>() * 2
    }

    #[cfg(test)]
    pub fn first_hole(&self) -> Option<(usize, usize)> {
        if let Some(first) = self.first.next.as_ref() {
            Some((**first as usize, unsafe { first.get().size }))
        } else {
            None
        }
    }
}

pub struct Hole {
    pub size: usize,
    pub next: Option<Unique<Hole>>,
}

fn allocate_first_fit(previous: &mut Hole,
                      size: usize,
                      align: usize)
                      -> Option<(usize, Option<(usize, usize)>, Option<(usize, usize)>)> {
    let mut front_padding = None;
    let mut back_padding = None;

    if previous.next.is_some() {
        let hole_addr = **previous.next.as_ref().unwrap() as usize;
        let aligned_hole_addr = align_up(hole_addr, align);

        if aligned_hole_addr > hole_addr {
            if aligned_hole_addr < hole_addr + HoleList::min_size() {
                // hole would cause a new, too small hole. try next hole
                return allocate_first_fit(unsafe { previous.next.as_mut().unwrap().get_mut() },
                                          size,
                                          align);
            } else {
                let padding_hole_size = aligned_hole_addr - hole_addr;
                front_padding = Some((hole_addr, padding_hole_size));
            }
        }

        let aligned_hole_size = unsafe { previous.next.as_ref().unwrap().get().size } -
                                (aligned_hole_addr - hole_addr);

        if aligned_hole_size > size {
            if aligned_hole_size - size < HoleList::min_size() {
                // hole would cause a new, too small hole. try next hole
                return allocate_first_fit(unsafe { previous.next.as_mut().unwrap().get_mut() },
                                          size,
                                          align);
            } else {
                let padding_hole_size = aligned_hole_size - size;
                back_padding = Some((aligned_hole_addr + size, padding_hole_size));
            }
        }

        if aligned_hole_size >= size {
            previous.next = unsafe { previous.next.as_mut().unwrap().get_mut().next.take() };
            Some((aligned_hole_addr, front_padding, back_padding))
        } else {
            // hole is too small, try next hole
            return allocate_first_fit(unsafe { previous.next.as_mut().unwrap().get_mut() },
                                      size,
                                      align);
        }
    } else {
        None
    }
}

fn deallocate(hole: &mut Hole, addr: usize, size: usize) {
    let hole_addr = if hole.size == 0 {
        0   // dummy
    } else {
        hole as *mut _ as usize
    };
    assert!(addr >= hole_addr + hole.size);



    match mem::replace(&mut hole.next, None) {
        Some(ref mut next) if addr == hole_addr + hole.size && addr + size == **next as usize => {
            // block fills the gap between this hole and the next hole
            // before:  ___XXX____YYYYY____    where X is this hole and Y the next hole
            // after:   ___XXXFFFFYYYYY____    where F is the freed block
            hole.size += size + unsafe { next.get().size };
            hole.next = unsafe { next.get_mut() }.next.take();
        }
        Some(ref next) if addr == hole_addr + hole.size => {
            // block is right behind this hole but there is used memory after it
            // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
            // after:   ___XXXFFFF__YYYYY____    where F is the freed block
            hole.size += size;

            // hole.next should stay the same
            hole.next = Some(unsafe { Unique::new(**next) }); //hack to avoid implementing clone
        }
        Some(ref mut next) if addr + size == **next as usize => {
            // block is right before the next hole but there is used memory before it
            // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
            // after:   ___XXX__FFFFYYYYY____    where F is the freed block
            let next_hole_next = unsafe { next.get_mut() }.next.take();
            let next_hole_size = unsafe { next.get() }.size;
            hole.next = next_hole_next; // delete next block
            deallocate(hole, addr, size + next_hole_size); // free it again as a big block
        }
        Some(ref mut next) if addr >= **next as usize => {
            // block is behind the next hole, so we delegate it to the next hole
            // before:  ___XXX__YYYYY________    where X is this hole and Y the next hole
            // after:   ___XXX__YYYYY__FFFF__    where F is the freed block

            // hole.next should stay the same
            hole.next = Some(unsafe { Unique::new(**next) }); // hack to avoid implementing clone
            deallocate(unsafe { next.get_mut() }, addr, size);
        }
        next => {
            // block is between this and the next hole
            // before:  ___XXX________YYYYY_    where X is this hole and Y the next hole
            // after:   ___XXX__FFFF__YYYYY_    where F is the freed block

            // or: this is the last hole
            // before:  ___XXX_________    where X is this hole
            // after:   ___XXX__FFFF___    where F is the freed block

            let new_hole = Hole {
                size: size,
                next: next,
            };
            let ptr = addr as *mut Hole;
            mem::forget(mem::replace(unsafe { &mut *ptr }, new_hole));
            hole.next = Some(unsafe { Unique::new(ptr) });
        }
    }
}
