//! The GTS regular-expression profile (README §11.0.1, ADR-0006).
//!
//! Checks the shared ECMA-262 `u`/RE2 syntax subset and support bounds without
//! executing expressions. A dedicated parser is needed because engine parsers
//! accept extra syntax, including nested classes and set operations.

use std::fmt;

/// Maximum length in code points after expanding counted repetitions.
pub const MAX_EXPANDED_LEN: usize = 4096;
/// Deepest supported nesting of groups.
pub const MAX_GROUP_NESTING: usize = 32;
/// Largest supported repetition count, also as the product of nested counts.
pub const MAX_REPEAT: u64 = 1000;

/// Why an expression is outside the profile or its support bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(String);

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unsupported {}

/// Whether `expression` is supported, as `format: "regex"` asserts.
#[must_use]
pub fn is_supported(expression: &str) -> bool {
    check(expression).is_ok()
}

/// Checks that `expression` is in the profile and within its bounds.
///
/// # Errors
/// Why it is not.
pub fn check(expression: &str) -> Result<(), Unsupported> {
    Parser::new(expression)?.parse()
}

/// What a parsed node contributes to the support bounds.
#[derive(Clone, Copy)]
struct Size {
    /// Expanded length in code points, saturating.
    expanded: u64,
    /// Largest product of counted-repetition factors along a nesting path.
    product: u64,
}

impl Size {
    const fn spelled(len: usize) -> Self {
        Self {
            expanded: len as u64,
            product: 1,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            expanded: self.expanded.saturating_add(next.expanded),
            product: self.product.max(next.product),
        }
    }
}

const SYNTAX_CHARS: &str = r"^$\.*+?()[]{}|";

struct Parser {
    chars: Vec<char>,
    pos: usize,
    /// Groups currently open.
    depth: usize,
}

impl Parser {
    /// Reject overlong source early: expanded length is at least source length.
    fn new(expression: &str) -> Result<Self, Unsupported> {
        if expression.chars().nth(MAX_EXPANDED_LEN).is_some() {
            return Err(Unsupported(format!(
                "it is longer than {MAX_EXPANDED_LEN} code points"
            )));
        }
        Ok(Self {
            chars: expression.chars().collect(),
            pos: 0,
            depth: 0,
        })
    }

