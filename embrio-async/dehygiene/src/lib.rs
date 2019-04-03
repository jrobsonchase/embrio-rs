extern crate proc_macro;

use proc_macro2::{Ident, Span, TokenStream};
use proc_macro_hack::proc_macro_hack;
use quote::quote;

use syn::{parse_macro_input, FnArg, FnDecl, ItemFn, ReturnType, Type};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

fn generate_arg_name() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .skip_while(|c| c.is_numeric())
        .take(20)
        .collect()
}

fn ewait(
    input: proc_macro::TokenStream,
    arg_name: &str,
) -> proc_macro::TokenStream {
    let input: TokenStream = input.into();
    let arg = Ident::new(arg_name, Span::call_site());
    quote!({
        let mut pinned = #input;
        loop {
            // Safety: We trust users to only call this from within an
            // async_block created generator, they are static generators so must
            // be immovable in memory, so creating a pinned reference into a
            // generator-local is safe. de-referencing the argument pointer is
            // safe for reasons explained in the embrio-async safety notes.
            let polled = unsafe {
                let pin = ::core::pin::Pin::new_unchecked(&mut pinned);
                ::core::future::Future::poll(pin, #arg.get_context())
            };
            if let ::core::task::Poll::Ready(x) = polled {
                break x;
            }
            yield ::core::task::Poll::Pending;
        }
    })
    .into()
}

#[proc_macro_hack]
pub fn async_block(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let arg_name = generate_arg_name();
    let input: TokenStream = expand_ewait(input, &arg_name).into();
    let arg = Ident::new(&arg_name, Span::call_site());
    quote!({
        unsafe {
            ::embrio_async::make_future(move |mut #arg| {
                static move || {
                    if false { yield ::core::task::Poll::Pending }
                    #input
                }
            })
        }
    })
    .into()
}

#[proc_macro_hack]
pub fn async_stream_block(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    struct ReplaceYields;

    impl syn::visit_mut::VisitMut for ReplaceYields {
        fn visit_expr_yield_mut(&mut self, node: &mut syn::ExprYield) {
            syn::visit_mut::visit_expr_yield_mut(self, node);
            let expr = node
                .expr
                .take()
                .unwrap_or_else(|| syn::parse_str("()").unwrap());
            node.expr = Some(Box::new(
                syn::parse2(quote!(::core::task::Poll::Ready(#expr))).unwrap(),
            ));
        }
    }

    let input: TokenStream = input.into();
    let mut input: syn::Block = syn::parse2(quote!({ #input })).unwrap();
    syn::visit_mut::VisitMut::visit_block_mut(&mut ReplaceYields, &mut input);

    let arg_name = generate_arg_name();

    syn::visit_mut::VisitMut::visit_block_mut(
        &mut ExpandEwait(&arg_name),
        &mut input,
    );

    let arg = Ident::new(&arg_name, Span::call_site());
    quote!({
        unsafe {
            ::embrio_async::make_stream(move |mut #arg| {
                static move || {
                    if false { yield ::core::task::Poll::Pending }
                    #input
                }
            })
        }
    })
    .into()
}

#[proc_macro_attribute]
pub fn async_fn(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    assert!(attr.is_empty(), "async_fn attribute takes no arguments");
    async_fn_impl(parse_macro_input!(body)).into()
}

fn async_fn_impl(item: ItemFn) -> TokenStream {
    let ItemFn {
        attrs,
        vis,
        constness,
        unsafety,
        asyncness,
        abi,
        ident,
        decl,
        block,
    } = item;

    let FnDecl {
        generics,
        inputs,
        output,
        ..
    } = *decl;

    // add bound to existing lifetimes
    let lifetime_defs = generics.lifetimes().map(|lt| {
        if lt.colon_token.is_some() {
            quote! {
                #lt + 'future
            }
        } else {
            quote! {
                #lt : 'future
            }
        }
    });

    // add lifetime bound to existing generics
    let type_params = generics.type_params().map(|tp| {
        if tp.colon_token.is_some() {
            quote! {
                #tp + 'future
            }
        } else {
            quote! {
                #tp: 'future
            }
        }
    });

    // add lifetime bounds to impl Trait args
    let inputs = inputs.into_iter().map(|input| {
        match &input {
            FnArg::Captured(binding) => match &binding.ty {
                Type::ImplTrait(implty) => {
                    let pat = &binding.pat;
                    return quote! {
                        #pat : #implty + 'future
                    };
                }
                _ => {}
            },
            _ => {}
        }
        quote! {
            #input
        }
    });

    let where_clause = &generics.where_clause;

    let lifetimes = generics.lifetimes().map(|lt| lt.lifetime.clone());
    let output_type = match output {
        ReturnType::Default => quote! {
            impl Future<Output = ()> #(+ ::embrio_async::Captures<#lifetimes>)* + 'future
        },
        ReturnType::Type(_, ty) => {
            quote! { impl Future<Output = #ty> #(+ ::embrio_async::Captures<#lifetimes>)* + 'future }
        }
    };

    (quote! {
        #(#attrs)*
        #vis #constness #unsafety #asyncness #abi
        fn #ident <'future #(, #lifetime_defs)* #(, #type_params)*> (#(#inputs),*) -> #output_type
        #where_clause
        {
            ::embrio_async::async_block! #block
        }
    })
    .into()
}

struct ExpandEwait<'a>(&'a str);

impl<'a> syn::visit_mut::VisitMut for ExpandEwait<'a> {
    fn visit_expr_mut(&mut self, node: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, node);
        let body = match node {
            syn::Expr::Macro(m) => {
                let mac = &mut m.mac;
                let segs = &mut mac.path.segments;
                if segs.len() == 1
                    && segs.last().unwrap().value().ident.to_string() == "ewait"
                {
                    core::mem::replace(&mut mac.tts, TokenStream::new())
                } else {
                    return;
                }
            }
            _ => return,
        };

        let expanded = ewait(body.into(), self.0);
        *node = syn::parse(expanded).unwrap();
    }
}

fn expand_ewait(
    body: proc_macro::TokenStream,
    arg_name: &str,
) -> proc_macro::TokenStream {
    let body: TokenStream = body.into();
    let mut body: syn::Block = syn::parse_quote!({#body});
    syn::visit_mut::VisitMut::visit_block_mut(
        &mut ExpandEwait(arg_name),
        &mut body,
    );

    quote!(#body).into()
}
