# linked-list-allocator

[![Build Status](https://travis-ci.org/phil-opp/linked-list-allocator.svg?branch=master)](https://travis-ci.org/phil-opp/linked-list-allocator)

[Documentation](https://docs.rs/crate/linked_list_allocator)

## Usage

Create a static allocator in your root module:

```rust
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();
```

Before using this allocator, you need to init it:

```rust
pub fn init_heap() {
    let heap_start = …;
    let heap_end = …;
    let heap_size = heap_end - heap_start;
    unsafe {
        ALLOCATOR.lock().init(heap_start, heap_size);
    }
}
```

## License
This crate is dual-licensed under MIT or the Apache License (Version 2.0). See LICENSE-APACHE and LICENSE-MIT for details.
