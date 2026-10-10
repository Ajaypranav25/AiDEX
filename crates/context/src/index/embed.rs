//! Optional embeddings: the [`Embedder`] seam and the `vectors` table.
//!
//! AiDEX ships no default embedder; a later local embedding model plugs in here.
//! Vectors are stored per chunk and model and searched by brute-force cosine
//! similarity, which is adequate for one workspace and needs no extension.

use rusqlite::params;

use super::{Index, LineRange, SearchHit, search};
use crate::{ContextError, exclude};

/// Turns text into fixed-size vectors. Implementations must run locally (or be
/// gated by the caller's egress policy) and be deterministic per model.
pub trait Embedder: Send + Sync {
    /// Stable model identifier stored with each vector.
    fn model(&self) -> &str;
    fn dimensions(&self) -> usize;
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ContextError>;
}

fn encode(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn decode(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut left, mut right) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        dot += f64::from(*x) * f64::from(*y);
        left += f64::from(*x) * f64::from(*x);
        right += f64::from(*y) * f64::from(*y);
    }
    if left == 0.0 || right == 0.0 {
        0.0
    } else {
        dot / (left.sqrt() * right.sqrt())
    }
}

impl Index {
    /// Embed up to `limit` chunks that have no vector for `embedder`'s model yet;
    /// returns how many were stored.
    pub fn embed_missing(
        &self,
        embedder: &dyn Embedder,
        limit: usize,
    ) -> Result<usize, ContextError> {
        let pending: Vec<(i64, String)> = self.with_reader(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT c.id, c.text FROM chunks c
                     WHERE NOT EXISTS (SELECT 1 FROM vectors v WHERE v.chunk_id = c.id AND v.model = ?1)
                     ORDER BY c.id LIMIT ?2",
                )
                .map_err(|error| ContextError::sqlite("cannot list chunks to embed", error))?;
            let rows = statement
                .query_map(params![embedder.model(), limit as i64], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(|error| ContextError::sqlite("cannot list chunks to embed", error))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| ContextError::sqlite("cannot list chunks to embed", error))
        })?;
        if pending.is_empty() {
            return Ok(0);
        }
        let texts: Vec<&str> = pending.iter().map(|(_, text)| text.as_str()).collect();
        let vectors = embedder.embed(&texts)?;
        if vectors.len() != pending.len()
            || vectors
                .iter()
                .any(|vector| vector.len() != embedder.dimensions())
        {
            return Err(ContextError::InvalidRequest(
                "the embedder returned vectors of the wrong count or size".into(),
            ));
        }
        self.with_reader(|connection| {
            for ((chunk_id, _), vector) in pending.iter().zip(&vectors) {
                connection
                    .execute(
                        "INSERT OR REPLACE INTO vectors (chunk_id, model, dims, vector) VALUES (?1, ?2, ?3, ?4)",
                        params![chunk_id, embedder.model(), vector.len() as i64, encode(vector)],
                    )
                    .map_err(|error| ContextError::sqlite("cannot store a vector", error))?;
            }
            Ok(())
        })?;
        Ok(pending.len())
    }

    /// Chunks most similar to `text` under `embedder`, best first.
    pub fn semantic_search(
        &self,
        embedder: &dyn Embedder,
        text: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, ContextError> {
        let query = embedder.embed(&[text])?.into_iter().next().ok_or_else(|| {
            ContextError::InvalidRequest("the embedder returned no vector".into())
        })?;
        let terms = search::query_terms(text);
        self.with_reader(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT f.path, c.start_line, c.text, v.vector, f.language FROM vectors v
                     JOIN chunks c ON c.id = v.chunk_id JOIN files f ON f.id = c.file_id
                     WHERE v.model = ?1",
                )
                .map_err(|error| ContextError::sqlite("cannot search vectors", error))?;
            let rows = statement
                .query_map([embedder.model()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(|error| ContextError::sqlite("cannot search vectors", error))?;
            let mut hits = Vec::new();
            for row in rows {
                let (path, start, text, vector, language) =
                    row.map_err(|error| ContextError::sqlite("cannot search vectors", error))?;
                if exclude::is_secret_path(&path) {
                    continue;
                }
                let score = cosine(&query, &decode(&vector));
                let (line_range, snippet): (LineRange, String) =
                    search::snippet(&text, start, &terms);
                hits.push(SearchHit {
                    path,
                    line_range,
                    snippet: exclude::redact(&snippet).into_owned(),
                    score,
                    language,
                    symbol: None,
                });
            }
            hits.sort_by(|a, b| b.score.total_cmp(&a.score));
            hits.truncate(limit);
            Ok(hits)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_round_trip_and_compare() {
        let vector = [0.5f32, -1.0, 2.0];
        assert_eq!(decode(&encode(&vector)), vector);
        assert!((cosine(&vector, &vector) - 1.0).abs() < 1e-9);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }
}
