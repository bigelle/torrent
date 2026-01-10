#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::{env, fs};

use tcore::bencode::torrent::Torrent;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        panic!("this binary requires exactly 1 argument") // the binary itself is also an arg
    }

    let path = &args[1];

    let file = fs::read(path).expect("file must be available for reading");

    Torrent::from_file(&file).expect("torrent file must be parsed");
}
