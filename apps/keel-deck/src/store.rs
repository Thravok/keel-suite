//! Local JSON profile persistence (fully offline).

use crate::layout::{default_profile, ensure_page_size, ensure_profile_extras};
use crate::model::{AppSettings, LayoutKind, Page, Profile};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct DiskState {
    settings: AppSettings,
    profiles: Vec<Profile>,
}

pub struct ProfileStore {
    root: PathBuf,
    inner: Mutex<DiskState>,
}

impl ProfileStore {
    pub fn open(app_data: &Path) -> Result<Arc<Self>, StoreError> {
        let root = app_data.join("keel");
        fs::create_dir_all(root.join("profiles"))?;
        let path = root.join("state.json");
        let mut state = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            let profile = default_profile();
            DiskState {
                settings: AppSettings {
                    active_profile_id: Some(profile.id.clone()),
                    ..AppSettings::default()
                },
                profiles: vec![profile],
            }
        };
        for profile in &mut state.profiles {
            ensure_profile_extras(profile);
        }
        let store = Arc::new(Self {
            root,
            inner: Mutex::new(state),
        });
        store.persist()?;
        Ok(store)
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn persist(&self) -> Result<(), StoreError> {
        let guard = self.inner.lock();
        let json = serde_json::to_string_pretty(&*guard)?;
        fs::write(self.state_path(), json)?;
        Ok(())
    }

    pub fn list_profiles(&self) -> Vec<Profile> {
        self.inner.lock().profiles.clone()
    }

    pub fn settings(&self) -> AppSettings {
        self.inner.lock().settings.clone()
    }

    pub fn set_settings(&self, settings: AppSettings) -> Result<(), StoreError> {
        self.inner.lock().settings = settings;
        self.persist()
    }

    pub fn active_profile(&self) -> Option<Profile> {
        let guard = self.inner.lock();
        let id = guard.settings.active_profile_id.as_ref()?;
        guard.profiles.iter().find(|p| &p.id == id).cloned()
    }

    pub fn set_active_profile(&self, id: String) -> Result<(), StoreError> {
        let mut guard = self.inner.lock();
        if !guard.profiles.iter().any(|p| p.id == id) {
            return Err(StoreError::Msg("profile not found".into()));
        }
        guard.settings.active_profile_id = Some(id);
        drop(guard);
        self.persist()
    }

    pub fn save_profile(&self, mut profile: Profile) -> Result<Profile, StoreError> {
        ensure_profile_extras(&mut profile);
        if profile.pages.is_empty() {
            return Err(StoreError::Msg("profile needs at least one page".into()));
        }
        if !profile
            .pages
            .iter()
            .any(|p| p.id == profile.current_page_id)
        {
            profile.current_page_id = profile.pages[0].id.clone();
        }
        let mut guard = self.inner.lock();
        if let Some(slot) = guard.profiles.iter_mut().find(|p| p.id == profile.id) {
            *slot = profile.clone();
        } else {
            guard.profiles.push(profile.clone());
        }
        if guard.settings.active_profile_id.is_none() {
            guard.settings.active_profile_id = Some(profile.id.clone());
        }
        drop(guard);
        self.persist()?;
        Ok(profile)
    }

    pub fn create_profile(&self, name: String, layout: LayoutKind) -> Result<Profile, StoreError> {
        let page_id = Uuid::new_v4().to_string();
        let mut page = Page {
            id: page_id.clone(),
            name: "Home".into(),
            keys: vec![],
        };
        ensure_page_size(&mut page, layout);
        let mut profile = Profile {
            id: Uuid::new_v4().to_string(),
            name,
            layout,
            pages: vec![page],
            current_page_id: page_id,
            page_stack: vec![],
            encoders: vec![],
            touch_points: vec![],
        };
        ensure_profile_extras(&mut profile);
        self.save_profile(profile)
    }

    /// Switch active profile layout to match a connected device (resize keys/dials).
    pub fn adopt_layout(&self, layout: LayoutKind) -> Result<Profile, StoreError> {
        self.update_active_profile(|profile| {
            profile.layout = layout;
            ensure_profile_extras(profile);
        })
    }

    pub fn delete_profile(&self, id: &str) -> Result<(), StoreError> {
        let mut guard = self.inner.lock();
        guard.profiles.retain(|p| p.id != id);
        if guard.settings.active_profile_id.as_deref() == Some(id) {
            guard.settings.active_profile_id = guard.profiles.first().map(|p| p.id.clone());
        }
        drop(guard);
        self.persist()
    }

    pub fn update_active_profile<F>(&self, f: F) -> Result<Profile, StoreError>
    where
        F: FnOnce(&mut Profile),
    {
        let mut guard = self.inner.lock();
        let id = guard
            .settings
            .active_profile_id
            .clone()
            .ok_or_else(|| StoreError::Msg("no active profile".into()))?;
        let profile = guard
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| StoreError::Msg("active profile missing".into()))?;
        f(profile);
        ensure_profile_extras(profile);
        let out = profile.clone();
        drop(guard);
        self.persist()?;
        Ok(out)
    }
}
