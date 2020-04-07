- Update AllocRef implementation for latest API changes ([#30](https://github.com/phil-opp/linked-list-allocator/pull/30))

# 0.8.1

- AllocRef::alloc is now safe and allows zero-sized allocations ([#28](https://github.com/phil-opp/linked-list-allocator/pull/28))
    - This is technically a **breaking change** for the unstable `alloc_ref` feature of this crate because it now requires a newer nightly version of Rust.

# 0.8.0

- **Breaking**: Make AllocRef implementation optional behind new `alloc_ref` feature
    - To enable the `AllocRef` implementation again, enable the `alloc_ref` feature of this crate in your Cargo.toml
- Fix build on nightly 1.43.0 (05-03-2020) ([#25](https://github.com/phil-opp/linked-list-allocator/pull/25))

# 0.7.0

- Use new spinning_top crate instead of `spin` ([#23](https://github.com/phil-opp/linked-list-allocator/pull/23))

# 0.6.6

- The `Alloc` trait was renamed to `AllocRef` ([#20](https://github.com/phil-opp/linked-list-allocator/pull/20))

# 0.6.5

- Align up the Hole initialization address ([#18](https://github.com/phil-opp/linked-list-allocator/pull/18))
- Remove `alloc` feature gate, which is now stable
