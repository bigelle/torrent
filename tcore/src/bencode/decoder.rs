use std::io::{self, BufRead};

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

use super::stack::StructureError;

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
    #[error("invalid object structure: {0}")]
    InvalidStructure(#[from] StructureError),
    #[error("bencoded value is too large")]
    ValueTooLarge,
    #[error("unable to read file: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("unused trailing data in buffer")]
    TrailingDataInBuffer,
}

impl PartialEq for DecodeError {
    fn eq(&self, other: &Self) -> bool {
        use DecodeError::*;
        match (self, other) {
            (InvalidSyntax, InvalidSyntax) => true,
            (ValueTooLarge, ValueTooLarge) => true,
            (Io(_), Io(_)) => true,
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
        let result = loop {
            let maybe_token = self.next_token()?;

            match maybe_token {
                Some(token) => {
                    match token {
                        Token::Int(v) => match self.stack.push_value(Value::Int(v)) {
                            Ok(returned) => {
                                if let Some(v) = returned {
                                    break v;
                                }
                            }
                            Err(e) => return Err(DecodeError::InvalidStructure(e)),
                        },

                        Token::String(v) => match self.stack.push_value(Value::String(v)) {
                            Ok(returned) => {
                                if let Some(v) = returned {
                                    break v;
                                }
                            }
                            Err(e) => return Err(DecodeError::InvalidStructure(e)),
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
                                    break v;
                                }
                            }
                            Err(e) => return Err(DecodeError::InvalidStructure(e)),
                        },

                        Token::Invalid => return Err(DecodeError::InvalidSyntax),
                    };
                }

                None => {
                    let len = self.refill()?;
                    if len == 0 {
                        return Err(DecodeError::UnexpectedEof);
                    }
                }
            }
        };

        if !self.buf.is_empty() {
            return Err(DecodeError::TrailingDataInBuffer);
        }

        return Ok(result);
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
                self.advance_buf(len);
                Ok(maybe_token)
            }
            b'0'..=b'9' => {
                let (maybe_token, len) = match parser::parse_string(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buf(len);
                Ok(maybe_token)
            }
            b'l' => {
                self.advance_buf(1);
                Ok(Some(Token::BeginList))
            }
            b'd' => {
                self.advance_buf(1);
                Ok(Some(Token::BeginDict))
            }
            b'e' => {
                self.advance_buf(1);
                Ok(Some(Token::EndOfObj))
            }
            _ => Ok(Some(Token::Invalid)),
        }
    }

    pub fn refill(&mut self) -> Result<usize, DecodeError> {
        let available = self.src.fill_buf()?;

        if available.is_empty() {
            return Ok(0);
        }

        let size = available.len();

        self.buf.reserve(size);

        self.buf.put_slice(available);

        self.src.consume(size);

        Ok(size)
    }

    fn advance_buf(&mut self, n: usize) {
        if self.buf.len() >= n {
            self.buf.advance(n);
        }
    }
}

