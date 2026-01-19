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
    pub(super) torrent: Torrent,
    pub(super) save_to: PathBuf,
    pub(super) session: Arc<SessionShared>,
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

        let mut worker = Worker::new(command_rx, status_tx, stream_rx, self);

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

#[derive(Default, Clone)]
#[readonly::make]
pub struct TrackerStatus {
    pub progress: f64,
    pub download_speed: u32,
    pub peers: u32,
    pub seeds: u32,
    pub is_finished: bool,
}

impl TrackerStatus {
    pub(super) fn update_progress(&mut self, diff: f64) {
        self.progress += diff
    }

    pub(super) fn set_download_speed(&mut self, new: u32) {
        self.download_speed = new
    }

    pub(super) fn set_peers(&mut self, new: u32) {
        self.peers = new
    }

    pub(super) fn set_seeds(&mut self, new: u32) {
        self.seeds = new
    }

    pub(super) fn finish(&mut self) {
        self.is_finished = true
    }
}

pub struct Tracker {
    status_rx: watch::Receiver<TrackerStatus>,
    command_tx: mpsc::Sender<Command>,
    join: JoinHandle<()>,
}

impl Tracker {
    pub fn status(&self) -> TrackerStatus {
        self.status_rx.borrow().clone()
    }
}

#[derive(Error, Debug)]
pub enum TrackerError {
    #[error("invalid torrent file")]
    InvalidTorrent,
}
