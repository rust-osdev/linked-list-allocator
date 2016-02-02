#![feature(unique)]
#![feature(const_fn)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

use hole::HoleList;

mod hole;
#[cfg(test)]
mod test;

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
