use std::path::Path;
use thiserror::Error;
use tokio_rusqlite::Connection;
use visage_core::{Embedding, FaceModel};

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] tokio_rusqlite::Error),
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

/// SQLite-backed face model storage.
///
/// Uses `tokio-rusqlite` to run SQLite operations on a blocking thread
/// without starving the tokio runtime.
#[derive(Clone)]
pub struct FaceModelStore {
    conn: Connection,
}

impl FaceModelStore {
    /// Open (or create) the database at the given path and run migrations.
    pub async fn open(db_path: &Path) -> Result<Self, StoreError> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(db_path).await?;

        // Run migrations
        conn.call(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS faces (
                     id TEXT PRIMARY KEY,
                     user TEXT NOT NULL,
                     label TEXT NOT NULL,
                     embedding BLOB NOT NULL,
                     model_version TEXT NOT NULL,
                     quality_score REAL NOT NULL DEFAULT 0.0,
                     pose_label TEXT NOT NULL DEFAULT 'frontal',
                     created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_faces_user ON faces(user);",
            )?;
            Ok(())
        })
        .await?;

        Ok(Self { conn })
    }

    /// Insert a new face model. Returns the generated UUID.
    pub async fn insert(
        &self,
        user: &str,
        label: &str,
        embedding: &Embedding,
        quality_score: f32,
    ) -> Result<String, StoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let model_version = embedding
            .model_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let created_at = chrono::Utc::now().to_rfc3339();
        let blob = embedding_to_bytes(&embedding.values);

        let id_clone = id.clone();
        let user = user.to_string();
        let label = label.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO faces (id, user, label, embedding, model_version, quality_score, pose_label, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'frontal', ?7)",
                    rusqlite::params![id_clone, user, label, blob, model_version, quality_score, created_at],
                )?;
                Ok(())
            })
            .await?;

        Ok(id)
    }

    /// Get all face models for a user (the gallery for verification).
    pub async fn get_gallery_for_user(&self, user: &str) -> Result<Vec<FaceModel>, StoreError> {
        let user = user.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, user, label, embedding, model_version, created_at FROM faces WHERE user = ?1",
                )?;
                let rows = stmt.query_map([&user], |row| {
                    let id: String = row.get(0)?;
                    let user: String = row.get(1)?;
                    let label: String = row.get(2)?;
                    let blob: Vec<u8> = row.get(3)?;
                    let model_version: String = row.get(4)?;
                    let created_at: String = row.get(5)?;
                    Ok((id, user, label, blob, model_version, created_at))
                })?;

                let mut models = Vec::new();
                for row in rows {
                    let (id, user, label, blob, model_version, created_at) = row?;
                    models.push(FaceModel {
                        id,
                        user,
                        label,
                        embedding: Embedding {
                            values: bytes_to_embedding(&blob),
                            model_version: Some(model_version),
                        },
                        created_at,
                    });
                }
                Ok(models)
            })
            .await
            .map_err(StoreError::from)
    }

    /// List face models for a user (metadata only, no embeddings).
    pub async fn list_by_user(&self, user: &str) -> Result<Vec<ModelInfo>, StoreError> {
        let user = user.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, label, model_version, quality_score, created_at FROM faces WHERE user = ?1 ORDER BY created_at",
                )?;
                let rows = stmt.query_map([&user], |row| {
                    Ok(ModelInfo {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        model_version: row.get(2)?,
                        quality_score: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                })?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Remove a face model by ID, scoped to a user for cross-user protection.
    pub async fn remove(&self, user: &str, model_id: &str) -> Result<bool, StoreError> {
        let user = user.to_string();
        let model_id = model_id.to_string();
        self.conn
            .call(move |conn| {
                let affected =
                    conn.execute("DELETE FROM faces WHERE id = ?1 AND user = ?2", [&model_id, &user])?;
                Ok(affected > 0)
            })
            .await
            .map_err(StoreError::from)
    }

    /// Count total enrolled face models across all users.
    pub async fn count_all(&self) -> Result<u64, StoreError> {
        self.conn
            .call(|conn| {
                let count: u64 = conn.query_row("SELECT COUNT(*) FROM faces", [], |row| row.get(0))?;
                Ok(count)
            })
            .await
            .map_err(StoreError::from)
    }
}

/// Metadata about an enrolled face model (no embedding data).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub model_version: String,
    pub quality_score: f64,
    pub created_at: String,
}

/// Serialize f32 embedding to raw little-endian bytes.
fn embedding_to_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for &v in values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize raw little-endian bytes to f32 embedding.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_roundtrip() {
        let store = FaceModelStore::open(Path::new(":memory:")).await.unwrap();

        let embedding = Embedding {
            values: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            model_version: Some("w600k_r50".to_string()),
        };

        let id = store
            .insert("alice", "default", &embedding, 0.85)
            .await
            .unwrap();
        assert!(!id.is_empty());

        let gallery = store.get_gallery_for_user("alice").await.unwrap();
        assert_eq!(gallery.len(), 1);
        assert_eq!(gallery[0].id, id);
        assert_eq!(gallery[0].user, "alice");
        assert_eq!(gallery[0].label, "default");
        assert_eq!(gallery[0].embedding.values, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(
            gallery[0].embedding.model_version.as_deref(),
            Some("w600k_r50")
        );
    }

    #[tokio::test]
    async fn test_cross_user_protection() {
        let store = FaceModelStore::open(Path::new(":memory:")).await.unwrap();

        let emb = Embedding {
            values: vec![1.0; 5],
            model_version: None,
        };

        let id = store.insert("alice", "default", &emb, 0.9).await.unwrap();

        // Bob cannot see Alice's models
        let bob_gallery = store.get_gallery_for_user("bob").await.unwrap();
        assert!(bob_gallery.is_empty());

        // Bob cannot delete Alice's model
        let deleted = store.remove("bob", &id).await.unwrap();
        assert!(!deleted);

        // Alice can delete her own model
        let deleted = store.remove("alice", &id).await.unwrap();
        assert!(deleted);

        let gallery = store.get_gallery_for_user("alice").await.unwrap();
        assert!(gallery.is_empty());
    }

    #[tokio::test]
    async fn test_embedding_byte_fidelity() {
        // Verify that special float values survive the roundtrip
        let values = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            std::f32::consts::PI,
            0.123456789,
        ];
        let bytes = embedding_to_bytes(&values);
        let recovered = bytes_to_embedding(&bytes);
        assert_eq!(values.len(), recovered.len());
        for (orig, rec) in values.iter().zip(recovered.iter()) {
            assert_eq!(orig.to_bits(), rec.to_bits(), "mismatch: {orig} vs {rec}");
        }
    }

    #[tokio::test]
    async fn test_list_by_user() {
        let store = FaceModelStore::open(Path::new(":memory:")).await.unwrap();

        let emb = Embedding {
            values: vec![1.0; 5],
            model_version: Some("v1".to_string()),
        };

        store.insert("alice", "normal", &emb, 0.9).await.unwrap();
        store.insert("alice", "glasses", &emb, 0.8).await.unwrap();
        store.insert("bob", "default", &emb, 0.7).await.unwrap();

        let alice_models = store.list_by_user("alice").await.unwrap();
        assert_eq!(alice_models.len(), 2);
        assert_eq!(alice_models[0].label, "normal");
        assert_eq!(alice_models[1].label, "glasses");

        let count = store.count_all().await.unwrap();
        assert_eq!(count, 3);
    }
}
