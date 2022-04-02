pub use ctlogs_parser::parser::*;

use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct SignedTreeHead {
    pub tree_size: usize,
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operators {
    pub operators: Vec<Operator>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operator {
    pub name: String,
    pub logs: Vec<CtLog>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CtLog {
    pub log_id: String,
    pub url: String,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum State {
    Usable(Usable),
    Readonly(Readonly),
    Retired(Retired),
    Qualified(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usable {
    pub timestamp: DateTime<Utc>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Readonly {
    pub timestamp: String,
    pub final_tree_head: FinalTreeHead,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalTreeHead {
    pub sha256_root_hash: String,
    pub tree_size: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Retired {
    pub timestamp: String,
}
