//! Independent output-encryption keys. Only sealed results enter consensus;
//! decryption is limited to an already granted consumer on the existing mTLS API.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use converyn_core::model::outputs::{PublicKey, SealedValue, Specification};
use p256::{
    SecretKey,
    elliptic_curve::sec1::ToEncodedPoint,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding},
};
use ring::{
    aead, hkdf,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    active: String,
    keys: BTreeMap<String, String>,
}
pub struct Keyring {
    active: String,
    keys: BTreeMap<String, SecretKey>,
}
fn random_secret() -> Result<SecretKey> {
    loop {
        let mut seed = [0u8; 32];
        SystemRandom::new()
            .fill(&mut seed)
            .map_err(|_| anyhow::anyhow!("output key generation failed"))?;
        if let Ok(key) = SecretKey::from_slice(&seed) {
            return Ok(key);
        }
    }
}
fn descriptor(key: &SecretKey) -> PublicKey {
    let encoded = key.public_key().to_encoded_point(true);
    PublicKey {
        id: hex::encode(Sha256::digest(encoded.as_bytes())),
        public_key: STANDARD.encode(encoded.as_bytes()),
    }
}
impl Keyring {
    pub fn load(path: &Path) -> Result<Self> {
        let raw: KeyFile = serde_json::from_str(&super::security::secret(path)?)
            .map_err(|_| anyhow::anyhow!("invalid output keyring"))?;
        ensure!(
            !raw.keys.is_empty() && raw.keys.len() <= 256,
            "invalid output keyring size"
        );
        let mut keys = BTreeMap::new();
        for (id, pem) in raw.keys {
            let key = SecretKey::from_pkcs8_pem(&pem)
                .map_err(|_| anyhow::anyhow!("invalid output private key"))?;
            ensure!(descriptor(&key).id == id, "output key fingerprint differs");
            keys.insert(id, key);
        }
        ensure!(
            keys.contains_key(&raw.active),
            "active output key is missing"
        );
        Ok(Self {
            active: raw.active,
            keys,
        })
    }
    pub fn public_key(&self) -> PublicKey {
        descriptor(&self.keys[&self.active])
    }
    pub fn generate(path: &Path, rotate: bool) -> Result<PublicKey> {
        let mut raw = if rotate {
            let text = super::security::secret(path)?;
            let _: Self = Self::load(path)?;
            serde_json::from_str::<KeyFile>(&text)?
        } else {
            ensure!(!path.exists(), "output keyring already exists");
            KeyFile {
                active: String::new(),
                keys: BTreeMap::new(),
            }
        };
        ensure!(
            raw.keys.len() < 256,
            "output keyring limit reached; preserve recovery keys"
        );
        let key = random_secret()?;
        let public = descriptor(&key);
        raw.keys.insert(
            public.id.clone(),
            key.to_pkcs8_pem(LineEnding::LF)?.to_string(),
        );
        raw.active = public.id.clone();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary =
            path.with_extension(format!("{}.new", crate::credentials::pki::random_token()?));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(&raw)?)?;
        file.sync_all()?;
        if !rotate {
            ensure!(!path.exists(), "refusing to replace output keyring");
        }
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(path.parent().context("keyring parent missing")?)?.sync_all()?;
        Ok(public)
    }
    pub fn open(&self, sealed: &SealedValue, aad: &[u8], spec: &Specification) -> Result<Value> {
        let key = self
            .keys
            .get(&sealed.key_id)
            .context("output decryption key unavailable")?;
        let ephemeral = p256::PublicKey::from_sec1_bytes(&STANDARD.decode(&sealed.ephemeral_key)?)
            .map_err(|_| anyhow::anyhow!("invalid output ephemeral key"))?;
        let shared = p256::ecdh::diffie_hellman(key.to_nonzero_scalar(), ephemeral.as_affine());
        let cipher = derived(shared.raw_secret_bytes(), &sealed.key_id, aad)?;
        let nonce: [u8; 12] = STANDARD
            .decode(&sealed.nonce)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid output nonce"))?;
        let mut data = STANDARD.decode(&sealed.ciphertext)?;
        let plain = cipher
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut data,
            )
            .map_err(|_| anyhow::anyhow!("sealed output authentication failed"))?;
        ensure!(
            plain.len() == sealed.plaintext_bytes,
            "output plaintext commitment differs"
        );
        let value: Value = serde_json::from_slice(plain)
            .map_err(|_| anyhow::anyhow!("invalid sealed output JSON"))?;
        ensure!(
            spec.secret && spec.accepts(&value),
            "sealed output differs from declared contract"
        );
        Ok(value)
    }
}
struct KeyLength;
impl hkdf::KeyType for KeyLength {
    fn len(&self) -> usize {
        32
    }
}
fn derived(shared: &[u8], id: &str, aad: &[u8]) -> Result<aead::LessSafeKey> {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, id.as_bytes());
    let prk = salt.extract(shared);
    let info = [aad];
    let output = prk
        .expand(&info, KeyLength)
        .map_err(|_| anyhow::anyhow!("output key derivation failed"))?;
    let mut bytes = [0u8; 32];
    output
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("output key derivation failed"))?;
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &bytes)
            .map_err(|_| anyhow::anyhow!("output encryption key invalid"))?,
    ))
}
pub fn binding(run: &str, work: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&("converyn-step-output-v1", run, work, name))
        .expect("output AAD is serializable")
}
pub fn seal(public: &PublicKey, value: &Value, aad: &[u8]) -> Result<SealedValue> {
    let bytes = STANDARD.decode(&public.public_key)?;
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == public.id,
        "output recipient fingerprint differs"
    );
    let recipient = p256::PublicKey::from_sec1_bytes(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid output recipient"))?;
    let ephemeral = random_secret()?;
    let shared = p256::ecdh::diffie_hellman(ephemeral.to_nonzero_scalar(), recipient.as_affine());
    let cipher = derived(shared.raw_secret_bytes(), &public.id, aad)?;
    let mut data = serde_json::to_vec(value)?;
    let plaintext_bytes = data.len();
    let mut nonce = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow::anyhow!("output nonce generation failed"))?;
    cipher
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut data,
        )
        .map_err(|_| anyhow::anyhow!("output encryption failed"))?;
    Ok(SealedValue {
        key_id: public.id.clone(),
        ephemeral_key: STANDARD.encode(ephemeral.public_key().to_encoded_point(true).as_bytes()),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(data),
        plaintext_bytes,
    })
}
