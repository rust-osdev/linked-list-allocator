use super::*;
use core::alloc::Layout;
use std::mem::{align_of, size_of, MaybeUninit};
use std::prelude::v1::*;

#[repr(align(128))]
struct Chonk<const N: usize> {
    data: [MaybeUninit<u8>; N],
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
    assert!(heap.bottom() == assumed_location);
    assert!(heap.size() == HEAP_SIZE);
    heap
}

fn new_max_heap() -> Heap {
    const HEAP_SIZE: usize = 1024;
    const HEAP_SIZE_MAX: usize = 2048;
    let heap_space = Box::leak(Box::new(Chonk::<HEAP_SIZE_MAX>::new()));
    let data = &mut heap_space.data;
    let start_ptr = data.as_mut_ptr().cast();

    // Unsafe so that we have provenance over the whole allocation.
    let heap = unsafe { Heap::new(start_ptr, HEAP_SIZE) };
    assert!(heap.bottom() == start_ptr);
    assert!(heap.size() == HEAP_SIZE);
    heap
}

#[test]
fn empty() {
    let mut heap = Heap::empty();
    let layout = Layout::from_size_align(1, 1).unwrap();
    assert!(heap.allocate_first_fit(layout.clone()).is_err());
}

#[test]
fn oom() {
    let mut heap = new_heap();
    let layout = Layout::from_size_align(heap.size() + 1, align_of::<usize>());
    let addr = heap.allocate_first_fit(layout.unwrap());
    assert!(addr.is_err());
}

#[test]
fn allocate_double_usize() {
    let mut heap = new_heap();
    let size = size_of::<usize>() * 2;
    let layout = Layout::from_size_align(size, align_of::<usize>());
    let addr = heap.allocate_first_fit(layout.unwrap());
    assert!(addr.is_ok());
    let addr = addr.unwrap().as_ptr();
    assert!(addr == heap.bottom());
    let (hole_addr, hole_size) = heap.holes.first_hole().expect("ERROR: no hole left");
    assert!(hole_addr == heap.bottom().wrapping_add(size));
    assert!(hole_size == heap.size() - size);

    unsafe {
        assert_eq!(
            (*((addr.wrapping_add(size)) as *const Hole)).size,
            heap.size() - size
        );
    }
}

#[test]
fn allocate_and_free_double_usize() {
    let mut heap = new_heap();

    let layout = Layout::from_size_align(size_of::<usize>() * 2, align_of::<usize>()).unwrap();
    let x = heap.allocate_first_fit(layout.clone()).unwrap();
    unsafe {
        *(x.as_ptr() as *mut (usize, usize)) = (0xdeafdeadbeafbabe, 0xdeafdeadbeafbabe);

        heap.deallocate(x, layout.clone());
        let real_first = heap.holes.first.next.as_ref().unwrap().as_ref();

        assert_eq!(real_first.size, heap.size());
        assert!(real_first.next.is_none());
    }
}

#[test]
fn deallocate_right_before() {
    let mut heap = new_heap();
    let layout = Layout::from_size_align(size_of::<usize>() * 5, 1).unwrap();

    let x = heap.allocate_first_fit(layout.clone()).unwrap();
    let y = heap.allocate_first_fit(layout.clone()).unwrap();
    let z = heap.allocate_first_fit(layout.clone()).unwrap();

    unsafe {
        heap.deallocate(y, layout.clone());
        assert_eq!((*(y.as_ptr() as *const Hole)).size, layout.size());
        heap.deallocate(x, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, layout.size() * 2);
        heap.deallocate(z, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, heap.size());
    }
}

#[test]
fn deallocate_right_behind() {
    let mut heap = new_heap();
    let size = size_of::<usize>() * 5;
    let layout = Layout::from_size_align(size, 1).unwrap();

    let x = heap.allocate_first_fit(layout.clone()).unwrap();
    let y = heap.allocate_first_fit(layout.clone()).unwrap();
    let z = heap.allocate_first_fit(layout.clone()).unwrap();

    unsafe {
        heap.deallocate(x, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, size);
        heap.deallocate(y, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, size * 2);
        heap.deallocate(z, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, heap.size());
    }
}

#[test]
fn deallocate_middle() {
    let mut heap = new_heap();
    let size = size_of::<usize>() * 5;
    let layout = Layout::from_size_align(size, 1).unwrap();

    let x = heap.allocate_first_fit(layout.clone()).unwrap();
    let y = heap.allocate_first_fit(layout.clone()).unwrap();
    let z = heap.allocate_first_fit(layout.clone()).unwrap();
    let a = heap.allocate_first_fit(layout.clone()).unwrap();

    unsafe {
        heap.deallocate(x, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, size);
        heap.deallocate(z, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, size);
        assert_eq!((*(z.as_ptr() as *const Hole)).size, size);
        heap.deallocate(y, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, size * 3);
        heap.deallocate(a, layout.clone());
        assert_eq!((*(x.as_ptr() as *const Hole)).size, heap.size());
    }
}

