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

        if e_pos - 1 > 21 {
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

        if col_pos > 21 {
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
        let mut input: Vec<u8> = Vec::from(b"i1");

        input.extend_from_slice(&b"0".repeat(21));
        input.push(b'e');

        let mut dec = Decoder::new(&input);

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::TokenTooLarge
        ))
    }

    #[test]
    fn error_int_is_not_finished() {
        let input = b"i42"; 
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
        // 22 digits -> exceeds limit of 21
        let input: &[u8] = b"1000000000000000000000:"; // 10^21

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

    #[test]
    fn valid_int_variants() {
        let mut dec = Decoder::new(b"i0ei-1e");

        assert_eq!(dec.next_token().unwrap(), Token::Int(0));
        assert_eq!(dec.next_token().unwrap(), Token::Int(-1));
    }

    #[test]
    fn valid_empty_string_token() {
        let mut dec = Decoder::new(b"0:");

        assert_eq!(dec.next_token().unwrap(), Token::String(Cow::Borrowed(b"")));
    }

    #[test]
    fn valid_list_tokens() {
        let mut dec = Decoder::new(b"li1e3:abce");

        assert!(matches!(dec.next_token().unwrap(), Token::BeginList(0)));
        assert_eq!(dec.next_token().unwrap(), Token::Int(1));
        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"abc"))
        );
        assert!(matches!(dec.next_token().unwrap(), Token::EndObject(9)));
    }

    #[test]
    fn valid_dict_tokens() {
        let mut dec = Decoder::new(b"d3:foo3:bare");

        assert!(matches!(dec.next_token().unwrap(), Token::BeginDict(0)));
        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"foo"))
        );
        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"bar"))
        );
        assert!(matches!(dec.next_token().unwrap(), Token::EndObject(11)));
    }

    #[test]
    fn error_unknown_token() {
        let mut dec = Decoder::new(b"x");

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::UnknownToken(b'x')
        ));
    }

    #[test]
    fn error_string_missing_colon() {
        let mut dec = Decoder::new(b"4test");

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::MissingColonInString
        ));
    }

    #[test]
    fn error_wrong_int_syntax() {
        let mut dec = Decoder::new(b"i4xe");

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::WrongSyntax
        ));
    }

    #[test]
    fn error_wrong_string_syntax() {
        let mut dec = Decoder::new(b"3x:abc");

        assert!(matches!(
            dec.next_token().unwrap_err(),
            DecodeError::WrongSyntax
        ));
    }
}
