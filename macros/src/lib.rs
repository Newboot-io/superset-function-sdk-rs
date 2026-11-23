//! Procedural macros for `superset-function-sdk` (`newboot`):
//! - `#[derive(Settings)]` — parses route settings and exposes metadata for `function.toml`.
//! - `#[map]` — exports a `wasm-graph:processor/map@1.1.0` component on `wasm32`.
//! - `#[filter]` — exports a `wasm-graph:processor/filter@1.1.0` component on `wasm32`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::punctuated::Punctuated;
use syn::{
  parse_macro_input, Attribute, Data, DeriveInput, Expr, ExprLit, Fields, FnArg, GenericArgument,
  ItemFn, Lit, Meta, PathArguments, Token, Type,
};

#[proc_macro_derive(Settings, attributes(setting))]
pub fn derive_settings(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  expand_settings(input)
    .unwrap_or_else(|e| e.to_compile_error())
    .into()
}

#[proc_macro_attribute]
pub fn map(attr: TokenStream, item: TokenStream) -> TokenStream {
  let item = parse_macro_input!(item as ItemFn);
  expand_handler(attr, item, HandlerKind::Map)
    .unwrap_or_else(|e| e.to_compile_error())
    .into()
}

#[proc_macro_attribute]
pub fn filter(attr: TokenStream, item: TokenStream) -> TokenStream {
  let item = parse_macro_input!(item as ItemFn);
  expand_handler(attr, item, HandlerKind::Filter)
    .unwrap_or_else(|e| e.to_compile_error())
    .into()
}

#[derive(Clone, Copy)]
enum HandlerKind {
  Map,
  Filter,
}

