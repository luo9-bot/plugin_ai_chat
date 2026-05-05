use p256::ecdsa::{SigningKey, Signature, signature::{Signer, Verifier}};
use p256::ecdsa::VerifyingKey;
use p256::EncodedPoint;
use once_cell::sync::OnceCell;
use rand_core::OsRng;
use tracing::debug;

use crate::config;

static SIGNING_KEY: OnceCell<SigningKey> = OnceCell::new();

fn key_path() -> std::path::PathBuf {
    config::data_dir().join("ecc_key.pem")
}

/// 生成或加载 ECC 密钥对 (首次运行自动生成)
pub fn init() {
    let key = load_or_generate();
    let _ = SIGNING_KEY.set(key);
    let pubkey_hex = public_key_hex();
    debug!(public_key = %pubkey_hex, "crypto: ECC key ready");
}

fn load_or_generate() -> SigningKey {
    let path = key_path();
    if let Ok(hex_str) = std::fs::read_to_string(&path) {
        let hex_str = hex_str.trim();
        if let Ok(bytes) = hex::decode(hex_str) {
            if let Ok(key) = SigningKey::from_bytes(bytes.as_slice().into()) {
                return key;
            }
        }
    }
    let key = SigningKey::random(&mut OsRng);
    let hex_str = hex::encode(key.to_bytes());
    std::fs::write(&path, hex_str).ok();
    key
}

fn get_signing_key() -> &'static SigningKey {
    SIGNING_KEY.get().expect("crypto not initialized")
}

/// 获取公钥 (未压缩格式 hex: 04 + x + y)
pub fn public_key_hex() -> String {
    let key = get_signing_key();
    let verifying_key = VerifyingKey::from(key);
    let point = verifying_key.to_encoded_point(false);
    hex::encode(point.as_bytes())
}

/// 签名消息，返回 hex 编码的签名
pub fn sign_message(message: &str) -> String {
    let key = get_signing_key();
    let signature: Signature = key.sign(message.as_bytes());
    hex::encode(signature.to_bytes())
}

/// 验证签名 (用于本地测试)
pub fn verify_signature(public_key_hex: &str, message: &str, signature_hex: &str) -> bool {
    let Ok(pub_bytes) = hex::decode(public_key_hex) else { return false };
    let Ok(point) = EncodedPoint::from_bytes(&pub_bytes) else { return false };
    let Ok(verifying_key) = VerifyingKey::from_encoded_point(&point) else { return false };
    let Ok(sig_bytes) = hex::decode(signature_hex) else { return false };
    let Ok(signature) = Signature::from_slice(&sig_bytes) else { return false };
    verifying_key.verify(message.as_bytes(), &signature).is_ok()
}
