use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

// these params are derived from a combination of RFC 9106 and "OWASP Password Storage Cheat Sheet"

// - https://datatracker.ietf.org/doc/html/rfc9106
// - https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html

//reccs from each source:
// - RFC 9106: 64 KiB memory, 6 iterations
// - OWASP   : 32 KiB memory, 3 iterations

const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_ITERATIONS: u32 = 6;
const ARGON2_PARALLELISM: u32 = 1;
const MIN_ARGON2_MEMORY_KIB: u32 = 32 * 1024;
const MIN_ARGON2_ITERATIONS: u32 = 3;
const MIN_ARGON2_PARALLELISM: u32 = 1;
const MAX_ARGON2_MEMORY_KIB: u32 = 256 * 1024;
const MAX_ARGON2_ITERATIONS: u32 = 10;
const MAX_ARGON2_PARALLELISM: u32 = 4;
const MASTER_VERIFIER: &[u8] = b"termii-master-verifier-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSecret {
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterPasswordConfig {
    pub salt_b64: String,
    pub verifier: EncryptedSecret,
    #[serde(default = "default_argon2_memory_kib")]
    pub argon2_memory_kib: u32,
    #[serde(default = "default_argon2_iterations")]
    pub argon2_iterations: u32,
    #[serde(default = "default_argon2_parallelism")]
    pub argon2_parallelism: u32,
}

fn default_argon2_memory_kib() -> u32 {
    ARGON2_MEMORY_KIB
}

fn default_argon2_iterations() -> u32 {
    ARGON2_ITERATIONS
}

fn default_argon2_parallelism() -> u32 {
    ARGON2_PARALLELISM
}

fn derive_key(
    password: &str,
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<[u8; 32]> {
    let params = Params::new(memory_kib, iterations, parallelism, Some(32))
        .map_err(|e| anyhow!("failed to build Argon2 params: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("failed to derive encryption key: {}", e))?;
    Ok(key)
}

fn validate_argon2_params(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<()> {
    if memory_kib < MIN_ARGON2_MEMORY_KIB {
        return Err(anyhow!(
            "Argon2 memory must be at least {} KiB.",
            MIN_ARGON2_MEMORY_KIB
        ));
    }
    if memory_kib > MAX_ARGON2_MEMORY_KIB {
        return Err(anyhow!(
            "Argon2 memory must be at most {} KiB.",
            MAX_ARGON2_MEMORY_KIB
        ));
    }
    if iterations < MIN_ARGON2_ITERATIONS {
        return Err(anyhow!(
            "Argon2 iterations must be at least {}.",
            MIN_ARGON2_ITERATIONS
        ));
    }
    if iterations > MAX_ARGON2_ITERATIONS {
        return Err(anyhow!(
            "Argon2 iterations must be at most {}.",
            MAX_ARGON2_ITERATIONS
        ));
    }
    if parallelism < MIN_ARGON2_PARALLELISM {
        return Err(anyhow!(
            "Argon2 parallelism must be at least {}.",
            MIN_ARGON2_PARALLELISM
        ));
    }
    if parallelism > MAX_ARGON2_PARALLELISM {
        return Err(anyhow!(
            "Argon2 parallelism must be at most {}.",
            MAX_ARGON2_PARALLELISM
        ));
    }
    Ok(())
}

pub fn validate_master_password_policy(password: &str) -> Result<()> {
    if password.chars().count() < 8 {
        return Err(anyhow!("Master password must be at least 8 characters."));
    }
    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(anyhow!(
            "Master password must include at least 1 uppercase letter."
        ));
    }
    if !password
        .chars()
        .any(|c| c.is_ascii_punctuation() && !c.is_ascii_whitespace())
    {
        return Err(anyhow!("Master password must include at least 1 symbol."));
    }
    Ok(())
}

pub fn create_master_password(password: &str) -> Result<(MasterPasswordConfig, [u8; 32])> {
    validate_master_password_policy(password)?;

    let mut salt = [0u8; 16]; // 16 bytes
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(
        password,
        &salt,
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
    )?;
    let verifier = encrypt_secret_bytes(&key, MASTER_VERIFIER)?;

    Ok((
        MasterPasswordConfig {
            salt_b64: B64.encode(salt),
            verifier,
            argon2_memory_kib: ARGON2_MEMORY_KIB,
            argon2_iterations: ARGON2_ITERATIONS,
            argon2_parallelism: ARGON2_PARALLELISM,
        },
        key,
    ))
}

pub fn unlock_master_password(config: &MasterPasswordConfig, password: &str) -> Result<[u8; 32]> {
    let salt = B64
        .decode(&config.salt_b64)
        .context("invalid master-password salt encoding")?;
    validate_argon2_params(
        config.argon2_memory_kib,
        config.argon2_iterations,
        config.argon2_parallelism,
    )?;

    let key = derive_key(
        password,
        &salt,
        config.argon2_memory_kib,
        config.argon2_iterations,
        config.argon2_parallelism,
    )?;
    let clear = decrypt_secret_bytes(&key, &config.verifier)?;
    if clear.ct_eq(MASTER_VERIFIER).unwrap_u8() != 1 {
        return Err(anyhow!("Incorrect master password."));
    }
    Ok(key)
}

pub fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> Result<EncryptedSecret> {
    encrypt_secret_bytes(key, plaintext.as_bytes())
}

pub fn decrypt_secret(key: &[u8; 32], encrypted: &EncryptedSecret) -> Result<String> {
    let bytes = decrypt_secret_bytes(key, encrypted)?;
    String::from_utf8(bytes).context("decrypted bytes are not valid UTF-8")
}

fn encrypt_secret_bytes(key: &[u8; 32], plaintext: &[u8]) -> Result<EncryptedSecret> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; 24]; // 24 bytes
                               // generate a random nonce for each encryption
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext)
        .map_err(|_| anyhow!("encryption failed"))?;

    Ok(EncryptedSecret {
        nonce_b64: B64.encode(nonce),
        ciphertext_b64: B64.encode(ciphertext),
    })
}

fn decrypt_secret_bytes(key: &[u8; 32], encrypted: &EncryptedSecret) -> Result<Vec<u8>> {
    let nonce = B64
        .decode(&encrypted.nonce_b64)
        .context("invalid encrypted nonce encoding")?;
    if nonce.len() != 24 {
        // ensure that the nonce is 24 bytes
        return Err(anyhow!("invalid encrypted nonce length"));
    }
    let ciphertext = B64
        .decode(&encrypted.ciphertext_b64)
        .context("invalid encrypted ciphertext encoding")?;

    // chahca20 for the stream cipher
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| anyhow!("decryption failed"))
}
