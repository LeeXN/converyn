//! Bounded summary pages. Cursors bind the collection and filters and never
//! authorize access; read access is shared within the authenticated instance.
use super::*;
use base64::Engine;
use serde::Deserialize;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct PageQuery {
    pub(super) search: Option<String>,
    pub(super) workflow: Option<String>,
    pub(super) preset: Option<String>,
    pub(super) actor: Option<String>,
    pub(super) after: Option<String>,
    #[serde(default = "default_limit")]
    pub(super) limit: usize,
    pub(super) status: Option<String>,
}
fn default_limit() -> usize {
    50
}
impl PageQuery {
    fn filters(&self) -> String {
        converyn_core::model::domain::digest(&(
            &self.status,
            &self.search,
            &self.workflow,
            &self.preset,
            &self.actor,
        ))
    }
    pub(super) fn after(&self, kind: &str) -> Result<String, Response> {
        if !(1..=100).contains(&self.limit) {
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
        let Some(after) = &self.after else {
            return Ok(String::new());
        };
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(after)
            .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
        let (k, filters, id): (String, String, String) =
            serde_json::from_slice(&raw).map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
        if k != kind || filters != self.filters() {
            return Err(StatusCode::BAD_REQUEST.into_response());
        }
        Ok(id)
    }
    pub(super) fn cursor(&self, kind: &str, id: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&(kind, self.filters(), id)).unwrap())
    }
}

pub(super) fn status_matches(
    status: Option<converyn_core::orchestration::machine::Status>,
    filter: &Option<String>,
) -> bool {
    filter.as_ref().is_none_or(|expected| {
        status.is_some_and(|actual| {
            serde_json::to_value(actual).is_ok_and(|v| v.as_str() == Some(expected.as_str()))
        })
    })
}
