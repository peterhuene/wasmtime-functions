use heck::{CamelCase, ShoutySnakeCase, SnakeCase};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, ToTokens};
use std::rc::Rc;
use syn::{
    parse::{Parse, ParseStream},
    Error, Ident, LitInt, LitStr, Result, Token,
};

pub struct Input {
    paths: Vec<String>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            paths: input
                .parse_terminated::<_, Token![,]>(<LitStr as Parse>::parse)?
                .into_iter()
                .map(|i| i.value())
                .collect(),
        })
    }
}

struct IntType<'a>(&'a witx::IntRepr);

impl<'a> ToTokens for IntType<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self.0 {
            witx::IntRepr::U8 => quote!(u8),
            witx::IntRepr::U16 => quote!(u16),
            witx::IntRepr::U32 => quote!(u32),
            witx::IntRepr::U64 => quote!(u64),
        })
    }
}

struct Variant<'a> {
    name: &'a witx::Id,
    type_name: &'a Ident,
    value: String,
    docs: &'a str,
}

impl<'a> ToTokens for Variant<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let docs = self.docs;
        let type_name = self.type_name;
        let const_name = Ident::new(
            &format!("{}_{}", type_name, self.name.as_str()).to_shouty_snake_case(),
            Span::call_site(),
        );
        let value = LitInt::new(&self.value, Span::call_site());

        tokens.extend(quote!(
            #[doc = #docs]
            pub const #const_name: #type_name = #value;
        ));
    }
}

struct Enum<'a>(&'a str, &'a witx::EnumDatatype);

impl<'a> ToTokens for Enum<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let repr = IntType(&self.1.repr);
        let variants = self
            .1
            .variants
            .iter()
            .enumerate()
            .map(|(i, variant)| Variant {
                name: &variant.name,
                type_name: &name,
                value: i.to_string(),
                docs: &variant.docs,
            });

        tokens.extend(quote!(
            pub type #name = #repr;

            #(#variants)*
        ));
    }
}

struct Flags<'a>(&'a str, &'a witx::FlagsDatatype);

impl<'a> ToTokens for Flags<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let repr = IntType(&self.1.repr);
        let variants = self.1.flags.iter().enumerate().map(|(i, flag)| Variant {
            name: &flag.name,
            type_name: &name,
            value: format!("0x{:x}", 1 << i),
            docs: &flag.docs,
        });

        tokens.extend(quote!(
            pub type #name = #repr;

            #(#variants)*
        ));
    }
}

struct Int<'a>(&'a str, &'a witx::IntDatatype);

impl<'a> ToTokens for Int<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let repr = IntType(&self.1.repr);
        let variants = self.1.consts.iter().map(|c| Variant {
            name: &c.name,
            type_name: &name,
            value: c.value.to_string(),
            docs: &c.docs,
        });

        tokens.extend(quote!(
            pub type #name = #repr;

            #(#variants)*
        ));
    }
}

struct MemberVariant<'a> {
    name: &'a witx::Id,
    ty: &'a witx::TypeRef,
    docs: &'a str,
}

impl<'a> ToTokens for MemberVariant<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let docs = self.docs;
        let name = Ident::new(self.name.as_str(), Span::call_site());
        let ty = TypeRef(self.ty);

        tokens.extend(quote!(
            #[doc = #docs]
            pub #name: #ty;
        ));
    }
}

struct Struct<'a>(&'a str, &'a witx::StructDatatype);

impl<'a> ToTokens for Struct<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let members = self.1.members.iter().map(|m| MemberVariant {
            name: &m.name,
            ty: &m.tref,
            docs: &m.docs,
        });

        tokens.extend(quote!(
            #[repr(C)]
            #[derive(Copy, Clone)]
            pub struct #name {
                #(#members),*
            }
        ));
    }
}

struct Union<'a>(&'a str, &'a witx::UnionDatatype);

