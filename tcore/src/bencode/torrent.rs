use std::{borrow::Cow, fmt::Display};

use thiserror::Error;

use super::decoder::{DecodeError, Decoder, Token, TokenKind};

#[derive(Default, Debug)]
pub struct Torrent {
    announce: String,
    info: Info,
}

#[derive(Default, Debug)]
pub struct Info {
    // both are for info hash
    begin_pos: usize,
    end_pos: usize,

    name: String,
    piece_length: usize,
    pieces: Vec<u8>,
    length: Option<usize>,
    files: Option<Vec<File>>,
}

#[derive(Default, Debug)]
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
}

impl Torrent {
    pub fn from_file(src: &[u8]) -> Result<Torrent, TorrentFileError> {
        let torrent_builder = TorrentBuilder::new(&src);
        torrent_builder.build()
    }
}

struct TorrentBuilder<'builder> {
    state: TorrentBuilderState,
    src: &'builder [u8],
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
                            torrent.info.end_pos = pos;
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
                            return Err(TorrentFileError::ExpectedKey {
                                state: self.state.into(),
                                got: token.into(),
                            });
                        }
                    }
                }

                TorrentBuilderState::Finished => return Ok(torrent),
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

                let token = dec.next_token()?;

                match token {
                    Token::BeginDict(pos) => {
                        torrent.info.begin_pos = pos;
                        Ok(())
                    }
                    _ => {
                        return Err(TorrentFileError::UnexpectedTypeForKey {
                            key: TorrentKey::Info,
                            expected: TokenKind::BeginDict,
                            got: token.into(),
                        });
                    }
                }
            }

            _ => self.skip_value(dec),
        }
    }

    fn handle_announce(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let announce_url = match token {
            Token::String(url) => url,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::Announce,
                    expected: TokenKind::String,
                    got: token.into(),
                });
            }
        };

        let announce_url =
            String::from_utf8(announce_url.into_owned()).map_err(|e| e.utf8_error())?;
        torrent.announce = announce_url;
        Ok(())
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
                dec.next_token()?; // skipping 'l' token
                Ok(())
            }
            _ => self.skip_value(dec),
        }
    }

    fn handle_name(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let name = match token {
            Token::String(url) => url,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::InfoName,
                    expected: TokenKind::String,
                    got: token.into(),
                });
            }
        };

        let name = String::from_utf8(name.into_owned()).map_err(|e| e.utf8_error())?;
        torrent.info.name = name;
        Ok(())
    }

    fn handle_piece_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let piece_length = match token {
            Token::Int(len) => len,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::InfoPieceLength,
                    expected: TokenKind::Int,
                    got: token.into(),
                });
            }
        };

        torrent.info.piece_length = piece_length as usize;
        Ok(())
    }

    fn handle_pieces(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let pieces = match token {
            Token::String(url) => url,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::InfoPieces,
                    expected: TokenKind::String,
                    got: token.into(),
                });
            }
        };

        torrent.info.pieces = pieces.into_owned();
        Ok(())
    }

    fn handle_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let length = match token {
            Token::Int(len) => len,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::InfoLength,
                    expected: TokenKind::Int,
                    got: token.into(),
                });
            }
        };

        torrent.info.length = Some(length as usize);
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

                let token = dec.next_token()?;
                match token {
                    Token::BeginList(_) => Ok(()),
                    _ => {
                        return Err(TorrentFileError::UnexpectedTypeForKey {
                            key: TorrentKey::FilesPath,
                            expected: TokenKind::BeginList,
                            got: token.into(),
                        });
                    }
                }
            }

            _ => self.skip_value(dec),
        }
    }

    fn handle_file_length(
        &self,
        dec: &mut Decoder,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let token = dec.next_token()?;

        let length = match token {
            Token::Int(len) => len,
            _ => {
                return Err(TorrentFileError::UnexpectedTypeForKey {
                    key: TorrentKey::FilesLength,
                    expected: TokenKind::Int,
                    got: token.into(),
                });
            }
        };

        let files = torrent.info.files.as_mut().unwrap();
        match files.last_mut() {
            Some(file) => file.length = length as usize,
            None => {
                let mut file = File::default();
                file.length = length as usize;
                files.push(file);
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
            Token::EndObject(_) => return Err(TorrentFileError::UnexpectedObjectClosure),
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

#[cfg(test)]
mod test_torrent {

    use std::fs;

    use super::*;

    #[test]
    fn valid_bep_003_single_file_torrent() {
        let mut data = fs::read("../test_data/fixtures/single_bep_003.torrent")
            .expect("file must be opened and read");

        let res = Torrent::from_file(&mut data);
        assert!(res.is_ok(), "unexpected error: {:?}", res.err().unwrap());
    }

    #[test]
    fn valid_bep_003_multi_file_torrent() {
        let mut data = fs::read("../test_data/fixtures/multi_bep_003.torrent")
            .expect("file must be opened and read");

        Torrent::from_file(&mut data).unwrap();
    }
}
