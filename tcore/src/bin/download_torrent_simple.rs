use std::env;

use tcore::{bencode::Torrent, sessions::session::Session, };

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        panic!("this binary expects 2 arguments")
    }
    let torrent = Torrent::from_file(&args[1])?;

    let session = Session::bind().await?;
    let tracker = session.add_torrent(torrent).save_to("./out").begin().await?;

    loop {
        let st = tracker.status();
        println!(
            "{:.1}% | ↓ {} KB/s | peers {} | seeds {}",
            st.progress * 100.0,
            st.download_speed / 1024,
            st.peers,
            st.seeds,
        );

        if st.is_finished {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    Ok(())
}
