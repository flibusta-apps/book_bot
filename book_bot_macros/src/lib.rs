use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, Pat, PatType, Type, TypePath};

#[proc_macro_attribute]
pub fn log_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let handler_name = parse_macro_input!(attr as LitStr);
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    let log_stmt = generate_log_stmt(&input_fn, &handler_name);

    quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            #log_stmt
            let mut __metrics_guard = crate::handler_metrics::HandlerMetricsGuard::new(#handler_name);
            let __result: anyhow::Result<()> = #fn_block;
            if __result.is_err() {
                __metrics_guard.set_error();
            }
            __result
        }
    }
    .into()
}

fn get_type_ident(ty: &Type) -> Option<String> {
    if let Type::Path(TypePath { path, .. }) = ty {
        path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

fn get_param_ident(fn_arg: &FnArg) -> Option<(proc_macro2::Ident, String)> {
    if let FnArg::Typed(PatType { pat, ty, .. }) = fn_arg {
        if let Pat::Ident(pat_ident) = pat.as_ref() {
            if let Some(type_name) = get_type_ident(ty) {
                return Some((pat_ident.ident.clone(), type_name));
            }
        }
    }
    None
}

fn generate_log_stmt(input_fn: &ItemFn, handler_name: &LitStr) -> proc_macro2::TokenStream {
    for fn_arg in &input_fn.sig.inputs {
        if let Some((ident, type_name)) = get_param_ident(fn_arg) {
            match type_name.as_str() {
                "Message" => {
                    return quote! {
                        tracing::info!(
                            handler = #handler_name,
                            user_id = ?#ident.from.as_ref().map(|u| u.id.0)
                        );
                    };
                }
                "CallbackQuery" => {
                    return quote! {
                        tracing::info!(
                            handler = #handler_name,
                            user_id = #ident.from.id.0
                        );
                    };
                }
                _ => continue,
            }
        }
    }

    // Fallback: log without user_id if no known type found
    quote! {
        tracing::info!(handler = #handler_name);
    }
}
