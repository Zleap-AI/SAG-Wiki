use super::{http, PreparedShare, UploadedShare};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct ShareRecord {
    pub code: String,
    pub name: String,
    pub shared_at: u64,
    pub expires_at: String,
}
#[derive(Serialize, Deserialize)]
struct StoredRecord {
    #[serde(flatten)]
    record: ShareRecord,
    request_id: String,
    request_key: String,
    #[serde(default, skip_serializing)]
    cancelled: bool,
}
fn access<T>(
    directory: &Path,
    operation: impl FnOnce(&mut Vec<StoredRecord>) -> Result<(T, bool)>,
) -> Result<T> {
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("share-history.lock"))?;
    lock.lock()?;
    let path = directory.join("share-history.json");
    let mut records: Vec<StoredRecord> = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("分享记录文件损坏")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let mut removed = false;
    let mut retained = Vec::with_capacity(records.len());
    for item in records {
        let deadline = chrono::DateTime::parse_from_rfc3339(&item.record.expires_at)
            .context("分享到期时间无效")?
            .timestamp_millis();
        if item.cancelled || deadline < 0 || deadline as u128 <= now {
            removed = true;
        } else {
            retained.push(item);
        }
    }
    records = retained;
    let (result, changed) = operation(&mut records)?;
    if changed || removed {
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(&mut file, &records)?;
        file.persist(path)?;
    }
    Ok(result)
}
pub(super) fn record(uploaded: &UploadedShare, prepared: &PreparedShare) -> Result<()> {
    let directory = sag_protocol::data_dir()?;
    access(&directory, |records| {
        if records
            .iter()
            .any(|item| item.request_id == prepared.preview.id)
        {
            return Ok(((), false));
        }
        records.insert(
            0,
            StoredRecord {
                record: ShareRecord {
                    code: uploaded.code.clone(),
                    name: uploaded.name.clone(),
                    shared_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
                    expires_at: uploaded.expires_at.clone(),
                },
                request_id: prepared.preview.id.clone(),
                request_key: prepared.request_key.clone(),
                cancelled: false,
            },
        );
        Ok(((), true))
    })
    .context("分享已上传，但本地记录保存失败")
}
pub fn history() -> Result<Vec<ShareRecord>> {
    access(&sag_protocol::data_dir()?, |records| {
        Ok((
            records.iter().map(|item| item.record.clone()).collect(),
            false,
        ))
    })
}
pub async fn cancel_share(code: &str) -> Result<()> {
    let directory = sag_protocol::data_dir()?;
    let (id, key) = access(&directory, |records| {
        let item = records
            .iter()
            .find(|item| item.record.code == code)
            .context("找不到分享记录")?;
        Ok(((item.request_id.clone(), item.request_key.clone()), false))
    })?;
    let mut proof = reqwest::header::HeaderValue::from_str(&key)?;
    proof.set_sensitive(true);
    let response = http()?
        .delete(super::ENDPOINT)
        .query(&[("code", code), ("requestId", &id)])
        .header("X-Share-Request-Key", proof)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("取消分享失败，请检查网络后重试"))?;
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            bail!("无法验证此分享的管理权限")
        }
        return Err(super::response_error(response).await);
    }
    access(&directory, |records| {
        records.retain(|item| item.request_id != id);
        Ok(((), true))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_history_removes_expired_records_and_proofs_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("share-history.json");
        let record = |code: &str, expires: &str| StoredRecord {
            record: ShareRecord {
                code: code.into(),
                name: "Notes".into(),
                shared_at: 1,
                expires_at: expires.into(),
            },
            request_id: code.into(),
            request_key: format!("secret-{code}"),
            cancelled: false,
        };
        fs::write(
            &path,
            serde_json::to_vec(&vec![
                record("EXPIRED1", "2020-01-01T00:00:00Z"),
                record("ACTIVE22", "2099-01-01T00:00:00Z"),
            ])
            .unwrap(),
        )
        .unwrap();
        access(dir.path(), |records| {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].record.code, "ACTIVE22");
            Ok(((), false))
        })
        .unwrap();
        let stored = fs::read_to_string(path).unwrap();
        assert!(!stored.contains("EXPIRED1"));
        assert!(!stored.contains("secret-EXPIRED1"));
        assert!(stored.contains("ACTIVE22"));
    }
    #[test]
    fn history_keeps_proofs_private_and_removes_legacy_cancelled_records() {
        let dir = tempfile::tempdir().unwrap();
        access(dir.path(), |records| {
            records.push(StoredRecord {
                record: ShareRecord {
                    code: "ABCDEFGH".into(),
                    name: "Notes".into(),
                    shared_at: 1,
                    expires_at: "2099-01-01T00:00:00Z".into(),
                },
                request_id: "request".into(),
                request_key: "secret".into(),
                cancelled: false,
            });
            Ok(((), true))
        })
        .unwrap();
        access(dir.path(), |records| {
            let public = serde_json::to_string(&records[0].record).unwrap();
            assert!(!public.contains("secret"));
            assert!(!public.contains("request"));
            Ok(((), false))
        })
        .unwrap();
        let path = dir.path().join("share-history.json");
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        legacy[0]["cancelled"] = true.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        access(dir.path(), |records| {
            assert!(records.is_empty());
            Ok(((), false))
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("share-history.json")).unwrap(),
            "[]"
        );
    }
}
