use std::prelude::v1::*;
use std::mem::{size_of, align_of};
use super::*;
use super::hole::*;

fn new_heap() -> Heap {
    const HEAP_SIZE: usize = 1000;
    let heap_space = Box::into_raw(Box::new([0u8; HEAP_SIZE]));

    let heap = unsafe { Heap::new(heap_space as usize, HEAP_SIZE) };
    assert!(heap.bottom == heap_space as usize);
    assert!(heap.size == HEAP_SIZE);
    heap
}

#[test]
fn empty() {
    let mut heap = Heap::empty();
    assert!(heap.allocate_first_fit(1, 1).is_none());
}

#[test]
fn oom() {
    let mut heap = new_heap();
    let size = heap.size() + 1;
    let addr = heap.allocate_first_fit(size, align_of::<usize>());
    assert!(addr.is_none());
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
    assert!(hole_size == heap.size - size);

    unsafe {
        assert_eq!((*((addr + size) as *const Hole)).size, heap.size - size);
    }
}

#[test]
fn allocate_and_free_double_usize() {
    let mut heap = new_heap();

    let x = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
    unsafe {
        *(x as *mut (usize, usize)) = (0xdeafdeadbeafbabe, 0xdeafdeadbeafbabe);

        heap.deallocate(x, size_of::<usize>() * 2, align_of::<usize>());
        assert_eq!((*(heap.bottom as *const Hole)).size, heap.size);
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

    unsafe {
        heap.deallocate(y, size, 1);
        assert_eq!((*(y as *const Hole)).size, size);
        heap.deallocate(x, size, 1);
        assert_eq!((*(x as *const Hole)).size, size * 2);
        heap.deallocate(z, size, 1);
        assert_eq!((*(x as *const Hole)).size, heap.size);
    }
}

#[test]
fn deallocate_right_behind() {
    let mut heap = new_heap();
    let size = size_of::<usize>() * 5;

    let x = heap.allocate_first_fit(size, 1).unwrap();
    let y = heap.allocate_first_fit(size, 1).unwrap();
    let z = heap.allocate_first_fit(size, 1).unwrap();

    unsafe {
        heap.deallocate(x, size, 1);
        assert_eq!((*(x as *const Hole)).size, size);
        heap.deallocate(y, size, 1);
        assert_eq!((*(x as *const Hole)).size, size * 2);
        heap.deallocate(z, size, 1);
        assert_eq!((*(x as *const Hole)).size, heap.size);
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

    unsafe {
        heap.deallocate(x, size, 1);
        assert_eq!((*(x as *const Hole)).size, size);
        heap.deallocate(z, size, 1);
        assert_eq!((*(x as *const Hole)).size, size);
        assert_eq!((*(z as *const Hole)).size, size);
        heap.deallocate(y, size, 1);
        assert_eq!((*(x as *const Hole)).size, size * 3);
        heap.deallocate(a, size, 1);
        assert_eq!((*(x as *const Hole)).size, heap.size);
    }
}

#[test]
fn reallocate_double_usize() {
    let mut heap = new_heap();

    let x = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
    unsafe {
        heap.deallocate(x, size_of::<usize>() * 2, align_of::<usize>());
    }

    let y = heap.allocate_first_fit(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
    unsafe {
        heap.deallocate(y, size_of::<usize>() * 2, align_of::<usize>());
    }

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

    unsafe {
        heap.deallocate(x, base_size * 2, base_align);
    }

    let a = heap.allocate_first_fit(base_size * 4, base_align).unwrap();
    let b = heap.allocate_first_fit(base_size * 2, base_align).unwrap();
    assert_eq!(b, x);

    unsafe {
        heap.deallocate(y, base_size * 7, base_align);
        heap.deallocate(z, base_size * 3, base_align * 4);
        heap.deallocate(a, base_size * 4, base_align);
        heap.deallocate(b, base_size * 2, base_align);
    }
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
    unsafe {
        heap.deallocate(x, size_of::<usize>() * 2, 1);
    }

    let z = heap.allocate_first_fit(size_of::<usize>(), 1);
    assert!(z.is_some());
    let z = z.unwrap();
    assert_eq!(x, z);

    unsafe {
        heap.deallocate(y, size_of::<usize>() * 2, 1);
        heap.deallocate(z, size_of::<usize>(), 1);
    }
}

#[test]
// see https://github.com/phil-opp/blog_os/issues/160
fn align_from_small_to_big() {
    let mut heap = new_heap();

    // allocate 28 bytes so that the heap end is only 4 byte aligned
    assert!(heap.allocate_first_fit(28, 4).is_some());
    // try to allocate a 8 byte aligned block
    assert!(heap.allocate_first_fit(8, 8).is_some());
}
