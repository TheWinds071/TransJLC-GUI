use aes_gcm::aead::generic_array::{typenum::U16, GenericArray};
use aes_gcm::{
    aead::{Aead, KeyInit},
    aes::Aes128,
    AesGcm,
};
use anyhow::{anyhow, Context, Result};
use rand::{rngs::OsRng, RngCore};
use rsa::pkcs8::DecodePublicKey;
use rsa::{Oaep, RsaPublicKey};
use sha2::Sha256;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub(crate) struct KeyMaterial {
    aes_key: [u8; 16],
    aes_iv: [u8; 16],
    enc_key: Vec<u8>,
    enc_iv: Vec<u8>,
}

impl KeyMaterial {
    pub(crate) fn generate(public_key_pem: &str) -> Result<Self> {
        let mut aes_key = [0u8; 16];
        let mut aes_iv = [0u8; 16];
        OsRng.fill_bytes(&mut aes_key);
        OsRng.fill_bytes(&mut aes_iv);

        let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
            .context("Invalid embedded RSA public key")?;
        let enc_key = public_key
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &aes_key)
            .context("Encrypt AES key")?;
        let enc_iv = public_key
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &aes_iv)
            .context("Encrypt AES IV")?;

        Ok(Self {
            aes_key,
            aes_iv,
            enc_key,
            enc_iv,
        })
    }
}

pub(crate) fn encrypt_and_write(
    svg: &str,
    key_material: &KeyMaterial,
    output: &Path,
) -> Result<()> {
    // AES-128-GCM with 16-byte nonce to mirror the original script (WebCrypto)
    type Aes128Gcm16 = AesGcm<Aes128, U16>;
    let cipher = Aes128Gcm16::new_from_slice(&key_material.aes_key).context("Create AES cipher")?;
    let nonce = GenericArray::<u8, U16>::from_slice(&key_material.aes_iv);
    let ciphertext = cipher
        .encrypt(nonce, svg.as_bytes())
        .map_err(|e| anyhow!("Encrypt silkscreen SVG: {:?}", e))?;

    let mut file = File::create(output).with_context(|| format!("Create {}", output.display()))?;
    file.write_all(&key_material.enc_key)
        .with_context(|| format!("Write AES key for {}", output.display()))?;
    file.write_all(&key_material.enc_iv)
        .with_context(|| format!("Write AES IV for {}", output.display()))?;
    file.write_all(&ciphertext)
        .with_context(|| format!("Write ciphertext for {}", output.display()))?;

    Ok(())
}
