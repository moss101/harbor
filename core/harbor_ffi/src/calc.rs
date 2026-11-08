//! Exact arithmetic for questions that are nothing but an expression
//! ("what is 3-7*5"). A small chat model gets operator precedence and
//! signs wrong often enough (it answered 16 here) that sending pure
//! arithmetic to it is a wrong answer waiting to happen; the calculator
//! is always right and costs no tokens.
//!
//! Anything that is not a pure expression — words, units, variables —
//! returns `None` and goes to the model as before.

/// `executed_on` for answers the calculator produced.
pub const CALCULATOR: &str = "calculator";

/// The answer text when `question` is a pure arithmetic expression, e.g.
/// `"3 - 7 × 5 = -32"`; `None` otherwise (including division by zero and
/// results too large to state exactly).
pub fn arithmetic_answer(question: &str) -> Option<String> {
    let expr = expression_of(question)?;
    let tokens = tokenize(&expr)?;
    // At least one binary operator: a bare number is not a calculation.
    if !tokens.iter().skip(1).any(|t| matches!(t, Tok::Op(_))) {
        return None;
    }
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let value = p.sum()?;
    if p.pos != tokens.len() || !value.is_finite() || value.abs() >= 1e15 {
        return None;
    }
    Some(format!("{} = {}", show_expr(&tokens), show_num(value)))
}

/// Strip the question wording around an expression; `None` when anything
/// but an expression remains.
fn expression_of(question: &str) -> Option<String> {
    let mut s = question.trim().to_lowercase();
    for prefix in [
        "what is",
        "what's",
        "whats",
        "calculate",
        "compute",
        "evaluate",
        "solve",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start().to_string();
            break;
        }
    }
    let s = s
        .trim_end_matches(['?', '.', '!', ' '])
        .trim_end_matches('=')
        .trim()
        .to_string();
    let allowed = |c: char| c.is_ascii_digit() || " .+-*/x×÷^()−".contains(c);
    (s.chars().any(|c| c.is_ascii_digit()) && s.chars().all(allowed)).then_some(s)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    Open,
    Close,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                out.push(Tok::Num(text.parse().ok()?));
            }
            '+' => (out.push(Tok::Op('+')), i += 1).1,
            '-' | '−' => (out.push(Tok::Op('-')), i += 1).1,
            '*' | 'x' | '×' => (out.push(Tok::Op('*')), i += 1).1,
            '/' | '÷' => (out.push(Tok::Op('/')), i += 1).1,
            '^' => (out.push(Tok::Op('^')), i += 1).1,
            '(' => (out.push(Tok::Open), i += 1).1,
            ')' => (out.push(Tok::Close), i += 1).1,
            _ => return None,
        }
    }
    Some(out)
}

/// Precedence climbing: sum > product > power (right-assoc) > unary.
struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Tok> {
        self.tokens.get(self.pos).copied()
    }

    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek() {
            self.pos += 1;
            let r = self.product()?;
            v = if op == '+' { v + r } else { v - r };
        }
        Some(v)
    }

    fn product(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek() {
            self.pos += 1;
            let r = self.power()?;
            if op == '/' && r == 0.0 {
                return None;
            }
            v = if op == '*' { v * r } else { v / r };
        }
        Some(v)
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            let exp = self.power()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            Tok::Op('-') => {
                self.pos += 1;
                Some(-self.unary()?)
            }
            Tok::Op('+') => {
                self.pos += 1;
                self.unary()
            }
            Tok::Num(n) => {
                self.pos += 1;
                Some(n)
            }
            Tok::Open => {
                self.pos += 1;
                let v = self.sum()?;
                (self.peek()? == Tok::Close).then(|| self.pos += 1)?;
                Some(v)
            }
            _ => None,
        }
    }
}

fn show_expr(tokens: &[Tok]) -> String {
    let mut out = String::new();
    for (i, t) in tokens.iter().enumerate() {
        let unary = i == 0 || matches!(tokens[i - 1], Tok::Op(_) | Tok::Open);
        match t {
            Tok::Num(n) => out.push_str(&show_num(*n)),
            Tok::Op(op) if unary => out.push(*op),
            Tok::Op(op) => {
                let shown = match op {
                    '*' => '×',
                    '/' => '÷',
                    other => *other,
                };
                out.push(' ');
                out.push(shown);
                out.push(' ');
            }
            Tok::Open => out.push('('),
            Tok::Close => out.push(')'),
        }
    }
    out
}

/// Integers exactly; other values to at most 10 decimal places, without
/// trailing zeros.
fn show_num(v: f64) -> String {
    if v == v.trunc() {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::arithmetic_answer as calc;

    #[test]
    fn precedence_and_sign_are_exact() {
        // The question a 2B model answered "16".
        assert_eq!(calc("what is 3-7*5").as_deref(), Some("3 - 7 × 5 = -32"));
        assert_eq!(calc("(3-7)*5?").as_deref(), Some("(3 - 7) × 5 = -20"));
        assert_eq!(calc("2^3^2").as_deref(), Some("2 ^ 3 ^ 2 = 512"));
        assert_eq!(calc("-2 + 5").as_deref(), Some("-2 + 5 = 3"));
        assert_eq!(calc("Calculate 10 ÷ 4 =").as_deref(), Some("10 ÷ 4 = 2.5"));
        assert_eq!(calc("1/3").as_deref(), Some("1 ÷ 3 = 0.3333333333"));
        assert_eq!(calc("what's 12 x 12").as_deref(), Some("12 × 12 = 144"));
    }

    #[test]
    fn anything_but_a_pure_expression_goes_to_the_model() {
        for q in [
            "what is the capital of France",
            "what is 3 apples plus 2",
            "42",
            "what is 2024",
            "1/0",
            "(1+2",
            "3 % 2",
            "x",
            "",
        ] {
            assert_eq!(calc(q), None, "{q}");
        }
    }
}
