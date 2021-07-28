
// this code vendored in from https://github.com/terry90/internationalization-rs
// the original crate is targeted at a non-workspace, `std` project structure. The adaptations here
// make it suitable for `no_std` and workspace integration.

use crate::project_root;

use glob::glob;
use lazy_static::lazy_static;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;

type Key = String;
type Locale = String;
type Value = String;
type Translations = HashMap<Key, HashMap<Locale, Value>>;

fn read_locales() -> Translations {
    let mut translations: Translations = HashMap::new();

    let project_dir = project_root();
    let build_directory = project_dir.to_str().unwrap();
    let locales = format!("{}/**/locales/**/*.json", build_directory);
    println!("Reading {}", &locales);

    for entry in glob(&locales).expect("Failed to read glob pattern") {
        let entry = entry.unwrap();
        println!("cargo:rerun-if-changed={}", entry.display());
        let file = File::open(entry).expect("Failed to open the file");
        let mut reader = std::io::BufReader::new(file);
        let mut content = String::new();
        reader
            .read_to_string(&mut content)
            .expect("Failed to read the file");
        let res: HashMap<String, HashMap<String, String>> =
            serde_json::from_str(&content).expect("Cannot parse locale file");
        translations.extend(res);
    }
    translations
}

fn extract_vars(tr: &str) -> Vec<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new("\\$[a-zA-Z0-9_-]+").unwrap();
    }

    let mut a = RE
        .find_iter(tr)
        .map(|mat| mat.as_str().to_owned())
        .collect::<Vec<String>>();
    a.sort();

    //println!("-----\n{:?}\n-----", &a);
    a
}

fn convert_vars_to_idents(vars: &Vec<String>) -> Vec<Ident> {
    vars.iter()
        .map(|var| Ident::new(&var[1..], Span::call_site()))
        .collect()
}

fn generate_code(translations: Translations) -> proc_macro2::TokenStream {
    let mut branches = Vec::<TokenStream>::new();

    for (key, trs) in translations {
        let mut langs = Vec::<TokenStream>::new();
        let mut needs_interpolation = false;
        let mut vars = Vec::new();
        for (lang, tr) in trs {
            let lang_vars = extract_vars(&tr);
            needs_interpolation = lang_vars.len() > 0;

            if needs_interpolation {
                let idents = convert_vars_to_idents(&lang_vars);
                vars.extend(lang_vars.clone());

                langs.push(quote! {
                    #lang => #tr#(.replace(#lang_vars, $#idents))*,
                });
            } else {
                langs.push(quote! {
                    #lang => #tr,
                });
            }
        }

        vars.sort();
        vars.dedup();
        let vars_ident = convert_vars_to_idents(&vars);
        if needs_interpolation {
            branches.push(quote! {
                (#key, #(#vars_ident: $#vars_ident:expr, )*$lang:expr) => {
                    match $lang.as_ref() {
                        #(#langs)*
                        e => panic!("Missing language: {}", e)
                    }
                };
            });
            branches.push(quote! {
                (#key, $($e:tt)*) => {
                    compile_error!(stringify!(Please provide: #(#vars_ident),* >> The order matters!));
                };
            });
        } else {
            branches.push(quote! {
                (#key, $lang:expr) => {
                    match $lang.as_ref() {
                        #(#langs)*
                        e => panic!("Missing language: {}", e)
                    }
                };
            });
        }
    }

    quote! {
        #[macro_export]
        macro_rules! t {
            #(#branches)*
            ($key:expr, $lang:expr) => {
                compile_error!("Missing translation");
            }
        }
    }
}

fn write_code(code: TokenStream) {
    let mut dest = project_root();
    dest.push("locales");
    dest.push("src");
    let mut output = File::create(&std::path::Path::new(&dest).join("generated.rs")).unwrap();
    output
        .write(code.to_string().as_bytes())
        .expect("Cannot write generated i18n code");
}

pub fn generate_locales() {
    let translations = read_locales();
    let code = generate_code(translations);
    //println!("{}", &code);
    write_code(code);
}