#[test]
fn reallocate_double_usize() {
    let mut heap = new_heap();

    let layout = Layout::from_size_align(size_of::<usize>() * 2, align_of::<usize>()).unwrap();

    let x = heap.allocate_first_fit(layout.clone()).unwrap();
    unsafe {
        heap.deallocate(x, layout.clone());
    }

    let y = heap.allocate_first_fit(layout.clone()).unwrap();
    unsafe {
        heap.deallocate(y, layout.clone());
    }

    assert_eq!(x, y);
}

#[test]
fn allocate_many_size_aligns() {
    use core::ops::{Range, RangeInclusive};

    #[cfg(not(miri))]
    const SIZE: RangeInclusive<usize> = 1..=512;

    #[cfg(miri)]
    const SIZE: RangeInclusive<usize> = 256..=(256 + core::mem::size_of::<crate::hole::Hole>());

    #[cfg(not(miri))]
    const ALIGN: Range<usize> = 0..10;

    #[cfg(miri)]
    const ALIGN: Range<usize> = 1..4;

    const STRATS: Range<usize> = 0..4;

    let mut heap = new_heap();
    assert_eq!(heap.size(), 1000);

    heap.holes.debug();

    let max_alloc = Layout::from_size_align(1000, 1).unwrap();
    let full = heap.allocate_first_fit(max_alloc).unwrap();
    unsafe {
        heap.deallocate(full, max_alloc);
    }

    heap.holes.debug();

    struct Alloc {
        alloc: NonNull<u8>,
        layout: Layout,
    }

    // NOTE: Printing to the console SIGNIFICANTLY slows down miri.

    for strat in STRATS {
        for align in ALIGN {
            for size in SIZE {
                #[cfg(not(miri))]
                {
                    println!("=========================================================");
                    println!("Align: {}", 1 << align);
                    println!("Size:  {}", size);
                    println!("Free Pattern: {}/0..4", strat);
                    println!();
                }
                let mut allocs = vec![];

                let layout = Layout::from_size_align(size, 1 << align).unwrap();
                while let Ok(alloc) = heap.allocate_first_fit(layout) {
                    #[cfg(not(miri))]
                    heap.holes.debug();
                    allocs.push(Alloc { alloc, layout });
                }

                #[cfg(not(miri))]
                println!("Allocs: {} - {} bytes", allocs.len(), allocs.len() * size);

                match strat {
                    0 => {
                        // Forward
                        allocs.drain(..).for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                    }
                    1 => {
                        // Backwards
                        allocs.drain(..).rev().for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                    }
                    2 => {
                        // Interleaved forwards
                        let mut a = Vec::new();
                        let mut b = Vec::new();
                        for (i, alloc) in allocs.drain(..).enumerate() {
                            if (i % 2) == 0 {
                                a.push(alloc);
                            } else {
                                b.push(alloc);
                            }
                        }
                        a.drain(..).for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                        b.drain(..).for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                    }
                    3 => {
                        // Interleaved backwards
                        let mut a = Vec::new();
                        let mut b = Vec::new();
                        for (i, alloc) in allocs.drain(..).rev().enumerate() {
                            if (i % 2) == 0 {
                                a.push(alloc);
                            } else {
                                b.push(alloc);
                            }
                        }
                        a.drain(..).for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                        b.drain(..).for_each(|a| unsafe {
                            heap.deallocate(a.alloc, a.layout);
                            #[cfg(not(miri))]
                            heap.holes.debug();
                        });
                    }
                    _ => panic!(),
                }

                #[cfg(not(miri))]
                println!("MAX CHECK");

                let full = heap.allocate_first_fit(max_alloc).unwrap();
                unsafe {
                    heap.deallocate(full, max_alloc);
                }

                #[cfg(not(miri))]
                println!();
            }
        }
    }
}

#[test]
fn allocate_multiple_sizes() {
    let mut heap = new_heap();
    let base_size = size_of::<usize>();
    let base_align = align_of::<usize>();

    let layout_1 = Layout::from_size_align(base_size * 2, base_align).unwrap();
    let layout_2 = Layout::from_size_align(base_size * 7, base_align).unwrap();
    let layout_3 = Layout::from_size_align(base_size * 3, base_align * 4).unwrap();
    let layout_4 = Layout::from_size_align(base_size * 4, base_align).unwrap();

    let x = heap.allocate_first_fit(layout_1.clone()).unwrap();
    let y = heap.allocate_first_fit(layout_2.clone()).unwrap();
    assert_eq!(y.as_ptr() as usize, x.as_ptr() as usize + base_size * 2);
    let z = heap.allocate_first_fit(layout_3.clone()).unwrap();
    assert_eq!(z.as_ptr() as usize % (base_size * 4), 0);

    unsafe {
        heap.deallocate(x, layout_1.clone());
    }

    let a = heap.allocate_first_fit(layout_4.clone()).unwrap();
    let b = heap.allocate_first_fit(layout_1.clone()).unwrap();
    assert_eq!(b, x);

    unsafe {
        heap.deallocate(y, layout_2);
        heap.deallocate(z, layout_3);
        heap.deallocate(a, layout_4);
        heap.deallocate(b, layout_1);
    }
}

