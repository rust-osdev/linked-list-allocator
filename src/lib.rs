#![feature(const_fn)]
#![feature(unique)]
#![feature(core_intrinsics)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

use core::ptr::Unique;
use core::mem::{self, size_of};

use hole::Hole;
use small_hole::SmallHole;

mod hole;
mod small_hole;

pub struct Heap {
    holes: Hole, // dummy
    small_holes: SmallHole, // dummy
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap {
            holes: Hole {
                size: 0,
                next: None,
            },
            small_holes: SmallHole { next: None },
        }
    }

    pub fn new(heap_bottom: usize, heap_top: usize) -> Heap {
        assert!(size_of::<SmallHole>() == size_of::<usize>());
        assert!(size_of::<Hole>() == size_of::<usize>() * 2);

        let first_hole = Hole {
            size: heap_top - heap_bottom,
            next: None,
        };

        let mut first_hole_ptr = unsafe { Unique::new(heap_bottom as *mut Hole) };
        unsafe { mem::forget(mem::replace(first_hole_ptr.get_mut(), first_hole)) };

        let mut heap = Heap::empty();
        heap.holes.next = Some(first_hole_ptr);
        heap
    }

    pub fn allocate_first_fit(&mut self, mut size: usize, align: usize) -> Option<*mut u8> {
        size = align_up(size, size_of::<usize>());
        let mut ret = None;

        if size == size_of::<SmallHole>() {
            ret = ret.or_else(|| {
                self.small_holes.get_first_fit(align).map(|hole| {
                    let hole_start_addr = *hole as usize;
                    assert!(hole_start_addr % align == 0);
                    hole_start_addr as *mut u8
                })
            });
        }

        ret = ret.or_else(|| {
            self.holes.get_first_fit(size, align).map(|hole| {
                let hole_start_addr = *hole as usize;
                let aligned_address = align_up(hole_start_addr, align);
                let padding = aligned_address - hole_start_addr;
                if padding > 0 {
                    assert!(unsafe { hole.get().size } - padding >= size);
                    self.deallocate(*hole as *mut u8, padding, 1);
                }
                aligned_address as *mut u8
            })
        });
        
        ret
    }

    pub fn deallocate(&mut self, ptr: *mut u8, mut size: usize, _align: usize) {
        if size <= size_of::<SmallHole>() {
            let hole = SmallHole { next: None };
            let mut hole_ptr = unsafe { Unique::new(ptr as *mut SmallHole) };
            unsafe { mem::forget(mem::replace(hole_ptr.get_mut(), hole)) };

            self.small_holes.add_hole(hole_ptr);
        } else {
            if size < size_of::<Hole>() {
                size = size_of::<Hole>();
            }
            let hole = Hole {
                size: size,
                next: None,
            };
            let mut hole_ptr = unsafe { Unique::new(ptr as *mut Hole) };
            unsafe { mem::forget(mem::replace(hole_ptr.get_mut(), hole)) };

            self.holes.add_hole(hole_ptr);
        }
    }
}

fn align_down(value: usize, align: usize) -> usize {
    value / align * align
}

fn align_up(value: usize, align: usize) -> usize {
    align_down(value + align - 1, align)
}

#[cfg(test)]
mod test {
    use std::prelude::v1::*;
    use std::mem::{size_of, align_of};
    use super::*;

    fn new_heap() -> Heap {
        const HEAP_SIZE: usize = 1000;
        let dummy = Box::into_raw(Box::new([0u8; HEAP_SIZE]));

        let heap_bottom = dummy as usize;
        let heap_top = heap_bottom + HEAP_SIZE;
        Heap::new(heap_bottom, heap_top)
    }

    #[test]
    fn allocate_double_usize() {
        let mut heap = new_heap();
        assert!(heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).is_some());
    }

    #[test]
    fn allocate_and_free_double_usize() {
        let mut heap = new_heap();

        let x = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
        unsafe {
            *(x as *mut (usize, usize)) = (0xdeafdeadbeafbabe, 0xdeafdeadbeafbabe);
        }
        heap.deallocate(x, size_of::<usize>() * 2, align_of::<usize>());
    }

    #[test]
    fn reallocate_double_usize() {
        let mut heap = new_heap();

        let x = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
        heap.deallocate(x, size_of::<usize>() * 2, align_of::<usize>());

        let y = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
        heap.deallocate(y, size_of::<usize>() * 2, align_of::<usize>());

        assert_eq!(x, y);
    }

    #[test]
    fn allocate_multiple_sizes() {
        let mut heap = new_heap();
        let base_size = size_of::<usize>();
        let base_align = align_of::<usize>();

        let x = heap.allocate_first_fit(base_size * 2, base_align).unwrap();
        let y = heap.allocate_first_fit(base_size * 7, base_align).unwrap();
        assert_eq!(y as usize, x as usize + base_size * 2);
        let z = heap.allocate_first_fit(base_size * 3, base_align * 4).unwrap();
        assert_eq!(z as usize % (base_size * 4), 0);

        heap.deallocate(x, base_size * 2, base_align);

        let a = heap.allocate_first_fit(base_size * 4, base_align).unwrap();
        let b = heap.allocate_first_fit(base_size * 2, base_align).unwrap();
        assert_eq!(b, x);

        heap.deallocate(y, base_size * 7, base_align);
        heap.deallocate(z, base_size * 3, base_align * 4);
        heap.deallocate(a, base_size * 4, base_align);
        heap.deallocate(b, base_size * 2, base_align);
    }

    #[test]
    fn allocate_usize() {
        let mut heap = new_heap();

        assert!(heap.allocate_first_fit(size_of::<usize>(), 1).is_some());
    }
    
    
    #[test]
    fn allocate_usize_in_bigger_block() {
        let mut heap = new_heap();

        let x = heap.allocate_first_fit(size_of::<usize>() * 2, 1).unwrap();        
        let y = heap.allocate_first_fit(size_of::<usize>() * 2, 1).unwrap();
        heap.deallocate(x, size_of::<usize>() * 2, 1);
        
        let z = heap.allocate_first_fit(size_of::<usize>(), 1);
        assert!(z.is_some());
        let z = z.unwrap();
        assert_eq!(x, z);
        
        heap.deallocate(y, size_of::<usize>() * 2, 1);
        heap.deallocate(z, size_of::<usize>(), 1);
    }
}
