#[cfg(test)]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

mod bencode;
