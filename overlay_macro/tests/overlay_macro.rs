use overlay::Overlay;
use overlay_macro::overlay;

#[overlay]
#[derive(Clone, Debug, Default)]
pub struct InquiryCommand {
    #[overlay(byte=0, bits=0..8)]
    pub op_code: u8,

    #[overlay(byte = 1, bit = 0)]
    pub product_data: bool,

    #[overlay(byte=2, bits=1..=7)]
    pub page_code: u8,

    #[overlay(bytes=3..=4, bits=0..14)]
    pub allocation_length: u16,
}

#[overlay]
#[derive(Clone)]
pub struct NoDebug {
    #[overlay(byte=0, bits=0..=7)]
    pub op_code: u8,
}

#[test]
fn integer_bool_getters() {
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
fn integer_bool_setters() {
    let mut bytes = [
        5_u8,
        5, // true, 1<<4 is ignored
        1 | (3 << 1),
        1,
        4, // 1 << 8 | 4, i.e. 260
    ];

    {
        let inq = InquiryCommand::overlay_mut(&mut bytes).unwrap();

        inq.set_page_code(23);
        assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 1, 4]);

        inq.set_allocation_length(1281);
        assert_eq!(inq.0, [5, 5, 1 | (23 << 1), 5, 1]);
    }

    {
        bytes[2] = 1 | (2 << 1); // something without the low-bit set
        let inq = InquiryCommand::overlay_mut(&mut bytes).unwrap();
        assert_eq!(inq.page_code(), 2);

        inq.set_page_code(158);
        assert_eq!(inq.0, [5, 5, 1 | (158 << 1), 5, 1]);
    }

    {
        // something with the high-bit set
        bytes[0] = 0b1000_1000;
        let inq = InquiryCommand::overlay_mut(&mut bytes).unwrap();
        assert_eq!(inq.op_code(), 0b1000_1000);
    }
}

#[test]
fn repr() {
    let bytes = [0; 5];
    let inq = InquiryCommand::overlay(&bytes).unwrap();

    assert_eq!(bytes.as_ptr(), inq.as_bytes().as_ptr());
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

    #[overlay]
    #[derive(Debug)]
    struct Outer {
        #[overlay(byte = 0)]
        header: u8,

        #[overlay(bytes=1..=3, nested)]
        inner: Inner,
    }

    #[overlay]
    #[derive(Debug)]
    struct Inner {
        #[overlay(bytes=0..=1, bits=0..16)]
        a: u16,

        #[overlay(byte = 2)]
        b: u8,
    }

    let o = Outer::overlay(&[1, 2, 3, 4]).unwrap();
    assert_eq!(
        &format!("{:?}", o),
        "Outer { header: 1, inner: Inner { a: 515, b: 4 } }"
    );
}

#[test]
fn byte_array_getters() {
    #[overlay]
    struct Abc {
        #[overlay(byte=0, bits=0..=7)]
        pad: u8,

        #[overlay(bytes=1..=3)]
        bytes: [u8; 3],

        #[overlay(byte=4, bits=0..=7)]
        pad2: u8,
    }
    let mut bytes = [1, 2, 3, 4, 5];
    let abc = Abc::overlay_mut(&mut bytes).unwrap();

    assert_eq!(abc.bytes(), &[2, 3, 4]);

    abc.set_bytes(&[99, 3, 255]);
    assert_eq!(abc.as_bytes(), &[1, 99, 3, 255, 5]);
}

#[test]
fn new() {
    assert_eq!(InquiryCommand::new().as_bytes(), &[0; 5]);
    assert_eq!(InquiryCommand::default().as_bytes(), &[0; 5]);
}

