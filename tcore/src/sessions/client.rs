use crate::{bencode::Torrent, sessions::tracker::TrackerBuilder};

pub struct Session {
    peer_id: [u8; 20],
}

impl Session {
    pub fn new() -> Session {
        Session {
            peer_id: "bibaboba012345678910".as_bytes().try_into().unwrap(), // FIXME: generate new
                                                                            // peer id for every session
        }
    }

    pub fn download(&self, torrent: &Torrent) -> TrackerBuilder {
        todo!("build a tracker builder") // TODO:
    }
}
