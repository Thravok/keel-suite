//! Install / list plaintext Stream Deck SDK plugins under Keel app-data.

use crate::plugin_manifest::{ManifestError, PluginManifest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginStoreError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: PluginManifest,
}

pub struct PluginStore {
    root: PathBuf,
}

impl PluginStore {
    pub fn open(app_data: &Path) -> Result<Self, PluginStoreError> {
        let root = app_data.join("keel").join("plugins");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>, PluginStoreError> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if !name.ends_with(".sdPlugin") {
                continue;
            }
            let manifest = PluginManifest::load(&path.join("manifest.json"))?;
            out.push(InstalledPlugin {
                id: name,
                dir: path,
                manifest,
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Import a `.streamDeckPlugin` ZIP or a `.sdPlugin` directory.
    pub fn import_path(&self, src: &Path) -> Result<InstalledPlugin, PluginStoreError> {
        if !src.exists() {
            return Err(PluginStoreError::Msg(format!("path not found: {}", src.display())));
        }
        if src.is_dir() {
            return self.import_sdplugin_dir(src);
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "streamdeckplugin" || ext == "zip" {
            return self.import_zip(src);
        }
        Err(PluginStoreError::Msg(
            "expected a .sdPlugin folder or .streamDeckPlugin / .zip package".into(),
        ))
    }

    fn import_sdplugin_dir(&self, src: &Path) -> Result<InstalledPlugin, PluginStoreError> {
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| PluginStoreError::Msg("invalid plugin folder name".into()))?
            .to_string();
        if !name.ends_with(".sdPlugin") {
            return Err(PluginStoreError::Msg(
                "plugin folder must end with .sdPlugin".into(),
            ));
        }
        let manifest = PluginManifest::load(&src.join("manifest.json"))?;
        if manifest.is_html_code_path() {
            // Still allow install for listing; host will refuse to spawn HTML.
        }
        let dest = self.root.join(&name);
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_dir_all(src, &dest)?;
        Ok(InstalledPlugin {
            id: name,
            dir: dest,
            manifest,
        })
    }

    fn import_zip(&self, src: &Path) -> Result<InstalledPlugin, PluginStoreError> {
        // ZIP may contain a single top-level *.sdPlugin/ directory.
        let tmp = self.root.join(format!(".import-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp)?;
        let result = (|| {
            let file = fs::File::open(src)?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| PluginStoreError::Msg(format!("zip: {e}")))?;
            archive
                .extract(&tmp)
                .map_err(|e| PluginStoreError::Msg(format!("extract: {e}")))?;
            // Find *.sdPlugin directory
            let mut found: Option<PathBuf> = None;
            for entry in fs::read_dir(&tmp)? {
                let p = entry?.path();
                if p.is_dir()
                    && p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.ends_with(".sdPlugin"))
                        .unwrap_or(false)
                {
                    found = Some(p);
                    break;
                }
            }
            // Also search one level deeper
            if found.is_none() {
                for entry in walkdir_one_level(&tmp)? {
                    if entry.is_dir()
                        && entry
                            .file_name()
                            .and_then(|s| s.to_str())
                            .map(|s| s.ends_with(".sdPlugin"))
                            .unwrap_or(false)
                    {
                        found = Some(entry);
                        break;
                    }
                }
            }
            let plugin_dir = found.ok_or_else(|| {
                PluginStoreError::Msg("ZIP does not contain a .sdPlugin directory".into())
            })?;
            self.import_sdplugin_dir(&plugin_dir)
        })();
        let _ = fs::remove_dir_all(&tmp);
        result
    }
}

fn walkdir_one_level(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        let p = entry?.path();
        if p.is_dir() {
            for child in fs::read_dir(&p)? {
                out.push(child?.path());
            }
        }
    }
    Ok(out)
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
