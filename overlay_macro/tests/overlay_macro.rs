use overlay::Overlay;
use overlay_macro::overlay;

#[overlay]
#[derive(Clone, Debug)]
pub struct InquiryCommand {
    #[bit_byte(7, 0, 0, 0)]
    pub op_code: u8,

    #[bit_byte(0, 0, 1, 1)]
    pub product_data: bool,

    #[bit_byte(7, 1, 2, 2)]
    pub page_code: u8,

    #[bit_byte(13, 0, 3, 4)]
    pub allocation_length: u16,
}

#[overlay]
#[derive(Clone)]
pub struct NoDebug {
    #[bit_byte(7, 0, 0, 0)]
    pub op_code: u8,
}

#[test]
fn getters() {
    let mut bytes = [
        5_u8,
        5, // true, 1<<4 is ignored
        1 | (3 << 1),
        1,
        4, // 1 << 8 | 4, i.e. 260
    ];
    let inq = InquiryCommand::overlay(&mut bytes).unwrap();

    assert_eq!(inq.op_code(), 5);
    assert_eq!(inq.product_data(), true);
    assert_eq!(inq.page_code(), 3);
    assert_eq!(inq.allocation_length(), 260);

    let _attr_propagation: InquiryCommand = Clone::clone(inq);
}

#[test]
fn setters() {
    let mut bytes = [
        5_u8,
        5, // true, 1<<4 is ignored
        1 | (3 << 1),
        1,
        4, // 1 << 8 | 4, i.e. 260
    ];
    let inq = InquiryCommand::overlay_mut(&mut bytes).unwrap();

    inq.set_page_code(23);
    assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 1, 4]);

    inq.set_allocation_length(1281);
    assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 5, 1]);
}

#[test]
fn debug() {
    let mut bytes = [
        5_u8,
        5, // true, 1<<4 is ignored
        1 | (3 << 1),
        1,
        4, // 1 << 8 | 4, i.e. 260
    ];
    let inq = InquiryCommand::overlay_mut(&mut bytes).unwrap();

    let mut out = String::new();
    use std::fmt::Write;
    write!(&mut out, "{inq:?}").unwrap();

    assert_eq!(
        &out,
        "InquiryCommand { op_code: 5, product_data: true, page_code: 3, allocation_length: 260 }"
    );
}
