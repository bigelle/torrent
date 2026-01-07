use std::borrow::Cow;

use atoi::FromRadix10SignedChecked;
use thiserror::Error;

#[derive(PartialEq, Debug)]
pub enum Token<'a> {
    Int(i64),
    String(Cow<'a, [u8]>),
    BeginDict, //Cumberbatch
    BeginList,
    EndObject,
}

pub struct Decoder<'a> {
    src: &'a [u8],
    pos: usize,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("unfinished string: expected {0} bytes, got {1}")]
    UnfinishedString(usize, usize),
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
        if self.pos > self.src.len() {
            return Err(DecodeError::PosOutOfBounds);
        }

        match self.current_byte() {
            b'i' => self.give_int_token(),
            b'0'..=b'9' => self.give_string_token(),
            b'l' => {
                self.pos += 1;
                Ok(Token::BeginList)
            }
            b'd' => {
                self.pos += 1;
                Ok(Token::BeginDict)
            }
            b'e' => {
                self.pos += 1;
                Ok(Token::EndObject)
            }
            v => Err(DecodeError::UnknownToken(v)),
        }
    }

    fn give_int_token(&mut self) -> Result<Token<'a>, DecodeError> {
        let e_pos = match self.src[self.pos..].iter().position(|x| *x == b'e') {
            Some(pos) => pos,
            None => return Err(DecodeError::UnfinishedInt),
        };

        if e_pos - 1 > 12 {
            return Err(DecodeError::TokenTooLarge);
        }

        self.pos += 1;

        dbg!(self.pos, self.pos + e_pos);
        let (maybe_n, used) =
            i64::from_radix_10_signed_checked(&self.src[self.pos..self.pos + e_pos]);

        match maybe_n {
            Some(n) => {
                if used != e_pos -1{
                    dbg!(used, e_pos);
                    return Err(DecodeError::WrongSyntax);
                }
                self.pos += e_pos;
                Ok(Token::Int(n))
            }
            None => return Err(DecodeError::WrongSyntax),
        }
    }

    fn give_string_token(&mut self) -> Result<Token<'a>, DecodeError> {
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

        let len = match maybe_len {
            Some(len) => {
                if size != col_pos {
                    dbg!(size, col_pos);
                    return Err(DecodeError::WrongSyntax);
                }
                self.pos += col_pos;
                len
            }
            None => {
                return Err(DecodeError::WrongSyntax);
            }
        } as usize;

        self.pos += 1; // stepping through the ":'

        let have = self.src[self.pos..].len() as usize;
        if have < len {
            return Err(DecodeError::UnfinishedString(len, have));
        }

        let token = Token::String(Cow::Borrowed(&self.src[self.pos..self.pos + len]));

        self.pos += len;

        Ok(token)
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
        assert_eq!(dec.pos, 3)
    }

    #[test]
    fn multiple_valid_int_token() {
        let input = b"i42ei6e";
        let mut dec = Decoder::new(input);

        assert_eq!(dec.next_token().unwrap(), Token::Int(42));
        assert_eq!(dec.pos, 4);

        assert_eq!(dec.next_token().unwrap(), Token::Int(6));
        assert_eq!(dec.pos, 7);
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
        assert_eq!(dec.pos, 6);
    }

    #[test]
    fn multiple_valid_string_token() {
        let input = b"4:test3:foo";
        let mut dec = Decoder::new(input);

        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"test"))
        );
        assert_eq!(dec.pos, 6);

        assert_eq!(
            dec.next_token().unwrap(),
            Token::String(Cow::Borrowed(b"foo"))
        );
        assert_eq!(dec.pos, 11);
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
            DecodeError::UnfinishedString(4, 3)
        ))
    }
}
