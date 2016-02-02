#![feature(unique)]
#![feature(const_fn)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

use hole::HoleList;

mod hole;

pub struct Heap {
    bottom: usize,
    top: usize,
    holes: HoleList,
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap {
            top: 0,
            bottom: 0,
            holes: HoleList::empty(),
        }
    }

    pub unsafe fn new(heap_bottom: usize, heap_top: usize) -> Heap {
        Heap {
            bottom: heap_bottom,
            top: heap_top,
            holes: HoleList::new(heap_bottom, heap_top - heap_bottom),
        }
    }

    pub fn allocate_first_fit(&mut self, mut size: usize, align: usize) -> Option<*mut u8> {
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }

        self.holes.allocate_first_fit(size, align)
    }

    pub unsafe fn deallocate(&mut self, ptr: *mut u8, mut size: usize, _align: usize) {
        if size < HoleList::min_size() {
            size = HoleList::min_size();
        }
        self.holes.deallocate(ptr, size);
    }

    pub fn bottom(&self) -> usize {
        self.bottom
    }

    pub fn top(&self) -> usize {
        self.top
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
    use super::hole::*;

    fn new_heap() -> Heap {
        const HEAP_SIZE: usize = 1000;
        let heap_space = Box::into_raw(Box::new([0u8; HEAP_SIZE]));

        let heap_bottom = heap_space as usize;
        let heap_top = heap_bottom + HEAP_SIZE;
        let heap = Heap::new(heap_bottom, heap_top);
        assert!(heap.bottom == heap_bottom);
        assert!(heap.top == heap_top);
        heap
    }

    #[test]
    fn allocate_double_usize() {
        let mut heap = new_heap();
        let size = size_of::<usize>() * 2;
        let addr = heap.allocate_first_fit(size, align_of::<usize>());
        assert!(addr.is_some());
        let addr = addr.unwrap() as usize;
        assert!(addr == heap.bottom);
        let (hole_addr, hole_size) = heap.holes.first_hole().expect("ERROR: no hole left");
        assert!(hole_addr == heap.bottom + size);
        assert!(hole_size == heap.top - heap.bottom - size);

        unsafe {
            assert_eq!((*((addr + size) as *const Hole)).size,
                       heap.top - heap.bottom - size);
        }
    }

    #[test]
    fn allocate_and_free_double_usize() {
        let mut heap = new_heap();

        let x = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
        unsafe {
            *(x as *mut (usize, usize)) = (0xdeafdeadbeafbabe, 0xdeafdeadbeafbabe);
        }
        heap.deallocate(x, size_of::<usize>() * 2, align_of::<usize>());

        unsafe {
            assert_eq!((*(heap.bottom as *const Hole)).size, heap.top - heap.bottom);
            assert!((*(heap.bottom as *const Hole)).next.is_none());
        }
    }

    #[test]
    fn deallocate_right_before() {
        let mut heap = new_heap();
        let size = size_of::<usize>() * 5;

        let x = heap.allocate_first_fit(size, 1).unwrap();
        let y = heap.allocate_first_fit(size, 1).unwrap();
        let z = heap.allocate_first_fit(size, 1).unwrap();

        heap.deallocate(y, size, 1);
        unsafe {
            assert_eq!((*(y as *const Hole)).size, size);
        }
        heap.deallocate(x, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size * 2);
        }
        heap.deallocate(z, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, heap.top - heap.bottom);
        }
    }

    #[test]
    fn deallocate_right_behind() {
        let mut heap = new_heap();
        let size = size_of::<usize>() * 5;

        let x = heap.allocate_first_fit(size, 1).unwrap();
        let y = heap.allocate_first_fit(size, 1).unwrap();
        let z = heap.allocate_first_fit(size, 1).unwrap();

        heap.deallocate(x, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size);
        }
        heap.deallocate(y, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size * 2);
        }
        heap.deallocate(z, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, heap.top - heap.bottom);
        }
    }

    #[test]
    fn deallocate_middle() {
        let mut heap = new_heap();
        let size = size_of::<usize>() * 5;

        let x = heap.allocate_first_fit(size, 1).unwrap();
        let y = heap.allocate_first_fit(size, 1).unwrap();
        let z = heap.allocate_first_fit(size, 1).unwrap();
        let a = heap.allocate_first_fit(size, 1).unwrap();

        heap.deallocate(x, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size);
        }
        heap.deallocate(z, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size);
            assert_eq!((*(z as *const Hole)).size, size);
        }
        heap.deallocate(y, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, size * 3);
        }
        heap.deallocate(a, size, 1);
        unsafe {
            assert_eq!((*(x as *const Hole)).size, heap.top - heap.bottom);
        }
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
