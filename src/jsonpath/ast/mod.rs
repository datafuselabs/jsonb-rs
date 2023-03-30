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

mod expr;
mod json_path;

pub use expr::*;
pub use json_path::*;

use std::fmt::Display;
use std::fmt::Formatter;

use crate::jsonpath::exception::Span;

// Identifier of field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub name: String,
    pub quote: Option<char>,
    pub span: Span,
}

impl Identifier {
    pub fn is_quoted(&self) -> bool {
        self.quote.is_some()
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(c) = self.quote {
            write!(f, "{}", c)?;
            write!(f, "{}", self.name)?;
            write!(f, "{}", c)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// Write input items into `1, 2, 3`
pub(crate) fn write_comma_separated_list(
    f: &mut Formatter<'_>,
    items: impl IntoIterator<Item = impl Display>,
) -> std::fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

/// Write input items into `'a', 'b', 'c'`
pub(crate) fn write_quoted_comma_separated_list(
    f: &mut Formatter<'_>,
    items: impl IntoIterator<Item = impl Display>,
) -> std::fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "'{item}'")?;
    }
    Ok(())
}