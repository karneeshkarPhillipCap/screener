use crate::data::Bars;
use crate::indicators;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PineError {
    #[error("{0}")]
    Syntax(String),
    #[error("{0}")]
    Name(String),
    #[error("{0}")]
    Eval(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Num(f64),
    Name(String),
    Unary {
        op: char,
        operand: Box<Node>,
    },
    Not(Box<Node>),
    Bin {
        op: char,
        left: Box<Node>,
        right: Box<Node>,
    },
    Compare {
        op: String,
        left: Box<Node>,
        right: Box<Node>,
    },
    Bool {
        op: String,
        left: Box<Node>,
        right: Box<Node>,
    },
    Call {
        name: String,
        args: Vec<Node>,
        col: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Num,
    Name,
    Op,
    Lp,
    Rp,
    Comma,
    End,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    value: String,
    col: usize,
}

fn tokenize(expr: &str) -> Result<Vec<Token>, PineError> {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            let mut saw_dot = ch == '.';
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && !saw_dot)) {
                if chars[i] == '.' {
                    saw_dot = true;
                }
                i += 1;
            }
            out.push(Token {
                kind: TokenKind::Num,
                value: chars[start..i].iter().collect(),
                col: start,
            });
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token {
                kind: TokenKind::Name,
                value: chars[start..i].iter().collect(),
                col: start,
            });
            continue;
        }
        if i + 1 < chars.len() {
            let two: String = chars[i..i + 2].iter().collect();
            if [">=", "<=", "==", "!="].contains(&two.as_str()) {
                out.push(Token {
                    kind: TokenKind::Op,
                    value: two,
                    col: i,
                });
                i += 2;
                continue;
            }
        }
        let kind = match ch {
            '(' => TokenKind::Lp,
            ')' => TokenKind::Rp,
            ',' => TokenKind::Comma,
            '+' | '-' | '*' | '/' | '>' | '<' => TokenKind::Op,
            _ => {
                return Err(PineError::Syntax(format!(
                    "Unexpected character {ch:?} at column {i}"
                )));
            }
        };
        out.push(Token {
            kind,
            value: ch.to_string(),
            col: i,
        });
        i += 1;
    }
    out.push(Token {
        kind: TokenKind::End,
        value: String::new(),
        col: chars.len(),
    });
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn consume(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    fn expect(&mut self, kind: TokenKind, value: Option<&str>) -> Result<Token, PineError> {
        let tok = self.peek();
        if tok.kind != kind || value.is_some_and(|v| tok.value != v) {
            let expected = value.unwrap_or("token");
            return Err(PineError::Syntax(format!(
                "Expected {expected:?} at column {}, got {:?}",
                tok.col, tok.value
            )));
        }
        Ok(self.consume())
    }

    fn parse(mut self) -> Result<Node, PineError> {
        let node = self.parse_or()?;
        if self.peek().kind != TokenKind::End {
            return Err(PineError::Syntax(format!(
                "Unexpected {:?} at column {}",
                self.peek().value,
                self.peek().col
            )));
        }
        Ok(node)
    }

    fn parse_or(&mut self) -> Result<Node, PineError> {
        let mut left = self.parse_and()?;
        while self.peek().kind == TokenKind::Name && self.peek().value == "or" {
            self.consume();
            let right = self.parse_and()?;
            left = Node::Bool {
                op: "or".to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Node, PineError> {
        let mut left = self.parse_not()?;
        while self.peek().kind == TokenKind::Name && self.peek().value == "and" {
            self.consume();
            let right = self.parse_not()?;
            left = Node::Bool {
                op: "and".to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Node, PineError> {
        if self.peek().kind == TokenKind::Name && self.peek().value == "not" {
            self.consume();
            return Ok(Node::Not(Box::new(self.parse_not()?)));
        }
        self.parse_compare()
    }

    fn parse_compare(&mut self) -> Result<Node, PineError> {
        let left = self.parse_add()?;
        let tok = self.peek().clone();
        if tok.kind == TokenKind::Op
            && [">", ">=", "<", "<=", "==", "!="].contains(&tok.value.as_str())
        {
            self.consume();
            let right = self.parse_add()?;
            return Ok(Node::Compare {
                op: tok.value,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Node, PineError> {
        let mut left = self.parse_mul()?;
        while self.peek().kind == TokenKind::Op && ["+", "-"].contains(&self.peek().value.as_str())
        {
            let op = self.consume().value.chars().next().unwrap();
            let right = self.parse_mul()?;
            left = Node::Bin {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Node, PineError> {
        let mut left = self.parse_unary()?;
        while self.peek().kind == TokenKind::Op && ["*", "/"].contains(&self.peek().value.as_str())
        {
            let op = self.consume().value.chars().next().unwrap();
            let right = self.parse_unary()?;
            left = Node::Bin {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Node, PineError> {
        if self.peek().kind == TokenKind::Op && ["+", "-"].contains(&self.peek().value.as_str()) {
            let op = self.consume().value.chars().next().unwrap();
            return Ok(Node::Unary {
                op,
                operand: Box::new(self.parse_unary()?),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Node, PineError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Num => {
                self.consume();
                Ok(Node::Num(tok.value.parse().map_err(|_| {
                    PineError::Syntax(format!(
                        "Invalid number {:?} at column {}",
                        tok.value, tok.col
                    ))
                })?))
            }
            TokenKind::Lp => {
                self.consume();
                let node = self.parse_or()?;
                self.expect(TokenKind::Rp, None)?;
                Ok(node)
            }
            TokenKind::Name => {
                self.consume();
                if self.peek().kind == TokenKind::Lp {
                    self.consume();
                    let mut args = Vec::new();
                    if self.peek().kind != TokenKind::Rp {
                        args.push(self.parse_or()?);
                        while self.peek().kind == TokenKind::Comma {
                            self.consume();
                            args.push(self.parse_or()?);
                        }
                    }
                    self.expect(TokenKind::Rp, None)?;
                    return Ok(Node::Call {
                        name: tok.value,
                        args,
                        col: tok.col,
                    });
                }
                if tok.value == "true" {
                    return Ok(Node::Num(1.0));
                }
                if tok.value == "false" {
                    return Ok(Node::Num(0.0));
                }
                Ok(Node::Name(tok.value))
            }
            _ => Err(PineError::Syntax(format!(
                "Unexpected token {:?} at column {}",
                tok.value, tok.col
            ))),
        }
    }
}

pub fn parse(expr: &str) -> Result<Node, PineError> {
    if expr.trim().is_empty() {
        return Err(PineError::Syntax("Empty expression".to_string()));
    }
    Parser {
        tokens: tokenize(expr)?,
        pos: 0,
    }
    .parse()
}

pub fn evaluate(node: &Node, bars: &Bars) -> Result<Vec<f64>, PineError> {
    if bars.is_empty() {
        return Ok(Vec::new());
    }
    eval(node, bars)
}

fn scalar(value: f64, len: usize) -> Vec<f64> {
    vec![value; len]
}

fn as_bool(value: f64) -> bool {
    value != 0.0
}

fn eval(node: &Node, bars: &Bars) -> Result<Vec<f64>, PineError> {
    let len = bars.len();
    match node {
        Node::Num(value) => Ok(scalar(*value, len)),
        Node::Name(name) => bars
            .series(name)
            .ok_or_else(|| PineError::Name(format!("Unknown identifier: {name:?}"))),
        Node::Unary { op, operand } => {
            let values = eval(operand, bars)?;
            Ok(match op {
                '-' => values.into_iter().map(|v| -v).collect(),
                '+' => values,
                _ => return Err(PineError::Syntax(format!("Unknown operator: {op:?}"))),
            })
        }
        Node::Not(operand) => Ok(eval(operand, bars)?
            .into_iter()
            .map(|v| (!as_bool(v)) as i32 as f64)
            .collect()),
        Node::Bin { op, left, right } => {
            let left = eval(left, bars)?;
            let right = eval(right, bars)?;
            Ok(left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    _ => f64::NAN,
                })
                .collect())
        }
        Node::Compare { op, left, right } => {
            let left = eval(left, bars)?;
            let right = eval(right, bars)?;
            Ok(left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| {
                    let hit = match op.as_str() {
                        ">" => a > b,
                        ">=" => a >= b,
                        "<" => a < b,
                        "<=" => a <= b,
                        "==" => a == b,
                        "!=" => a != b,
                        _ => false,
                    };
                    hit as i32 as f64
                })
                .collect())
        }
        Node::Bool { op, left, right } => {
            let left = eval(left, bars)?;
            let right = eval(right, bars)?;
            Ok(left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| match op.as_str() {
                    "and" => (as_bool(*a) && as_bool(*b)) as i32 as f64,
                    "or" => (as_bool(*a) || as_bool(*b)) as i32 as f64,
                    _ => 0.0,
                })
                .collect())
        }
        Node::Call { name, args, col } => eval_call(name, args, *col, bars),
    }
}

fn int_literal(node: &Node, func: &str, arg: &str) -> Result<usize, PineError> {
    let Node::Num(value) = node else {
        return Err(PineError::Syntax(format!(
            "{func}() argument {arg:?} must be an integer literal"
        )));
    };
    let n = *value as usize;
    if *value <= 0.0 || n as f64 != *value {
        return Err(PineError::Syntax(format!(
            "{func}() argument {arg:?} must be a positive integer, got {value}"
        )));
    }
    Ok(n)
}

fn eval_call(name: &str, args: &[Node], col: usize, bars: &Bars) -> Result<Vec<f64>, PineError> {
    match name {
        "sma" | "ema" | "rsi" | "highest" | "lowest" => {
            if args.len() != 2 {
                return Err(PineError::Syntax(format!(
                    "{name}() takes 2 arguments (source, length), got {}",
                    args.len()
                )));
            }
            let source = eval(&args[0], bars)?;
            let length = int_literal(&args[1], name, "length")?;
            Ok(match name {
                "sma" => indicators::sma(&source, length),
                "ema" => indicators::ema(&source, length),
                "rsi" => indicators::rsi(&source, length),
                "highest" => indicators::highest(&source, length),
                "lowest" => indicators::lowest(&source, length),
                _ => unreachable!(),
            })
        }
        "atr" => {
            if args.len() != 1 {
                return Err(PineError::Syntax(format!(
                    "atr() takes 1 argument (length), got {}",
                    args.len()
                )));
            }
            let length = int_literal(&args[0], "atr", "length")?;
            Ok(indicators::atr(bars, length))
        }
        "crossover" | "crossunder" => {
            if args.len() != 2 {
                return Err(PineError::Syntax(format!(
                    "{name}() takes 2 arguments, got {}",
                    args.len()
                )));
            }
            let a = eval(&args[0], bars)?;
            let b = eval(&args[1], bars)?;
            Ok(if name == "crossover" {
                indicators::crossover(&a, &b)
            } else {
                indicators::crossunder(&a, &b)
            })
        }
        _ => Err(PineError::Name(format!(
            "Unknown function: {name:?} at column {col}"
        ))),
    }
}

pub fn required_lookback(node: &Node) -> usize {
    fn visit(node: &Node, max_len: &mut usize) {
        match node {
            Node::Call { name, args, .. } => {
                if ["sma", "ema", "rsi", "highest", "lowest"].contains(&name.as_str())
                    && args.len() == 2
                    && let Node::Num(n) = args[1]
                {
                    *max_len = (*max_len).max(n as usize);
                }
                if name == "atr"
                    && args.len() == 1
                    && let Node::Num(n) = args[0]
                {
                    *max_len = (*max_len).max(n as usize);
                }
                if ["crossover", "crossunder"].contains(&name.as_str()) {
                    *max_len = (*max_len).max(1);
                }
                for arg in args {
                    visit(arg, max_len);
                }
            }
            Node::Bin { left, right, .. }
            | Node::Compare { left, right, .. }
            | Node::Bool { left, right, .. } => {
                visit(left, max_len);
                visit(right, max_len);
            }
            Node::Unary { operand, .. } | Node::Not(operand) => visit(operand, max_len),
            Node::Num(_) | Node::Name(_) => {}
        }
    }
    let mut max_len = 0;
    visit(node, &mut max_len);
    max_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Bar, Bars};
    use chrono::NaiveDate;

    fn bars() -> Bars {
        let mut rows = Vec::new();
        for i in 0..5 {
            rows.push(Bar {
                date: NaiveDate::from_ymd_opt(2024, 1, 1 + i).unwrap(),
                open: 10.0 + i as f64,
                high: 11.0 + i as f64,
                low: 9.0 + i as f64,
                close: 10.0 + i as f64,
                volume: 100.0,
                adj_close: None,
                dividend: None,
            });
        }
        Bars::new(rows)
    }

    #[test]
    fn parses_and_evaluates_sma_comparison() {
        let ast = parse("close > sma(close, 3)").unwrap();
        assert_eq!(required_lookback(&ast), 3);
        let out = evaluate(&ast, &bars()).unwrap();
        assert_eq!(out, vec![0.0, 0.0, 1.0, 1.0, 1.0]);
    }
}
