A proc-macro for generating a struct which can be overlaid. See the documentation for usage and examples.

## Usage

```rust
#[overlay]
#[derive(Clone, Debug)]
pub struct InquiryCommand {
    #[overlay(byte=0, bits=0..8)]
    pub op_code: u8,

    #[overlay(byte=1)]
    pub enable_vital_product_data: bool,

    #[overlay(byte=2, bits=0..=7)]
    pub page_code: u8,

    #[overlay(bytes=3..=4, bits=0..=7)]
    pub allocation_length: u16,

    ...
}
```

This will create a wrapper struct around an array of bytes, with generated getters and setters for each "field", accessing the bytes/bits at the given offsets.

## Todo

- Support for nested structs
- Support for individual fields larger than `u32` ?
- `compile_error!()`
