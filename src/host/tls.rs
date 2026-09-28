use axum_server::tls_rustls::RustlsConfig;
use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, KeyPair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::IpAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
use time::{Duration, OffsetDateTime};

use super::Result;

#[derive(Deserialize, Serialize)]
struct Identity {
    ip: IpAddr,
    expires: i64,
    cert: String,
    key: String,
}

pub async fn load_or_create(dir: &Path, ip: IpAddr) -> Result<(RustlsConfig, String)> {
    let identity = identity(dir, ip)?;
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(pem::parse(&identity.cert)?.contents())
    );
    let config =
        RustlsConfig::from_pem(identity.cert.into_bytes(), identity.key.into_bytes()).await?;
    Ok((config, fingerprint))
}

fn identity(dir: &Path, ip: IpAddr) -> Result<Identity> {
    let path = dir.join("tls.json");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if !meta.file_type().is_file() {
            return Err("TLS identity must be a regular file".into());
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let saved: Identity = serde_json::from_slice(&fs::read(&path)?)?;
        if saved.ip == ip
            && saved.expires > (OffsetDateTime::now_utc() + Duration::days(7)).unix_timestamp()
        {
            return Ok(saved);
        }
    }

    let now = OffsetDateTime::now_utc();
    let mut params = CertificateParams::new(vec![ip.to_string()])?;
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(365);
    params
        .distinguished_name
        .push(DnType::CommonName, "Pinhole");
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let fresh = Identity {
        ip,
        expires: params.not_after.unix_timestamp(),
        cert: cert.pem(),
        key: key.serialize_pem(),
    };
    let temp = dir.join("tls.json.tmp");
    let _ = fs::remove_file(&temp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    serde_json::to_writer(&mut file, &fresh)?;
    file.flush()?;
    fs::rename(temp, path)?;
    Ok(fresh)
}