#[test]
fn enum_getters_setters() {
    #[derive(Debug, Eq, PartialEq)]
    #[allow(dead_code)]
    enum E {
        X,
        Y,
        Z,
    }

    impl TryFrom<u8> for E {
        type Error = ();

        fn try_from(v: u8) -> Result<Self, Self::Error> {
            Ok(match v {
                0 => Self::X,
                1 => Self::Y,
                2 => Self::Z,
                _ => return Err(()),
            })
        }
    }

    #[overlay]
    struct Abc {
        #[overlay(byte = 0)] // no bits given, so 0..=7 is implied
        e0: E,

        #[overlay(byte=1, bits=2..=4)]
        e1: E,

        #[overlay(byte=2, bits=0..=7)]
        u: u8,
    }
    let mut bytes = [E::Y as _, (3 << 2) | 3, 7];
    let abc = Abc::overlay_mut(&mut bytes).unwrap();

    assert_eq!(abc.e0(), Ok(E::Y));
    assert_eq!(abc.e1(), Err(()));

    abc.set_e0(E::Z);
    assert_eq!(abc.as_bytes(), &[E::Z as _, (3 << 2) | 3, 7]);

    abc.set_e1(E::Y);
    assert_eq!(abc.as_bytes(), &[E::Z as _, ((E::Y as u8) << 2) | 3, 7]);
}

#[test]
fn enum_repr() {
    #[derive(Debug, Eq, PartialEq)]
    #[repr(u8)]
    enum E {
        A,
        B,
    }

    #[overlay]
    struct Abc {
        #[overlay(bytes=0..=1)] // byte 0..=1, i.e. 2 bytes / u16
        e: E, // even though E is repr(u8)
    }

    impl TryFrom<u16> for E {
        type Error = ();

        fn try_from(v: u16) -> Result<Self, Self::Error> {
            Ok(match v {
                0 => Self::A,
                1 => Self::B,
                _ => return Err(()),
            })
        }
    }

    let mut bytes = [0, E::B as u8];
    let abc = Abc::overlay_mut(&mut bytes).unwrap();

    assert_eq!(abc.e(), Ok(E::B));

    abc.set_e(E::A);
    assert_eq!(abc.as_bytes(), &[0, E::A as _]);
}

#[test]
fn edge_cases() {
    #[overlay]
    struct Inner {
        #[overlay(bytes=0..=1, bits=0..16)]
        a: u16,

        #[overlay(byte = 2)]
        b: u8,
    }

    assert_eq!(Inner::BYTE_LEN, 3);

    let mut bytes = [0xff, 0xff, 0xff];
    let inner: &mut Inner = Inner::overlay_mut(&mut bytes).unwrap();
    assert_eq!(inner.a(), 0xffff);
    assert_eq!(inner.b(), 0xff);

    inner.set_a(0);
    assert_eq!(inner.as_bytes(), &[0, 0, 0xff]);

    inner.set_a(0xffff);
    inner.set_b(0);
    assert_eq!(&bytes, &[0xff, 0xff, 0]);

    let mut bytes = [0; 3];
    let inner: &mut Inner = Inner::overlay_mut(&mut bytes).unwrap();
    assert_eq!(inner.a(), 0);
    assert_eq!(inner.b(), 0);

    inner.set_a(0xffff);
    assert_eq!(inner.as_bytes(), &[0xff, 0xff, 0]);

    inner.set_a(0);
    inner.set_b(0xff);
    assert_eq!(&bytes, &[0, 0, 0xff]);
}