    fn parse(&mut self) -> Result<(), Unsupported> {
        let size = self.disjunction()?;
        if let Some(&c) = self.chars.get(self.pos) {
            // Only an unmatched `)` stops a top-level disjunction early.
            return Err(self.error(format!("unmatched '{c}'")));
        }
        if size.expanded > MAX_EXPANDED_LEN as u64 {
            return Err(Unsupported(format!(
                "its expanded length exceeds {MAX_EXPANDED_LEN} code points"
            )));
        }
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn error(&self, reason: impl fmt::Display) -> Unsupported {
        Unsupported(format!("{reason} at offset {}", self.pos))
    }

    /// Stops at the end of input or before an unmatched `)`.
    fn disjunction(&mut self) -> Result<Size, Unsupported> {
        let mut size = self.alternative()?;
        while self.eat('|') {
            size = size.then(Size::spelled(1)).then(self.alternative()?);
        }
        Ok(size)
    }

    fn alternative(&mut self) -> Result<Size, Unsupported> {
        let mut size = Size::spelled(0);
        while let Some(c) = self.peek() {
            let term = match c {
                '|' | ')' => break,
                '^' | '$' => {
                    self.pos += 1;
                    if self.peek().is_some_and(is_quantifier_start) {
                        return Err(self.error(format!("quantified assertion '{c}'")));
                    }
                    Size::spelled(1)
                }
                _ => {
                    let atom = self.atom()?;
                    self.quantified(atom)?
                }
            };
            size = size.then(term);
            // Bail out early: lengths only grow.
            if size.expanded > MAX_EXPANDED_LEN as u64 {
                return Err(Unsupported(format!(
                    "its expanded length exceeds {MAX_EXPANDED_LEN} code points"
                )));
            }
        }
        Ok(size)
    }

    fn atom(&mut self) -> Result<Size, Unsupported> {
        let Some(c) = self.peek() else {
            return Err(self.error("unexpected end"));
        };
        match c {
            '(' => self.group(),
            '[' => self.class(),
            '\\' => {
                let start = self.pos;
                self.pos += 1;
                self.atom_escape()?;
                Ok(Size::spelled(self.pos - start))
            }
            '*' | '+' | '?' | '{' => Err(self.error(format!("quantifier '{c}' without operand"))),
            '}' | ']' => Err(self.error(format!("unescaped '{c}'"))),
            // `.` or a literal.
            _ => {
                self.pos += 1;
                Ok(Size::spelled(1))
            }
        }
    }

    fn group(&mut self) -> Result<Size, Unsupported> {
        let start = self.pos;
        self.pos += 1;
        if self.eat('?') && !self.eat(':') {
            return Err(Unsupported(format!(
                "only capturing and '(?:' groups are supported, at offset {start}"
            )));
        }
        let opening = self.pos - start;
        self.depth += 1;
        if self.depth > MAX_GROUP_NESTING {
            return Err(self.error(format!("groups nest deeper than {MAX_GROUP_NESTING}")));
        }
        let inner = self.disjunction()?;
        if !self.eat(')') {
            return Err(Unsupported(format!(
                "unclosed group opened at offset {start}"
            )));
        }
        self.depth -= 1;
        Ok(Size::spelled(opening).then(inner).then(Size::spelled(1)))
    }

    /// After `\` outside a class.
    fn atom_escape(&mut self) -> Result<(), Unsupported> {
        let Some(c) = self.peek() else {
            return Err(self.error("trailing backslash"));
        };
        if matches!(c, 'd' | 'D' | 'w' | 'W' | 's' | 'S') {
            self.pos += 1;
            return Ok(());
        }
        self.char_escape(false).map(|_| ())
    }

    /// After `\`: an escape denoting one code point, which it returns.
    fn char_escape(&mut self, in_class: bool) -> Result<char, Unsupported> {
        let Some(c) = self.peek() else {
            return Err(self.error("trailing backslash"));
        };
        self.pos += 1;
        let literal = match c {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'f' => '\u{c}',
            'v' => '\u{b}',
            'x' => {
                let digits: Option<u32> = (0..2)
                    .map(|i| self.peek_at(i).and_then(|d| d.to_digit(16)))
                    .try_fold(0, |acc, digit| Some(acc * 16 + digit?));
                let Some(code) = digits.and_then(char::from_u32) else {
                    return Err(self.error("'\\x' takes exactly two hex digits"));
                };
                self.pos += 2;
                code
            }
            '/' => '/',
            '-' if in_class => '-',
            _ if SYNTAX_CHARS.contains(c) => c,
            _ => return Err(self.error(format!("unsupported escape '\\{c}'"))),
        };
        Ok(literal)
    }

    fn class(&mut self) -> Result<Size, Unsupported> {
        let start = self.pos;
        self.pos += 1;
        self.eat('^');
        if self.peek() == Some(']') {
            return Err(self.error("a class cannot be empty or start with ']'"));
        }
        let first = self.pos;
        loop {
            let Some(c) = self.peek() else {
                return Err(Unsupported(format!(
                    "unclosed class opened at offset {start}"
                )));
            };
            if c == ']' {
                self.pos += 1;
                break;
            }
            if c == '-' && self.pos != first && self.peek_at(1) != Some(']') {
                // A range operator after a shorthand class, or a second one.
                return Err(self.error("'-' must be the first or last item of a class"));
            }
            let low = self.class_atom()?;
            if self.peek() == Some('-') && self.peek_at(1).is_some_and(|next| next != ']') {
                self.unambiguous_in_class()?;
                self.pos += 1;
                let high = self.class_atom()?;
                let (Some(low), Some(high)) = (low, high) else {
                    return Err(self.error("a shorthand class cannot bound a range"));
                };
                if low > high {
                    return Err(self.error(format!("reversed range '{low}-{high}'")));
                }
            }
        }
        Ok(Size::spelled(self.pos - start))
    }

    /// One class item; `None` for a shorthand class.
    fn class_atom(&mut self) -> Result<Option<char>, Unsupported> {
        let Some(c) = self.peek() else {
            return Err(self.error("unexpected end"));
        };
        match c {
            '[' => Err(self.error("unescaped '[' in a class")),
            '\\' => {
                self.pos += 1;
                if self
                    .peek()
                    .is_some_and(|e| matches!(e, 'd' | 'D' | 'w' | 'W' | 's' | 'S'))
                {
                    self.pos += 1;
                    return Ok(None);
                }
                self.char_escape(true).map(Some)
            }
            _ => {
                self.unambiguous_in_class()?;
                self.pos += 1;
                Ok(Some(c))
            }
        }
    }

    /// Reject `&&`, `--` and `~~`, which Rust reads as class set operations.
    fn unambiguous_in_class(&self) -> Result<(), Unsupported> {
        match self.peek() {
            Some(c @ ('&' | '-' | '~')) if self.peek_at(1) == Some(c) => {
                Err(self.error(format!("ambiguous '{c}{c}' in a class")))
            }
            _ => Ok(()),
        }
    }

    /// Applies an optional quantifier to an atom of the given size.
    fn quantified(&mut self, atom: Size) -> Result<Size, Unsupported> {
        let start = self.pos;
        let size = match self.peek() {
            Some('*' | '+' | '?') => {
                self.pos += 1;
                self.eat('?');
                Size::spelled(self.pos - start).then(atom)
            }
            Some('{') => {
                self.pos += 1;
                let min = self.count()?;
                let (factor, copies) = if self.eat('}') {
                    (min, min.max(1))
                } else if self.eat(',') {
                    if self.eat('}') {
                        (min, min + 1)
                    } else {
                        let max = self.count()?;
                        if !self.eat('}') {
                            return Err(self.error("malformed counted repetition"));
                        }
                        if min > max {
                            return Err(self.error(format!("reversed repetition {{{min},{max}}}")));
                        }
                        (max, max.max(1))
                    }
                } else {
                    return Err(self.error("malformed counted repetition"));
                };
                self.eat('?');
                let product = factor.max(1).saturating_mul(atom.product);
                if product > MAX_REPEAT {
                    return Err(self.error(format!(
                        "nested counted repetitions exceed {MAX_REPEAT} in total"
                    )));
                }
                Size {
                    expanded: ((self.pos - start) as u64)
                        .saturating_add(copies.saturating_mul(atom.expanded)),
                    product,
                }
            }
            _ => return Ok(atom),
        };
        if self.peek().is_some_and(is_quantifier_start) {
            return Err(self.error("stacked or possessive quantifier"));
        }
        Ok(size)
    }

    /// A repetition count of at most [`MAX_REPEAT`].
    fn count(&mut self) -> Result<u64, Unsupported> {
        let start = self.pos;
        let mut value: u64 = 0;
        while let Some(digit) = self.peek().and_then(|c| c.to_digit(10)) {
            value = value.saturating_mul(10).saturating_add(u64::from(digit));
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("malformed counted repetition"));
        }
        // RE2 reads `{01}` as literal text, not a repetition.
        if self.pos - start > 1 && self.chars[start] == '0' {
            return Err(Unsupported(format!(
                "repetition count at offset {start} has a leading zero"
            )));
        }
        if value > MAX_REPEAT {
            return Err(Unsupported(format!(
                "repetition count {value} at offset {start} exceeds {MAX_REPEAT}"
            )));
        }
        Ok(value)
    }
}

fn is_quantifier_start(c: char) -> bool {
    matches!(c, '*' | '+' | '?' | '{')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::non_ascii_literal)]
#[path = "regex_profile_test.rs"]
mod regex_profile_test;
