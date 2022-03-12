use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct LogEntry {
    /// The `leaf_input` field is a `String` of base64 encoded data. The data is a DER encoded
    /// MerkleTreeHeader, which has the following structure.
    ///
    /// ```no_compile
    /// [0] [1] [2..=9] [10..=11] [12..=14] [15..]
    /// |   |     |        |         |      |
    /// |   |     |        |         |      |- rest
    /// |   |     |        |         |
    /// |   |     |        |         |- length
    /// |   |     |        |
    /// |   |     |        | - log entry type
    /// |   |     |
    /// |   |     | - timestamp
    /// |   |
    /// |   | - signature type
    /// |
    /// | - version
    /// ```
    ///
    pub leaf_input: String,
    pub extra_data: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct STH {
    pub tree_size: usize,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operators {
    pub operators: Vec<Operator>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operator {
    pub name: String,
    pub email: Vec<String>,
    pub logs: Vec<Log>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    pub description: String,
    pub log_id: String,
    pub key: String,
    pub url: String,
    pub mmd: i64,
    pub state: State,
    pub temporal_interval: Option<TemporalInterval>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub usable: Option<Usable>,
    pub readonly: Option<Readonly>,
    pub retired: Option<Retired>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usable {
    pub timestamp: String,
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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalInterval {
    pub start_inclusive: String,
    pub end_exclusive: String,
}
