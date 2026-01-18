#![warn(clippy::all)]
use std::{borrow::Cow, fmt::Display, fs};

use thiserror::Error;

use crate::cryptos::hash::make_sha1;

use super::decoder::{DecodeError, Decoder, Token, TokenKind};

#[derive(Default, Debug)]
pub struct Torrent {
    announce: String,
    info: Info,
}

impl Torrent {
    fn is_valid(&self) -> Result<(), TorrentFileError> {
        if self.announce.is_empty() {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::MetaInfo,
                key: TorrentKey::Announce,
            });
        }
        if self.info == Info::default() {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::MetaInfo,
                key: TorrentKey::Info,
            });
        }
        Ok(())
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct Info {
    info_hash: [u8; 20],

    name: String,
    piece_length: u64,
    pieces: Vec<u8>,
    length: Option<u64>,
    files: Option<Vec<File>>,
}

impl Info {
    fn is_valid(&self) -> Result<(), TorrentFileError> {
        if self.name.is_empty() {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::Info,
                key: TorrentKey::InfoName,
            });
        }

        if self.piece_length == 0 {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::Info,
                key: TorrentKey::InfoPieceLength,
            });
        }

        if self.pieces.is_empty() {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::Info,
                key: TorrentKey::InfoPieces,
            });
        }

        if !self.pieces.len().is_multiple_of(20) {
            return Err(TorrentFileError::InvalidPiecesLength(self.pieces.len()));
        }

        if self.length.is_none() && self.files.is_none() {
            return Err(TorrentFileError::MissingRequiredKey {
                state: TorrentBuilderStateKind::Info,
                key: TorrentKey::InfoFiles,
            });
        }

        Ok(())
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct File {
    length: usize,
    path: Vec<String>,
}

#[derive(Debug)]
pub enum TorrentKey {
    Announce,
    Info,
    InfoName,
    InfoPieceLength,
    InfoPieces,
    InfoLength,
    InfoFiles,
    FilesLength,
    FilesPath,
}

impl Display for TorrentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            TorrentKey::Announce => write!(f, "Announce"),
            TorrentKey::Info => write!(f, "Info"),
            TorrentKey::InfoName => write!(f, "InfoName"),
            TorrentKey::InfoPieceLength => write!(f, "InfoPieceLength"),
            TorrentKey::InfoPieces => write!(f, "InfoPieces"),
            TorrentKey::InfoLength => write!(f, "InfoLength"),
            TorrentKey::InfoFiles => write!(f, "InfoFiles"),
            TorrentKey::FilesLength => write!(f, "FilesLength"),
            TorrentKey::FilesPath => write!(f, "FilesPath"),
        }
    }
}

#[derive(Error, Debug)]
pub enum TorrentFileError {
    #[error("error reading .torrent file: {0}")]
    Io(#[from] std::io::Error),
    #[error("error while decoding .torrent file: {0}")]
    Decode(#[from] DecodeError),
    #[error("expected one of the {state} keys, got {got}")]
    ExpectedKey {
        state: TorrentBuilderStateKind,
        got: TokenKind,
    },
    #[error("{state} is missing {key}")]
    MissingRequiredKey {
        state: TorrentBuilderStateKind,
        key: TorrentKey,
    },
    #[error("'files' and 'length' keys are mutually exclusive")]
    MutualExclusiveKeys,
    #[error(".torrent file is missing meta info opener")]
    MissingMetaInfoOpener,
    #[error("{key} key expected value of type {expected}, got {got}")]
    UnexpectedTypeForKey {
        key: TorrentKey,
        expected: TokenKind,
        got: TokenKind,
    },
    #[error("unexpected object closure")]
    UnexpectedObjectClosure,
    #[error("not valid UTF-8 when UTF-8 string is expected")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("pieces has invalid length of {0} which is not divisible by 20")]
    InvalidPiecesLength(usize),
}

impl Torrent {
    pub fn from_file(path: &str) -> Result<Torrent, TorrentFileError> {
        let data = fs::read(path)?;
        let torrent_builder = TorrentBuilder::new(&data);
        torrent_builder.build()
    }

    pub fn from_bytes(src: &[u8]) -> Result<Torrent, TorrentFileError> {
        let torrent_builder = TorrentBuilder::new(src);
        torrent_builder.build()
    }

    pub fn info_hash(&self) -> Option<[u8;20]> {
        if self.info.info_hash.is_empty() {
            return None
        }
        Some(self.info.info_hash)
    }
}

struct TorrentBuilder<'builder> {
    state: TorrentBuilderState,
    src: &'builder [u8],
    info_begin: usize,
}

enum TorrentBuilderState {
    Begin,
    MetaInfo,
    Info,
    Files,
    SingularFile,
    SingularFilePath,
    Finished,
}

#[derive(Debug)]
pub enum TorrentBuilderStateKind {
    Begin,
    MetaInfo,
    Info,
    Files,
    SingularFile,
    SingularFilePath,
    Finished,
}

impl Display for TorrentBuilderStateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            TorrentBuilderStateKind::Begin => write!(f, "Begin"),
            TorrentBuilderStateKind::MetaInfo => write!(f, "MetaInfo"),
            TorrentBuilderStateKind::Info => write!(f, "Info"),
            TorrentBuilderStateKind::Files => write!(f, "Files"),
            TorrentBuilderStateKind::SingularFile => write!(f, "SingularFile"),
            TorrentBuilderStateKind::SingularFilePath => write!(f, "SingularFilePath"),
            TorrentBuilderStateKind::Finished => write!(f, "Finished"),
        }
    }
}

