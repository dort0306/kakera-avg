//! Owned domain diagnostics; no formatter, IO, platform or application dependency.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub parameters: BTreeMap<String, String>,
}
impl Diagnostic {
    pub fn new(code: &str, parameters: &[(&str, String)]) -> Self {
        Self {
            code: code.into(),
            parameters: parameters
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A stable machine representation survives third-party error context chains.
        // Only the presentation boundary resolves codes into human language.
        write!(
            f,
            "KDIAG{}",
            serde_json::to_string(self).map_err(|_| std::fmt::Error)?
        )
    }
}
impl std::error::Error for Diagnostic {}
impl From<String> for Diagnostic {
    fn from(reason: String) -> Self {
        Self::new("host-external-cause", &[("reason", reason)])
    }
}
impl From<&str> for Diagnostic {
    fn from(reason: &str) -> Self {
        reason.to_owned().into()
    }
}
