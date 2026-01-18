use std::{path::PathBuf, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{self},
        watch::{self},
    },
    task::JoinHandle,
};

use crate::{
    bencode::Torrent,
    sessions::{client::SessionShared, downloader::Downloader},
};

pub struct TrackerBuilder {
    session: Arc<SessionShared>,
    torrent: Torrent,
    save_to: PathBuf,
}

impl TrackerBuilder {
    pub(super) fn new(session: Arc<SessionShared>, torrent: Torrent) -> TrackerBuilder {
        TrackerBuilder {
            session,
            torrent,
            save_to: "./".into(),
        }
    }

    pub fn to(mut self, dir: impl Into<PathBuf>) -> Self {
        self.save_to = dir.into();
        self
    }

    // TODO: some other stuff

    pub async fn begin(self) -> Result<Tracker, TrackerError> {
        let (status_tx, status_rx) = watch::channel(Status::default());
        let (command_tx, command_rx) = mpsc::channel::<Command>(32);

        let mut downloader =
            Downloader::new(self.session.http.clone(), self, status_tx, command_rx);
        let join = tokio::spawn(async move {
            downloader.run().await;
        });

        Ok(Tracker {
            status_rx: status_rx,
            command_tx: command_tx,
            join: join,
        })
    }
}

#[derive(Clone, Default)]
pub struct Status {
    progress: f64,
    download_speed: u64,
    peers: u32,
    seeds: u32,
    is_finished: bool,
}

impl Status {
    pub fn progress(&self) -> f64 {
        self.progress
    }

    pub(crate) fn update_progress(&mut self, diff: f64) {
        self.progress += diff
    }

    pub fn download_speed(&self) -> u64 {
        self.download_speed
    }

    pub(crate) fn set_download_speed(&mut self, new: u64) {
        self.download_speed = new
    }

    pub fn peers(&self) -> u32 {
        self.peers
    }

    pub(crate) fn set_peers(&mut self, new: u32) {
        self.peers = new
    }

    pub fn seeds(&self) -> u32 {
        self.seeds
    }

    pub(crate) fn set_seeds(&mut self, new: u32) {
        self.seeds = new
    }

    pub fn is_finished(&self) -> bool {
        self.is_finished
    }

    pub(crate) fn set_is_finished(&mut self, new: bool) {
        self.is_finished = new
    }
}

// TODO: control commands
pub enum Command {}

pub struct Tracker {
    status_rx: watch::Receiver<Status>,
    command_tx: mpsc::Sender<Command>,
    join: JoinHandle<()>,
}

impl Tracker {
    pub fn status(&self) -> Status {
        self.status_rx.borrow().clone()
    }

    pub async fn status_async(&mut self) -> Status {
        self.status_rx.changed().await.expect("read current status"); // FIXME: no panics
        self.status_rx.borrow().clone()
    }

    // TODO: tracker controls
}

#[derive(Error, Debug)]
pub enum TrackerError {
    #[error("error downloading from tracker: {0}")]
    Io(#[from] std::io::Error),
}