impl From<TorrentBuilderState> for TorrentBuilderStateKind {
    fn from(value: TorrentBuilderState) -> Self {
        match value {
            TorrentBuilderState::Begin => TorrentBuilderStateKind::Begin,
            TorrentBuilderState::MetaInfo => TorrentBuilderStateKind::MetaInfo,
            TorrentBuilderState::Info => TorrentBuilderStateKind::Info,
            TorrentBuilderState::Files => TorrentBuilderStateKind::Files,
            TorrentBuilderState::SingularFile => TorrentBuilderStateKind::SingularFile,
            TorrentBuilderState::SingularFilePath => TorrentBuilderStateKind::SingularFilePath,
            TorrentBuilderState::Finished => TorrentBuilderStateKind::Finished,
        }
    }
}

impl Display for TorrentBuilderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            TorrentBuilderState::Begin => write!(f, "Begin"),
            TorrentBuilderState::MetaInfo => write!(f, "MetaInfo"),
            TorrentBuilderState::Info => write!(f, "Info"),
            TorrentBuilderState::Files => write!(f, "Files"),
            TorrentBuilderState::SingularFile => write!(f, "SingularFile"),
            TorrentBuilderState::SingularFilePath => write!(f, "SingularFilePath"),
            TorrentBuilderState::Finished => write!(f, "Finished"),
        }
    }
}

