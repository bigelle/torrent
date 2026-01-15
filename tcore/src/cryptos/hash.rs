use sha1::{Sha1, Digest};

pub fn make_sha1(src: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();

    hasher.update(src);

    hasher.finalize().into()
}

pub fn is_equal_sha1(left: &[u8; 20], right: &[u8; 20]) -> bool {
    *left == *right
}

pub fn is_equal_sha1_slice(left: &[u8], right: &[u8]) -> bool {
    left[..] == right[..]
}

