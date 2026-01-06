use crate::bencode::{DecodeError, value::Value};
use atoi::FromRadix10SignedChecked;

#[derive(PartialEq, Debug)]
pub enum Token {
    Primitive(Value),
    BeginList,
    BeginDict, // Cumberbatch
    EndOfObj,
    Invalid,
}

/// returns Token when found one, or None if not enough data
pub(in crate::bencode) fn parse_string(buf: &[u8]) -> Result<(Option<Token>, usize), DecodeError> {
    let i = match buf.iter().position(|x| *x == b':') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    if buf[0] == b'0' && buf[1] != b':' {
        return Err(DecodeError::InvalidSyntax);
    }

    // 9_999_999 - 10 MB string, too large
    // NOTE: maybe i want to configure it
    if i > 7 {
        return Err(DecodeError::ValueTooLarge);
    }

    let len = match atoi::atoi::<usize>(&buf[..i]) {
        Some(len) => len,
        None => return Err(DecodeError::InvalidSyntax),
    };

    if buf.len() < len + i + 1 {
        return Ok((None, 0));
    }

    Ok((
        Some(Token::Primitive(Value::String(
            buf[i + 1..i + 1 + len].into(),
        ))),
        i + len + 1,
    ))
}

/// returns Token when found one, or None if not enough data
pub(in crate::bencode) fn parse_int(buf: &[u8]) -> Result<(Option<Token>, usize), DecodeError> {
    let i = match buf.iter().position(|x| *x == b'e') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    if buf[1] == b'-' && buf[2] == b'0' {
        return Err(DecodeError::InvalidSyntax);
    }

    if buf[1] == b'0' && buf[2] != b'e' {
        return Err(DecodeError::InvalidSyntax);
    }

    // not including i and e
    if i - 1 > 19 {
        return Err(DecodeError::ValueTooLarge);
    }

    let (maybe_n, len) = i64::from_radix_10_signed_checked(&buf[1..i]);
    match maybe_n {
        Some(n) => {
            if len != i - 1 {
                return Err(DecodeError::InvalidSyntax);
            }
            Ok((Some(Token::Primitive(Value::Int(n))), i + 1))
        }
        None => Err(DecodeError::InvalidSyntax),
    }
}

#[cfg(test)]
mod test_parsers {

    use super::*;

    #[test]
    fn valid_string() {
        let buf = b"4:test";
        assert_eq!(
            parse_string(buf).unwrap(),
            (
                Some(Token::Primitive(Value::String(b"test".to_vec()))),
                6 as usize
            )
        )
    }

    #[test]
    fn valid_int() {
        let buf = b"i42e";
        assert_eq!(
            parse_int(buf).unwrap(),
            (Some(Token::Primitive(Value::Int(42))), 4)
        )
    }

    //TODO: test for failing cases
}
