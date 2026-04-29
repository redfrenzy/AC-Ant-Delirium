use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Debug, Default, Clone)]
pub struct ProjectFileMetadata {
    pub parent_dir: String,
    pub stem: String,
    pub extension: String,
}

impl ProjectFileMetadata {
    pub fn from_path(path: &str) -> Self {
        let normalized = normalize_project_path(path);
        let path = Path::new(&normalized);

        Self {
            parent_dir: path
                .parent()
                .map(|value| normalize_project_path(&value.to_string_lossy()))
                .unwrap_or_default(),
            stem: path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            extension: path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProjectFileContext {
    pub path: String,
    pub content: String,
    pub detected_dna: String,
    pub taxonomy_path: String,
    pub metadata: ProjectFileMetadata,
}

impl ProjectFileContext {
    pub fn new(path: String, content: String, detected_dna: String, taxonomy_path: String) -> Self {
        let normalized = normalize_project_path(&path);
        let metadata = ProjectFileMetadata::from_path(&normalized);

        Self {
            path: normalized,
            content,
            detected_dna,
            taxonomy_path,
            metadata,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProjectContext {
    pub files: Vec<ProjectFileContext>,
    pub index: HashMap<String, usize>,
    pub known_paths: HashSet<String>,
}

impl ProjectContext {
    pub fn register_known_path<S: AsRef<str>>(&mut self, path: S) {
        let normalized = normalize_project_path(path.as_ref());
        if !normalized.is_empty() {
            self.known_paths.insert(normalized);
        }
    }

    pub fn insert_loaded_file(&mut self, file: ProjectFileContext) {
        let normalized = normalize_project_path(&file.path);
        self.known_paths.insert(normalized.clone());

        if let Some(index) = self.index.get(&normalized).copied() {
            self.files[index] = file;
            return;
        }

        let index = self.files.len();
        self.index.insert(normalized, index);
        self.files.push(file);
    }

    pub fn file(&self, path: &str) -> Option<&ProjectFileContext> {
        let normalized = normalize_project_path(path);
        self.index
            .get(&normalized)
            .and_then(|index| self.files.get(*index))
    }

    pub fn contains_path(&self, path: &str) -> bool {
        let normalized = normalize_project_path(path);
        self.known_paths.contains(&normalized)
    }

    pub fn resolve_sibling_path(&self, current_path: &str, sibling_name: &str) -> String {
        let parent = Path::new(current_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let candidate = if parent.as_os_str().is_empty() {
            PathBuf::from(sibling_name)
        } else {
            parent.join(sibling_name)
        };

        normalize_project_path(&candidate.to_string_lossy())
    }

    pub fn contains_sibling(&self, current_path: &str, sibling_name: &str) -> bool {
        let sibling = self.resolve_sibling_path(current_path, sibling_name);
        self.contains_path(&sibling)
    }

    pub fn loaded_file_count(&self) -> usize {
        self.files.len()
    }

    pub fn known_path_count(&self) -> usize {
        self.known_paths.len()
    }
}

pub fn normalize_project_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }

    if normalized == "." {
        String::new()
    } else {
        normalized
    }
}

pub fn empty_project_context() -> &'static ProjectContext {
    static EMPTY: LazyLock<ProjectContext> = LazyLock::new(ProjectContext::default);
    &EMPTY
}
