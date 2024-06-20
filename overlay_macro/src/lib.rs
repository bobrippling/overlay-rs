use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, Meta, NestedMeta, Type};

enum FieldTy {
    Integer,
    Bool,
    Enum,
    ByteArray,
}

/**
 * Attribute macro for overlaying a byte/bit-level description of a struct on arbitrary byte data.
 *
 * # Field Attributes
 *
 * | Name          | Description |
 * |---------------|-------------|
 * | start_byte    | The byte in the struct where this field starts (zero based) |
 * | end_byte      | The byte in the struct where this field ends (zero based, inclusive) |
 * | start_bit     | The bit within the collection of bytes demarked above, where this field starts (zero based, inclusive) |
 * | end_bit       | The bit within the collection of bytes demarked above, where this field ends (zero based, inclusive) |
 *
 * # Example
 *
 * ```rust
 * use overlay_macro::overlay;
 *
 * #[overlay]
 * #[derive(Clone, Debug)]
 * pub struct InquiryCommand {
 *     #[bit_byte(7, 0, 0, 0)]
 *     pub op_code: u8,
 *
 *     #[bit_byte(0, 0, 1, 1)]
 *     pub enable_vital_product_data: bool,
 *
 *     #[bit_byte(7, 0, 2, 2)]
 *     pub page_code: u8,
 *
 *     #[bit_byte(7, 0, 3, 4)]
 *     pub allocation_length: u16,
 * }
 * ```
 *
 * Note that attributes must be specified after `#\[overlay\]` to ensure they apply to the
 * generated byte-array, not the fields beforehand.
 *
 * The `Debug` attribute is plucked from the `derive` attribute (if present) and implemented by
 * calling each property in turn, as-if the struct was a POD.
 */

