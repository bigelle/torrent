use std::io::{self, BufRead};

use bytes::{Buf, BytesMut};
use thiserror::Error;

use super::parser;
use super::parser::Token;
use super::stack::Stack;
use super::value::Value;

pub struct Decoder<T>
where
    T: BufRead,
{
    src: T,
    buf: BytesMut,
    stack: Stack,
}

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("invalid bencode syntax")]
    InvalidSyntax,
    #[error("bencoded value is too large")]
    ValueTooLarge,
    #[error("unable to read file: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("pushing {0} instead of string as dictionary key")]
    PushToDictError(Value),
}

impl PartialEq for DecodeError {
    fn eq(&self, other: &Self) -> bool {
        use DecodeError::*;
        match (self, other) {
            (InvalidSyntax, InvalidSyntax) => true,
            (ValueTooLarge, ValueTooLarge) => true,
            (Io(_), Io(_)) => true, // сравниваем только, что это Io, не сам io::Error
            (UnexpectedEof, UnexpectedEof) => true,
            _ => false,
        }
    }
}

impl<T> Decoder<T>
where
    T: BufRead,
{
    pub fn new(src: T) -> Decoder<T> {
        Decoder {
            src: src,
            buf: BytesMut::with_capacity(4096),
            stack: Stack::new(),
        }
    }

    pub fn with_capacity(src: T, cap: usize) -> Decoder<T> {
        Decoder {
            src: src,
            buf: BytesMut::with_capacity(cap),
            stack: Stack::new(),
        }
    }

    pub fn decode(&mut self) -> Result<Value, DecodeError> {
        loop {
            let maybe_token = self.next_token()?;

            match maybe_token {
                Some(token) => {
                    match token {
                        Token::Int(v) => match self.stack.push_value(Value::Int(v)) {
                            Ok(returned) => {
                                if let Some(v) = returned {
                                    return Ok(v);
                                }
                            }
                            Err(e) => return Err(DecodeError::PushToDictError(e.0)),
                        },
                        Token::String(v) => match self.stack.push_value(Value::String(v)) {
                            Ok(returned) => {
                                if let Some(v) = returned {
                                    return Ok(v);
                                }
                            }
                            Err(e) => return Err(DecodeError::PushToDictError(e.0)),
                        },
                        Token::BeginList => {
                            self.stack.push_list();
                        }
                        Token::BeginDict => {
                            self.stack.push_dict();
                        }
                        Token::EndOfObj => match self.stack.pop_container() {
                            Ok(returned) => {
                                if let Some(v) = returned {
                                    return Ok(v);
                                }
                            }
                            Err(e) => return Err(DecodeError::PushToDictError(e.0)),
                        },
                        Token::Invalid => return Err(DecodeError::InvalidSyntax),
                    };
                }
                None => match self.refill() {
                    Ok(len) => {
                        if len == 0 {
                            return Err(DecodeError::UnexpectedEof);
                        }
                    }
                    Err(e) => return Err(e),
                },
            }
        }
    }

    /// returns None if it needs more bytes
    pub fn next_token(&mut self) -> Result<Option<Token>, DecodeError> {
        let b = match self.buf.first() {
            Some(b) => b,
            None => return Ok(None),
        };

        match b {
            b'i' => {
                let (maybe_token, len) = match parser::parse_int(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(maybe_token)
            }
            b'0'..=b'9' => {
                let (maybe_token, len) = match parser::parse_string(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(maybe_token)
            }
            b'l' => {
                self.advance_buff(1);
                Ok(Some(Token::BeginList))
            }
            b'd' => {
                self.advance_buff(1);
                Ok(Some(Token::BeginDict))
            }
            b'e' => {
                self.advance_buff(1);
                Ok(Some(Token::EndOfObj))
            }
            _ => Ok(Some(Token::Invalid)),
        }
    }

    pub fn refill(&mut self) -> Result<usize, DecodeError> {
        let tmp = match self.src.fill_buf() {
            Ok(tmp) => tmp,
            Err(e) => return Err(DecodeError::Io(e)),
        };
        let len = tmp.len();
        if len > 0 {
            self.buf.extend_from_slice(tmp);
            self.src.consume(len);
        }
        Ok(len)
    }

    fn advance_buff(&mut self, n: usize) {
        if self.buf.len() >= n {
            self.buf.advance(n);
        }
    }
}

#[cfg(test)]
mod test_decoder {
    use std::collections::BTreeMap;

    use super::*;

    // STRINGS:
    #[test]
    fn string_valid() {
        let src = b"4:test";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::string("test"));
    }

    #[test]
    fn string_valid_empty() {
        let src = b"0:";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::string(""));
    }

    #[test]
    fn string_error_leading_zero_in_length() {
        let src = b"04:test";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::InvalidSyntax)));
    }

    // INTEGERS:
    #[test]
    fn int_valid_only_zero() {
        let src = b"i0e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::int(0));
    }

    #[test]
    fn int_valid_positive() {
        let src = b"i42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::int(42));
    }

    #[test]
    fn int_valid_negative() {
        let src = b"i-42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::int(-42));
    }

    #[test]
    fn int_error_invalid_syntax() {
        let src = b"i4a2e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode(), Err(DecodeError::InvalidSyntax));
    }

    #[test]
    fn int_error_leading_zero() {
        let src = b"i042e";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::InvalidSyntax)));
    }

    #[test]
    fn int_error_negative_zero() {
        let src = b"i-0e";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::InvalidSyntax)));
    }

    #[test]
    fn int_error_unexpected_eof() {
        let src = b"i42";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::UnexpectedEof)));
    }

    // LISTS:
    #[test]
    fn list_valid_flat() {
        let src = b"li42e4:teste";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::list(vec![Value::int(42), Value::string("test")])
        );
    }

    #[test]
    fn list_valid_empty() {
        let src = b"le";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::list(Vec::new()));
    }

    #[test]
    fn list_valid_with_nested_objects() {
        let src = b"li42e4:testd3:cow3:mooel3:egg4:spamee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::list(vec![
                42.into(),
                "test".into(),
                Value::dictionary(BTreeMap::from([("cow".into(), "moo".into(),)])),
                Value::list(vec!["egg".into(), "spam".into()])
            ])
        );
    }

    #[test]
    fn list_error_unexpected_eof() {
        let src = b"li42e4:test";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::UnexpectedEof)));
    }

    // DICTS:
    #[test]
    fn dict_valid_flat() {
        let src = b"d4:testi42ee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::dictionary(BTreeMap::from([(Vec::from("test"), Value::int(42))]))
        )
    }

    #[test]
    fn dict_valid_empty() {
        let src = b"de";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::dictionary(BTreeMap::new()))
    }

    #[test]
    fn dict_valid_with_nested_objects() {
        let src = b"d4:testi42e4:listl3:cow3:mooe4:dictd3:egg4:spamee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::dictionary(BTreeMap::from([
                ("test".into(), 42.into()),
                ("list".into(), Value::list(vec!["cow".into(), "moo".into()])),
                (
                    "dict".into(),
                    Value::dictionary(BTreeMap::from([("egg".into(), "spam".into())]))
                )
            ]))
        );
    }

    #[test]
    fn dict_error_unexpected_eof() {
        let src = b"d4:testi42e4:listl3:cow3:mooe4:dictd3:egg4:spame";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::UnexpectedEof)));
    }

    #[test]
    fn dict_error_not_string_key() {
        let src = b"di42e4:teste";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::PushToDictError(_))));
    }
}
