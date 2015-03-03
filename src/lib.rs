// Copyright 2014 The Gfx-rs Developers.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#![feature(plugin_registrar, quote, box_syntax, rustc_private)]
#![deny(missing_copy_implementations)]

//! Macro extensions crate.
//! Implements `shaders!` macro as well as `#[shader_param]` and
//! `#[vertex_format]` attributes.

extern crate rustc;
extern crate syntax;

use syntax::{ast, attr, ext, codemap};
use syntax::parse::token;
use syntax::fold::Folder;
use syntax::ptr::P;

pub mod shader_param;
pub mod vertex_format;

/// Entry point for the plugin phase
#[plugin_registrar]
pub fn registrar(reg: &mut rustc::plugin::Registry) {
    use syntax::parse::token::intern;
    use syntax::ext::base;
    // Register the `#[shader_param]` attribute.
    reg.register_syntax_extension(intern("shader_param"),
        base::Decorator(box shader_param::ShaderParam));
    // Register the `#[vertex_format]` attribute.
    reg.register_syntax_extension(intern("vertex_format"),
        base::Decorator(box vertex_format::VertexFormat));
}

/// Scan through the field's attributes and extract the field vertex name. If
/// multiple names are found, use the first name and emit a warning.
fn find_name(cx: &mut ext::base::ExtCtxt, span: codemap::Span,
             attributes: &[ast::Attribute]) -> Option<token::InternedString> {
    attributes.iter().fold(None, |name, attribute| {
        match attribute.node.value.node {
            ast::MetaNameValue(ref attr_name, ref attr_value) => {
                match (&attr_name[..], &attr_value.node) {
                    ("name", &ast::LitStr(ref new_name, _)) => {
                        attr::mark_used(attribute);
                        name.map_or(Some(new_name.clone()), |name| {
                            cx.span_warn(span, &format!(
                                "Extra field name detected: {:?} - \
                                ignoring in favour of: {:?}", new_name, name
                            ));
                            None
                        })
                    }
                    _ => None,
                }
            }
            _ => name,
        }
    })
}

/// Marker string to base the unique identifier generated by `extern_crate_hack()` on
static EXTERN_CRATE_HACK: &'static str = "__gfx_extern_crate_hack";

/// Inserts a module with a unique identifier that reexports
/// The `gfx` crate, and returns that identifier
fn extern_crate_hack<F>(context: &mut ext::base::ExtCtxt,
                        span: codemap::Span,
                        mut push: F) -> ast::Ident where F: FnMut(P<ast::Item>) {
    use syntax::ext::build::AstBuilder;
    let extern_crate_hack = token::gensym_ident(EXTERN_CRATE_HACK);
    //let item = quote_item!(context, span, mod $extern_crate_hack {
    //    extern crate gfx_ = "gfx";
    //    pub use gfx_ as gfx;
    //}).unwrap();
    let item = context.item_mod(
        span,
        span,
        extern_crate_hack,
        vec![],
        vec![
            P(ast::Item {
                span: span,
                vis: ast::Inherited,
                attrs: vec![],
                node: ast::ItemExternCrate(
                    Some((
                        token::InternedString::new("gfx"),
                        ast::CookedStr
                    )),
                ),
                id: ast::DUMMY_NODE_ID,
                ident: token::str_to_ident("gfx_")
            }),
            context.item_use_simple_(
                span,
                ast::Public,
                context.ident_of("gfx"),
                context.path(span, vec![
                    context.ident_of("self"),
                    context.ident_of("gfx_")
                ])
            ),
        ]
    );
    push(item);
    extern_crate_hack
}

/// This Folder gets used to fixup all paths generated by the
/// #[derive trait impl to point to the unique module
/// containing the `gfx` reexport.
struct ExternCrateHackFolder {
    path_root: ast::Ident
}

impl Folder for ExternCrateHackFolder {
    fn fold_path(&mut self, p: ast::Path) -> ast::Path {
        let p = syntax::fold::noop_fold_path(p, self);
        let needs_fix = (p.segments).get(0)
                         .map(|s| s.identifier.as_str() == EXTERN_CRATE_HACK)
                         .unwrap_or(false);
        let needs_fix_self = (p.segments).get(0)
                              .map(|s| s.identifier.as_str() == "self")
                              .unwrap_or(false) &&
                             (p.segments).get(1)
                              .map(|s| s.identifier.as_str() == EXTERN_CRATE_HACK)
                              .unwrap_or(false);

        if needs_fix {
            let mut p = p.clone();
            p.segments[0].identifier = self.path_root;
            p.global = false;
            p
        } else if needs_fix_self {
            let mut p = p.clone();
            p.segments[1].identifier = self.path_root;
            p.global = false;
            p
        } else {
            p
        }

    }
}

/// Simply applies the `ExternCrateHackFolder`
fn fixup_extern_crate_paths(item: P<ast::Item>, path_root: ast::Ident) -> P<ast::Item> {
    ExternCrateHackFolder {
        path_root: path_root
    }.fold_item(item).into_iter().next().unwrap()
}