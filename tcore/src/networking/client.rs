use reqwest::Client;
use serde::Serialize;
use thiserror::Error;

use crate::bencode::torrent::Torrent;

pub struct TorrentClient {
    client: Client,
    peer_id: [u8; 20],
    port: usize,
}

impl TorrentClient {
    fn new(file: Torrent) -> TorrentClient {
        TorrentClient {
            client: Client::new(),
            peer_id: generate_peer_id(),
            port: 6881, //FIXME: search for available port and give up on 6890
        }
    }

    async fn announce(
        &self,
        url: String,
        info_hash: [u8; 20],
    ) -> Result<TorrentResponse, TorrentClientError> {
        let query = &AnnounceRequest {
            info_hash: info_hash,
            peer_id: self.peer_id,
            port: self.port,
            ip: None,
            uploaded: 0,
            downloaded: 0,
            left: 0,
            event: None,
        };

        let body = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await?
            .bytes()
            .await?;

        self.parse_announce_response(&body)
    }

    fn parse_announce_response(&self, body: &[u8]) -> Result<TorrentResponse, TorrentClientError> {
        todo!("basically need another state machine")
    }
}

#[derive(Serialize)]
struct AnnounceRequest {
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    ip: Option<String>,
    port: usize,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    event: Option<String>, //FIXME: maybe is better to use enum
}

pub enum TorrentResponse {
    Success(TorrentResponseSuccess),
    Failed(String),
}

pub struct TorrentResponseSuccess {
    interval: usize,
    peers: Vec<Peer>,
}

pub struct Peer {
    peer_id: String, // is it String?
    ip: String,
    port: usize,
}

#[derive(Debug, Error)]
pub enum TorrentClientError {
    #[error("error sending HTTP request: {0}")]
    Reqwest(#[from] reqwest::Error),
}

fn generate_peer_id() -> [u8; 20] {
    todo!("figure it out")
}
