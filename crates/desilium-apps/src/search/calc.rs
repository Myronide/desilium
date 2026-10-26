//! Built-in calculator provider: a bounded recursive-descent parser for
//! `+ - * / % ^`, unary signs and parentheses. Pure; never panics.

const MAX_INPUT_CHARS: usize = 256;
const MAX_DEPTH: usize = 64;
const DISPLAY_DECIMALS: usize = 10;
/// Integers below this magnitude are shown without a fractional part.
const INTEGER_DISPLAY_LIMIT: f64 = 1e15;
const BINARY_OPERATORS: [char; 6] = ['+', '-', '*', '/', '%', '^'];

/// Why an expression has no value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalcError {
    /// The input has more than 256 characters.
    #[error("expression is too long")]
    TooLong,
    /// Parentheses or powers are nested more than 64 levels deep.
    #[error("expression is nested too deeply")]
    TooDeep,
    /// Unexpected or missing input at this character position.
    #[error("syntax error at position {0}")]
    Syntax(usize),
    /// Division or modulo by zero.
    #[error("division by zero")]
    DivisionByZero,
    /// Overflow, or a result that is not a finite number.
    #[error("result is not a finite number")]
    NotFinite,
}

/// Evaluates an arithmetic expression.
pub fn evaluate(input: &str) -> Result<f64, CalcError> {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() > MAX_INPUT_CHARS {
        return Err(CalcError::TooLong);
    }
    let mut parser = Parser {
        chars,
        pos: 0,
        depth: 0,
    };
    let value = parser.expression()?;
    parser.skip_spaces();
    if parser.pos < parser.chars.len() {
        return Err(CalcError::Syntax(parser.pos));
    }
    Ok(value)
}

/// Formats a result for display: integers plainly, other values with up to ten decimals.
pub fn format(value: f64) -> String {
    // Adding zero turns -0.0 into 0.0 so the display never shows "-0".
    let value = value + 0.0;
    if value.fract() == 0.0 && value.abs() < INTEGER_DISPLAY_LIMIT {
        return format!("{value:.0}");
    }
    let text = format!("{:.*}", DISPLAY_DECIMALS, value);
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// The formatted result when `input` is an expression with a binary operator that
/// evaluates. Plain numbers and bare signs (`-5`) return `None`, as they are not sums.
pub fn answer(input: &str) -> Option<String> {
    if !has_binary_operator(input) {
        return None;
    }
    evaluate(input).ok().map(format)
}

fn has_binary_operator(input: &str) -> bool {
    let significant: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
    significant.windows(2).any(|pair| match pair {
        [left, op] => BINARY_OPERATORS.contains(op) && (left.is_ascii_digit() || *left == ')'),
        _ => false,
    })
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn expression(&mut self) -> Result<f64, CalcError> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.bump();
                    value = checked(value + self.term()?)?;
                }
                Some('-') => {
                    self.bump();
                    value = checked(value - self.term()?)?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.unary()?;
        while let Some(op @ ('*' | '/' | '%')) = self.peek() {
            self.bump();
            let rhs = self.unary()?;
            value = apply(op, value, rhs)?;
        }
        Ok(value)
    }

    /// Unary signs bind looser than `^`, so `-2 ^ 2` is `-(2 ^ 2)`.
    fn unary(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some('-') => {
                self.bump();
                let inner = self.nested(Self::unary)?;
                Ok(-inner)
            }
            Some('+') => {
                self.bump();
                self.nested(Self::unary)
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.primary()?;
        if self.peek() != Some('^') {
            return Ok(base);
        }
        self.bump();
        let exponent = self.nested(Self::unary)?;
        checked(base.powf(exponent))
    }

    fn primary(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some('(') => {
                self.bump();
                let value = self.nested(Self::expression)?;
                if self.peek() != Some(')') {
                    return Err(CalcError::Syntax(self.pos));
                }
                self.bump();
                Ok(value)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.number(),
            _ => Err(CalcError::Syntax(self.pos)),
        }
    }

    fn number(&mut self) -> Result<f64, CalcError> {
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_digit() || *c == '.')
        {
            self.bump();
        }
        let text: String = self
            .chars
            .get(start..self.pos)
            .unwrap_or_default()
            .iter()
            .collect();
        let value = text.parse::<f64>().map_err(|_| CalcError::Syntax(start))?;
        checked(value)
    }

    /// Runs one recursive step, refusing to go deeper than [`MAX_DEPTH`].
    fn nested(&mut self, step: fn(&mut Self) -> Result<f64, CalcError>) -> Result<f64, CalcError> {
        if self.depth >= MAX_DEPTH {
            return Err(CalcError::TooDeep);
        }
        self.depth += 1;
        let result = step(self);
        self.depth -= 1;
        result
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_spaces();
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn skip_spaces(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.bump();
        }
    }
}

