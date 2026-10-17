#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Term {
    pub term: String,
    pub score: f64,
    pub kind: String,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Trace {
    pub hop: usize,
    pub candidates: Vec<i64>,
    pub terms: Vec<Term>,
    pub evidence: Vec<i64>,
}