impl<'a> ToTokens for Union<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let members = self.1.variants.iter().map(|v| MemberVariant {
            name: &v.name,
            ty: v.tref.as_ref().unwrap(),
            docs: &v.docs,
        });

        tokens.extend(quote!(
            #[repr(C)]
            #[derive(Copy, Clone)]
            pub union #name {
                #(#members),*
            }
        ));
    }
}

struct Handle<'a>(&'a str, &'a witx::HandleDatatype);

impl<'a> ToTokens for Handle<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());

        tokens.extend(quote!(
            pub type #name = u32;
        ));
    }
}

struct TypeRef<'a>(&'a witx::TypeRef);

impl<'a> ToTokens for TypeRef<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self.0 {
            witx::TypeRef::Name(t) => {
                let name = Ident::new(&t.name.as_str().to_camel_case(), Span::call_site());
                tokens.extend(quote!(types::#name));
            }
            witx::TypeRef::Value(v) => match &**v {
                witx::Type::Builtin(t) => BuiltInType(t).to_tokens(tokens),
                witx::Type::Array(t) => {
                    let ty = TypeRef(t);
                    tokens.extend(quote!(&[#ty]));
                }
                witx::Type::Pointer(t) => {
                    let ty = TypeRef(t);
                    tokens.extend(quote!(*mut #ty));
                }
                witx::Type::ConstPointer(t) => {
                    let ty = TypeRef(t);
                    tokens.extend(quote!(*const #ty));
                }
                t => panic!("reference to anonymous {} not possible!", t.kind()),
            },
        }
    }
}

struct BuiltInType<'a>(&'a witx::BuiltinType);

impl<'a> ToTokens for BuiltInType<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self.0 {
            witx::BuiltinType::String => quote!(&str),
            // Char8 represents a UTF8 code unit so treat as u8 rather than char
            witx::BuiltinType::U8 | witx::BuiltinType::Char8 => quote!(u8),
            witx::BuiltinType::U16 => quote!(u16),
            witx::BuiltinType::U32 => quote!(u32),
            witx::BuiltinType::U64 => quote!(u64),
            witx::BuiltinType::S8 => quote!(i8),
            witx::BuiltinType::S16 => quote!(i16),
            witx::BuiltinType::S32 => quote!(i32),
            witx::BuiltinType::S64 => quote!(i64),
            witx::BuiltinType::F32 => quote!(f32),
            witx::BuiltinType::F64 => quote!(f64),
            witx::BuiltinType::USize => quote!(usize),
        });
    }
}

struct Alias<'a>(&'a str, &'a witx::TypeRef);

impl<'a> ToTokens for Alias<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(&self.0.to_camel_case(), Span::call_site());
        let ty = TypeRef(self.1);

        tokens.extend(quote!(pub type #name));

        if self.1.type_().passed_by() == witx::TypePassedBy::PointerLengthPair {
            tokens.extend(quote!(<'a>));
        }

        tokens.extend(quote!(= #ty;));
    }
}

struct Type(Rc<witx::NamedType>);

impl ToTokens for Type {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.name.as_str();
        match &self.0.tref {
            witx::TypeRef::Value(ty) => match &**ty {
                witx::Type::Enum(e) => Enum(name, e).to_tokens(tokens),
                witx::Type::Flags(f) => Flags(name, f).to_tokens(tokens),
                witx::Type::Int(c) => Int(name, c).to_tokens(tokens),
                witx::Type::Struct(s) => Struct(name, s).to_tokens(tokens),
                witx::Type::Union(u) => Union(name, u).to_tokens(tokens),
                witx::Type::Handle(h) => Handle(name, h).to_tokens(tokens),
                witx::Type::Array { .. }
                | witx::Type::Pointer { .. }
                | witx::Type::ConstPointer { .. }
                | witx::Type::Builtin { .. } => Alias(name, &self.0.tref).to_tokens(tokens),
            },
            witx::TypeRef::Name(_) => Alias(name, &self.0.tref).to_tokens(tokens),
        }
    }
}

struct Types<'a>(&'a witx::Document);

impl<'a> ToTokens for Types<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let types = self.0.typenames().map(Type);

        tokens.extend(quote!(
            pub mod types {
                pub type Result<T> = core::result::Result<T, Error>;

                #(#types)*
            }
        ));
    }
}

struct FunctionParam<'a>(&'a witx::InterfaceFuncParam);