#[test]
fn awkward_offsets_and_overlap() {
    #[overlay]
    struct Inner {
        #[overlay(bytes=0..=1, bits=3..14)]
        a: u16,

        #[overlay(bytes=0..=1, bits=9..=15)]
        b: u16,

        #[overlay(bytes=0..4, bits=5..=27)]
        c: u32,
    }

    let mut bytes = [
        0b0101_0111, // 31..24
        0b1000_0101, // 23..16
        0b1111_1111, // 15.. 8
        0b1111_1000, //  7.. 0
    ];

    let inner: &mut Inner = Overlay::overlay_mut(&mut bytes).unwrap();

    let orig_a = (0b0101_0111_1000_0101 & 0b0011_1111_1111_1000) >> 3;
    let orig_b = (0b0101_0111_1000_0101 & 0b1111_1110_0000_0000) >> 9;
    let orig_c = (0b0101_0111_1000_0101_1111_1111_1111_1000
        & 0b0000_1111_1111_1111_1111_1111_1110_0000)
        >> 5;
    assert_eq!(inner.a(), orig_a);
    assert_eq!(inner.b(), orig_b);
    assert_eq!(inner.c(), orig_c);

    inner.set_a(0);
    assert_eq!(inner.a(), 0);
    assert_eq!(
        inner.b(),
        (0b0100_0000_0000_0101 & 0b1111_1110_0000_0000) >> 9
    );
    assert_eq!(
        inner.c(),
        (0b0100_0000_0000_0101_1111_1111_1111_1000 & 0b0000_1111_1111_1111_1111_1111_1110_0000)
            >> 5
    );

    inner.set_a(orig_a);
    inner.set_b(0);
    assert_eq!(inner.a(), orig_a & 0b0000_0000_0011_1111);
    assert_eq!(inner.b(), 0);
    assert_eq!(
        inner.c(),
        (0b0000_0001_1000_0101_1111_1111_1111_1000 & 0b0000_1111_1111_1111_1111_1111_1110_0000)
            >> 5
    );

    inner.set_b(orig_b);
    inner.set_c(0);
    assert_eq!(inner.a(), 0b0001_0000_0000_0000 >> 3);
    assert_eq!(inner.b(), 0b0101_0000_0000_0000 >> 9);
    assert_eq!(inner.c(), 0);
}

#[test]
fn u32_example() {
    #[overlay]
    #[derive(Clone, Copy, Eq, PartialEq, Debug)]
    pub struct ReadCapacity10Response {
        #[overlay(bytes= 0..= 3)]
        pub max_lba: u32,

        #[overlay(bytes= 4..= 7)]
        pub block_size: u32,
    }

    let mut readcap = ReadCapacity10Response::new();

    readcap.set_max_lba(0xf1_b3_c7_d9);
    readcap.set_block_size(0x9d_7c_3b_1f);

    let mut expected = [0; 8];

    expected[0..4].copy_from_slice(&0xf1_b3_c7_d9_u32.to_be_bytes());
    expected[4..8].copy_from_slice(&0x9d_7c_3b_1f_u32.to_be_bytes());

    assert_eq!(readcap.as_bytes(), &expected,);
}

#[test]
fn nested_struct() {
    #[overlay]
    struct Outer {
        #[overlay(byte = 0)]
        header: u8,

        #[overlay(bytes=1..=3, nested)]
        inner: Inner,
    }
    assert_eq!(Outer::BYTE_LEN, 4);

    #[overlay]
    struct Inner {
        #[overlay(bytes=0..=1, bits=0..16)]
        a: u16,

        #[overlay(byte = 2)]
        b: u8,
    }
    assert_eq!(Inner::BYTE_LEN, 3);

    let bytes = [23, 186, 3, 9];
    {
        let inner = Inner::overlay(&bytes[1..]).unwrap();
        assert_eq!(inner.a(), 186 << 8 | 3);
        assert_eq!(inner.b(), 9);
    }

    let outer = Outer::overlay(&bytes).unwrap();
    assert_eq!(outer.header(), 23);

    let inner: &_ = outer.inner();
    assert_eq!(inner as *const _ as *const u8, &bytes[1] as *const _);
    assert_eq!(inner.a(), 186 << 8 | 3);
    assert_eq!(inner.b(), 9);

    let mut bytes = bytes;
    let outer = Outer::overlay_mut(&mut bytes).unwrap();
    let inner = outer.inner_mut();
    inner.set_a(65439);
    inner.set_b(253);
    outer.set_header(187);

    assert_eq!(bytes, [187, (65439_u16 >> 8) as u8, 65439_u16 as u8, 253]);
}
