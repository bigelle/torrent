use std::{
    fmt::{Display, write},
    time::Duration,
};

use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
    time,
};
use url::form_urlencoded;

use crate::sessions::tracker::{Command, TrackerBuilder, TrackerStatus};

pub struct Worker {
    command_rx: mpsc::Receiver<Command>,
    status_tx: watch::Sender<TrackerStatus>,
    stream_rx: mpsc::Receiver<TcpStream>,

    base_url: String,
    http: reqwest::Client,

    uploaded: u64,
    downloaded: u64,
    left: u64,

    worker_state: WorkerState,
    tracker_state: TrackerState,
}

#[derive(Default)]
enum WorkerState {
    #[default]
    Running,
    Paused,
    Aborted,
}

#[derive(Default)]
enum TrackerState {
    #[default]
    Started,
    Completed,
    Stopped,
    Empty,
}

impl Display for TrackerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackerState::Started => write!(f, "started"),
            TrackerState::Completed => write!(f, "completed"),
            TrackerState::Stopped => write!(f, "stopped"),
            TrackerState::Empty => write!(f, "empty"),
        }
    }
}

impl Worker {
    pub fn new(
        command_rx: mpsc::Receiver<Command>,
        status_tx: watch::Sender<TrackerStatus>,
        stream_rx: mpsc::Receiver<TcpStream>,
        context: TrackerBuilder,
    ) -> Worker {
        let base_url = format!(
            "{0}?info_hash={1}&peer_id={2}&port={3}",
            context.torrent.announce,
            escape_hash(
                context
                    .torrent
                    .info_hash()
                    .expect("torrent file must be decoded at this moment")
            ),
            escape_hash(context.session.peer_id),
            context.session.listen_addr.port(),
        );

        Worker {
            command_rx,
            status_tx,
            stream_rx,
            base_url,
            http: context.session.http.clone(),
            worker_state: WorkerState::Running,
            uploaded: 0,
            downloaded: 0,
            left: context.torrent.total_length(),
            tracker_state: TrackerState::Started,
        }
    }

    pub async fn work(&mut self) {
        loop {
            // 1. check for commands
            match self.command_rx.try_recv() {
                Ok(cmd) => self.handle_cmd(cmd),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.worker_state = WorkerState::Aborted
                }
                Err(mpsc::error::TryRecvError::Empty) => {}
            }

            // 2. do the job
            match self.worker_state {
                WorkerState::Paused => time::sleep(Duration::from_millis(500)).await,
                WorkerState::Aborted => break,
                WorkerState::Running => self.tick().await,
            }

            // 3. update tracker status
            // TODO:
        }
    }

    fn handle_cmd(&mut self, cmd: Command) {
        match cmd {
            Command::Pause => self.worker_state = WorkerState::Paused,
            Command::Resume => self.worker_state = WorkerState::Running,
            Command::Abort => self.worker_state = WorkerState::Aborted,
        }
    }

    fn build_url(&self) -> String {
        let dynamic_params = format!(
            "uploaded={0}&downloaded={1}&left={2}",
            self.uploaded, self.downloaded, self.left
        );
        format!("{0}&{1}", self.base_url, dynamic_params)
    }

    async fn tick(&mut self) {
        let url = self.build_url();
        dbg!(&url);

        let resp = self.http.get(url).send().await.unwrap();
        dbg!(resp.status());
        dbg!(resp.url()); 
        dbg!(resp.headers().get("content-type"));
        dbg!(resp.bytes().await.unwrap());
    }
}

mod tests {
    use crate::bencode::Torrent;

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn tick_real_tracker() {
        let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
        let (status_tx, _status_rx) = tokio::sync::watch::channel(TrackerStatus::default());
        let (_stream_tx, stream_rx) = tokio::sync::mpsc::channel(1);

        let info_hash =
            Torrent::from_file("../test_data/fixtures/ubuntu-25.04-desktop-amd64_archive.torrent")
                .expect("file exists and can be read")
                .info_hash()
                .expect("there must be info hash at this point");
        let info_hash = escape_hash(info_hash);

        let mut worker = Worker {
            command_rx: cmd_rx,
            status_tx,
            stream_rx,

            base_url: format!(
                "http://bt1.archive.org:6969/announce?info_hash={info_hash}&peer_id=RANDOMTESTPEERID1234&port=51413"
            ),
            http: reqwest::Client::new(),

            uploaded: 0,
            downloaded: 0,
            left: 123456,

            worker_state: WorkerState::default(),
            tracker_state: TrackerState::default(),
        };

        worker.tick().await;
    }
}

fn escape_hash(hash: [u8; 20]) -> String {
    form_urlencoded::byte_serialize(&hash[..]).collect()
}
