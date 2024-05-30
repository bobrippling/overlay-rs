A proc-macro for generating a struct which can be overlaid. See the documentation for usage and examples.

## Usage

```rust
#[overlay]
#[derive(Clone, Debug)]
pub struct InquiryCommand {
    #[bit_byte(7, 0, 0, 0)]
    pub op_code: u8,

    #[bit_byte(0, 0, 1, 1)]
    pub enable_vital_product_data: bool,

    #[bit_byte(7, 0, 2, 2)]
    pub page_code: u8,

    #[bit_byte(7, 0, 3, 4)]
    pub allocation_length: u16,

    ...
}
```

This will create a wrapper struct around an array of bytes, with generated getters and setters for each "field", accessing the bytes/bits at the given offsets.

## Todo

- Support for enums
- Support for individual fields larger than `u32` ?
