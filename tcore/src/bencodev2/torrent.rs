use std::{
    borrow::Cow,
    io::{BufRead, BufReader, Error, Read},
    str,
};

use thiserror::Error;

use crate::bencodev2::decoder::{self, DecodeError, Decoder, Token};

#[derive(Default)]
pub struct Torrent {
    announce: String,
    info: Info,
}

#[derive(Default)]
pub struct Info {
    name: String,
    piece_length: usize,
    pieces: Vec<u8>, // NOTE: or Vec<String>?
    length: Option<usize>,
    files: Option<Vec<File>>,
}

#[derive(Default)]
pub struct File {
    length: usize,
    path: String, // probably better than Vec<String>
}

#[derive(Error, Debug)]
pub enum TorrentFileError {
    #[error("error reading .torrent file: {0}")]
    Io(#[from] Error),
    #[error("error while decoding .torrent file: {0}")]
    Decode(#[from] DecodeError),
    #[error(".torrent file is missing metainfo section opener")]
    MissingMetaInfoOpener,
    #[error(".torrent file is missing info section opener")]
    MissingInfoOpener,
    #[error(".torrent file is missing announce key in metainfo section")]
    MissingAnnounceKey,
    #[error(".torrent file is missing announce URL in metainfo section")]
    MissingAnnounceURL,
    #[error(".torrent file is missing info section in metainfo section")]
    MissingInfoKey,
    #[error(".torrent file is missing list of files opener in info section")]
    MissingListOfFilesOpener,
    #[error(".torrent file is missing file opener in files section")]
    MissingNextFileOpener,
    #[error(".torrent file is missing file length in files section")]
    MissingFileLength,
    #[error(".torrent file is missing file path in files section")]
    MissingFilePath,
    #[error("not valid UTF-8 when UTF-8 string is expected")]
    Utf8(#[from] str::Utf8Error),
}

impl Torrent {
    pub fn from_file<R: Read>(src: R) -> Result<Torrent, TorrentFileError> {
        let mut buf = BufReader::new(src);
        let data = buf.fill_buf()?;
        let torrent_builder = TorrentBuilder::new(data);
        torrent_builder.build()
    }
}

struct TorrentBuilder<'a> {
    state: TorrentBuilderState,
    src: &'a [u8],
}

enum TorrentBuilderState {
    ExpectingAnnounce,
    ExpectingInfo,
    FillingInfo,
    FillingFiles,
    End, // do i need it?
}

impl<'a> TorrentBuilder<'a> {
    fn new(src: &[u8]) -> TorrentBuilder {
        TorrentBuilder {
            state: TorrentBuilderState::ExpectingAnnounce,
            src,
        }
    }

    fn build(mut self) -> Result<Torrent, TorrentFileError> {
        let mut buf = BufReader::new(self.src);
        let data = match buf.fill_buf() {
            Ok(data) => data,
            Err(e) => return Err(TorrentFileError::Io(e)),
        };

        let mut dec = Decoder::new(data);

        let mut torrent = Torrent::default();

        let metainfo_open = dec.next_token()?;
        if metainfo_open != Token::BeginDict {
            return Err(TorrentFileError::MissingMetaInfoOpener);
        }

        // announce key
        if !self.is_next_key(&mut dec, b"announce")? {
            return Err(TorrentFileError::MissingAnnounceKey);
        }
        let announce_url = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceURL),
        };
        torrent.announce = String::from_utf8(announce_url).map_err(|e| e.utf8_error())?;

        //info
        if !self.is_next_key(&mut dec, b"info")? {
            return Err(TorrentFileError::MissingInfoKey);
        }
        let info = dec.next_token()?;
        if info != Token::BeginDict {
            return Err(TorrentFileError::MissingInfoOpener);
        }

        //info:name
        if !self.is_next_key(&mut dec, b"name")? {
            return Err(TorrentFileError::MissingAnnounceKey); // FIXME: not announce key
        }
        let name = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.name = String::from_utf8(name).map_err(|e| e.utf8_error())?;

        //info: piece length
        if !self.is_next_key(&mut dec, b"piece length")? {
            return Err(TorrentFileError::MissingAnnounceKey); // FIXME: not announce key
        }
        let piece_length = match dec.next_token()? {
            Token::Int(len) => len,
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.piece_length = piece_length as usize;

        //info: pieces
        if !self.is_next_key(&mut dec, b"pieces")? {
            return Err(TorrentFileError::MissingAnnounceKey); // FIXME: not announce key
        }
        let pieces = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.pieces = pieces;

        //info: length OR files

        if let Some(key) = self.next_key(&mut dec)? {
            if is_key(b"length", &key) {
                let length = match dec.next_token()? {
                    Token::Int(len) => len,
                    _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
                };
                torrent.info.length = Some(length as usize);
            } else if is_key(b"files", &key) {
                let files_list_opener = dec.next_token()?;
                if files_list_opener != Token::BeginList {
                    return Err(TorrentFileError::MissingListOfFilesOpener);
                }

                loop {
                    match dec.next_token()? {
                        Token::BeginDict => {
                            let mut file = File::default();

                            if self.is_next_key(&mut dec, b"length")? {
                                    let length = match dec.next_token()? {
                                        Token::Int(len) => len,
                                        _ => return Err(TorrentFileError::MissingFileLength),
                                    };
                                    file.length = length as usize;
                            } else {
                                return Err(TorrentFileError::MissingFileLength);
                            }

                            if self.is_next_key(&mut dec, b"path")? {
                                    match dec.next_token()? {
                                        Token::BeginList => (),
                                        _ => return Err(TorrentFileError::MissingFilePath),
                                    }

                                    let mut path = String::new();
                                    loop {
                                        let path_piece = match dec.next_token()? {
                                            Token::String(cow) => cow.into_owned(),
                                            Token::EndObject => break,
                                            _ => return Err(TorrentFileError::MissingFilePath),
                                        };
                                        let path_piece = String::from_utf8(path_piece)
                                            .map_err(|e| e.utf8_error())?;
                                        path.reserve(path_piece.len());
                                        path.push_str(&path_piece);
                                    }
                                    file.path = path;
                            } else {
                                return Err(TorrentFileError::MissingFileLength);
                            }

                            match dec.next_token()? {
                                Token::EndObject => {
                                    if !torrent.info.files.is_some() {
                                        torrent.info.files = Some(Vec::new())
                                    }
                                    torrent.info.files.as_mut().unwrap().push(file);
                                }
                                _ => return Err(TorrentFileError::MissingInfoOpener), // FIXME: not
                                                                                      // file
                                                                                      // opener
                            }
                        }

                        Token::EndObject => break,
                        _ => return Err(TorrentFileError::MissingNextFileOpener),
                    }
                }
            } else {
                return Err(TorrentFileError::MissingInfoOpener); // FIXME: missing length or files
            }
        } else {
            return Err(TorrentFileError::MissingInfoOpener); // FIXME: missing length or files
        }

        Ok(torrent)
    }

    /// IT CONSUMES THE TOKEN
    fn is_next_key(&self, dec: &mut Decoder<'a>, key: &'a [u8]) -> Result<bool, TorrentFileError> {
        if let Some(next_key) = self.next_key(dec)? {
            return Ok(next_key == Cow::Borrowed(key));
        } else {
            return Ok(false);
        }
    }

    /// IT CONSUMES THE TOKEN
    fn next_key(&self, dec: &mut Decoder<'a>) -> Result<Option<Cow<'a, [u8]>>, TorrentFileError> {
        match dec.next_token()? {
            Token::String(cow) => Ok(Some(cow)),
            _ => Ok(None),
        }
    }
}

fn is_key<'a>(expected: &[u8], got: &Cow<'a, [u8]>) -> bool {
    return Cow::Borrowed(expected) == *got;
}

#[cfg(test)]
mod test_torrent {

    use std::fs;

    use super::*;

    #[test]
    fn valid_bep_003_single_file_torrent() {
        let data = fs::File::open("../test_data/fixtures/single_bep_003.torrent")
            .expect("file must be opened and read");

        Torrent::from_file(data).expect("parsed file");
    }

    #[test]
    fn valid_bep_003_multi_file_torrent() {
        let data = fs::File::open("../test_data/fixtures/multi_bep_003.torrent")
            .expect("file must be opened and read");

        Torrent::from_file(data).expect("parsed file");
    }
}