// This test makes sure that the heap works correctly when the input slice has
// a variety of non-Hole aligned starting addresses
#[test]
fn allocate_multiple_unaligned() {
    for offset in 0..=Layout::new::<Hole>().size() {
        let mut heap = new_heap_skip(offset);
        let base_size = size_of::<usize>();
        let base_align = align_of::<usize>();

        let layout_1 = Layout::from_size_align(base_size * 2, base_align).unwrap();
        let layout_2 = Layout::from_size_align(base_size * 7, base_align).unwrap();
        let layout_3 = Layout::from_size_align(base_size * 3, base_align * 4).unwrap();
        let layout_4 = Layout::from_size_align(base_size * 4, base_align).unwrap();

        let x = heap.allocate_first_fit(layout_1.clone()).unwrap();
        let y = heap.allocate_first_fit(layout_2.clone()).unwrap();
        assert_eq!(y.as_ptr() as usize, x.as_ptr() as usize + base_size * 2);
        let z = heap.allocate_first_fit(layout_3.clone()).unwrap();
        assert_eq!(z.as_ptr() as usize % (base_size * 4), 0);

        unsafe {
            heap.deallocate(x, layout_1.clone());
        }

        let a = heap.allocate_first_fit(layout_4.clone()).unwrap();
        let b = heap.allocate_first_fit(layout_1.clone()).unwrap();
        assert_eq!(b, x);

        unsafe {
            heap.deallocate(y, layout_2);
            heap.deallocate(z, layout_3);
            heap.deallocate(a, layout_4);
            heap.deallocate(b, layout_1);
        }
    }
}

fn new_heap_skip(ct: usize) -> Heap {
    const HEAP_SIZE: usize = 1000;
    let heap_space = Box::leak(Box::new(Chonk::<HEAP_SIZE>::new()));
    let data = &mut heap_space.data[ct..];
    let heap = Heap::from_slice(data);
    heap
}

#[test]
fn allocate_usize() {
    let mut heap = new_heap();

    let layout = Layout::from_size_align(size_of::<usize>(), 1).unwrap();

    assert!(heap.allocate_first_fit(layout.clone()).is_ok());
}

#[test]
fn allocate_usize_in_bigger_block() {
    let mut heap = new_heap();

    let layout_1 = Layout::from_size_align(size_of::<usize>() * 2, 1).unwrap();
    let layout_2 = Layout::from_size_align(size_of::<usize>(), 1).unwrap();

    let x = heap.allocate_first_fit(layout_1.clone()).unwrap();
    let y = heap.allocate_first_fit(layout_1.clone()).unwrap();
    unsafe {
        heap.deallocate(x, layout_1.clone());
    }

    let z = heap.allocate_first_fit(layout_2.clone());
    assert!(z.is_ok());
    let z = z.unwrap();
    assert_eq!(x, z);

    unsafe {
        heap.deallocate(y, layout_1.clone());
        heap.deallocate(z, layout_2);
    }
}

#[test]
// see https://github.com/phil-opp/blog_os/issues/160
fn align_from_small_to_big() {
    let mut heap = new_heap();

    let layout_1 = Layout::from_size_align(28, 4).unwrap();
    let layout_2 = Layout::from_size_align(8, 8).unwrap();

    // allocate 28 bytes so that the heap end is only 4 byte aligned
    assert!(heap.allocate_first_fit(layout_1.clone()).is_ok());
    // try to allocate a 8 byte aligned block
    assert!(heap.allocate_first_fit(layout_2.clone()).is_ok());
}

#[test]
fn extend_empty_heap() {
    let mut heap = new_max_heap();

    unsafe {
        heap.extend(1024);
    }

    // Try to allocate full heap after extend
    let layout = Layout::from_size_align(2048, 1).unwrap();
    assert!(heap.allocate_first_fit(layout.clone()).is_ok());
}

#[test]
fn extend_full_heap() {
    let mut heap = new_max_heap();

    let layout = Layout::from_size_align(1024, 1).unwrap();

    // Allocate full heap, extend and allocate again to the max
    assert!(heap.allocate_first_fit(layout.clone()).is_ok());
    unsafe {
        heap.extend(1024);
    }
    assert!(heap.allocate_first_fit(layout.clone()).is_ok());
}

#[test]
fn extend_fragmented_heap() {
    let mut heap = new_max_heap();

    let layout_1 = Layout::from_size_align(512, 1).unwrap();
    let layout_2 = Layout::from_size_align(1024, 1).unwrap();

    let alloc1 = heap.allocate_first_fit(layout_1.clone());
    let alloc2 = heap.allocate_first_fit(layout_1.clone());

    assert!(alloc1.is_ok());
    assert!(alloc2.is_ok());

    unsafe {
        // Create a hole at the beginning of the heap
        heap.deallocate(alloc1.unwrap(), layout_1.clone());
    }

    unsafe {
        heap.extend(1024);
    }

    // We got additional 1024 bytes hole at the end of the heap
    // Try to allocate there
    assert!(heap.allocate_first_fit(layout_2.clone()).is_ok());
}
