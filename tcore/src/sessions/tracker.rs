use std::{path::PathBuf, sync::Arc};

use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
    task::JoinHandle,
};

use crate::{
    bencode::Torrent,
    sessions::{
        session::{SessionEvent, SessionShared},
        worker::Worker,
    },
};

pub struct TrackerBuilder {
    torrent: Torrent,
    save_to: PathBuf,
    session: Arc<SessionShared>,
}

impl TrackerBuilder {
    pub(super) fn new(session: Arc<SessionShared>, torrent: Torrent) -> TrackerBuilder {
        TrackerBuilder {
            torrent,
            save_to: "./".into(),
            session,
        }
    }

    pub fn save_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.save_to = path.into();
        self
    }

    pub async fn begin(self) -> Result<Tracker, TrackerError> {
        let (command_tx, command_rx) = mpsc::channel::<Command>(32);
        let (status_tx, status_rx) = watch::channel(TrackerStatus::default());

        let (stream_tx, stream_rx) = mpsc::channel::<TcpStream>(1024);

        let info_hash = match self.torrent.info_hash() {
            Some(hash) => hash,
            None => return Err(TrackerError::InvalidTorrent),
        };

        let _ = self
            .session
            .incoming_tx
            .send(SessionEvent::RegisterWorker(info_hash, stream_tx))
            .await;

        let worker = Worker::new(command_rx, status_tx, stream_rx, self);

        let join = tokio::spawn(async move {
            worker.work().await;
        });

        Ok(Tracker {
            status_rx,
            command_tx,
            join,
        })
    }
}

pub(super) enum Command {
    Pause,
    Resume,
    Abort,
}

#[derive(Default)]
pub struct TrackerStatus {}

pub struct Tracker {
    status_rx: watch::Receiver<TrackerStatus>,
    command_tx: mpsc::Sender<Command>,
    join: JoinHandle<()>,
}

#[derive(Error, Debug)]
pub enum TrackerError {
    #[error("invalid torrent file")]
    InvalidTorrent,
}
