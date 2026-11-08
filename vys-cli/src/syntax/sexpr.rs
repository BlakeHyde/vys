use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Atom(String),
    String(String),
    List(Vec<Expr>),
}

impl Expr {
    pub fn atom(&self) -> Option<&str> {
        match self {
            Self::Atom(s) => Some(s),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&[Expr]> {
        match self {
            Self::List(xs) => Some(xs),
            _ => None,
        }
    }

    pub fn head(&self) -> Option<&str> {
        self.list()?.first()?.atom()
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Atom(s) => write!(f, "{s}"),
            Self::String(s) => {
                write!(f, "\"")?;
                for c in s.chars() {
                    match c {
                        '\\' => write!(f, "\\\\")?,
                        '"' => write!(f, "\\\"")?,
                        '\n' => write!(f, "\\n")?,
                        '\r' => write!(f, "\\r")?,
                        '\t' => write!(f, "\\t")?,
                        c => write!(f, "{c}")?,
                    }
                }
                write!(f, "\"")
            }
            Self::List(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
        }
    }
}

pub fn parse(input: &str, source: &str) -> Result<Vec<Expr>, String> {
    struct Parser<'a> {
        text: &'a str,
        source: &'a str,
        pos: usize,
    }
    impl Parser<'_> {
        fn error(&self, message: &str) -> String {
            let prefix = &self.text[..self.pos];
            let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
            let col = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            format!("{}:{line}:{col}: {message}", self.source)
        }
        fn peek(&self) -> Option<char> {
            self.text[self.pos..].chars().next()
        }
        fn next(&mut self) -> Option<char> {
            let c = self.peek()?;
            self.pos += c.len_utf8();
            Some(c)
        }
        fn space(&mut self) {
            loop {
                while self.peek().is_some_and(char::is_whitespace) {
                    self.next();
                }
                if self.peek() != Some(';') {
                    break;
                }
                while self.next().is_some_and(|c| c != '\n') {}
            }
        }
        fn expr(&mut self, depth: usize) -> Result<Expr, String> {
            if depth > 256 {
                return Err(self.error("expression nesting exceeds 256 levels"));
            }
            self.space();
            match self.next() {
                Some('(') => {
                    let mut xs = Vec::new();
                    loop {
                        self.space();
                        match self.peek() {
                            Some(')') => {
                                self.next();
                                return Ok(Expr::List(xs));
                            }
                            None => return Err(self.error("unclosed list")),
                            _ => xs.push(self.expr(depth + 1)?),
                        }
                    }
                }
                Some(')') => Err(self.error("unexpected ')'")),
                Some('"') => {
                    let mut s = String::new();
                    loop {
                        match self.next() {
                            Some('"') => return Ok(Expr::String(s)),
                            Some('\\') => s.push(match self.next() {
                                Some('n') => '\n',
                                Some('r') => '\r',
                                Some('t') => '\t',
                                Some('"') => '"',
                                Some('\\') => '\\',
                                _ => return Err(self.error("unsupported string escape")),
                            }),
                            Some(c) => s.push(c),
                            None => return Err(self.error("unterminated string")),
                        }
                    }
                }
                Some(c) => {
                    let mut s = String::from(c);
                    while let Some(c) = self.peek() {
                        if c.is_whitespace() || matches!(c, '(' | ')' | ';' | '"') {
                            break;
                        }
                        s.push(self.next().unwrap());
                    }
                    Ok(Expr::Atom(s))
                }
                None => Err(self.error("expected expression")),
            }
        }
    }
    let mut p = Parser {
        text: input,
        source,
        pos: 0,
    };
    let mut forms = Vec::new();
    p.space();
    while p.peek().is_some() {
        forms.push(p.expr(0)?);
        p.space();
    }
    Ok(forms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_comments_and_unicode_round_trip() {
        let forms = parse("; hi\n(file \"hé\\n\\\"\\\\\")", "test").unwrap();
        assert_eq!(parse(&forms[0].to_string(), "test").unwrap(), forms);
    }

    #[test]
    fn syntax_errors_have_locations() {
        assert!(
            parse("\n(module x", "test")
                .unwrap_err()
                .starts_with("test:2:")
        );
        for text in [")", "(x", "\"oops", "\"\\q\""] {
            assert!(parse(text, "test").is_err(), "{text}");
        }
    }
}
