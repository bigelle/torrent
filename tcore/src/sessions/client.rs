use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use chrono::Utc;
use rand::RngCore;
use sha1::{Digest, Sha1};
use thiserror::Error;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};

use crate::{bencode::Torrent, sessions::tracker::TrackerBuilder};

pub struct Session {
    shared: Arc<SessionShared>,

    local_addr: SocketAddr,

    routes: Arc<Mutex<HashMap<[u8; 20], mpsc::Sender<(TcpStream, SocketAddr)>>>>,
}

pub(super) struct SessionShared {
    pub peer_id: [u8; 20],
    pub http: reqwest::Client,
}

impl Session {
    pub async fn make() -> Result<Session, Box<dyn std::error::Error>> {
        let peer_id = new_peer_id();
        let http = reqwest::Client::new();

        let shared = SessionShared { peer_id, http };

        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let local_addr = listener.local_addr()?;

        let (incomig_tx, mut incoming_rx) = mpsc::channel::<(TcpStream, SocketAddr)>(1024);

        let routes = Arc::new(Mutex::new(HashMap::<
            [u8; 20],
            mpsc::Sender<(TcpStream, SocketAddr)>,
        >::new()));
        let routes_cloned = routes.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let _ = incomig_tx.send((stream, addr)).await;
                    }
                    Err(_) => break,
                }
            }
        });

        tokio::spawn(async move {
            while let Some((stream, addr)) = incoming_rx.recv().await {
                // TODO:
                // 1. read first 19 bytes and make sure it says "BitTorrent Protocol"
                // 2. step through next 8 reserved empty bytes (maybe even check if they're actually
                //    empty)
                // 3. read next 20 bytes and check if there's a downloader waiting for stream
                // 4. forward this stream to downloader
                let info_hash: [u8; 20] = "let me compile"
                    .as_bytes()
                    .try_into()
                    .expect("shouldn't be any issues with this bytes");

                if let Some(tx) = routes_cloned.lock().await.get(&info_hash) {
                    tx.send((stream, addr));
                }
            }
        });

        Ok(Session {
            shared: Arc::new(shared),
            local_addr,
            routes,
        })
    }

    pub fn add_tracker(&self, torrent: Torrent) -> TrackerBuilder {
        TrackerBuilder::new(Arc::clone(&self.shared), torrent)
    }

    pub(super) async fn add_downloader(
        &mut self,
        info_hash: [u8; 20],
    ) -> Result<mpsc::Receiver<(TcpStream, SocketAddr)>, SessionError> {
        if self.routes.lock().await.contains_key(&info_hash) {
            return Err(SessionError::TrackerDublicate);
        }

        let (stream_tx, stream_rx) = mpsc::channel(128);
        self.routes.lock().await.insert(info_hash, stream_tx);

        Ok(stream_rx)
    }
}

fn new_peer_id() -> [u8; 20] {
    let ts = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp());

    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);

    let mut hasher = Sha1::new();
    hasher.update(ts.to_string().as_bytes());
    hasher.update(b"|BIBUSBOBUS42|"); // FIXME: awful
    hasher.update(&salt);

    hasher.finalize().into()
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("attempt to create a dublicate of tracker")]
    TrackerDublicate,
}
