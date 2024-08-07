use std::ops::{Range, RangeInclusive};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Data, DeriveInput, Fields, Ident, LitInt, Meta, NestedMeta, Token, Type,
};

enum FieldTy {
    Integer,
    Bool,
    Enum,
    Struct,
    ByteArray,
}

#[derive(Debug)]
struct OverlayAttribute {
    byte: SingleOrRange,
    bits: Option<SingleOrRange>,
    nested: bool,
}

#[derive(Debug)]
enum SingleOrRange {
    Single(u32),
    Range(Range<u32>),
    RangeIncl(RangeInclusive<u32>),
}

#[doc = include_str!("../README.md")]
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
            const ATTR_NAME: &str = "overlay";
            if attr.path.is_ident(ATTR_NAME) {
                found = true;

                let ranges: OverlayAttribute = attr.parse_args().unwrap();

                let nested = ranges.nested;
                ranges.byte.assert_range_valid("byte");
                if let Some(bits) = &ranges.bits {
                    bits.assert_range_valid("bit");
                    assert!(!nested, "cannot have a nested struct at a bit-offset");
                }

                last_byte = last_byte.max(ranges.byte.end_inclusive());

                let ty = &field.ty;
                let vis = &field.vis;

                let field_ty = if nested {
                    Some(FieldTy::Struct)
                } else {
                    match_type(ty)
                }.expect("invalid field type: expected integer, bool, C-style enum, nested struct or [u8; N]");

                // e.g. `_x: u8` -> `set__x()`
                //                       ^ rustc warns about this
                let setter_attr = quote! { #[allow(non_snake_case)] };

                let start_byte = ranges.byte.start() as usize;
                let end_byte = ranges.byte.end_inclusive() as usize;

                let setter_name = format_ident!("set_{}", field_name);

                let (getter, setter) = match field_ty {
                    FieldTy::Bool => {
                        let start_bit = match &ranges.bits {
                            None => 0,
                            Some(bits) => {
                                if bits.end_inclusive() != bits.start() {
                                    panic!("Bit range for a bool must be a single number (e.g. 1, or 1..=1)")
                                }
                                bits.start()
                            }
                        };

                        (
                            quote! {
                                #vis fn #field_name(&self) -> bool {
                                    let byte = self.0[#start_byte];
                                    (byte >> #start_bit) & 1 != 0
                                }
                            },
                            quote! {
                                #setter_attr
                                #vis fn #setter_name(&mut self, val: bool) {
                                    let bit_value = if val { 1 } else { 0 };
                                    self.0[#start_byte] &= !(1 << #start_bit);
                                    self.0[#start_byte] |= (bit_value << #start_bit) as u8;
                                }
                            },
                        )
                    }
                    FieldTy::Integer | FieldTy::Enum => {
                        let lim = (0, ranges.byte.len() * 8 - 1);
                        let (start_bit, end_bit) = match &ranges.bits {
                            None => lim,
                            Some(bits) => (bits.start(), bits.end_inclusive()),
                        };

                        if start_bit > lim.1 || end_bit > lim.1 {
                            panic!(
                                "start and end bits ({start_bit} & {end_bit}) must be inside the byte-range ({}..={})",
                                lim.0,
                                lim.1
                            );
                        }

                        let getter_body = quote! {
                            let mut value = 0_u32;
                            for i in #start_byte..=#end_byte {
                                value <<= 8;
                                value |= self.0[i] as u32;
                            }

                            // mask off end_bit..
                            if #end_bit > 0 {
                                value &= !0_u32 >> (31 - #end_bit);
                            }

                            value >>= #start_bit;
                        };

                        (
                            if matches!(field_ty, FieldTy::Enum) {
                                let enum_repr = match end_byte - start_byte + 1 {
                                    1 => quote! { u8 },
                                    2 => quote! { u16 },
                                    4 => quote! { u32 },
                                    8 => quote! { u64 },
                                    size => {
                                        panic!("can't determine size of field for {size}-byte enum")
                                    }
                                };

                                quote! {
                                    #vis fn #field_name(
                                        &self
                                    ) -> Result<#ty, <#ty as core::convert::TryFrom<#enum_repr>>::Error> {
                                        #getter_body

                                        let value = value as #enum_repr;
                                        #ty::try_from(value)
                                    }
                                }
                            } else {
                                quote! {
                                    #vis fn #field_name(&self) -> #ty {
                                        #getter_body

                                        value as _
                                    }
                                }
                            },
                            quote! {
                                #setter_attr
                                #vis fn #setter_name(&mut self, val: #ty) {
                                    let mut mask = !0_u32 << #start_bit;
                                    if #end_bit > 0 {
                                        mask &= !0_u32 >> (31 - #end_bit);
                                    }

                                    let mut new = ((val as u32) << #start_bit) & mask;

                                    for i in (#start_byte..=#end_byte).rev() {
                                        self.0[i] = self.0[i] & (!mask as u8) | (new as u8);
                                        new >>= 8;
                                        mask >>= 8;
                                    }
                                }
                            },
                        )
                    }
                    FieldTy::Struct => {
                        let setter_name = format_ident!("{}_mut", field_name);

                        (
                            quote! {
                                #vis fn #field_name(&self) -> &#ty {
                                    let p = &self.0[#start_byte..=#end_byte];

                                    // could make this unsafe
                                    overlay::Overlay::overlay(p).unwrap()
                                }

                                #setter_attr
                                #vis fn #setter_name(&mut self) -> &mut #ty {
                                    let p = &mut self.0[#start_byte..=#end_byte];

                                    overlay::Overlay::overlay_mut(p).unwrap()
                                }
                            },
                            // setter isn't provided, we just expose a &mut to the nested struct (above, with the getter)
                            quote! {},
                        )
                    }
                    FieldTy::ByteArray => {
                        assert!(ranges.bits.is_none(), "byte arrays cannot have a bit-range");

                        (
                            quote! {
                                #vis fn #field_name(&self) -> &#ty {
                                    return self
                                        .0[#start_byte..=#end_byte]
                                        .try_into()
                                        .unwrap(); // could make this unsafe and drop the try
                                }
                            },
                            quote! {
                                #setter_attr
                                #vis fn #setter_name(&mut self, bytes: &#ty) {
                                    self.0[#start_byte..=#end_byte]
                                        .copy_from_slice(bytes);
                                }
                            },
                        )
                    }
                };
                getters.push(getter);
                setters.push(setter);
            }
        }

        assert!(
            found,
            "No #[overlay(...)] attribute found for {}",
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

    let byte_count = last_byte as usize + 1;
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
                // and all-zero bit-pattern is valid for these.
                // For enums, if the bit-pattern isn't valid, it's caught when we
                // attempt to read that field, via `try_from`.
                unsafe {
                    use core::mem;
                    mem::zeroed()
                }
            }

            pub const BYTE_LEN: usize = #byte_count;
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

