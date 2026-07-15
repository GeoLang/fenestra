//! Feature data source abstraction plus a Ptolemy-backed implementation.
//!
//! Layer/collection names map directly to Ptolemy dataset names. The default
//! branch is `main` when present, otherwise the first branch.

use async_trait::async_trait;
use fenestra_core::{Feature, FeatureCollection};
use serde::Deserialize;

/// Metadata about an available collection (a Ptolemy dataset).
#[derive(Debug, Clone)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub geometry_type: String,
}

#[derive(Debug)]
pub enum SourceError {
    NotFound(String),
    Upstream(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::NotFound(s) => write!(f, "not found: {s}"),
            SourceError::Upstream(s) => write!(f, "upstream error: {s}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// A source of geospatial features keyed by layer/collection name.
///
/// Implementations return features in EPSG:4326 (lon/lat); reprojection for
/// rendering happens in the request handlers.
#[async_trait]
pub trait FeatureSource: Send + Sync {
    /// List available collections.
    async fn collections(&self) -> Result<Vec<Collection>, SourceError>;

    /// Fetch features for a layer, up to `limit` (None means source default).
    async fn features(
        &self,
        layer: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Feature>, SourceError>;
}

#[derive(Deserialize)]
struct DatasetDto {
    id: String,
    name: String,
    #[serde(default)]
    geometry_type: String,
}

#[derive(Deserialize)]
struct BranchDto {
    id: String,
    name: String,
}

/// Feature source backed by a running Ptolemy instance.
pub struct PtolemyFeatureSource {
    base: String,
    client: reqwest::Client,
}

impl PtolemyFeatureSource {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Build from `PTOLEMY_URL`, defaulting to the in-network address.
    pub fn from_env() -> Self {
        let base =
            std::env::var("PTOLEMY_URL").unwrap_or_else(|_| "http://ptolemy:3000".to_string());
        Self::new(base)
    }

    async fn datasets(&self) -> Result<Vec<DatasetDto>, SourceError> {
        let url = format!("{}/api/v1/datasets", self.base);
        self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))?
            .json()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))
    }

    async fn dataset_id(&self, name: &str) -> Result<String, SourceError> {
        self.datasets()
            .await?
            .into_iter()
            .find(|d| d.name == name)
            .map(|d| d.id)
            .ok_or_else(|| SourceError::NotFound(format!("dataset {name}")))
    }

    async fn branch_id(&self, dataset_id: &str) -> Result<String, SourceError> {
        let url = format!("{}/api/v1/datasets/{dataset_id}/branches", self.base);
        let branches: Vec<BranchDto> = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))?
            .json()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))?;
        branches
            .iter()
            .find(|b| b.name == "main")
            .or_else(|| branches.first())
            .map(|b| b.id.clone())
            .ok_or_else(|| SourceError::NotFound(format!("branch for dataset {dataset_id}")))
    }
}

#[async_trait]
impl FeatureSource for PtolemyFeatureSource {
    async fn collections(&self) -> Result<Vec<Collection>, SourceError> {
        Ok(self
            .datasets()
            .await?
            .into_iter()
            .map(|d| Collection {
                title: d.name.clone(),
                id: d.name,
                geometry_type: d.geometry_type,
            })
            .collect())
    }

    async fn features(
        &self,
        layer: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Feature>, SourceError> {
        let dataset_id = self.dataset_id(layer).await?;
        let branch_id = self.branch_id(&dataset_id).await?;
        let mut url = format!("{}/api/v1/branches/{branch_id}/export/geojson", self.base);
        if let Some(limit) = limit {
            url.push_str(&format!("?limit={limit}"));
        }
        let fc: FeatureCollection = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))?
            .json()
            .await
            .map_err(|e| SourceError::Upstream(e.to_string()))?;
        Ok(fc.features)
    }
}
