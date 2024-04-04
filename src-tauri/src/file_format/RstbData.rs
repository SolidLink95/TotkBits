use base64::decode;
use flate2::read::ZlibDecoder;
use std::io::Read;

pub fn get_rstb_data() -> Vec<String> {
    let decoded_bytes = decode(base64data)
        .expect("Failed to decode Base64 string");

    let mut zlib_decoder = ZlibDecoder::new(&decoded_bytes[..]);
    let mut decompressed_bytes = Vec::new();
    zlib_decoder
        .read_to_end(&mut decompressed_bytes)
        .expect("Failed to decompress data");
    let decompressed_string = String::from_utf8(decompressed_bytes)
        .expect("Failed to convert decompressed bytes to String");

    // Step 4: Split the string by ";" and collect into a Vec<String>
    let vec_strings: Vec<String> = decompressed_string
        .split(';')
        .map(|s| s.to_string())
        .collect();
    vec_strings
}