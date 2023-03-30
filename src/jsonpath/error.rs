// Copyright 2023 Datafuse Labs.
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

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::Write;
use std::num::IntErrorKind;
use std::num::ParseIntError;

use crate::jsonpath::exception::pretty_print_error;
use crate::jsonpath::exception::Range;
use itertools::Itertools;

use crate::jsonpath::input::Input;
use crate::jsonpath::parser::token::*;
use crate::jsonpath::util::transform_span;

const MAX_DISPLAY_ERROR_COUNT: usize = 6;

/// This error type accumulates errors and their position when backtracking
/// through a parse tree. This take a deepest error at `alt` combinator.
#[derive(Clone, Debug)]
pub struct Error<'a> {
    /// The span of the next token when encountering an error.
    pub span: Range,
    /// List of errors tried in various branches that consumed
    /// the same (farthest) length of input.
    pub errors: Vec<ErrorKind>,
    /// The backtrace stack of the error.
    pub contexts: Vec<(Range, &'static str)>,
    /// The extra backtrace of error in optional branches.
    pub backtrace: &'a Backtrace,
}

/// ErrorKind is the error type returned from parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Error generated by `match_token` function
    ExpectToken(TokenKind),
    /// Error generated by `match_text` function
    ExpectText(&'static str),
    /// Plain text description of an error
    Other(&'static str),
}

/// Record the farthest position in the input before encountering an error.
///
/// This is similar to the `Error`, but the information will not get lost
/// even the error is from a optional branch.
#[derive(Debug, Clone, Default)]
pub struct Backtrace {
    inner: RefCell<Option<BacktraceInner>>,
}

impl Backtrace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&self) {
        self.inner.replace(None);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktraceInner {
    /// The span of the next token when encountering an error.
    span: Range,
    /// List of errors tried in various branches that consumed
    /// the same (farthest) length of input.
    errors: Vec<ErrorKind>,
}

impl<'a> nom::error::ParseError<Input<'a>> for Error<'a> {
    fn from_error_kind(i: Input<'a>, _: nom::error::ErrorKind) -> Self {
        Error {
            span: transform_span(&i[..1]).unwrap(),
            errors: vec![],
            contexts: vec![],
            backtrace: i.1,
        }
    }

    fn append(_: Input<'a>, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }

    fn from_char(_: Input<'a>, _: char) -> Self {
        unreachable!()
    }

    fn or(mut self, mut other: Self) -> Self {
        match self.span.start.cmp(&other.span.start) {
            Ordering::Equal => {
                self.errors.append(&mut other.errors);
                self.contexts.clear();
                self
            }
            Ordering::Less => other,
            Ordering::Greater => self,
        }
    }
}

impl<'a> nom::error::ContextError<Input<'a>> for Error<'a> {
    fn add_context(input: Input<'a>, ctx: &'static str, mut other: Self) -> Self {
        other
            .contexts
            .push((transform_span(&input.0[..1]).unwrap(), ctx));
        other
    }
}

impl<'a> Error<'a> {
    pub fn from_error_kind(input: Input<'a>, kind: ErrorKind) -> Self {
        let mut inner = input.1.inner.borrow_mut();
        if let Some(ref mut inner) = *inner {
            match input.0[0].span.start.cmp(&inner.span.start) {
                Ordering::Equal => {
                    inner.errors.push(kind);
                }
                Ordering::Less => (),
                Ordering::Greater => {
                    *inner = BacktraceInner {
                        span: transform_span(&input.0[..1]).unwrap(),
                        errors: vec![kind],
                    };
                }
            }
        } else {
            *inner = Some(BacktraceInner {
                span: transform_span(&input.0[..1]).unwrap(),
                errors: vec![kind],
            })
        }

        Error {
            span: transform_span(&input.0[..1]).unwrap(),
            errors: vec![kind],
            contexts: vec![],
            backtrace: input.1,
        }
    }
}

impl From<fast_float::Error> for ErrorKind {
    fn from(_: fast_float::Error) -> Self {
        ErrorKind::Other("unable to parse float number")
    }
}

impl From<ParseIntError> for ErrorKind {
    fn from(err: ParseIntError) -> Self {
        let msg = match err.kind() {
            IntErrorKind::InvalidDigit => {
                "unable to parse number because it contains invalid characters"
            }
            IntErrorKind::PosOverflow => "unable to parse number because it positively overflowed",
            IntErrorKind::NegOverflow => "unable to parse number because it negatively overflowed",
            _ => "unable to parse number",
        };
        ErrorKind::Other(msg)
    }
}

pub fn display_parser_error(error: Error, source: &str) -> String {
    let inner = &*error.backtrace.inner.borrow();
    let inner = match inner {
        Some(inner) => inner,
        None => return String::new(),
    };

    let mut labels = vec![];

    // Plain text error has the highest priority. Only display it if exists.
    for kind in &inner.errors {
        if let ErrorKind::Other(msg) = kind {
            labels = vec![(inner.span, msg.to_string())];
            break;
        }
    }

    // List all expected tokens in alternative branches.
    if labels.is_empty() {
        let expected_tokens = error
            .errors
            .iter()
            .chain(&inner.errors)
            .filter_map(|kind| match kind {
                ErrorKind::ExpectToken(Eoi) => None,
                ErrorKind::ExpectToken(token) if token.is_keyword() => {
                    Some(format!("`{:?}`", token))
                }
                ErrorKind::ExpectToken(token) => Some(format!("<{:?}>", token)),
                ErrorKind::ExpectText(text) => Some(format!("`{}`", text)),
                _ => None,
            })
            .unique()
            .collect::<Vec<_>>();

        let mut msg = String::new();
        let mut iter = expected_tokens.iter().enumerate().peekable();
        while let Some((i, error)) = iter.next() {
            if i == MAX_DISPLAY_ERROR_COUNT {
                let more = expected_tokens
                    .len()
                    .saturating_sub(MAX_DISPLAY_ERROR_COUNT);
                write!(msg, ", or {} more ...", more).unwrap();
                break;
            } else if i == 0 {
                msg += "expected ";
            } else if iter.peek().is_none() && i == 1 {
                msg += " or ";
            } else if iter.peek().is_none() {
                msg += ", or ";
            } else {
                msg += ", ";
            }
            msg += error;
        }

        labels = vec![(inner.span, msg)];
    }

    // Append contexts as secondary labels.
    labels.extend(
        error
            .contexts
            .iter()
            .map(|(span, msg)| (*span, format!("while parsing {}", msg))),
    );

    pretty_print_error(source, labels)
}