fn expand_handler(
  attr: TokenStream,
  item: ItemFn,
  kind: HandlerKind,
) -> syn::Result<proc_macro2::TokenStream> {
  if !attr.is_empty() {
    return Err(syn::Error::new(
      proc_macro2::Span::call_site(),
      "expected no arguments on #[map] / #[filter]",
    ));
  }
  if item.sig.asyncness.is_some() {
    return Err(syn::Error::new_spanned(
      item.sig.asyncness,
      "#[map] / #[filter] functions must be synchronous",
    ));
  }
  if item.sig.unsafety.is_some() {
    return Err(syn::Error::new_spanned(
      item.sig.unsafety,
      "#[map] / #[filter] functions must not be unsafe",
    ));
  }
  if !item.sig.generics.params.is_empty() {
    return Err(syn::Error::new_spanned(
      &item.sig.generics,
      "#[map] / #[filter] functions cannot be generic",
    ));
  }
  if item.sig.abi.is_some() {
    return Err(syn::Error::new_spanned(
      &item.sig.abi,
      "#[map] / #[filter] functions must use the default Rust ABI",
    ));
  }
  if item.sig.variadic.is_some() {
    return Err(syn::Error::new_spanned(
      &item.sig.variadic,
      "#[map] / #[filter] functions cannot be variadic",
    ));
  }
  if item.sig.inputs.len() != 1 {
    return Err(syn::Error::new_spanned(
      &item.sig.inputs,
      "function must take exactly one parameter: `Call<Settings>`",
    ));
  }
  let settings_ty = extract_call_settings_type(item.sig.inputs.first().unwrap())?;
  let vis = &item.vis;
  let fn_ident = &item.sig.ident;
  let manifest_fn_ident = format_ident!("{fn_ident}_manifest");
  let wasm_mod_ident = format_ident!("__newboot_wasm_export_{fn_ident}");
  let kind_expr = match kind {
    HandlerKind::Map => quote!(::newboot::FunctionKind::Map),
    HandlerKind::Filter => quote!(::newboot::FunctionKind::Filter),
  };

  let wasm_export = match kind {
    HandlerKind::Map => quote! {
      #[cfg(target_arch = "wasm32")]
      mod #wasm_mod_ident {
        use super::*;
        use ::newboot::__private::wit_map::exports::wasm_graph::processor::map as __wit_map;
        use ::newboot::__private::wit_map::wasm_graph::processor::types as __wit_types;

        static SETTINGS: ::std::sync::Mutex<::core::option::Option<#settings_ty>> =
          ::std::sync::Mutex::new(::core::option::Option::None);

        struct __NewbootComponent;

        impl __wit_map::Guest for __NewbootComponent {
          fn init(configuration: __wit_types::ModuleConfiguration) -> bool {
            match <#settings_ty as ::newboot::Settings>::from_properties(&configuration.properties) {
              ::core::result::Result::Ok(parsed) => {
                *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) =
                  ::core::option::Option::Some(parsed);
                true
              }
              ::core::result::Result::Err(err) => {
                ::newboot::log::emit(::newboot::log::Level::Error, "init", &err.to_string());
                false
              }
            }
          }

          fn process(
            data: __wit_types::DataModel,
          ) -> ::core::result::Result<__wit_types::DataModel, __wit_types::Error> {
            let settings = {
              let guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
              guard
                .as_ref()
                .map(::newboot::Settings::clone_for_call)
                .ok_or_else(|| __wit_types::Error {
                  message: ::std::string::String::from("function not initialized"),
                })?
            };
            let (raw_ts, raw_schema, message) = ::newboot::__private::from_map_data_model(data)?;
            let out = #fn_ident(::newboot::Call { message, settings })
              .map_err(|e| __wit_types::Error { message: e.to_string() })?;
            ::core::result::Result::Ok(::newboot::__private::to_map_data_model(raw_ts, raw_schema, out))
          }
        }

        ::newboot::__private::export_map!(__NewbootComponent);
      }
    },
    HandlerKind::Filter => quote! {
      #[cfg(target_arch = "wasm32")]
      mod #wasm_mod_ident {
        use super::*;
        use ::newboot::__private::wit_filter::exports::wasm_graph::processor::filter as __wit_filter;
        use ::newboot::__private::wit_filter::wasm_graph::processor::types as __wit_types;

        static SETTINGS: ::std::sync::Mutex<::core::option::Option<#settings_ty>> =
          ::std::sync::Mutex::new(::core::option::Option::None);

        struct __NewbootComponent;

        impl __wit_filter::Guest for __NewbootComponent {
          fn init(configuration: __wit_types::ModuleConfiguration) -> bool {
            match <#settings_ty as ::newboot::Settings>::from_properties(&configuration.properties) {
              ::core::result::Result::Ok(parsed) => {
                *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) =
                  ::core::option::Option::Some(parsed);
                true
              }
              ::core::result::Result::Err(err) => {
                ::newboot::log::emit(::newboot::log::Level::Error, "init", &err.to_string());
                false
              }
            }
          }

          fn process(data: __wit_types::DataModel) -> ::core::result::Result<bool, __wit_types::Error> {
            let settings = {
              let guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
              guard
                .as_ref()
                .map(::newboot::Settings::clone_for_call)
                .ok_or_else(|| __wit_types::Error {
                  message: ::std::string::String::from("function not initialized"),
                })?
            };
            let (_, _, message) = ::newboot::__private::from_filter_data_model(data)?;
            #fn_ident(::newboot::Call { message, settings })
              .map_err(|e| __wit_types::Error { message: e.to_string() })
          }
        }

        ::newboot::__private::export_filter!(__NewbootComponent);
      }
    },
  };

  Ok(quote! {
    #item

    /// Build the [`newboot::FunctionManifest`] for this function from its `Settings` type.
    #[allow(dead_code)]
    #vis fn #manifest_fn_ident(
      name: impl ::core::convert::Into<::std::string::String>,
      version: impl ::core::convert::Into<::std::string::String>,
      description: impl ::core::convert::Into<::std::string::String>,
    ) -> ::newboot::FunctionManifest {
      ::newboot::FunctionManifest {
        description: description.into(),
        kind: #kind_expr,
        name: name.into(),
        settings: <#settings_ty as ::newboot::Settings>::specs(),
        version: version.into(),
      }
    }

    #wasm_export
  })
}

