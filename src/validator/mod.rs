use proc_macro::TokenStream;
use quote::quote;
use syn::{Ident, ItemFn, Pat, Path, parse_macro_input};

pub fn validator_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(attr as ValidationAspectInput);
    let validator_path = input.validator;
    let param_name = input.param;

    let mut func = parse_macro_input!(item as ItemFn);

    if let Err(e) = ensure_param_exists(&func, &param_name) {
        return e.to_compile_error().into();
    }

    let original_block = &func.block;

    let injected_block: syn::Block = syn::parse_quote!({
        if let Err(e) = crate::validation_rules::validate_entity(&#validator_path, &#param_name) {
            return ::core::convert::Into::into(e);
        }
        #original_block
    });

    *func.block = injected_block;

    TokenStream::from(quote! { #func })
}

/// Without this a typo produces rustc's generic "cannot find value" error
/// pointing into macro-generated code.
fn ensure_param_exists(func: &ItemFn, param: &Ident) -> syn::Result<()> {
    let mut bound = Vec::new();
    for arg in &func.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            collect_bindings(&pat_type.pat, &mut bound);
        }
    }
    if bound.iter().any(|name| param == name) {
        return Ok(());
    }

    let fn_name = &func.sig.ident;
    let available = if bound.is_empty() {
        format!("`{fn_name}` has no named parameters")
    } else {
        format!("available: {}", bound.join(", "))
    };
    Err(syn::Error::new(
        param.span(),
        format!("#[validator] parameter `{param}` is not a parameter of `{fn_name}`; {available}"),
    ))
}

/// Names bound by a parameter pattern, including destructured extractors
/// such as `Json(body)`, `State(AppState { db, .. })` or `(a, b)`.
fn collect_bindings(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Ident(p) => {
            out.push(p.ident.to_string());
            if let Some((_, sub)) = &p.subpat {
                collect_bindings(sub, out);
            }
        }
        Pat::TupleStruct(p) => p.elems.iter().for_each(|e| collect_bindings(e, out)),
        Pat::Tuple(p) => p.elems.iter().for_each(|e| collect_bindings(e, out)),
        Pat::Slice(p) => p.elems.iter().for_each(|e| collect_bindings(e, out)),
        Pat::Struct(p) => p.fields.iter().for_each(|f| collect_bindings(&f.pat, out)),
        Pat::Reference(p) => collect_bindings(&p.pat, out),
        Pat::Type(p) => collect_bindings(&p.pat, out),
        Pat::Paren(p) => collect_bindings(&p.pat, out),
        _ => {}
    }
}

struct ValidationAspectInput {
    validator: Path,
    param: Ident,
}

impl syn::parse::Parse for ValidationAspectInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const USAGE: &str = "expected #[validator(ValidatorPath, param_name)]";
        let validator: Path = input
            .parse()
            .map_err(|e| syn::Error::new(e.span(), format!("{USAGE}: invalid validator path")))?;
        let _: syn::Token![,] = input
            .parse()
            .map_err(|e| syn::Error::new(e.span(), format!("{USAGE}: missing comma")))?;
        let param: Ident = input
            .parse()
            .map_err(|e| syn::Error::new(e.span(), format!("{USAGE}: invalid parameter name")))?;
        let _: Option<syn::Token![,]> = input.parse()?;
        if !input.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                format!("{USAGE}: unexpected tokens"),
            ));
        }
        Ok(ValidationAspectInput { validator, param })
    }
}
