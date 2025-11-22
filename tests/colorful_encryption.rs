use aes_gcm::{
    aead::{Aead, KeyInit},
    aes::Aes128,
    AesGcm,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;

type Aes128Gcm16 = AesGcm<Aes128, aes_gcm::aead::generic_array::typenum::U16>;

const TOP_KEY: [u8; 16] = [
    96, 251, 63, 138, 10, 53, 0, 203, 150, 177, 136, 29, 83, 139, 70, 130,
];
const TOP_IV: [u8; 16] = [
    102, 221, 239, 247, 95, 55, 142, 28, 51, 165, 68, 208, 7, 250, 86, 5,
];

const BOTTOM_KEY: [u8; 16] = [
    90, 50, 20, 193, 6, 67, 43, 228, 131, 126, 89, 124, 77, 32, 217, 139,
];
const BOTTOM_IV: [u8; 16] = [
    122, 69, 187, 114, 43, 165, 6, 126, 105, 24, 156, 226, 43, 12, 151, 131,
];

fn read_file_bytes(path: &Path) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut f = File::open(path).expect("open file");
    f.read_to_end(&mut buf).expect("read file");
    buf
}

fn encrypt_with(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes128Gcm16::new_from_slice(key).expect("cipher");
    let nonce = aes_gcm::aead::generic_array::GenericArray::from_slice(iv);
    cipher.encrypt(nonce, plaintext).expect("encrypt")
}

fn base_path() -> &'static Path {
    Path::new("tests/data/colorful")
}

#[test]
fn top_ciphertext_tail_matches_sample() {
    let base = base_path();
    let plaintext = read_file_bytes(&base.join("test_top.svg"));
    let expected_full = read_file_bytes(&base.join("Fabrication_ColorfulTopSilkscreen.FCTS"));
    assert!(
        expected_full.len() > 512,
        "expected encrypted file to include RSA header"
    );

    let ciphertext = encrypt_with(&TOP_KEY, &TOP_IV, &plaintext);
    // Layout: RSA(enc_key)||RSA(enc_iv)||ciphertext_with_tag
    let tail = &expected_full[512..];
    assert_eq!(tail, ciphertext.as_slice());
}

#[test]
fn bottom_ciphertext_tail_matches_sample() {
    let base = base_path();
    let plaintext = read_file_bytes(&base.join("test_bottom.svg"));
    let expected_full = read_file_bytes(&base.join("Fabrication_ColorfulBottomSilkscreen.FCBS"));
    assert!(
        expected_full.len() > 512,
        "expected encrypted file to include RSA header"
    );

    let ciphertext = encrypt_with(&BOTTOM_KEY, &BOTTOM_IV, &plaintext);
    let tail = &expected_full[512..];
    assert_eq!(tail, ciphertext.as_slice());
}
