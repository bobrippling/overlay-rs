# Todo

- Support for nested structs
- Support for individual fields larger than `u32` ?
- `compile_error!()` / remove `unwrap` / `expect`
    - And use [trybuild](https://crates.io/crates/trybuild)
    - Emit a fake struct to avoid further errors about it
- Permit missing start/end in byte/bit ranges
- Upgrade to `syn` 2.0
- Don't drop all other attrs on enum members
- Turn off all syn (crate dep) features (except necessary)
