//! Log upload is independent of execution/reporting. Its durable cursor and queue
//! survive disconnects; bounded local retention may explicitly truncate old bytes.
use super::agent::{Client, Config, durable};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const CHUNK: usize = 65536;
pub async fn run(config: Config, api: Client) {
    let mut delay = 2;
    loop {
        match tick(&config, &api).await {
            Ok(()) => delay = 2,
            Err(e) => {
                tracing::debug!(error=%e,"log upload deferred; execution proceeds independently");
                delay = (delay * 2).min(30);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    }
}
async fn tick(config: &Config, api: &Client) -> Result<()> {
    let cached = config.state_dir.join("log-storage.json");
    let fresh = std::fs::metadata(&cached)
        .ok()
        .and_then(|m| m.modified().ok())
        .is_some_and(|m| m.elapsed().is_ok_and(|v| v.as_secs() < 60));
    let v: Value = if fresh {
        serde_json::from_slice(&std::fs::read(&cached)?)?
    } else {
        let (status, v) = api
            .request(
                reqwest::Method::GET,
                &format!("/api/v1/agents/{}/log-storage", config.node_uid),
                None,
            )
            .await?;
        ensure!(status == 200, "log storage status unavailable");
        durable(&cached, &serde_json::to_vec(&v)?)?;
        v
    };
    if v["enabled"] != true {
        return Ok(());
    }
    let queue = config.state_dir.join("uploads");
    std::fs::create_dir_all(&queue)?;
    let mut candidates = Vec::new();
    if let Ok(bytes) = std::fs::read(config.state_dir.join("active.json")) {
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(job) => candidates.push((None, job)),
            Err(error) => {
                tracing::warn!(%error,"active log metadata unreadable; execution journal retained")
            }
        }
    }
    for file in std::fs::read_dir(&queue)? {
        let p = file?.path();
        if p.extension().is_some_and(|e| e == "json")
            && !p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".cursor.json")
        {
            match serde_json::from_slice(&std::fs::read(&p)?) {
                Ok(job) => candidates.push((Some(p.clone()), job)),
                Err(error) => {
                    tracing::warn!(%error,path=%p.display(),"corrupt upload metadata quarantined; original receipt retained");
                    std::fs::rename(&p, p.with_extension("invalid"))?;
                }
            }
            if candidates.len() >= 100 {
                break;
            }
        }
    }
    let mut deferred = None;
    for (pending, job) in candidates {
        let result:Result<()>=async {
        if job["work"]["outputs"].as_object().is_some_and(|o|o.values().any(|v|v["secret"]==true)) {
            if let Some(path)=&pending {std::fs::remove_file(path)?;}
            return Ok(());
        }
        let run = job["run_id"].as_str().context("upload run missing")?;
        let work = job["work"]["id"].as_str().context("upload work missing")?;
        let id = hex::encode(Sha256::digest(format!("{run}/{work}")));
        let cursor_path = queue.join(format!("{id}.cursor.json"));
        let mut cursor = std::fs::read(&cursor_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or(json!({}));
        let terminal = job["receipt"]["logs"].is_object();
        let mut done = true;
        for stream in ["stdout", "stderr"] {
            if cursor[format!("{stream}_complete")] == true {
                continue;
            }
            for _ in 0..128 {
            let path = if job["work"]["operation"]["kind"] == "exec" {
                let root = config.state_dir.join("executions").join(&id);
                let path = root.join(format!("{stream}.log"));
                if path.exists() {
                    path
                } else {
                    root.join(format!("supervisor.{stream}.log"))
                }
            } else {
                config
                    .state_dir
                    .join("native")
                    .join(format!("{id}.{stream}.log"))
            };
            let expected = job["receipt"]["logs"][stream]["stored_bytes"].as_u64();
            let offset = cursor[stream].as_u64().unwrap_or(0);
            let data = read_chunk(&path, offset)?;
            if !terminal && data.len() < CHUNK {
                if !data.is_empty() {
                    let sha = hex::encode(Sha256::digest(&data));
                    {
                        let body = json!({"grant":job["grant"],"offset":offset,"data":base64::engine::general_purpose::STANDARD.encode(&data),"sha256":sha});
                        let (status, _) = api.request(reqwest::Method::POST, &format!("/api/v1/agents/{}/logs/{run}/{work}/{stream}/live",config.node_uid),Some(&body)).await?;
                        ensure!(status == 200, "live log fragment deferred");
                        cursor[format!("{stream}_live_sha")] = json!(sha);
                        durable(&cursor_path, &serde_json::to_vec(&cursor)?)?;
                    }
                }
                done = false;
                break;
            }
            let missing = !path.exists() && expected.is_some_and(|n| n > offset);
            if missing {
                // Retention is allowed to reclaim terminal local logs to keep execution
                // available. Record the loss rather than synthesizing successful upload.
                durable(
                    &queue.join(format!("{id}.{stream}.missing")),
                    b"local log reclaimed before upload; remote stream incomplete",
                )?;
            }
            let complete =
                missing || terminal && expected.is_some_and(|n| offset + data.len() as u64 >= n);
            let sha = hex::encode(Sha256::digest(&data));
            let body = json!({"offset":offset,"data":base64::engine::general_purpose::STANDARD.encode(&data),"sha256":sha,"complete":complete,"missing":missing});
            let (status, result) = api
                .request(
                    reqwest::Method::POST,
                    &format!(
                        "/api/v1/agents/{}/logs/{run}/{work}/{stream}",
                        config.node_uid
                    ),
                    Some(&body),
                )
                .await?;
            ensure!(status == 200 && result["ok"] == true, "log chunk deferred");
            cursor[stream] = (offset + data.len() as u64).into();
            cursor[format!("{stream}_complete")] = complete.into();
            durable(&cursor_path, &serde_json::to_vec(&cursor)?)?;
            if !complete { done = false; } else { break; }
            }
        }
        if done && let Some(path) = pending {
            std::fs::remove_file(path)?;
            std::fs::File::open(&queue)?.sync_all()?;
        }
        Ok(())
        }.await;
        if let Err(error) = result {
            tracing::debug!(%error,"one log upload deferred; other jobs remain eligible");
            deferred = Some(error);
        }
    }
    if let Some(error) = deferred {
        return Err(error);
    }
    Ok(())
}
fn read_chunk(path: &Path, offset: u64) -> Result<Vec<u8>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut data = Vec::new();
    file.take(CHUNK as u64).read_to_end(&mut data)?;
    Ok(data)
}