fn apply(op: char, lhs: f64, rhs: f64) -> Result<f64, CalcError> {
    let result = match op {
        '*' => lhs * rhs,
        '/' | '%' if rhs == 0.0 => return Err(CalcError::DivisionByZero),
        '/' => lhs / rhs,
        _ => lhs % rhs,
    };
    checked(result)
}

fn checked(value: f64) -> Result<f64, CalcError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CalcError::NotFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_table() {
        let cases: &[(&str, f64)] = &[
            ("24 * 7", 168.0),
            ("1 + 2 * 3", 7.0),
            ("(1 + 2) * 3", 9.0),
            ("10 - 4 - 3", 3.0),
            ("2 ^ 3 ^ 2", 512.0),
            ("-2 ^ 2", -4.0),
            ("--3", 3.0),
            ("7 % 4", 3.0),
            ("1 / 4", 0.25),
            ("  .5 + 1.5  ", 2.0),
            ("2 * (3 + (4 - 1)) / 3", 4.0),
        ];
        for (input, expected) in cases {
            let got = evaluate(input).unwrap_or_else(|err| panic!("{input}: {err}"));
            assert!((got - expected).abs() < 1e-12, "{input} gave {got}");
        }
    }

    #[test]
    fn error_table() {
        assert_eq!(evaluate("1 / 0"), Err(CalcError::DivisionByZero));
        assert_eq!(evaluate("5 % 0"), Err(CalcError::DivisionByZero));
        assert_eq!(evaluate("1 +"), Err(CalcError::Syntax(3)));
        assert_eq!(evaluate("(1 + 2"), Err(CalcError::Syntax(6)));
        assert_eq!(evaluate("1 2"), Err(CalcError::Syntax(2)));
        assert_eq!(evaluate("1.2.3"), Err(CalcError::Syntax(0)));
        assert_eq!(evaluate("10 ^ 400"), Err(CalcError::NotFinite));
        assert_eq!(evaluate(&"1+".repeat(200)), Err(CalcError::TooLong));
        let deep = format!("{}1{}", "(".repeat(100), ")".repeat(100));
        assert_eq!(evaluate(&deep), Err(CalcError::TooDeep));
    }

    #[test]
    fn formatting_table() {
        let cases: &[(f64, &str)] = &[
            (168.0, "168"),
            (-0.0, "0"),
            (0.25, "0.25"),
            (1.0 / 3.0, "0.3333333333"),
            (2.5e15, "2500000000000000"),
            (-7.0, "-7"),
        ];
        for (value, expected) in cases {
            assert_eq!(format(*value), *expected, "value {value}");
        }
    }

    #[test]
    fn answer_requires_a_binary_operator() {
        assert_eq!(answer("24 * 7").as_deref(), Some("168"));
        assert_eq!(answer("-5"), None);
        assert_eq!(answer("42"), None);
        assert_eq!(answer("firefox"), None);
        assert_eq!(answer("2 +"), None);
    }
}
