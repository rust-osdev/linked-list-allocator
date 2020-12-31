# linked-list-allocator

[![Build Status](https://travis-ci.org/phil-opp/linked-list-allocator.svg?branch=master)](https://travis-ci.org/phil-opp/linked-list-allocator)

[![Documentation](https://docs.rs/linked_list_allocator/badge.svg)](https://docs.rs/linked_list_allocator/)

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

## Features

- **`use_spin`** (default): Provide a `LockedHeap` type that implements the [`GlobalAlloc`] trait by using a spinlock.
- **`const_mut_refs`** (default): Makes the `Heap::empty` function `const`; requires nightly Rust.
- **`use_spin_nightly`** (default): Makes the `LockedHeap::empty` function `const`, automatically enables `use_spin` and `const_mut_refs`; requires nightly Rust.
- **`alloc_ref`**: Provide an implementation of the unstable [`AllocRef`] trait; requires nightly Rust.
    - Warning: The `AllocRef` trait is still regularly changed on the Rust side, so expect some regular breakage when using this feature.

[`GlobalAlloc`]: https://doc.rust-lang.org/nightly/core/alloc/trait.GlobalAlloc.html
[`AllocRef`]: https://doc.rust-lang.org/nightly/core/alloc/trait.AllocRef.html

## License
This crate is dual-licensed under MIT or the Apache License (Version 2.0). See LICENSE-APACHE and LICENSE-MIT for details.
