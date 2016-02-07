// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use backtrack::{self, Backtrack};
use dfa::{self, Dfa};
use input::{ByteInput, CharInput};
use literals::Literals;
use nfa::Nfa;
use program::{Program, ProgramBuilder};
use re::CaptureIdxs;
use Error;

/// Executor manages the execution of a regular expression.
///
/// In particular, this manages the various compiled forms of a single regular
/// expression and the choice of which matching engine to use to execute a
/// regular expression.
#[derive(Clone, Debug)]
pub struct Executor {
    /// A compiled program that is used in the NFA simulation and backtracking.
    /// It can be byte-based or Unicode codepoint based.
    ///
    /// N.B. It is not possibly to make this byte-based from the public API.
    /// It is only used for testing byte based programs in the NFA simulations.
    prog: Program,
    /// A compiled byte based program for DFA execution. This is only used
    /// if a DFA can be executed. (Currently, only word boundary assertions are
    /// not supported.) Note that this program contains an embedded `.*?`
    /// preceding the first capture group, unless the regex is anchored at the
    /// beginning.
    dfa: Program,
    /// The same as above, except the program is reversed (and there is no
    /// preceding `.*?`). This is used by the DFA to find the starting location
    /// of matches.
    dfa_reverse: Program,
    /// Set to true if and only if the DFA can be executed.
    can_dfa: bool,
    /// A preference for matching engine selection.
    ///
    /// This defaults to Automatic, which means the matching engine is selected
    /// based on heuristics (such as the nature and size of the compiled
    /// program, in addition to the size of the search text).
    ///
    /// If either Nfa or Backtrack is set, then it is always used because
    /// either is capable of executing every compiled program on any input
    /// size.
    ///
    /// If anything else is set, behaviour may be incorrect.
    match_engine: MatchEngine,
}

impl Executor {
    pub fn new(
        re: &str,
        match_engine: MatchEngine,
        size_limit: usize,
        bytes: bool,
    ) -> Result<Executor, Error> {
        let prog = try!(
            ProgramBuilder::new(re)
                           .size_limit(size_limit)
                           .bytes(bytes)
                           .compile());
        let dfa = try!(
            ProgramBuilder::new(re)
                           .size_limit(size_limit)
                           .dfa(true)
                           .compile());
        let dfa_reverse = try!(
            ProgramBuilder::new(re)
                           .size_limit(size_limit)
                           .dfa(true)
                           .reverse(true)
                           .compile());
        let can_dfa = dfa::can_exec(&prog.insts);
        Ok(Executor {
            prog: prog,
            dfa: dfa,
            dfa_reverse: dfa_reverse,
            can_dfa: can_dfa,
            match_engine: match_engine,
        })
    }

    pub fn regex_str(&self) -> &str {
        &self.prog.original
    }

    pub fn capture_names(&self) -> &[Option<String>] {
        &self.prog.cap_names
    }

    pub fn alloc_captures(&self) -> Vec<Option<usize>> {
        self.prog.alloc_captures()
    }

    pub fn exec(
        &self,
        caps: &mut CaptureIdxs,
        text: &str,
        start: usize,
    ) -> bool {
        match self.match_engine {
            MatchEngine::Nfa => self.exec_nfa(caps, text, start),
            MatchEngine::Backtrack => self.exec_backtrack(caps, text, start),
            MatchEngine::Literals => self.exec_literals(caps, text, start),
            MatchEngine::Automatic => self.exec_auto(caps, text, start),
        }
    }

    pub fn exec_auto(
        &self,
        caps: &mut CaptureIdxs,
        text: &str,
        start: usize,
    ) -> bool {
        if self.can_dfa && caps.len() <= 2 {
            let end = match Dfa::exec(&self.dfa, text.as_bytes(), start) {
                None => return false,
                Some(end) => end,
            };
            match caps.len() {
                0 => return true,
                2 => {
                    if end == start {
                        caps[0] = Some(end);
                        caps[1] = Some(end);
                        return true;
                    }
                    assert!(self.dfa_reverse.reverse);
                    let start = Dfa::exec(
                        &self.dfa_reverse, text.as_bytes(), end).unwrap();
                    caps[0] = Some(start);
                    caps[1] = Some(end);
                    return true;
                }
                n => panic!("invalid capture size: {}", n),
            }
        }

        if caps.len() <= 2 && self.prog.is_prefix_match() {
            self.exec_literals(caps, text, start)
        } else if backtrack::should_exec(self.prog.insts.len(), text.len()) {
            self.exec_backtrack(caps, text, start)
        } else {
            self.exec_nfa(caps, text, start)
        }
    }

    fn exec_nfa(
        &self,
        caps: &mut CaptureIdxs,
        text: &str,
        start: usize,
    ) -> bool {
        if self.prog.insts.is_bytes() {
            Nfa::exec(&self.prog, caps, ByteInput::new(text), start)
        } else {
            Nfa::exec(&self.prog, caps, CharInput::new(text), start)
        }
    }

    fn exec_backtrack(
        &self,
        caps: &mut CaptureIdxs,
        text: &str,
        start: usize,
    ) -> bool {
        if self.prog.insts.is_bytes() {
            Backtrack::exec(&self.prog, caps, ByteInput::new(text), start)
        } else {
            Backtrack::exec(&self.prog, caps, CharInput::new(text), start)
        }
    }

    fn exec_literals(
        &self,
        caps: &mut CaptureIdxs,
        text: &str,
        start: usize,
    ) -> bool {
        match self.prog.prefixes.find(&text.as_bytes()[start..]) {
            None => false,
            Some((s, e)) => {
                if caps.len() == 2 {
                    caps[0] = Some(start + s);
                    caps[1] = Some(start + e);
                }
                true
            }
        }
    }
}

/// The matching engines offered by this regex implementation.
///
/// N.B. This is exported for use in testing.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum MatchEngine {
    /// Automatically choose the best matching engine based on heuristics.
    Automatic,
    /// A bounded backtracking implementation. About twice as fast as the
    /// NFA, but can only work on small regexes and small input.
    Backtrack,
    /// A full NFA simulation. Can always be employed but almost always the
    /// slowest choice.
    Nfa,
    /// If the entire regex is a literal and no capture groups have been
    /// requested, then we can degrade to a simple substring match.
    Literals,
}