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
    pieces: String, // NOTE: or Vec<String>?
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
    #[error(".torrent file is missing announce key in metainfo section")]
    MissingAnnounceKey,
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
            _ => return Err(TorrentFileError::MissingAnnounceKey),
        };
        torrent.announce = String::from_utf8(announce_url).map_err(|e| e.utf8_error())?;

        //info

        //info:piece_length

        Ok(torrent)
    }
}