impl Parse for OverlayAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let (mut byte, mut bits, mut nested) = (None, None, false);

        loop {
            if input.is_empty() {
                break;
            }

            let keyword = input.parse::<Ident>()?.to_string();

            if keyword == "nested" {
                nested = true;
            } else {
                input.parse::<Token![=]>()?;

                let (is_byte, is_singular) = match keyword.as_str() {
                    "byte" => (true, true),
                    "bytes" => (true, false),
                    "bit" => (false, true),
                    "bits" => (false, false),
                    _ => panic!("invalid specifier {keyword}"),
                };

                let span: SingleOrRange = input.parse()?;

                match (is_singular, &span) {
                    (true, SingleOrRange::Single(_))
                    | (false, SingleOrRange::RangeIncl(_) | SingleOrRange::Range(_)) => {}
                    _ => {
                        // TODO: compile_error!()
                        panic!("invalid combination of {keyword} and single/range span");
                    }
                }

                let old = if is_byte {
                    byte.replace(span)
                } else {
                    bits.replace(span)
                };
                if old.is_some() {
                    panic!("duplicate specifier for {keyword}");
                }
            }

            if input.parse::<Token![,]>().is_err() {
                break;
            }
        }

        if !input.is_empty() {
            panic!("unused tokens");
        }

        Ok(Self {
            byte: byte.expect("no byte specifier"),
            bits,
            nested,
        })
    }
}

impl Parse for SingleOrRange {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let start = input.parse::<LitInt>()?.base10_parse()?;

        let range_incl = input.parse::<Token![..=]>().is_ok();
        let mut range = false;
        if !range_incl {
            range = input.parse::<Token![..]>().is_ok();
        }

        let mut end = None::<LitInt>;
        if range_incl || range {
            end = input.parse()?;
        }

        match end {
            None => Ok(Self::Single(start)),
            Some(end) => {
                let end = end.base10_parse()?;

                Ok(if range {
                    Self::Range(start..end)
                } else {
                    Self::RangeIncl(start..=end)
                })
            }
        }
    }
}

impl SingleOrRange {
    fn assert_range_valid(&self, what: &str) {
        match self {
            SingleOrRange::Single(_) => {}
            SingleOrRange::Range(r) => assert!(
                r.start < r.end,
                "start {what} ({}) must be less than end {what} ({})",
                r.start,
                r.end,
            ),
            SingleOrRange::RangeIncl(r) => assert!(
                r.start() <= r.end(),
                "start {what} ({}) must not be greater than end {what} ({})",
                r.start(),
                r.end(),
            ),
        }
    }

    fn end_inclusive(&self) -> u32 {
        match self {
            &SingleOrRange::Single(x) => x,
            SingleOrRange::Range(r) => r.end - 1,
            SingleOrRange::RangeIncl(r) => *r.end(),
        }
    }

    fn start(&self) -> u32 {
        match self {
            &SingleOrRange::Single(x) => x,
            SingleOrRange::Range(r) => r.start,
            SingleOrRange::RangeIncl(r) => *r.start(),
        }
    }

    fn len(&self) -> u32 {
        let start = self.start();
        let end = self.end_inclusive();

        end - start + 1
    }
}