#[proc_macro_attribute]
pub fn overlay(macro_attrs: TokenStream, item: TokenStream) -> TokenStream {
    assert!(macro_attrs.is_empty(), "No attributes expected");

    let mut input = parse_macro_input!(item as DeriveInput);
    let name = input.ident;

    if !input.generics.params.is_empty() {
        return TokenStream::from(quote! {
            compile_error!("Generics are not supported");
        });
    }

    let fields = if let Data::Struct(data_struct) = input.data {
        if let Fields::Named(fields) = data_struct.fields {
            fields.named
        } else {
            unimplemented!("only named-field structs are supported")
        }
    } else {
        unimplemented!("only structs can be overlaid")
    };

    let mut getters = vec![];
    let mut setters = vec![];
    let mut field_names = vec![];
    let mut last_byte = 0;
    for field in fields {
        let field_name = field.ident.expect("named field");
        field_names.push(field_name.clone());

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
                let vis = &field.vis;

                let field_ty = match_type(ty)
                    .expect("invalid field type: expected integer, bool, C-style enum or [u8; N]");

                let getter = match field_ty {
                    FieldTy::Bool => {
                        quote! {
                            #vis fn #field_name(&self) -> bool {
                                let byte = self.0[#start_byte];
                                (byte >> #start_bit) & 1 != 0
                            }
                        }
                    }
                    FieldTy::Integer => {
                        quote! {
                            #vis fn #field_name(&self) -> #ty {
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
                    }
                    FieldTy::Enum => todo!("enum fields"),
                    FieldTy::ByteArray => {
                        assert!(
                            start_bit == 0 && end_bit == 0,
                            "byte arrays must have start & end bit set to zero"
                        );

                        quote! {
                            #vis fn #field_name(&self) -> &#ty {
                                return self
                                    .0[#start_byte..=#end_byte]
                                    .try_into()
                                    .unwrap(); // could make this unsafe and drop the try
                            }
                        }
                    }
                };
                getters.push(getter);

                // e.g. `_x: u8` -> `set__x()`
                //                       ^ rustc warns about this
                let setter_attr = quote! { #[allow(non_snake_case)] };

                let setter_name = format_ident!("set_{}", field_name);
                let setter = match field_ty {
                    FieldTy::Bool => {
                        quote! {
                            #setter_attr
                            #vis fn #setter_name(&mut self, val: bool) {
                                let bit_value = if val { 1 } else { 0 };
                                self.0[#start_byte] &= !(1 << #start_bit);
                                self.0[#start_byte] |= (bit_value << #start_bit) as u8;
                            }
                        }
                    }
                    FieldTy::Integer => {
                        quote! {
                            #setter_attr
                            #vis fn #setter_name(&mut self, val: #ty) {
                                let mut mask = (!0_u32 << #start_bit);
                                if #end_bit > 0 {
                                    mask &= (!0_u32 >> (32 - #end_bit - 1));
                                }

                                let mut new = ((val as u32) << #start_bit) & mask;

                                for i in (#start_byte..=#end_byte).rev() {
                                    self.0[i] = self.0[i] & (!mask as u8) | (new as u8);
                                    new >>= 8;
                                    mask >>= 8;
                                }
                            }
                        }
                    }
                    FieldTy::Enum => todo!("enum fields"),
                    FieldTy::ByteArray => {
                        quote! {
                            #setter_attr
                            #vis fn #setter_name(&mut self, bytes: &#ty) {
                                self.0[#start_byte..=#end_byte]
                                    .copy_from_slice(bytes);
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

    let mut implement_debug = false;
    for attr in &mut input.attrs {
        if let Ok(Meta::List(meta_list)) = attr.parse_meta() {
            if meta_list.path.is_ident("derive") {
                let filtered_traits = meta_list.nested.iter().filter(|nested| {
                    if let NestedMeta::Meta(Meta::Path(path)) = nested {
                        if path.is_ident("Debug") {
                            implement_debug = true;
                            return false;
                        }
                    }
                    true
                });

                let new_meta_list = quote! {
                    #[derive(#(#filtered_traits),*)]
                };

                let new_attr = syn::parse_quote!(#new_meta_list);
                *attr = new_attr;
            }
        }
    }

    let debug_impl = if implement_debug {
        quote! {
            impl core::fmt::Debug for #name {
                fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    fmt.debug_struct(stringify!(#name))
                        #(.field(stringify!(#field_names), &self.#field_names()))*
                        .finish()
                }
            }
        }
    } else {
        quote! {}
    };

    let byte_count = last_byte + 1;
    let vis = input.vis;
    let attrs = input.attrs;

    let expanded = quote! {
        #(#attrs)*
        #[repr(transparent)]
        #vis struct #name([u8; #byte_count]);

        impl #name {
            #(#getters)*
            #(#setters)*

            pub fn as_bytes(&self) -> &[u8; #byte_count] {
                &self.0
            }

            pub fn as_bytes_mut(&mut self) -> &mut [u8; #byte_count] {
                &mut self.0
            }

            pub const fn new() -> Self {
                // SAFETY: all fields are POD (specifically int/bool/array thereof),
                // and all-zero byte-pattern is valid for these
                unsafe {
                    use core::mem;
                    mem::zeroed()
                }
            }
        }

        impl overlay::Overlay for #name {
            fn overlay(bytes: &[u8]) -> core::result::Result<&Self, overlay::Error> {
                if bytes.len() < #byte_count {
                    return Err(overlay::Error::InsufficientLength);
                }

                let p: *const Self = bytes as *const _ as *const Self;
                // SAFETY: newtype wrapper, length checked
                Ok(unsafe { &*p })
            }

            fn overlay_mut(bytes: &mut [u8]) -> core::result::Result<&mut Self, overlay::Error> {
                if bytes.len() < #byte_count {
                    return Err(overlay::Error::InsufficientLength);
                }

                let p: *mut Self = bytes as *mut _ as *mut Self;
                // SAFETY: newtype wrapper, length checked
                Ok(unsafe { &mut *p })
            }

        }

        #debug_impl
    };

    TokenStream::from(expanded)
}

fn unwrap_int_lit(args: &mut dyn Iterator<Item = &NestedMeta>, name: &str) -> usize {
    match args.next() {
        Some(NestedMeta::Lit(Lit::Int(lit))) => lit.base10_parse().unwrap(),
        _ => panic!("Expected integer literal for {name}"),
    }
}

fn match_type(ty: &Type) -> Option<FieldTy> {
    match ty {
        Type::Path(path) => {
            let segment = path.path.segments.last().unwrap();
            return Some(match segment.ident.to_string().as_str() {
                "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "u128" | "i128"
                | "usize" | "isize" => FieldTy::Integer,
                "bool" => FieldTy::Bool,
                _ => FieldTy::Enum,
            });
        }
        Type::Array(array) => {
            if let Type::Path(ref path) = *array.elem {
                if path.path.segments.last().unwrap().ident == "u8" {
                    return Some(FieldTy::ByteArray);
                }
            }
        }
        _ => {}
    }

    None
}
