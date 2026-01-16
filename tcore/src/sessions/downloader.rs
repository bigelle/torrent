use tokio::sync::{mpsc, watch};

use crate::sessions::tracker::{Command, Status, TrackerBuilder};

pub(super) struct Downloader {
    builder: TrackerBuilder,
    status_tx: watch::Sender<Status>,
    command_rx: mpsc::Receiver<Command>,
    state: DownloaderState,
}

enum DownloaderState {
    // TODO:
    LETMECOMPILE,
}

impl Downloader {
    pub fn new(
        builder: TrackerBuilder,
        status_tx: watch::Sender<Status>,
        command_rx: mpsc::Receiver<Command>,
    ) -> Downloader {
        Downloader {
            builder: builder,
            status_tx: status_tx,
            command_rx: command_rx,
            state: DownloaderState::LETMECOMPILE,
        }
    }

    pub async fn start(&mut self) {}
}