impl<'builder> TorrentBuilder<'builder> {
    fn new(src: &'builder [u8]) -> TorrentBuilder<'builder> {
        TorrentBuilder {
            state: TorrentBuilderState::Begin,
            info_begin: 0,
            src,
        }
    }

    fn build(mut self) -> Result<Torrent, TorrentFileError> {
        let mut dec = Decoder::new(self.src);

        let mut torrent = Torrent::default();

        loop {
            match self.state {
                TorrentBuilderState::Begin => match dec.next_token()? {
                    Token::BeginDict(_) => self.state = TorrentBuilderState::MetaInfo,
                    _ => return Err(TorrentFileError::MissingMetaInfoOpener),
                },

                TorrentBuilderState::MetaInfo => {
                    let token = dec.next_token()?;
                    match token {
                        Token::String(key) => self.handle_meta_keys(key, &mut dec, &mut torrent)?,

                        // stepping out of the root, we're done:
                        Token::EndObject(_) => self.state = TorrentBuilderState::Finished,

                        _ => {
                            return Err(TorrentFileError::ExpectedKey {
                                state: self.state.into(),
                                got: token.into(),
                            });
                        }
                    };
                }

                TorrentBuilderState::Info => {
                    let token = dec.next_token()?;
                    match token {
                        Token::String(key) => self.handle_info_keys(key, &mut dec, &mut torrent)?,

                        // stepping back by one state:
                        Token::EndObject(pos) => {
                            torrent.info.info_hash = make_sha1(self.get_info_slice(pos));
                            torrent.info.is_valid()?;
                            self.state = TorrentBuilderState::MetaInfo
                        }

                        _ => {
                            return Err(TorrentFileError::ExpectedKey {
                                state: self.state.into(),
                                got: token.into(),
                            });
                        }
                    };
                }

                TorrentBuilderState::Files => {
                    if torrent.info.files.is_none() {
                        torrent.info.files = Some(Vec::new());
                    }

                    let token = dec.next_token()?;
                    match token {
                        Token::BeginDict(_) => {
                            self.state = TorrentBuilderState::SingularFile;
                            torrent.info.files.as_mut().unwrap().reserve(1);
                        }

                        // stepping back by one state:
                        Token::EndObject(_) => self.state = TorrentBuilderState::Info,

                        _ => {
                            return Err(TorrentFileError::ExpectedKey {
                                state: self.state.into(),
                                got: token.into(),
                            });
                        }
                    };
                }

                TorrentBuilderState::SingularFile => {
                    let token = dec.next_token()?;
                    match token {
                        Token::String(key) => self.handle_file_keys(key, &mut dec, &mut torrent)?,

                        // stepping back by one state:
                        Token::EndObject(_) => self.state = TorrentBuilderState::Files,

                        _ => {
                            return Err(TorrentFileError::ExpectedKey {
                                state: self.state.into(),
                                got: token.into(),
                            });
                        }
                    }
                }

                TorrentBuilderState::SingularFilePath => {
                    let token = dec.next_token()?;
                    match token {
                        Token::String(path) => self.handle_file_path(path, &mut torrent)?,

                        // stepping back by one state:
                        Token::EndObject(_) => self.state = TorrentBuilderState::SingularFile,

                        _ => {
                            return Err(TorrentFileError::UnexpectedTypeForKey {
                                key: TorrentKey::FilesPath,
                                expected: TokenKind::String,
                                got: token.into(),
                            });
                        }
                    }
                }

                TorrentBuilderState::Finished => {
                    torrent.is_valid()?;
                    return Ok(torrent);
                }
            }
        }
    }

    fn handle_meta_keys(
        &mut self,
        key: Cow<'builder, [u8]>,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        match &*key {
            b"announce" => self.handle_announce(dec, torrent),

            b"info" => {
                self.state = TorrentBuilderState::Info;

                self.info_begin =
                    expect_extract(dec, TorrentKey::Info, TokenKind::BeginDict, |t| match t {
                        Token::BeginDict(i) => Some(*i),
                        _ => None,
                    })?;
                Ok(())
            }

            _ => self.skip_value(dec),
        }
    }

    fn handle_announce(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let announce_url =
            expect_extract(dec, TorrentKey::Announce, TokenKind::String, |t| match t {
                Token::String(cow) => Some(cow.clone().into_owned()),
                _ => None,
            })?;

        let announce_url = String::from_utf8(announce_url).map_err(|e| e.utf8_error())?;
        torrent.announce = announce_url;
        Ok(())
    }

    fn get_info_slice(&self, end_pos: usize) -> &[u8] {
        &self.src[self.info_begin..end_pos + 1]
    }

