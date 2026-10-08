//! What the game keeps on the device, and files it hands the player:
//!
//! - wasm: the page's `localStorage`, and downloads (see [`crate::web`]),
//! - native: files in the user's config directory, and saving into the
//!   downloads directory (as the platform has them, see [`dirs`]).

/// The text stored under `key`, if any.
pub fn load(key: &str) -> Option<String> {
    imp::load(key)
}

/// Stores `text` under `key`; failures are only logged: what is stored
/// is a convenience.
pub fn save(key: &str, text: &str) {
    imp::save(key, text);
}

/// Hands the player a text file. Returns where it went, for telling
/// them, or why it could not be saved.
pub fn download(name: &str, text: &str) -> Result<String, String> {
    imp::download(name, text)
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use crate::web;

    pub fn load(key: &str) -> Option<String> {
        web::load(key)
    }

    pub fn save(key: &str, text: &str) {
        web::save(key, text);
    }

    pub fn download(name: &str, text: &str) -> Result<String, String> {
        web::download(name, text);
        Ok("your downloads".to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::PathBuf;

    use bevy::log::warn;

    const APP: &str = "hibedi";

    fn file(key: &str) -> Option<PathBuf> {
        Some(dirs::config_dir()?.join(APP).join(format!("{key}.json")))
    }

    pub fn load(key: &str) -> Option<String> {
        std::fs::read_to_string(file(key)?).ok()
    }

    pub fn save(key: &str, text: &str) {
        let Some(path) = file(key) else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        if let Err(error) = written {
            warn!("could not save {}: {error}", path.display());
        }
    }

    /// Saves into the downloads directory if there is one, else the
    /// working directory, never overwriting an earlier file.
    pub fn download(name: &str, text: &str) -> Result<String, String> {
        let downloads = dirs::download_dir()
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| PathBuf::from("."));
        let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
        let path = (1..)
            .map(|n| match n {
                1 => downloads.join(name),
                _ => downloads.join(format!("{stem}-{n}.{extension}")),
            })
            .find(|path| !path.exists())
            .expect("some name is free");
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
        Ok(path.display().to_string())
    }
}
