- Fix build on nightly 1.43.0 (05-03-2020) ([#25](https://github.com/phil-opp/linked-list-allocator/pull/25))

# 0.7.0

- Use new spinning_top crate instead of `spin` ([#23](https://github.com/phil-opp/linked-list-allocator/pull/23))

# 0.6.6

- The `Alloc` trait was renamed to `AllocRef` ([#20](https://github.com/phil-opp/linked-list-allocator/pull/20))

# 0.6.5

- Align up the Hole initialization address ([#18](https://github.com/phil-opp/linked-list-allocator/pull/18))
- Remove `alloc` feature gate, which is now stable
