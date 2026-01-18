use tokio::{
    sync::{mpsc, watch},
};

use crate::sessions::{
    tracker::{Command, Status, TrackerBuilder},
};

pub(super) struct Downloader {
    builder: TrackerBuilder,
    status_tx: watch::Sender<Status>,
    command_rx: mpsc::Receiver<Command>,
    state: DownloaderState,

    client: reqwest::Client,
}

enum DownloaderState {
    // TODO:
    LETMECOMPILE,
}

impl Downloader {
    pub fn new(
        client: reqwest::Client,
        builder: TrackerBuilder,
        status_tx: watch::Sender<Status>,
        command_rx: mpsc::Receiver<Command>,
    ) -> Downloader {
        Downloader {
            client,
            builder,
            status_tx,
            command_rx,
            state: DownloaderState::LETMECOMPILE,
        }
    }

    pub async fn run(&mut self) {
        todo!("use tracker info to download stuff")
    }
}
