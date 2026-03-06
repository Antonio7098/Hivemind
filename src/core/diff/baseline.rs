use super::*;

impl Baseline {
    /// Captures a baseline from a directory.
    pub fn capture(root: &Path) -> io::Result<Self> {
        let mut files = HashMap::new();
        capture_recursive(root, root, &mut files)?;

        let git_head = get_git_head(root).ok();
        let git_branches = get_git_branches(root).ok().unwrap_or_default();

        Ok(Self {
            id: Uuid::new_v4(),
            root: root.to_path_buf(),
            git_head,
            git_branches,
            files,
            captured_at: chrono::Utc::now(),
        })
    }

    /// Gets a file snapshot by path.
    pub fn get(&self, path: &Path) -> Option<&FileSnapshot> {
        self.files.get(path)
    }

    /// Returns the number of files in the baseline.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

fn get_git_branches(path: &Path) -> io::Result<Vec<String>> {
    use std::process::Command;

    let output = Command::new("git")
        .current_dir(path)
        .args(["for-each-ref", "refs/heads", "--format=%(refname:short)"])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other("Failed to list git branches"));
    }

    let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

pub(super) fn capture_recursive(
    root: &Path,
    current: &Path,
    files: &mut HashMap<PathBuf, FileSnapshot>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

        if relative.starts_with(".git") {
            continue;
        }

        let metadata = entry.metadata()?;
        let is_dir = metadata.is_dir();

        let hash = if is_dir {
            None
        } else {
            compute_hash(&path).ok()
        };

        files.insert(
            relative.clone(),
            FileSnapshot {
                path: relative,
                hash,
                size: metadata.len(),
                is_dir,
            },
        );

        if is_dir {
            capture_recursive(root, &path, files)?;
        }
    }

    Ok(())
}

fn compute_hash(path: &Path) -> io::Result<FileHash> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut file = fs::File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn get_git_head(path: &Path) -> io::Result<String> {
    use std::process::Command;

    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "HEAD"])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(io::Error::other("Failed to get git HEAD"))
    }
}
