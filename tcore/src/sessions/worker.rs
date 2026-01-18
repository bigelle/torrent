use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
};

use crate::sessions::tracker::{Command, TrackerBuilder, TrackerStatus};

pub struct Worker {
    command_rx: mpsc::Receiver<Command>,
    status_tx: watch::Sender<TrackerStatus>,
    stream_rx: mpsc::Receiver<TcpStream>,
    context: TrackerBuilder,
}

impl Worker {
    pub fn new(
        command_rx: mpsc::Receiver<Command>,
        status_tx: watch::Sender<TrackerStatus>,
        stream_rx: mpsc::Receiver<TcpStream>,
        context: TrackerBuilder,
    ) -> Worker {
        Worker {
            command_rx,
            status_tx,
            stream_rx,
            context,
        }
    }

    pub async fn work(&self) {
        todo!("actual work")
    }
}
