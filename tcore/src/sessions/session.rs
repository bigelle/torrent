use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use chrono::Utc;
use rand::RngCore;
use sha1::{Digest, Sha1};
use thiserror::Error;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
    task::JoinHandle,
};

use crate::{bencode::Torrent, sessions::tracker::TrackerBuilder};

pub struct Session {
    shared: Arc<SessionShared>,

    accept_join: JoinHandle<()>,
    dispath_join: JoinHandle<()>,

    routes: Arc<Mutex<HashMap<[u8; 20], mpsc::Sender<TcpStream>>>>,
}

pub(super) struct SessionShared {
    pub peer_id: [u8; 20],
    pub http: reqwest::Client,
    pub listen_addr: SocketAddr,
    pub incoming_tx: mpsc::Sender<SessionEvent>,
}

impl Session {
    pub async fn bind() -> Result<Session, SessionError> {
        let peer_id = new_peer_id();
        let http = reqwest::Client::new();

        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let listen_addr = listener.local_addr()?;

        let (incoming_tx, mut incoming_rx) = mpsc::channel::<SessionEvent>(1024);
        let incoming_tx_shared = incoming_tx.clone();

        let accept_join = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let _ = incoming_tx.send(SessionEvent::NewConn(stream, addr)).await; // FIXME: how do i handle
                        // the error?
                    }
                    Err(_) => break,
                }
            }
        });

        let routes = Arc::new(Mutex::new(HashMap::new()));
        let routes_clone = routes.clone();

        let dispath_join = tokio::spawn(async move {
            while let Some(event) = incoming_rx.recv().await {
                match event {
                    SessionEvent::NewConn(stream, addr) => {
                        // TODO:
                        // 1. read first 19 bytes
                        // 2. make sure it says "BitTorrent Protocol"
                        // 3. skip next 8 empty reserved bytes
                        // 4. read next 20 bytes as info hash
                        // 5. find a worker for this tracker
                        todo!(
                            "get info_hash from stream and find a worker. read more in comment below"
                        );
                    }
                    SessionEvent::RegisterWorker(key, tx) => {
                        if routes_clone.lock().await.contains_key(&key) {
                            todo!("do something with duplicate key?")
                        }
                        routes_clone.lock().await.insert(key, tx);
                    }
                    SessionEvent::UnregisterWorker(key) => {
                        routes_clone.lock().await.remove(&key);
                    }
                }
            }
        });

        Ok(Session {
            shared: Arc::new(SessionShared {
                peer_id,
                http,
                listen_addr,
                incoming_tx: incoming_tx_shared,
            }),
            accept_join,
            dispath_join,
            routes,
        })
    }

    pub fn add_torrent(&self, torrent: Torrent) -> TrackerBuilder {
        TrackerBuilder::new(self.shared.clone(), torrent)
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
    hasher.update(b"|");
    hasher.update(b"BIBUSBOBUS42"); //FIXME: some real user agent
    hasher.update(b"|");
    hasher.update(salt);

    hasher.finalize().into()
}

pub(super) enum SessionEvent {
    NewConn(TcpStream, SocketAddr),
    RegisterWorker([u8; 20], mpsc::Sender<TcpStream>),
    UnregisterWorker([u8; 20]),
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("i/o error:{0}")]
    Io(#[from] std::io::Error),
}