    fn handle_info_keys(
        &mut self,
        key: Cow<'builder, [u8]>,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        match &*key {
            b"name" => self.handle_name(dec, torrent),
            b"piece length" => self.handle_piece_length(dec, torrent),
            b"pieces" => self.handle_pieces(dec, torrent),
            b"length" => {
                if torrent.info.files.is_some() {
                    return Err(TorrentFileError::MutualExclusiveKeys);
                }
                self.handle_length(dec, torrent)
            }
            b"files" => {
                if torrent.info.length.is_some() {
                    return Err(TorrentFileError::MutualExclusiveKeys);
                }
                self.state = TorrentBuilderState::Files;

                expect_token(dec, TorrentKey::InfoFiles, TokenKind::BeginList, |t| {
                    matches!(t, Token::BeginList(_))
                })
            }
            _ => self.skip_value(dec),
        }
    }

    fn handle_name(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let name = expect_extract(dec, TorrentKey::InfoName, TokenKind::String, |t| match t {
            Token::String(cow) => Some(cow.clone().into_owned()),
            _ => None,
        })?;

        let name = String::from_utf8(name).map_err(|e| e.utf8_error())?;
        torrent.info.name = name;
        Ok(())
    }

    fn handle_piece_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let piece_length =
            expect_extract(dec, TorrentKey::InfoLength, TokenKind::Int, |t| match t {
                Token::Int(i) => Some(*i),
                _ => None,
            })?;

        torrent.info.piece_length = piece_length as u64;
        Ok(())
    }

    fn handle_pieces(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let pieces = expect_extract(
            dec,
            TorrentKey::InfoPieces,
            TokenKind::String,
            |t| match t {
                Token::String(cow) => Some(cow.clone().into_owned()),
                _ => None,
            },
        )?;

        torrent.info.pieces = pieces;
        Ok(())
    }

    fn handle_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let len = expect_extract(dec, TorrentKey::InfoLength, TokenKind::Int, |t| match t {
            Token::Int(i) => Some(*i),
            _ => None,
        })?;

        torrent.info.length = Some(len as u64);
        Ok(())
    }

    fn handle_file_keys(
        &mut self,
        key: Cow<'builder, [u8]>,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        match &*key {
            b"length" => self.handle_file_length(dec, torrent),

            b"path" => {
                self.state = TorrentBuilderState::SingularFilePath;

                expect_token(dec, TorrentKey::FilesPath, TokenKind::BeginList, |t| {
                    matches!(t, Token::BeginList(_))
                })
            }

            _ => self.skip_value(dec),
        }
    }

    fn handle_file_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let length = expect_extract(dec, TorrentKey::FilesLength, TokenKind::Int, |t| match t {
            Token::Int(i) => Some(*i as usize),
            _ => None,
        })?;

        let files = torrent.info.files.as_mut().unwrap();
        match files.last_mut() {
            Some(file) => file.length = length,
            None => {
                files.push(File {
                    length,
                    path: Vec::new(),
                });
            }
        }
        Ok(())
    }

    fn handle_file_path(
        &self,
        path: Cow<'builder, [u8]>,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let path = String::from_utf8(path.into_owned()).map_err(|e| e.utf8_error())?;

        let files = torrent.info.files.as_mut().unwrap();
        match files.last_mut() {
            Some(file) => file.path.push(path),
            None => {
                let mut file = File::default();
                file.path.push(path);
                files.push(file);
            }
        }
        Ok(())
    }

    /// USE IT ONLY TO SKIP VALUES OF UNKNOWN KEYS
    fn skip_value(&self, dec: &mut Decoder) -> Result<(), TorrentFileError> {
        match dec.next_token()? {
            Token::Int(_) | Token::String(_) => Ok(()), // already skipped
            Token::EndObject(_) => Err(TorrentFileError::UnexpectedObjectClosure),
            Token::BeginDict(_) | Token::BeginList(_) => self.skip_nested_value(dec),
        }
    }

    /// USE IT ONLY TO SKIP VALUES OF UNKNOWN KEYS
    fn skip_nested_value(&self, dec: &mut Decoder) -> Result<(), TorrentFileError> {
        let mut counter = 1; // already have 1 opened object

        while counter != 0 {
            match dec.next_token()? {
                Token::Int(_) | Token::String(_) => continue,
                Token::BeginDict(_) | Token::BeginList(_) => counter += 1,
                Token::EndObject(_) => counter -= 1,
            }
        }

        Ok(())
    }
}

fn expect_token<F>(
    dec: &mut Decoder,
    key: TorrentKey,
    expected: TokenKind,
    ok: F,
) -> Result<(), TorrentFileError>
where
    F: FnOnce(&Token) -> bool,
{
    let token = dec.next_token()?;
    if ok(&token) {
        Ok(())
    } else {
        Err(TorrentFileError::UnexpectedTypeForKey {
            key,
            expected,
            got: token.into(),
        })
    }
}

fn expect_extract<T>(
    dec: &mut Decoder,
    key: TorrentKey,
    expected: TokenKind,
    f: impl FnOnce(&Token) -> Option<T>,
) -> Result<T, TorrentFileError> {
    let token = dec.next_token()?;
    if let Some(v) = f(&token) {
        Ok(v)
    } else {
        Err(TorrentFileError::UnexpectedTypeForKey {
            key,
            expected,
            got: token.into(),
        })
    }
}

#[cfg(test)]
mod test_torrent {

