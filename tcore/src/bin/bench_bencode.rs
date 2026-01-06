#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::{env, fs, io::BufReader};

use tcore::bencode;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        panic!("this binary requires exactly 1 argument") // the binary itself is also an arg
    }

    let path = &args[1];

    let file = fs::File::open(path).expect("file exists and can be read");
    let file = BufReader::new(file);

    let mut dec = bencode::Decoder::new(file);
    let value = dec.decode().expect("decoded value");
    println!("{}", value)
}
