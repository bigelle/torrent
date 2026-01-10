use std::{borrow::Cow, fmt::Display};

use atoi::FromRadix10SignedChecked;
use thiserror::Error;

#[derive(PartialEq, Debug)]
pub enum Token<'a> {
    Int(i64),
    String(Cow<'a, [u8]>),
    BeginDict(usize), //Cumberbatch
    BeginList(usize),
    EndObject(usize),
}

#[derive(Debug)]
pub enum TokenKind {
    Int,
    String,
    BeginDict,
    BeginList,
    EndObject,
}

impl<'a> Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Int => write!(f, "Int"),
            Self::String => write!(f, "String"),
            Self::BeginDict => write!(f, "BeginDict"),
            Self::BeginList => write!(f, "BeginList"),
            Self::EndObject => write!(f, "EndObject"),
        }
    }
}

impl<'a> From<Token<'a>> for TokenKind {
    fn from(value: Token<'a>) -> Self {
        match value {
            Token::Int(_) => TokenKind::Int,
            Token::String(_) => TokenKind::String,
            Token::BeginDict(_) => TokenKind::BeginDict,
            Token::BeginList(_) => TokenKind::BeginList,
            Token::EndObject(_) => TokenKind::EndObject,
        }
    }
}

pub struct Decoder<'a> {
    src: &'a [u8],
    pos: usize,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("unfinished string at index {0}: expected {1} bytes, got {2}")]
    UnfinishedString(usize, usize, usize),
    #[error("unfinished int")]
    UnfinishedInt,
    #[error("unknown token: {0}")]
    UnknownToken(u8),
    #[error("decoder position out of buffer bounds")]
    PosOutOfBounds,
    #[error("token is too large")]
    TokenTooLarge,
    #[error("syntax error")]
    WrongSyntax,
    #[error("missing colon in string")]
    MissingColonInString,
}

impl<'a> Decoder<'a> {
    pub fn new(src: &'a [u8]) -> Decoder<'a> {
        Decoder { src: src, pos: 0 }
    }

    pub fn next_token(&mut self) -> Result<Token<'a>, DecodeError> {
        let (token, size) = self.peek_token()?;
        self.step_forward(size)?;
        Ok(token)
    }

    pub fn peek_token(&self) -> Result<(Token<'a>, usize), DecodeError> {
        match self.current_byte() {
            b'i' => self.give_int_token(),
            b'0'..=b'9' => self.give_string_token(),
            b'l' => Ok((Token::BeginList(self.pos), 1)),
            b'd' => Ok((Token::BeginDict(self.pos), 1)),
            b'e' => Ok((Token::EndObject(self.pos), 1)),
            v => Err(DecodeError::UnknownToken(v)),
        }
    }

    pub fn step_forward(&mut self, steps: usize) -> Result<(), DecodeError> {
        if self.pos + steps > self.src.len() {
            return Err(DecodeError::PosOutOfBounds);
        }
        self.pos += steps;
        Ok(())
    }

    fn give_int_token(&self) -> Result<(Token<'a>, usize), DecodeError> {
        let e_pos = match self.src[self.pos..].iter().position(|x| *x == b'e') {
            Some(pos) => pos,
            None => return Err(DecodeError::UnfinishedInt),
        };

        if e_pos - 1 > 12 {
            return Err(DecodeError::TokenTooLarge);
        }

        let (maybe_n, used) =
            i64::from_radix_10_signed_checked(&self.src[self.pos + 1..self.pos + 1 + e_pos]);

        match maybe_n {
            Some(n) => {
                if used != e_pos - 1 {
                    return Err(DecodeError::WrongSyntax);
                }
                Ok((Token::Int(n), e_pos + 1))
            }
            None => return Err(DecodeError::WrongSyntax),
        }
    }

    fn give_string_token(&self) -> Result<(Token<'a>, usize), DecodeError> {
        let col_pos = match self.src[self.pos..].iter().position(|x| *x == b':') {
            Some(pos) => pos,
            None => return Err(DecodeError::MissingColonInString),
        };

        if col_pos > 12 {
            // who needs a trillion of bytes?
            return Err(DecodeError::TokenTooLarge);
        }

        let (maybe_len, size) =
            u64::from_radix_10_signed_checked(&self.src[self.pos..self.pos + col_pos]);

        let size = match maybe_len {
            Some(len) => {
                if size != col_pos {
                    return Err(DecodeError::WrongSyntax);
                }
                len
            }
            None => {
                return Err(DecodeError::WrongSyntax);
            }
        } as usize;

        let have = self.src[self.pos + col_pos + 1..].len() as usize;
        if have < size {
            return Err(DecodeError::UnfinishedString(self.pos, size, have));
        }

        let token = Token::String(Cow::Borrowed(
            &self.src[self.pos + col_pos + 1..self.pos + col_pos + size + 1],
        ));

        Ok((token, col_pos + size + 1))
    }

    fn current_byte(&self) -> u8 {
        self.src[self.pos]
    }
}

#[cfg(test)]
mod test_decode {
    use super::*;

    #[test]
    fn single_valid_int_token() {
        let input = b"i4e";
        let mut dec = Decoder::new(input);

        assert_eq!(dec.next_token().unwrap(), Token::Int(4));
    }

    #[test]
    fn multiple_valid_int_token() {
        let input = b"i42ei6e";
        let mut dec = Decoder::new(input);

        assert_eq!(dec.next_token().unwrap(), Token::Int(42));

        assert_eq!(dec.next_token().unwrap(), Token::Int(6));
    }

    #[test]
    fn error_int_too_large() {
        let input = b"i1000000000000e"; // more than 999_999_999_999 by one
        let mut dec = Decoder::new(input);

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::TokenTooLarge
        ))
    }

    #[test]
    fn error_int_is_not_finished() {
        let input = b"i42"; // more than 999_999_999_999 by one
        let mut dec = Decoder::new(input);

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::UnfinishedInt
        ))
    }

    #[test]
    fn single_valid_string_token() {
        let input = b"4:test";
        let mut dec = Decoder::new(input);

        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"test"))
        );
    }

    #[test]
    fn multiple_valid_string_token() {
        let input = b"4:test3:foo";
        let mut dec = Decoder::new(input);

        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"test"))
        );

        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"foo"))
        );
    }

    #[test]
    fn error_string_too_large() {
        const PREFIX: &[u8] = b"1000000000000:";
        const TOTAL_LEN: usize = 1_000_000_014;

        let mut input = Vec::with_capacity(1_000_000_014);

        // prefix
        input.extend_from_slice(PREFIX);

        // fill with 's'
        input.resize(TOTAL_LEN, b's');

        let mut dec = Decoder::new(&input);

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::TokenTooLarge
        ));
    }

    #[test]
    fn error_string_is_not_finished() {
        let input = b"4:tes";
        let mut dec = Decoder::new(input);

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::UnfinishedString(0, 4, 3)
        ))
    }
}
