//! Native dataflow scheduling and grant-scoped secret delivery. No secondary service/state.
use super::*;
use converyn_core::{
    model::outputs::{Specification, StoredValue},
    orchestration::machine::Status,
};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) async fn materialize_loop(a: App) {
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if a.raft.metrics().borrow().current_leader != Some(a.id) {
            continue;
        }
        for _ in 0..32 {
            let Ok(m) = a.store.machine().await else {
                break;
            };
            let candidate = m.state.runs.values().find_map(|r| {
                r.materializable()
                    .first()
                    .cloned()
                    .map(|work| (r.id.clone(), work, r.revision))
            });
            let c = if let Some((run_id, work_id, expected_revision)) = candidate {
                Command::Materialize {
                    run_id,
                    work_id,
                    expected_revision,
                    now_ms: crate::executors::host::now_ms(),
                }
            } else {
                break;
            };
            match a
                .raft
                .client_write(authorized(c, &a.peer_fingerprint))
                .await
            {
                Ok(reply) if reply.data.ok => {}
                _ => break,
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputsQuery {
    grant: String,
}

fn redact_values(value: &serde_json::Value, patterns: &mut Vec<Vec<u8>>) {
    match value {
        serde_json::Value::Object(object) => {
            for value in object.values() {
                redact_values(value, patterns)
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                redact_values(value, patterns)
            }
        }
        serde_json::Value::String(text) => {
            if !text.is_empty() {
                patterns.push(text.as_bytes().to_vec())
            }
        }
        _ => patterns.push(value.to_string().into_bytes()),
    }
}
fn resolve_inputs(
    run: &converyn_core::orchestration::machine::Run,
    work: &converyn_core::model::domain::Work,
    keys: &crate::credentials::outputs::Keyring,
    now: u64,
    secrets: &BTreeMap<String, BTreeMap<String, converyn_core::model::outputs::SealedValue>>,
) -> Result<crate::credentials::secrets::Resolved> {
    let environment = crate::credentials::secrets::environment(&work.operation)?;
    let mut result = crate::credentials::secrets::Resolved::default();
    let mut opened: BTreeMap<(String, String), serde_json::Value> = BTreeMap::new();
    for (name, value) in &environment {
        if let converyn_core::model::variables::Value::Secret(reference) = value {
            let sealed = secrets
                .get(&reference.secret)
                .and_then(|v| v.get(&reference.version))
                .context("required_secret_missing")?;
            let plain = crate::credentials::secrets::open(
                keys,
                &reference.secret,
                &reference.version,
                sealed,
            )?;
            result.redactions.push(plain.as_bytes().to_vec());
            if let Ok(document) = serde_json::from_str::<serde_json::Value>(&plain) {
                redact_values(&document, &mut result.redactions);
            }
            result.values.insert(name.clone(), plain);
            continue;
        }
        let converyn_core::model::variables::Value::OutputSecret(reference) = value else {
            continue;
        };
        let reference = &reference.output_secret;
        ensure!(
            reference.run_id == run.id,
            "cross_run_secret_reference_denied"
        );
        let mut values = Vec::new();
        let mut commitments = Vec::new();
        for id in &reference.work_ids {
            let producer = run.declared_work(id).context("secret producer missing")?;
            ensure!(
                producer.step_id == reference.step,
                "secret producer differs"
            );
            let specification: &Specification = producer
                .outputs
                .get(&reference.name)
                .context("secret contract missing")?;
            let attempt = &run.attempts[id];
            ensure!(
                attempt.status == Status::Succeeded,
                "secret producer is not successful"
            );
            let at = attempt.completed_ms.context("secret commit time missing")?;
            ensure!(
                now < at.saturating_add(specification.ttl_seconds * 1000),
                "secret_output_expired"
            );
            let output = attempt
                .receipt
                .as_ref()
                .and_then(|r| r.outputs.get(&reference.name))
                .context("secret result missing")?;
            commitments.push((id, output));
            let StoredValue::Secret { sealed } = output else {
                anyhow::bail!("secret result differs")
            };
            let cache = (id.clone(), reference.name.clone());
            if !opened.contains_key(&cache) {
                let plain = keys.open(
                    sealed,
                    &crate::credentials::outputs::binding(&run.id, id, &reference.name),
                    specification,
                )?;
                opened.insert(cache.clone(), plain);
            }
            let original = &opened[&cache];
            let selected = reference
                .pointer
                .as_ref()
                .map_or(Some(original), |p| original.pointer(p))
                .context("secret_output_pointer_missing")?
                .clone();
            redact_values(&selected, &mut result.redactions);
            values.push(
                json!({"node_uid":producer.node_uid,"batch":producer.batch,"value":selected}),
            );
        }
        ensure!(
            converyn_core::model::domain::digest(&commitments) == reference.sha256,
            "secret input commitment differs"
        );
        let value = if reference.collect {
            serde_json::Value::Array(values)
        } else {
            ensure!(values.len() == 1, "secret input is ambiguous");
            values.remove(0)["value"].take()
        };
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        if !text.is_empty() {
            result.redactions.push(text.as_bytes().to_vec());
        }
        result.values.insert(name.clone(), text);
    }
    let public_bytes: usize = environment
        .values()
        .filter_map(|v| {
            if let converyn_core::model::variables::Value::Literal(s) = v {
                Some(s.len())
            } else {
                None
            }
        })
        .sum();
    ensure!(
        public_bytes + result.values.values().map(String::len).sum::<usize>() <= 128 * 1024,
        "resolved_input_environment_too_large"
    );
    result.redactions.sort();
    result.redactions.dedup();
    Ok(result)
}

/// This endpoint deliberately excludes human administrators: only this Node's
/// existing committed grant may receive its own ephemeral plaintext inputs.
pub(super) async fn work_inputs(
    State(a): State<App>,
    ApiPath((node, run_id, work_id)): ApiPath<(String, String, String)>,
    Query(query): Query<InputsQuery>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
) -> Response {
    if let Err(response) = node_proof(&a, &node, &headers, &peer.0).await {
        return response;
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(Some(run)) = a.store.load_run(&run_id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(work) = run.effective_work(&work_id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let attempt = &run.attempts[&work_id];
    if work.node_uid != node
        || attempt.grant.as_ref() != Some(&query.grant)
        || !matches!(attempt.status, Status::Authorized | Status::Running)
        || run.cancel_requested
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(path) = a.output_keyring.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code":"output_key_unavailable"})),
        )
            .into_response();
    };
    let Ok(_permit) = a.plugin_admission.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let Ok(machine) = a.store.machine().await else {
        return unavailable(&a);
    };
    let secrets = machine.state.administration.secrets.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        let keys = crate::credentials::outputs::Keyring::load(&path)
            .map_err(|_| anyhow::anyhow!("output_decryption_key_unavailable"))?;
        resolve_inputs(
            &run,
            &work,
            &keys,
            crate::executors::host::now_ms(),
            &secrets,
        )
    })
    .await;
    match resolved {
        Ok(Ok(inputs)) => (
            [(axum::http::header::CACHE_CONTROL, "no-store")],
            Json(inputs),
        )
            .into_response(),
        Ok(Err(error)) => {
            let message = error.to_string();
            let (status, code) = if message.contains("expired") {
                (StatusCode::GONE, "secret_output_expired")
            } else if message.contains("unavailable") {
                (StatusCode::SERVICE_UNAVAILABLE, "output_key_unavailable")
            } else {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_secret_output_input",
                )
            };
            (status, Json(json!({"code":code}))).into_response()
        }
        Err(_) => unavailable(&a),
    }
}
