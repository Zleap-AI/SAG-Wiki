use crate::types::Candidate;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExploreDocument {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub hash: String,
    pub file_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploreOverview {
    pub scope: String,
    pub version: i64,
    pub algorithm: String,
    pub total: usize,
    pub documents: Vec<ExploreDocument>,
    pub known_documents: Vec<ExploreDocument>,
    pub indexing: bool,
    pub partial: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploreEdge {
    pub source: i64,
    pub target: i64,
    pub source_hash: String,
    pub target_hash: String,
    pub score: f64,
    pub terms: Vec<String>,
    pub snapshot: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploreNeighbors {
    pub version: i64,
    pub algorithm: String,
    pub snapshot: String,
    pub center: ExploreDocument,
    pub documents: Vec<ExploreDocument>,
    pub edges: Vec<ExploreEdge>,
    pub total: usize,
    pub has_more: bool,
    pub next_offset: usize,
    pub candidate_limit_reached: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidencePair {
    pub left: Candidate,
    pub right: Candidate,
    pub terms: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploreEvidence {
    pub version: i64,
    pub algorithm: String,
    pub left: ExploreDocument,
    pub right: ExploreDocument,
    pub pairs: Vec<EvidencePair>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploreGraph {
    pub scope: String,
    pub version: i64,
    pub snapshot: String,
    pub documents: Vec<ExploreDocument>,
    pub edges: Vec<ExploreEdge>,
}
