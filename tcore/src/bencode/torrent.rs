use std::io::Read;

pub struct Torrent {
    announce: String,
    info: Info,
}

pub struct Info {
    name: String,
    piece_length: usize,
    pieces: String, // NOTE: or Vec<String>?
    length: Option<usize>,
    files: Option<Vec<File>>,
}

pub struct File {
    length: usize,
    path: String,
}

impl Torrent {
    pub fn from_file<R: Read>(file: R) -> Result<Torrent, &'static str> {
        todo!("read file using decoder manually using tokens")
    }
}
