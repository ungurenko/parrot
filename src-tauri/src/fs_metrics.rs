use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct DirSizeCache {
    sizes: HashMap<PathBuf, u64>,
}

impl DirSizeCache {
    pub(crate) fn get(&mut self, path: &Path) -> u64 {
        if let Some(size) = self.sizes.get(path) {
            return *size;
        }
        let size = dir_size_bytes(path);
        self.sizes.insert(path.to_path_buf(), size);
        size
    }

    pub(crate) fn clear(&mut self) {
        self.sizes.clear();
    }
}

pub(crate) fn dir_size_bytes(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else if meta.is_dir() {
                total = total.saturating_add(dir_size_bytes(&p));
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "parrot-fs-metrics-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn dir_size_bytes_should_sum_nested_files() {
        let dir = temp_dir("nested");
        let nested = dir.join("nested");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::write(dir.join("a.bin"), [1u8; 3]).expect("write first file");
        std::fs::write(nested.join("b.bin"), [1u8; 5]).expect("write nested file");

        assert_eq!(dir_size_bytes(&dir), 8);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_size_cache_should_reuse_value_until_clear() {
        let dir = temp_dir("cache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("a.bin"), [1u8; 3]).expect("write first file");

        let mut cache = DirSizeCache::default();
        assert_eq!(cache.get(&dir), 3);

        std::fs::write(dir.join("b.bin"), [1u8; 5]).expect("write second file");
        assert_eq!(cache.get(&dir), 3);

        cache.clear();
        assert_eq!(cache.get(&dir), 8);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
