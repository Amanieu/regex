// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(test)]

#[macro_use] extern crate lazy_static;
extern crate rand;
extern crate regex;
extern crate regex_syntax;
extern crate test;

// Due to macro scoping rules, this definition only applies for the modules
// defined below. Effectively, it allows us to use the same tests for both
// native and dynamic regexes.
macro_rules! regex(
    ($re:expr) => {{
        use regex::internal::ExecBuilder;
        ExecBuilder::new($re).nfa().build().unwrap().into_regex()
    }}
);

mod bench;
mod bench_dynamic_compile;
mod bench_dynamic_parse;
mod bench_sherlock;
