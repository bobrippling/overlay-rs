use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, Meta, NestedMeta, Type};

#[proc_macro_attribute]
pub fn overlay(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = input.ident;

    assert!(attr.is_empty(), "No attributes expected");

    let fields = if let Data::Struct(data_struct) = input.data {
        if let Fields::Named(fields) = data_struct.fields {
            fields.named
        } else {
            unimplemented!()
        }
    } else {
        unimplemented!()
    };

    let mut getters = vec![];
    let mut setters = vec![];
    let mut last_byte = 0;
    for field in fields {
        let field_name = field.ident.expect("named field");

        let mut found = false;
        for attr in field.attrs {
            const ATTR_NAME: &str = "bit_byte";
            if attr.path.is_ident(ATTR_NAME) {
                found = true;

                let Ok(Meta::List(meta_list)) = attr.parse_meta() else {
                    panic!("start/end bit and start/end byte required as arguments to {ATTR_NAME}");
                };

                let mut args = meta_list.nested.iter();
                let end_bit = unwrap_int_lit(&mut args, "start_bit");
                let start_bit = unwrap_int_lit(&mut args, "start_bit");
                let start_byte = unwrap_int_lit(&mut args, "start_byte");
                let end_byte = unwrap_int_lit(&mut args, "end_byte");
                assert!(args.next().is_none(), "too many arguments to {ATTR_NAME}");

                assert!(
                    start_bit <= end_bit,
                    "start bit ({start_bit}) must not be greater than end bit ({end_bit})"
                );
                assert!(
                    start_byte <= end_byte,
                    "start byte ({start_byte}) must not be greater than end byte ({end_byte})"
                );

                last_byte = last_byte.max(end_byte);

                let ty = &field.ty;
                let is_bool = match &ty {
                    Type::Path(type_path) => {
                        quote::ToTokens::into_token_stream(type_path.clone()).to_string() == "bool"
                    }
                    _ => false,
                };

                let getter = if is_bool {
                    quote! {
                        pub fn #field_name(&self) -> bool {
                            let byte = self.0[#start_byte];
                            (byte >> #start_bit) & 1 != 0
                        }
                    }
                } else {
                    quote! {
                        pub fn #field_name(&self) -> #ty {
                            let mut value = 0_u32;
                            for i in #start_byte..=#end_byte {
                                value <<= 8;
                                value |= self.0[i] as u32;
                            }

                            // mask off 0..start_bit
                            value &= !0_u32 << #start_bit;
                            // mask off end_bit..
                            if #end_bit > 0 {
                                value &= !0_u32 >> (32 - #end_bit);
                            }

                            (value >> #start_bit) as _
                        }
                    }
                };
                getters.push(getter);

                let setter_name = format_ident!("set_{}", field_name);
                let setter = if is_bool {
                    quote! {
                        pub fn #setter_name(&mut self, val: bool) {
                            let bit_value = if val { 1 } else { 0 };
                            self.0[#start_byte] &= !(1 << #start_bit);
                            self.0[#start_byte] |= (bit_value << #start_bit) as u8;
                        }
                    }
                } else {
                    quote! {
                        pub fn #setter_name(&mut self, val: #ty) {
                            let mut mask = (!0_u32 << #start_bit);
                            if #end_bit > 0 {
                                mask &= (!0_u32 >> (32 - #end_bit - 1));
                            }
                            let mask = mask;

                            let orig = self.#field_name();

                            let mut new = ((val as u32) << #start_bit) & mask;
                            new |= orig as u32 & !mask;

                            for i in (#start_byte..=#end_byte).rev() {
                                self.0[i] = new as u8;
                                new >>= 8;
                            }
                        }
                    }
                };
                setters.push(setter);
            }
        }

        assert!(
            found,
            "No #[bit_byte(...)] attribute found for {}",
            field_name
        );
    }

    let byte_count = last_byte + 1;
    let expanded = quote! {
        struct #name([u8; #byte_count]);

        impl #name {
            #(#getters)*
            #(#setters)*

            fn overlay(bytes: &[u8; #byte_count]) -> &Self {
                let p: *const Self = bytes as *const _ as *const Self;
                // SAFETY: newtype wrapper
                unsafe { &*p }
            }

            fn overlay_mut(bytes: &mut [u8; #byte_count]) -> &mut Self {
                let p: *mut Self = bytes as *mut _ as *mut Self;
                // SAFETY: newtype wrapper
                unsafe { &mut *p }
            }

            fn as_bytes(&self) -> &[u8; #byte_count] {
                &self.0
            }

            fn as_bytes_mut(&mut self) -> &mut [u8; #byte_count] {
                &mut self.0
            }
        }
    };

    TokenStream::from(expanded)
}

fn unwrap_int_lit(args: &mut dyn Iterator<Item = &NestedMeta>, name: &str) -> usize {
    match args.next() {
        Some(NestedMeta::Lit(Lit::Int(lit))) => lit.base10_parse().unwrap(),
        _ => panic!("Expected integer literal for {name}"),
    }
}
