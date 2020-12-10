# Unreleased

# 0.8.7 – 2020-12-10

- _Unstable Breakage:_ fix(alloc_ref): Use new nightly Allocator trait [#42](https://github.com/phil-opp/linked-list-allocator/pull/42)
- Build on stable without features [#43](https://github.com/phil-opp/linked-list-allocator/pull/43)
  - Adds a new `const_mut_refs` crate feature (enabled by default).
  - By disabling this feature, it's possible to build the crate on stable Rust.

# 0.8.6 – 2020-09-24

- Fix build error on latest nightly ([#35](https://github.com/phil-opp/linked-list-allocator/pull/35))

# 0.8.5 – 2020-08-13

- Update AllocRef implementation for latest API changes ([#33](https://github.com/phil-opp/linked-list-allocator/pull/33))

# 0.8.4

- Add function to get used and free heap size ([#32](https://github.com/phil-opp/linked-list-allocator/pull/32))

# 0.8.3

- Prevent writing to heap memory range when size too small ([#31](https://github.com/phil-opp/linked-list-allocator/pull/31))

# 0.8.2

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