fn extract_call_settings_type(arg: &FnArg) -> syn::Result<proc_macro2::TokenStream> {
  let pat_type = match arg {
    FnArg::Typed(pt) => pt,
    FnArg::Receiver(r) => {
      return Err(syn::Error::new_spanned(
        r,
        "#[map] / #[filter] must be a free function, not a method",
      ));
    }
  };
  let Type::Path(tp) = pat_type.ty.as_ref() else {
    return Err(syn::Error::new_spanned(
      &pat_type.ty,
      "parameter type must be `Call<Settings>`",
    ));
  };
  let Some(last) = tp.path.segments.last() else {
    return Err(syn::Error::new_spanned(
      &pat_type.ty,
      "parameter type must be `Call<Settings>`",
    ));
  };
  if last.ident != "Call" {
    return Err(syn::Error::new_spanned(
      last,
      "parameter type must be `Call<Settings>`",
    ));
  }
  match &last.arguments {
    PathArguments::None => Ok(quote!(::newboot::NoSettings)),
    PathArguments::AngleBracketed(ab) => {
      if ab.args.len() != 1 {
        return Err(syn::Error::new_spanned(
          ab,
          "Call takes at most one type argument: `Call<Settings>`",
        ));
      }
      match ab.args.first().unwrap() {
        GenericArgument::Type(ty) => Ok(quote!(#ty)),
        other => Err(syn::Error::new_spanned(
          other,
          "expected a Settings type argument in `Call<Settings>`",
        )),
      }
    }
    other => Err(syn::Error::new_spanned(
      other,
      "parameter type must be `Call<Settings>`",
    )),
  }
}

struct FieldAttr {
  default: Option<(String, proc_macro2::Span)>,
  key: Option<(String, proc_macro2::Span)>,
}

fn parse_field_attr(attrs: &[Attribute]) -> syn::Result<FieldAttr> {
  let mut out = FieldAttr {
    default: None,
    key: None,
  };
  for attr in attrs {
    if !attr.path().is_ident("setting") {
      continue;
    }
    let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    for meta in nested {
      let Meta::NameValue(nv) = meta else {
        return Err(syn::Error::new_spanned(
          meta,
          "expected `key = \"...\"` or `default = \"...\"`",
        ));
      };
      let Expr::Lit(ExprLit {
        lit: Lit::Str(s), ..
      }) = &nv.value
      else {
        return Err(syn::Error::new_spanned(
          &nv.value,
          "setting attribute value must be a string literal",
        ));
      };
      if nv.path.is_ident("key") {
        if out.key.is_some() {
          return Err(syn::Error::new_spanned(
            &nv.path,
            "duplicate `key` attribute on setting field",
          ));
        }
        out.key = Some((s.value(), s.span()));
      } else if nv.path.is_ident("default") {
        if out.default.is_some() {
          return Err(syn::Error::new_spanned(
            &nv.path,
            "duplicate `default` attribute on setting field",
          ));
        }
        out.default = Some((s.value(), s.span()));
      } else {
        return Err(syn::Error::new_spanned(
          &nv.path,
          "unknown #[setting(...)] attribute; expected `key` or `default`",
        ));
      }
    }
  }
  Ok(out)
}

fn doc_comment(attrs: &[Attribute]) -> String {
  let mut lines = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("doc") {
      continue;
    }
    if let Meta::NameValue(nv) = &attr.meta {
      if let Expr::Lit(ExprLit {
        lit: Lit::Str(s), ..
      }) = &nv.value
      {
        let trimmed = s.value().trim().to_string();
        if !trimmed.is_empty() {
          lines.push(trimmed);
        }
      }
    }
  }
  lines.join(" ")
}

fn snake_to_camel(raw: &str) -> String {
  let s = raw.strip_prefix("r#").unwrap_or(raw);
  let mut out = String::with_capacity(s.len());
  let mut upper_next = false;
  for ch in s.chars() {
    if ch == '_' {
      if !out.is_empty() {
        upper_next = true;
      }
    } else if upper_next {
      out.extend(ch.to_uppercase());
      upper_next = false;
    } else {
      out.push(ch);
    }
  }
  out
}

enum BaseType {
  String,
  F64,
  F32,
  I64,
  U64,
  I32,
  U32,
  Usize,
  Bool,
}

impl BaseType {
  fn from_ident(ident: &str) -> Option<Self> {
    match ident {
      "String" => Some(Self::String),
      "f64" => Some(Self::F64),
      "f32" => Some(Self::F32),
      "i64" => Some(Self::I64),
      "u64" => Some(Self::U64),
      "i32" => Some(Self::I32),
      "u32" => Some(Self::U32),
      "usize" => Some(Self::Usize),
      "bool" => Some(Self::Bool),
      _ => None,
    }
  }

  fn validate_default(&self, raw: &str, span: proc_macro2::Span) -> syn::Result<()> {
    let t = raw.trim();
    let ok = match self {
      Self::String => true,
      Self::F64 => t.parse::<f64>().is_ok_and(f64::is_finite),
      Self::F32 => t.parse::<f32>().is_ok_and(f32::is_finite),
      Self::I64 => t.parse::<i64>().is_ok(),
      Self::U64 => t.parse::<u64>().is_ok(),
      Self::I32 => t.parse::<i32>().is_ok(),
      Self::U32 => t.parse::<u32>().is_ok(),
      Self::Usize => t.parse::<u32>().is_ok(),
      Self::Bool => matches!(t, "true" | "false"),
    };
    if ok {
      Ok(())
    } else {
      Err(syn::Error::new(
        span,
        format!("invalid default literal `{raw}` for setting type"),
      ))
    }
  }

  fn setting_kind(&self) -> proc_macro2::TokenStream {
    match self {
      Self::String => quote!(::newboot::SettingKind::String),
      Self::F64 | Self::F32 => quote!(::newboot::SettingKind::Number),
      Self::I64 | Self::U64 | Self::I32 | Self::U32 | Self::Usize => {
        quote!(::newboot::SettingKind::Integer)
      }
      Self::Bool => quote!(::newboot::SettingKind::Boolean),
    }
  }

  fn parse_expr(&self, key: &str, raw: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    match self {
      Self::String => quote!(::core::result::Result::<::std::string::String, ::newboot::Error>::Ok(
        ::std::string::ToString::to_string(#raw)
      )),
      Self::F64 => quote!(::newboot::settings::parse_f64(#key, #raw)),
      Self::F32 => quote!(::newboot::settings::parse_f32(#key, #raw)),
      Self::I64 => quote!(::newboot::settings::parse_i64(#key, #raw)),
      Self::U64 => quote!(::newboot::settings::parse_u64(#key, #raw)),
      Self::I32 => quote!(::newboot::settings::parse_i32(#key, #raw)),
      Self::U32 => quote!(::newboot::settings::parse_u32(#key, #raw)),
      Self::Usize => quote!(::newboot::settings::parse_usize(#key, #raw)),
      Self::Bool => quote!(::newboot::settings::parse_bool(#key, #raw)),
    }
  }
}

fn classify_type(ty: &Type) -> syn::Result<(BaseType, bool)> {
  let Type::Path(tp) = ty else {
    return Err(syn::Error::new_spanned(
      ty,
      "unsupported Settings field type",
    ));
  };
  if tp.qself.is_some() {
    return Err(syn::Error::new_spanned(
      ty,
      "qualified self types are not supported in Settings",
    ));
  }
  let Some(seg) = tp.path.segments.last() else {
    return Err(syn::Error::new_spanned(
      ty,
      "unsupported Settings field type",
    ));
  };
  let name = seg.ident.to_string();
  if name == "Option" {
    if let PathArguments::AngleBracketed(ab) = &seg.arguments {
      if ab.args.len() == 1 {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
          let (base, is_opt) = classify_type(inner)?;
          if is_opt {
            return Err(syn::Error::new_spanned(
              ty,
              "nested Option<Option<T>> is not supported in Settings",
            ));
          }
          return Ok((base, true));
        }
      }
    }
    return Err(syn::Error::new_spanned(
      ty,
      "Option in Settings must specify exactly one inner type",
    ));
  }
  if !matches!(seg.arguments, PathArguments::None) {
    return Err(syn::Error::new_spanned(
      ty,
      "unsupported generic Settings field type",
    ));
  }
  BaseType::from_ident(&name)
    .map(|b| (b, false))
    .ok_or_else(|| {
      syn::Error::new_spanned(
        ty,
        "unsupported Settings field type; use String, bool, integer, float, or Option<...>",
      )
    })
}

fn expand_settings(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
  if !input.generics.params.is_empty() {
    return Err(syn::Error::new_spanned(
      &input.generics,
      "#[derive(Settings)] does not support generic structs",
    ));
  }
  let ident = &input.ident;
  let Data::Struct(data) = &input.data else {
    return Err(syn::Error::new_spanned(
      &input,
      "#[derive(Settings)] only supports structs",
    ));
  };

  match &data.fields {
    Fields::Unit => Ok(quote! {
      impl ::newboot::Settings for #ident {
        fn specs() -> &'static [::newboot::SettingSpec] {
          &[]
        }

        fn from_properties(
          __newboot_properties: &[(::std::string::String, ::std::string::String)],
        ) -> ::newboot::Result<Self> {
          let _ = __newboot_properties;
          ::core::result::Result::Ok(Self)
        }

        fn clone_for_call(&self) -> Self {
          Self
        }
      }
    }),
    Fields::Unnamed(_) => Err(syn::Error::new_spanned(
      &input,
      "#[derive(Settings)] does not support tuple structs; use named fields",
    )),
    Fields::Named(fields) => {
      let mut seen_keys = HashSet::new();
      let mut specs = Vec::new();
      let mut parses = Vec::new();
      let mut clones = Vec::new();
      let mut inits = Vec::new();

      for (idx, field) in fields.named.iter().enumerate() {
        let field_ident = field.ident.as_ref().unwrap();
        let local_ident = format_ident!("__newboot_field_{idx}");
        let attr = parse_field_attr(&field.attrs)?;
        let description = doc_comment(&field.attrs);
        let (key, key_span) = match attr.key {
          Some((k, sp)) => (k, sp),
          None => (snake_to_camel(&field_ident.to_string()), field_ident.span()),
        };
        if key.is_empty() {
          return Err(syn::Error::new(key_span, "setting key must not be empty"));
        }
        if !seen_keys.insert(key.clone()) {
          return Err(syn::Error::new(
            key_span,
            format!("duplicate setting key `{key}`"),
          ));
        }
        let (base, is_opt) = classify_type(&field.ty)?;
        if let Some((ref def_val, def_span)) = attr.default {
          base.validate_default(def_val, def_span)?;
        }
        let kind = base.setting_kind();
        let required = !is_opt && attr.default.is_none();
        let default_expr = match &attr.default {
          Some((d, _)) => quote!(::core::option::Option::Some(#d)),
          None => quote!(::core::option::Option::None),
        };

        specs.push(quote! {
          ::newboot::SettingSpec {
            default: #default_expr,
            description: #description,
            key: #key,
            kind: #kind,
            required: #required,
          }
        });

        let parse_val = base.parse_expr(&key, quote!(__newboot_raw));
        let parse_stmt = if is_opt {
          match &attr.default {
            Some((d, _)) => quote! {
              let #local_ident = match ::newboot::settings::find_property(__newboot_properties, #key) {
                ::core::option::Option::Some(__newboot_raw) => ::core::option::Option::Some(#parse_val?),
                ::core::option::Option::None => ::core::option::Option::Some({
                  let __newboot_raw = #d;
                  #parse_val?
                }),
              };
            },
            None => quote! {
              let #local_ident = match ::newboot::settings::find_property(__newboot_properties, #key) {
                ::core::option::Option::Some(__newboot_raw) => ::core::option::Option::Some(#parse_val?),
                ::core::option::Option::None => ::core::option::Option::None,
              };
            },
          }
        } else {
          match &attr.default {
            Some((d, _)) => quote! {
              let #local_ident = {
                let __newboot_raw =
                  ::newboot::settings::find_property(__newboot_properties, #key).unwrap_or(#d);
                #parse_val?
              };
            },
            None => quote! {
              let #local_ident = {
                let __newboot_raw =
                  ::newboot::settings::find_property(__newboot_properties, #key)
                    .ok_or_else(|| ::newboot::Error::missing_setting(#key))?;
                #parse_val?
              };
            },
          }
        };

        parses.push(parse_stmt);
        clones.push(quote!(#field_ident: ::core::clone::Clone::clone(&self.#field_ident)));
        inits.push(quote!(#field_ident: #local_ident));
      }

      Ok(quote! {
        impl ::newboot::Settings for #ident {
          fn specs() -> &'static [::newboot::SettingSpec] {
            static SPECS: &[::newboot::SettingSpec] = &[#(#specs),*];
            SPECS
          }

          fn from_properties(
            __newboot_properties: &[(::std::string::String, ::std::string::String)],
          ) -> ::newboot::Result<Self> {
            #(#parses)*
            ::core::result::Result::Ok(Self { #(#inits),* })
          }

          fn clone_for_call(&self) -> Self {
            Self { #(#clones),* }
          }
        }
      })
    }
  }
}
