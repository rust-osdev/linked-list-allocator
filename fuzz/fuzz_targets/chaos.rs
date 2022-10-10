#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use linked_list_allocator::Heap;
use std::alloc::Layout;
use std::ptr::NonNull;

#[derive(Debug, Arbitrary)]
enum Action {
    // allocate a chunk with the size specified
    Alloc { size: u16, align_bit: u8 },
    // free the pointer at the index specified
    Free { index: u8 },
    // extend the heap by amount specified
    Extend { additional: u16 },
}
use Action::*;

const MAX_HEAP_SIZE: usize = 5000;
static mut HEAP_MEM: [u8; MAX_HEAP_SIZE] = [0; MAX_HEAP_SIZE];
const DEBUG: bool = false;

fuzz_target!(|data: (u16, Vec<Action>)| {
    let (size, actions) = data;
    let _ = fuzz(size, actions);
});

fn fuzz(size: u16, actions: Vec<Action>) {
    // init heap
    let mut heap = unsafe {
        let size = size as usize;
        if size > MAX_HEAP_SIZE || size < 3 * core::mem::size_of::<usize>() {
            return;
        }

        Heap::new(HEAP_MEM.as_mut_ptr(), size)
    };
    let mut ptrs: Vec<(NonNull<u8>, Layout)> = Vec::new();

    if DEBUG {
        heap.debug();
    }

    // process operations
    for action in actions {
        if DEBUG {
            println!("-----\nnext action: {:?}", action);
        }
        match action {
            Alloc { size, align_bit } => {
                let layout = {
                    let align = 1_usize.rotate_left(align_bit as u32);
                    if align == 1 << 63 {
                        return;
                    }
                    Layout::from_size_align(size as usize, align).unwrap()
                };

                if let Ok(ptr) = heap.allocate_first_fit(layout) {
                    if DEBUG {
                        println!("alloc'd {:?}", ptr);
                    }
                    ptrs.push((ptr, layout));
                } else {
                    return;
                }
            }
            Free { index } => {
                if index as usize >= ptrs.len() {
                    return;
                }

                let (ptr, layout) = ptrs.swap_remove(index as usize);
                if DEBUG {
                    println!("removing {:?}, size: {}", ptr, layout.size());
                }
                unsafe {
                    heap.deallocate(ptr, layout);
                }
            }
            Extend { additional } =>
            // safety: new heap size never exceeds MAX_HEAP_SIZE
            unsafe {
                let remaining_space = HEAP_MEM
                    .as_mut_ptr()
                    .add(MAX_HEAP_SIZE)
                    .offset_from(heap.top());
                assert!(remaining_space >= 0);

                if additional as isize > remaining_space {
                    return;
                }

                heap.extend(additional as usize);
                if DEBUG {
                    println!("new heap size: {}, top: {:?}", heap.size(), heap.top());
                }
            },
        }
        if DEBUG {
            println!("after action:");
            print!("live allocs: ");
            for ptr in &ptrs {
                print!("({:?}, {},{}), ", ptr.0, ptr.1.size(), ptr.1.align());
            }
            println!();
            heap.debug();
        }
    }

    // free the remaining allocations
    for (ptr, layout) in ptrs {
        if DEBUG {
            println!("removing {:?}, size: {}", ptr, layout.size());
        }
        unsafe {
            heap.deallocate(ptr, layout);
        }
    }
}
