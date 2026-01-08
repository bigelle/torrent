use std::{
    borrow::Cow,
    io::{BufRead, BufReader, Error, Read},
    str::{self, SplitTerminator},
};

use thiserror::Error;

use crate::bencodev2::decoder::{DecodeError, Decoder, Token};

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
    #[error("expected key")]
    ExpectedKey,
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
    Begin,
    InMetaInfo,
    InInfo,
    InFile,
}

impl<'a> TorrentBuilder<'a> {
    fn new(src: &'a [u8]) -> TorrentBuilder<'a> {
        TorrentBuilder {
            state: TorrentBuilderState::Begin,
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

        loop {
            match self.state {
                TorrentBuilderState::Begin => {
                    let metainfo_open = dec.next_token()?;
                    if metainfo_open != Token::BeginDict {
                        return Err(TorrentFileError::MissingMetaInfoOpener);
                    }
                    self.state = TorrentBuilderState::InMetaInfo;
                }

                TorrentBuilderState::InMetaInfo => {
                    let token = dec.next_token()?;

                    match token {
                        Token::String(key) => self.handle_meta_keys(key, &mut dec, &mut torrent)?,
                        Token::EndObject => break,
                        _ => return Err(TorrentFileError::ExpectedKey),
                    };
                }

                TorrentBuilderState::InInfo => todo!("expect info keys or section closing token"),

                TorrentBuilderState::InFile => {
                    todo!("expect file path, length and section closing token")
                }
            }
        }

        Ok(torrent)
    }

    fn handle_meta_keys<'b>(
        &mut self,
        key: Cow<'b, [u8]>,
        dec: &mut Decoder<'b>,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        match &*key {
            b"announce" => self.handle_announce(dec, torrent),
            b"info" => {
                self.state = TorrentBuilderState::InInfo;
                Ok(())
            }
            _ => todo!("use key skipper"),
        }
    }

    fn handle_announce<'b>(
        &self,
        dec: &mut Decoder<'b>,
        torrent: &mut Torrent,
    ) -> Result<(), TorrentFileError> {
        let announce_url = match dec.next_token()? {
            Token::String(url) => url,
            _ => return Err(TorrentFileError::MissingAnnounceURL),
        };

        let announce_url =
            String::from_utf8(announce_url.into_owned()).map_err(|e| e.utf8_error())?;
        torrent.announce = announce_url;
        Ok(())
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
