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
        let mut torrent_builder = TorrentBuilder::new(src);
        torrent_builder.build()
    }
}

struct TorrentBuilder<R>
where
    R: Read,
{
    state: TorrentBuilderState,
    src: R,
}

enum TorrentBuilderState {
    ExpectingAnnounce,
    ExpectingInfo,
    FillingInfo,
    FillingFiles,
    End, // do i need it?
}

impl<R> TorrentBuilder<R>
where
    R: Read,
{
    fn new(src: R) -> TorrentBuilder<R> {
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
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"announce") => {}
            Ok(_) => return Err(TorrentFileError::MissingAnnounceKey),
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };
        let announce_url = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceURL),
        };
        torrent.announce = String::from_utf8(announce_url).map_err(|e| e.utf8_error())?;

        //info
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"info") => {}
            Ok(_) => return Err(TorrentFileError::MissingInfoKey),
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };
        let info = dec.next_token()?;
        if info != Token::BeginDict {
            return Err(TorrentFileError::MissingInfoOpener);
        }

        //info:name
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"name") => {}
            Ok(_) => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };
        let name = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.name = String::from_utf8(name).map_err(|e| e.utf8_error())?;

        //info: piece length
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"piece length") => {}
            Ok(_) => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };
        let piece_length = match dec.next_token()? {
            Token::Int(len) => len,
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.piece_length = piece_length as usize;

        //info: pieces
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"pieces") => {}
            Ok(_) => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };
        let pieces = match dec.next_token()? {
            Token::String(cow) => cow.into_owned(),
            _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
        };
        torrent.info.pieces = pieces;

        //info: length OR files
        match dec.next_token() {
            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"length") => {
                // info: length
                let length = match dec.next_token()? {
                    Token::Int(len) => len,
                    _ => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
                };
                torrent.info.length = Some(length as usize);
            }

            Ok(Token::String(cow)) if cow == Cow::Borrowed(b"files") => {
                //info: files
                let files_list_opener = dec.next_token()?;
                if files_list_opener != Token::BeginList {
                    return Err(TorrentFileError::MissingListOfFilesOpener);
                }

                loop {
                    match dec.next_token()? {
                        Token::BeginDict => {
                            let mut file = File::default();

                            match dec.next_token()? {
                                Token::String(cow) if cow == Cow::Borrowed(b"length") => {
                                    let length = match dec.next_token()? {
                                        Token::Int(len) => len,
                                        _ => return Err(TorrentFileError::MissingFileLength),
                                    };
                                    file.length = length as usize;
                                }
                                _ => return Err(TorrentFileError::MissingFileLength),
                            }

                            match dec.next_token()? {
                                Token::String(cow) if cow == Cow::Borrowed(b"path") => {
                                    match dec.next_token()? {
                                        Token::BeginList => (),
                                        _ => return Err(TorrentFileError::MissingFilePath),
                                    }

                                    let mut path = String::new();
                                    loop {
                                        let path_piece = match dec.next_token()? {
                                            Token::String(cow) => cow.into_owned(),
                                            Token::EndObject => break,
                                            _ => return Err(TorrentFileError::MissingFilePath), //FIXME:
                                                                                                //not file length
                                        };
                                        let path_piece = String::from_utf8(path_piece)
                                            .map_err(|e| e.utf8_error())?;
                                        path.reserve(path_piece.len());
                                        path.push_str(&path_piece);
                                    }
                                    file.path = path;
                                }
                                _ => return Err(TorrentFileError::MissingFileLength),
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
            }

            Ok(_) => return Err(TorrentFileError::MissingAnnounceKey), // FIXME: not announce key
            Err(e) => return Err(TorrentFileError::Decode(e)),
        };

        Ok(torrent)
    }
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
