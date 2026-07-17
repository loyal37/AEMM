use std::sync::Arc;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::profiles::validate_profile_name,
    database::{Database, ProfileStore},
    errors::AppError,
    models::Profile,
};

#[derive(Debug)]
pub struct ProfileService {
    store: ProfileStore,
    operation_lock: Arc<Mutex<()>>,
}

impl ProfileService {
    pub fn new(database: &Database, operation_lock: Arc<Mutex<()>>) -> Self {
        Self {
            store: ProfileStore::new(database.pool().clone()),
            operation_lock,
        }
    }

    pub async fn list(&self) -> Result<Vec<Profile>, AppError> {
        self.store.list().await
    }

    pub async fn get(&self, profile_id: Uuid) -> Result<Profile, AppError> {
        self.store.get(profile_id).await
    }

    pub async fn create(&self, name: String) -> Result<Profile, AppError> {
        let name = validate_profile_name(name)?;
        let _guard = self.operation_lock.lock().await;
        let profile = self.store.create(Uuid::new_v4(), &name).await?;
        tracing::info!(profile_id = %profile.id, name = %profile.name, "profile created");
        Ok(profile)
    }

    pub async fn rename(&self, profile_id: Uuid, name: String) -> Result<Profile, AppError> {
        let name = validate_profile_name(name)?;
        let _guard = self.operation_lock.lock().await;
        let profile = self.store.rename(profile_id, &name).await?;
        tracing::info!(profile_id = %profile.id, name = %profile.name, "profile renamed");
        Ok(profile)
    }

    pub async fn copy(&self, source_profile_id: Uuid, name: String) -> Result<Profile, AppError> {
        let name = validate_profile_name(name)?;
        let _guard = self.operation_lock.lock().await;
        let profile = self
            .store
            .copy(source_profile_id, Uuid::new_v4(), &name)
            .await?;
        tracing::info!(source_profile_id = %source_profile_id, profile_id = %profile.id, name = %profile.name, "profile copied");
        Ok(profile)
    }

    pub async fn delete(&self, profile_id: Uuid) -> Result<(), AppError> {
        let _guard = self.operation_lock.lock().await;
        self.store.delete(profile_id).await?;
        tracing::info!(profile_id = %profile_id, "profile deleted");
        Ok(())
    }
}
