use overlay_macro::overlay;

#[overlay]
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

#[test]
fn getters() {
    let mut bytes = [
        5_u8,
        5, // true, 1<<4 is ignored
        1 | (3 << 1),
        1,
        4, // 1 << 8 | 4, i.e. 260
    ];
    let inq = InquiryCommand::overlay(&mut bytes);

    assert_eq!(inq.op_code(), 5);
    assert_eq!(inq.product_data(), true);
    assert_eq!(inq.page_code(), 3);
    assert_eq!(inq.allocation_length(), 260);
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
    let inq = InquiryCommand::overlay_mut(&mut bytes);

    inq.set_page_code(23);
    assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 1, 4]);

    inq.set_allocation_length(1281);
    assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 5, 1]);
}