    use std::fs;

    use super::*;

    fn concat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn valid_bep_003_single_file_torrent() {
        let mut data = fs::read("../test_data/fixtures/single_bep_003.torrent")
            .expect("file must be opened and read");

        let res = Torrent::from_bytes(&mut data);
        assert!(res.is_ok(), "unexpected error: {:?}", res.err().unwrap());
    }

    #[test]
    fn valid_bep_003_multi_file_torrent() {
        let mut data = fs::read("../test_data/fixtures/multi_bep_003.torrent")
            .expect("file must be opened and read");

        Torrent::from_bytes(&mut data).unwrap();
    }

    #[test]
    fn get_info_slice_returns_info_dict_bytes() {
        let info_bytes = concat(&[
            b"d",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"6:lengthi123e",
            b"e",
        ]);

        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:info",
            info_bytes.as_slice(),
            b"e",
        ]);

        let mut builder = TorrentBuilder::new(&data);
        let info_begin = data.len() - info_bytes.len() - 1;
        builder.info_begin = info_begin;

        let end_pos = info_begin + info_bytes.len() - 1;
        assert_eq!(builder.get_info_slice(end_pos), info_bytes.as_slice());
    }

    #[test]
    fn unknown_root_keys_are_ignored() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"5:extra",
            b"l",
            b"i1e",
            b"i2e",
            b"e",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"6:lengthi123e",
            b"e",
            b"e",
        ]);

        let res = Torrent::from_bytes(&data);
        assert!(res.is_ok(), "unexpected error: {:?}", res.err().unwrap());
    }

    #[test]
    fn unknown_info_keys_are_ignored() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"6:lengthi123e",
            b"3:extd3:foo3:baree",
            b"e",
            b"e",
        ]);

        let res = Torrent::from_bytes(&data);
        assert!(res.is_ok(), "unexpected error: {:?}", res.err().unwrap());
    }

    #[test]
    fn unknown_file_keys_are_ignored() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"5:filesl",
            b"d",
            b"6:lengthi10e",
            b"4:pathl3:foo3:bare",
            b"5:extrad3:fooi1ee",
            b"e",
            b"e",
            b"e",
            b"e",
        ]);

        let res = Torrent::from_bytes(&data);
        assert!(res.is_ok(), "unexpected error: {:?}", res.err().unwrap());
    }

    #[test]
    fn error_on_files_and_length_in_info() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"6:lengthi123e",
            b"5:filesle",
            b"e",
            b"e",
        ]);

        let err = Torrent::from_bytes(&data).unwrap_err();
        assert!(matches!(err, TorrentFileError::MutualExclusiveKeys));
    }

    #[test]
    fn error_on_invalid_announce_type() {
        let data = concat(&[
            b"d",
            b"8:announcei1e",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"6:lengthi123e",
            b"e",
            b"e",
        ]);

        let err = Torrent::from_bytes(&data).unwrap_err();
        assert!(matches!(
            err,
            TorrentFileError::UnexpectedTypeForKey {
                key: TorrentKey::Announce,
                ..
            }
        ));
    }

    #[test]
    fn error_on_invalid_file_path_type() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:pieces20:12345678901234567890",
            b"5:filesl",
            b"d",
            b"6:lengthi10e",
            b"4:pathi1e",
            b"e",
            b"e",
            b"e",
            b"e",
        ]);

        let err = Torrent::from_bytes(&data).unwrap_err();
        assert!(matches!(
            err,
            TorrentFileError::UnexpectedTypeForKey {
                key: TorrentKey::FilesPath,
                ..
            }
        ));
    }

    #[test]
    fn error_on_missing_info_dict() {
        let data = concat(&[b"d", b"8:announce14:http://tracker", b"e"]);

        let res = Torrent::from_bytes(&data);
        assert!(res.is_err(), "expected error for missing info dict");
    }

    #[test]
    fn error_on_missing_pieces() {
        let data = concat(&[
            b"d",
            b"8:announce14:http://tracker",
            b"4:infod",
            b"4:name4:test",
            b"12:piece lengthi16384e",
            b"6:lengthi123e",
            b"e",
            b"e",
        ]);

        let res = Torrent::from_bytes(&data);
        assert!(res.is_err(), "expected error for missing pieces");
    }
}