impl<'a> ToTokens for FunctionParam<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(self.0.name.as_str(), Span::call_site());
        let ty = TypeRef(&self.0.tref);

        tokens.extend(quote!(
            #name: #ty
        ));
    }
}

struct FunctionArg<'a>(&'a witx::InterfaceFuncParam);

impl<'a> ToTokens for FunctionArg<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = Ident::new(self.0.name.as_ref(), Span::call_site());
        tokens.extend(match self.0.tref.type_().passed_by() {
            witx::TypePassedBy::Value(_) | witx::TypePassedBy::Pointer => quote!(#ident),
            witx::TypePassedBy::PointerLengthPair => quote!(#ident.as_ptr(), #ident.len()),
        });
    }
}

struct FunctionOutArg<'a>(&'a witx::InterfaceFuncParam);

impl<'a> ToTokens for FunctionOutArg<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = Ident::new(self.0.name.as_ref(), Span::call_site());
        tokens.extend(quote!(#ident.as_mut_ptr()));
    }
}

struct Function(Rc<witx::InterfaceFunc>);

impl ToTokens for Function {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = Ident::new(self.0.name.as_str(), Span::call_site());
        let docs = &self.0.docs;
        let params = self.0.params.iter().map(FunctionParam);
        let args = self.0.params.iter().map(FunctionArg);
        let comma = if self.0.params.is_empty() {
            quote!()
        } else {
            quote!(,)
        };
        let out_args = self.0.results.iter().skip(1).map(FunctionOutArg);
        let results: Vec<_> = self
            .0
            .results
            .iter()
            .skip(1)
            .map(|r| Ident::new(r.name.as_ref(), Span::call_site()))
            .collect();
        let mut returns_result = false;
        let mut returns_tuple = false;
        let ret_type = match self.0.results.get(0) {
            Some(first) => {
                returns_result = first.name.as_str() == "error";
                returns_tuple = self.0.results.len() != if returns_result { 2 } else { 1 };
                let types = self
                    .0
                    .results
                    .iter()
                    .skip(returns_result as usize)
                    .map(|r| TypeRef(&r.tref));

                if returns_result {
                    if returns_tuple {
                        quote!(-> Result<(#(#types),*)>)
                    } else {
                        quote!(-> Result<#(#types)*>)
                    }
                } else if returns_tuple {
                    quote!(-> (#(#types),*))
                } else {
                    quote!(-> #(#types)*)
                }
            }
            None => quote!(),
        };

        let ret = if returns_result {
            if returns_tuple {
                quote!(
                    if __ret == types::ERROR_OK {
                        Ok((#(#results.assume_init()),*))
                    } else {
                        Err(__ret)
                    }
                )
            } else {
                quote!(
                    if __ret == types::ERROR_OK {
                        Ok(#(#results.assume_init())*)
                    } else {
                        Err(__ret)
                    }
                )
            }
        } else if returns_tuple {
            quote!((#(#results.assume_init()),*))
        } else {
            quote!(__ret)
        };

        // TODO: doc params and results
        tokens.extend(quote!(
            #[doc = #docs]
            pub unsafe fn #name(#(#params),*) #ret_type {
                #(let mut #results = MaybeUninit::uninit();)*

                let __ret = raw::#name(#(#args),*#comma#(#out_args),*);

                #ret
            }
        ));
    }
}

struct RawFunctionParam<'a>(&'a witx::InterfaceFuncParam);

impl<'a> ToTokens for RawFunctionParam<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let is_param = match self.0.position {
            witx::InterfaceFuncParamPosition::Param(_) => true,
            _ => false,
        };

        tokens.extend(match self.0.tref.type_().passed_by() {
            witx::TypePassedBy::Value(_) => {
                let ty = TypeRef(&self.0.tref);
                if is_param {
                    let ident = Ident::new(self.0.name.as_ref(), Span::call_site());
                    quote!(#ident: #ty)
                } else {
                    quote!(#ty)
                }
            }
            witx::TypePassedBy::Pointer => {
                let ty = TypeRef(&self.0.tref);
                if is_param {
                    let ident = Ident::new(self.0.name.as_ref(), Span::call_site());
                    quote!(#ident: *mut #ty)
                } else {
                    quote!(*mut #ty)
                }
            }
            witx::TypePassedBy::PointerLengthPair => {
                assert!(is_param);
                let ident_ptr =
                    Ident::new(&format!("{}_ptr", self.0.name.as_ref()), Span::call_site());
                let ident_len =
                    Ident::new(&format!("{}_len", self.0.name.as_ref()), Span::call_site());

                match &*self.0.tref.type_() {
                    witx::Type::Array(t) => {
                        let ty = TypeRef(t);
                        quote!(#ident_ptr: *const #ty, #ident_len: usize)
                    }
                    witx::Type::Builtin(witx::BuiltinType::String) => {
                        quote!(#ident_ptr: *const u8, #ident_len: usize)
                    }
                    t => panic!("unexpected pointer length pair type {:?}", t),
                }
            }
        });
    }
}

struct RawFunctionOutParam<'a>(&'a witx::InterfaceFuncParam);

impl<'a> ToTokens for RawFunctionOutParam<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = Ident::new(self.0.name.as_ref(), Span::call_site());
        let ty = TypeRef(&self.0.tref);
        tokens.extend(quote!(
            #ident: *mut #ty
        ))
    }
}

struct RawFunction(Rc<witx::InterfaceFunc>);

impl ToTokens for RawFunction {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.name.as_str();
        let ident = Ident::new(&name.to_snake_case(), Span::call_site());
        let params = self.0.params.iter().map(RawFunctionParam);
        let out_params = self.0.results.iter().skip(1).map(RawFunctionOutParam);
        let comma = if self.0.params.is_empty() {
            quote!()
        } else {
            quote!(,)
        };
        let ret = if let Some(result) = self.0.results.get(0) {
            let ty = RawFunctionParam(result);
            quote!(-> #ty)
        } else {
            quote!()
        };

        tokens.extend(quote!(
            #[link_name = #name]
            pub fn #ident(#(#params),*#comma#(#out_params),*) #ret;
        ));
    }
}

struct Module<'a>(&'a witx::Module);

impl<'a> ToTokens for Module<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.name.as_str();
        let ident = Ident::new(&name.to_snake_case(), Span::call_site());
        let functions = self.0.funcs().map(Function);
        let raw_functions = self.0.funcs().map(RawFunction);

        tokens.extend(quote!(
            #[allow(unused_imports)]
            pub mod #ident {
                use core::mem::MaybeUninit;
                use super::{types::{Error, Result}, types};

                #(#functions)*

                mod raw {
                    use super::*;
                    #[link(wasm_import_module = #name)]
                    extern "C" {
                        #(#raw_functions)*
                    }
                }
            }
        ));
    }
}

struct Modules<'a>(&'a witx::Document);

impl<'a> ToTokens for Modules<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for module in self.0.modules() {
            Module(&module).to_tokens(tokens);
        }
    }
}

pub fn generate(input: Input) -> Result<TokenStream> {
    let docs = witx::load(&input.paths).map_err(|e| {
        Error::new(
            proc_macro2::Span::call_site(),
            format!("failed to parse witx: {:#?}", e),
        )
    })?;

    let types = Types(&docs);
    let modules = Modules(&docs);

    Ok(quote!(
        #types

        #modules
    )
    .into())
}