#[cfg(test)]
mod test_decoder_return_values {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn source_error_junk_trail_in_buffer() {
        let src = b"li42e4:teste3:cow";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(
            dec.decode(),
            Err(DecodeError::TrailingDataInBuffer)
        ));
    }

    // STRINGS:
    #[test]
    fn string_valid() {
        let src = b"4:test";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), &b"test"[..]);
    }

    #[test]
    fn string_valid_empty() {
        let src = b"0:";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), &b""[..]);
    }

    #[test]
    fn string_error_leading_zero_in_length() {
        let src = b"04:test";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::InvalidSyntax)));
    }

    #[test]
    fn string_valid_binary_bytes() {
        let src = b"3:\x00\x01\x02";
        let mut dec = Decoder::new(&src[..]);
        let val = dec.decode().unwrap();
        assert_eq!(val, &b"\x00\x01\x02"[..]);
        let Value::String(str) = val else {
            panic!("expected string, got {val}")
        };
        assert_eq!(str.len(), 3);
    }

    #[test]
    fn string_error_too_big() {
        let mut src: Vec<u8> = Vec::new();
        for _ in 0..100 {
            src.push(b'9');
        }
        src.push(b':');
        for _ in 0..100 {
            src.push(b'a');
        }
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::ValueTooLarge)));
    }

    // INTEGERS:
    #[test]
    fn int_valid_only_zero() {
        let src = b"i0e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), 0);
    }

    #[test]
    fn int_valid_positive() {
        let src = b"i42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), 42);
    }

    #[test]
    fn int_valid_negative() {
        let src = b"i-42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), -42);
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

    #[test]
    fn int_error_too_big() {
        let src = b"i99999999999999999999999999999999999999999999999999999999999999999999e";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(dec.decode(), Err(DecodeError::ValueTooLarge)));
    }

    // LISTS:
    #[test]
    fn list_valid_flat() {
        let src = b"li42e4:teste";
        let mut dec = Decoder::new(&src[..]);

        let expected: Value = vec![42.into(), "test".into()].into();
        assert_eq!(dec.decode().unwrap(), expected);
    }

    #[test]
    fn list_valid_empty() {
        let src = b"le";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Vec::<Value>::new());
    }

    #[test]
    fn list_valid_with_nested_objects() {
        let src = b"li42e4:testd3:cow3:mooel3:egg4:spamee";
        let mut dec = Decoder::new(&src[..]);

        let expected: Value = vec![
            42.into(),
            "test".into(),
            BTreeMap::from([(b"cow".into(), "moo".into())]).into(),
            vec!["egg".into(), "spam".into()].into(),
        ]
        .into();

        assert_eq!(dec.decode().unwrap(), expected);
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

        let mut expected = BTreeMap::new();
        expected.insert(b"test".into(), 42.into());

        assert_eq!(dec.decode().unwrap(), expected,)
    }

    #[test]
    fn dict_valid_empty() {
        let src = b"de";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), BTreeMap::new())
    }

    #[test]
    fn dict_valid_with_nested_objects() {
        let src = b"d4:testi42e4:listl3:cow3:mooe4:dictd3:egg4:spamee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            BTreeMap::from([
                (b"test".into(), 42.into()),
                (b"list".into(), vec!["cow".into(), "moo".into()].into()),
                (
                    b"dict".into(),
                    BTreeMap::from([(b"egg".into(), "spam".into())]).into()
                )
            ])
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
        assert!(matches!(
            dec.decode(),
            Err(DecodeError::InvalidStructure(_))
        ));
    }

    #[test]
    fn dict_error_orphaned_key() {
        let src = b"d4:teste";
        let mut dec = Decoder::new(&src[..]);
        assert!(matches!(
            dec.decode(),
            Err(DecodeError::InvalidStructure(_))
        ));
    }
}

#[cfg(test)]
mod test_decoder {
    use super::*;
    use std::{
        fs,
        io::{self, BufReader, Read},
        path::Path,
    };

    struct SlowReader<R: Read> {
        inner: R,
        limit: usize,
    }

    impl<R: Read> Read for SlowReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let max_to_read = std::cmp::min(buf.len(), self.limit);
            self.inner.read(&mut buf[..max_to_read])
        }
    }

    #[test]
    fn test_slow_stream_decoding() {
        let fixtures_dir = Path::new("../test_data/fixtures");

        let entries = fs::read_dir(fixtures_dir).expect("Failed to read fixtures directory");

        let mut not_failed = true;
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("torrent") {
                println!("Testing fixture: {:?}", path);

                let data = fs::read(&path).expect("Failed to read file");
                let src = BufReader::new(SlowReader {
                    inner: &data[..],
                    limit: 4,
                });

                let mut dec = Decoder::new(src);
                let result = dec.decode();

                if !result.is_ok() {
                    eprintln!(
                        "Failed to decode fixture: {:?}, cause: {}",
                        path,
                        result.unwrap_err()
                    );
                    not_failed = false;
                    continue;
                }
            }

            assert!(not_failed)
        }
    }

    #[test]
    fn test_huge_file() {
        let _profiler = dhat::Profiler::new_heap();

        let data = fs::read("../test_data/benchmarks/heavy.torrent").expect("file exists and can be read");

        let mut dec = Decoder::new(&data[..]);
        let result = dec.decode();

        if !result.is_ok() {
            panic!("Failed to decode fixture: {}", result.unwrap_err());
        }
    }
}
