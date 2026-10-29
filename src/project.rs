//! Project file tree, language, and open-buffer management.

use crate::language::ProjectLanguage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = ".hdl-ide.json";
const RECENT_LIMIT: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdeProject {
    pub name: String,
    pub root: PathBuf,
    pub language: ProjectLanguage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub name: String,
    pub root: PathBuf,
    pub language: ProjectLanguage,
}

#[derive(Debug)]
pub enum LoadError {
    NeedLanguage,
    Io(String),
}

#[derive(Serialize, Deserialize)]
struct ManifestFile {
    name: String,
    language: ProjectLanguage,
}

#[derive(Serialize, Deserialize, Default)]
struct RegistryFile {
    projects: Vec<ProjectRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileView {
    Text,
    Waveform,
}

#[derive(Clone, Debug)]
pub struct OpenFile {
    pub path: PathBuf,
    pub content: String,
    pub dirty: bool,
    pub cursor: usize,
    pub view: FileView,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

impl IdeProject {
    pub fn build_tree(&self) -> TreeNode {
        build_dir_tree(&self.root)
    }

    pub fn refresh_tree(&self) -> TreeNode {
        self.build_tree()
    }
}

/// Locate the bundled `samples/` directory (repo or cwd).
pub fn locate_samples_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(env) = std::env::var("VERILOG_IDE_SAMPLES_DIR") {
        if !env.is_empty() {
            candidates.push(PathBuf::from(env));
        }
    }

    candidates.push(PathBuf::from("samples"));

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("samples"));
        candidates.extend(walk_up(cwd).into_iter().map(|p| p.join("samples")));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("samples"));
            candidates.extend(walk_up(dir.to_path_buf()).into_iter().map(|p| p.join("samples")));
        }
    }

    candidates.into_iter().find(|p| {
        p.is_dir()
            && (p.join("half_adder").join("half_adder.v").is_file()
                || p.join("half_adder.v").is_file()
                || p.join("half_adder_vhdl").join("half_adder.vhd").is_file())
    })
}

/// Bundled sample for `language`, if this tree has one.
pub fn locate_language_sample(language: ProjectLanguage) -> Option<PathBuf> {
    let samples = locate_samples_dir()?;
    match language {
        ProjectLanguage::Verilog => locate_half_adder_sample(),
        ProjectLanguage::Vhdl => {
            let nested = samples.join("half_adder_vhdl");
            nested
                .join("half_adder.vhd")
                .is_file()
                .then_some(nested)
        }
    }
}

/// Folder where File → New Project creates a directory.
pub fn default_projects_dir() -> PathBuf {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.is_empty() {
            return PathBuf::from(profile).join("hdl-projects");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join("hdl-projects");
        }
    }
    PathBuf::from("hdl-projects")
}

pub fn sanitize_project_name(name: &str) -> Result<String, String> {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').trim_matches('-').to_string();
    if out.is_empty() || out == "." || out == ".." {
        return Err("Enter a project name using letters, numbers, or underscore.".into());
    }
    Ok(out)
}

/// A single file or folder name. Rejects paths so rename cannot escape the directory.
pub fn validate_entry_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.".into());
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.contains("..")
    {
        return Err("Use a single name, without folders.".into());
    }
    Ok(name.to_string())
}

pub fn load_project(root: PathBuf, choice: Option<ProjectLanguage>) -> Result<IdeProject, LoadError> {
    if !root.is_dir() {
        return Err(LoadError::Io(format!("Not a folder: {}", root.display())));
    }
    let existing = read_manifest(&root)?;
    let language = if let Some(language) = choice {
        language
    } else if let Some(manifest) = existing.as_ref() {
        manifest.language
    } else if let Some(inferred) = infer_language(&root) {
        inferred
    } else {
        return Err(LoadError::NeedLanguage);
    };
    let name = existing
        .as_ref()
        .map(|m| m.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| folder_name(&root));
    let project = IdeProject {
        name,
        root,
        language,
    };
    let stale = existing
        .as_ref()
        .map(|m| m.language != project.language || m.name != project.name)
        .unwrap_or(true);
    if stale {
        project.write_manifest().map_err(LoadError::Io)?;
    }
    Ok(project)
}

pub fn create_project(name: &str, language: ProjectLanguage) -> Result<IdeProject, String> {
    create_project_in(&default_projects_dir(), name, language)
}

pub fn create_project_in(
    dir: &Path,
    name: &str,
    language: ProjectLanguage,
) -> Result<IdeProject, String> {
    let folder = sanitize_project_name(name)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Create {}: {e}", dir.display()))?;
    let root = dir.join(&folder);
    if root.exists() {
        return Err(format!(
            "A folder named {folder} already exists in {}",
            dir.display()
        ));
    }
    std::fs::create_dir(&root).map_err(|e| format!("Create {}: {e}", root.display()))?;
    let project = IdeProject {
        name: folder,
        root: root.clone(),
        language,
    };
    project.write_manifest()?;
    let (design_name, design_body, tb_name, tb_body) =
        crate::templates::scaffold_pair(language, &project.name);
    std::fs::write(root.join(design_name), design_body).map_err(|e| e.to_string())?;
    std::fs::write(root.join(tb_name), tb_body).map_err(|e| e.to_string())?;
    Ok(project)
}

impl IdeProject {
    fn write_manifest(&self) -> Result<(), String> {
        let manifest = ManifestFile {
            name: self.name.clone(),
            language: self.language,
        };
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(self.root.join(MANIFEST_FILE), text)
            .map_err(|e| format!("Write project file: {e}"))
    }
}

fn read_manifest(root: &Path) -> Result<Option<ManifestFile>, LoadError> {
    let path = root.join(MANIFEST_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| LoadError::Io(format!("Read {}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| LoadError::Io(format!("Project file {}: {e}", path.display())))
}

fn folder_name(root: &Path) -> String {
    root.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into())
}

pub fn infer_language(root: &Path) -> Option<ProjectLanguage> {
    let verilog = !collect_hdl_sources(root).is_empty();
    let vhdl = !collect_vhdl_sources(root).is_empty();
    match (verilog, vhdl) {
        (true, false) => Some(ProjectLanguage::Verilog),
        (false, true) => Some(ProjectLanguage::Vhdl),
        _ => None,
    }
}

pub fn registry_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return PathBuf::from(appdata)
                .join("verilog-ide")
                .join("projects.json");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home)
                .join(".verilog-ide")
                .join("projects.json");
        }
    }
    PathBuf::from(".verilog-ide-data").join("projects.json")
}

#[allow(dead_code)]
pub fn load_recent() -> Vec<ProjectRecord> {
    let path = registry_path();
    let mut items = read_registry(&path);
    let before = items.len();
    items.retain(|item| item.root.is_dir());
    if items.len() != before {
        let _ = write_registry(&path, &items);
    }
    items
}

pub fn remember_project(project: &IdeProject) {
    let path = registry_path();
    let mut items = read_registry(&path);
    items.retain(|item| item.root != project.root);
    items.insert(
        0,
        ProjectRecord {
            name: project.name.clone(),
            root: project.root.clone(),
            language: project.language,
        },
    );
    items.truncate(RECENT_LIMIT);
    let _ = write_registry(&path, &items);
}

#[allow(dead_code)]
pub fn forget_recent(root: &Path) {
    let path = registry_path();
    let mut items = read_registry(&path);
    let before = items.len();
    items.retain(|item| item.root != root);
    if items.len() != before {
        let _ = write_registry(&path, &items);
    }
}

fn read_registry(path: &Path) -> Vec<ProjectRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<RegistryFile>(&text)
        .map(|file| file.projects)
        .unwrap_or_default()
}

fn write_registry(path: &Path, projects: &[ProjectRecord]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&RegistryFile {
        projects: projects.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

pub fn rename_file(from: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = validate_entry_name(new_name)?;
    if !from.is_file() {
        return Err("Select a file to rename.".into());
    }
    let parent = from
        .parent()
        .ok_or_else(|| "That file has no folder.".to_string())?;
    let dest = parent.join(&new_name);
    if dest == from {
        return Ok(dest);
    }
    if dest.exists() {
        return Err(format!("Already exists: {}", dest.display()));
    }
    std::fs::rename(from, &dest).map_err(|e| format!("Rename failed: {e}"))?;
    Ok(dest)
}

pub fn delete_file(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Select a file to delete.".into());
    }
    std::fs::remove_file(path).map_err(|e| format!("Delete failed: {e}"))
}

fn walk_up(mut start: PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    loop {
        if !start.pop() {
            break;
        }
        out.push(start.clone());
    }
    out
}

/// First Verilog source file under `root` (sorted by path).
pub fn find_first_verilog(root: &Path) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_verilog_files(root, &mut files);
    files.sort();
    files.into_iter().next()
}

/// The bundled Verilog half-adder sample, if this tree has one.
pub fn locate_half_adder_sample() -> Option<PathBuf> {
    let samples = locate_samples_dir()?;
    let nested = samples.join("half_adder");
    if nested.join("half_adder.v").is_file() && nested.join("half_adder_tb.v").is_file() {
        Some(nested)
    } else if samples.join("half_adder.v").is_file() {
        Some(samples)
    } else {
        None
    }
}

/// Files to open when a folder is loaded: the design, then its testbench.
pub fn initial_editor_files(root: &Path, language: ProjectLanguage) -> Vec<PathBuf> {
    let files = match language {
        ProjectLanguage::Verilog => collect_hdl_sources(root),
        ProjectLanguage::Vhdl => collect_vhdl_sources(root),
    };
    let (tb, dut): (Vec<_>, Vec<_>) = files.into_iter().partition(|path| is_tb_stem(path));
    let mut out = Vec::new();
    if let Some(path) = dut.into_iter().next() {
        out.push(path);
    }
    if let Some(path) = tb.into_iter().next() {
        out.push(path);
    }
    if out.is_empty() {
        if let Some(path) = find_first_verilog(root) {
            out.push(path);
        }
    }
    out
}

fn is_tb_stem(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    stem.ends_with("_tb")
        || stem.starts_with("tb_")
        || stem.ends_with("_testbench")
        || stem.ends_with("_test")
        || stem.contains("testbench")
}

/// Compile units for simulation: `.v` / `.sv` only (headers are included).
pub fn collect_hdl_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_verilog_files(root, &mut files);
    files.retain(|p| is_hdl_source(p));
    files.sort();
    files
}

/// A Value Change Dump (`.vcd`) file.
pub fn is_vcd_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("vcd"))
}

/// Waveform dumps opened by Surfer/wellen (`.vcd` / `.fst` / `.ghw`).
pub fn is_wave_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("vcd" | "fst" | "ghw")
    )
}

pub fn is_hdl_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("v" | "sv")
    )
}

pub fn is_vhdl_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("vhd" | "vhdl")
    )
}

/// VHDL compile units (`.vhd` / `.vhdl`).
pub fn collect_vhdl_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files(root, &mut files, &is_vhdl_path);
    files.sort();
    files
}

fn collect_verilog_files(dir: &Path, out: &mut Vec<PathBuf>) {
    walk_files(dir, out, &is_verilog_file);
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>, pred: &impl Fn(&Path) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip(&name) || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk_files(&path, out, pred);
        } else if pred(&path) {
            out.push(path);
        }
    }
}

fn is_verilog_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
        Some("v" | "vh" | "sv" | "svh" | "vl")
    )
}

/// Collect all directory paths in a tree (for expand-all in explorer).
pub fn collect_dir_paths(node: &TreeNode, out: &mut Vec<PathBuf>) {
    if node.is_dir {
        out.push(node.path.clone());
        for child in &node.children {
            collect_dir_paths(child, out);
        }
    }
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        "target"
            | ".git"
            | "node_modules"
            | ".verilog-ide-data"
            | ".idea"
            | ".vscode"
            | "vendor"
            | ".yavs"
    )
}

fn build_dir_tree(dir: &Path) -> TreeNode {
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

    let mut children = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (!is_dir, e.file_name().to_string_lossy().to_lowercase())
        });
        for entry in entries {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if should_skip(&fname) || fname.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                children.push(build_dir_tree(&path));
            } else {
                children.push(TreeNode {
                    path,
                    name: fname,
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }
    }

    TreeNode {
        path: dir.to_path_buf(),
        name,
        is_dir: true,
        children,
    }
}

pub fn load_file(path: &Path) -> Result<OpenFile, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Read {}: {e}", path.display()))?;
    Ok(OpenFile {
        path: path.to_path_buf(),
        content,
        dirty: false,
        cursor: 0,
        view: FileView::Text,
    })
}

pub fn save_file(file: &mut OpenFile) -> Result<(), String> {
    if let Some(parent) = file.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file.path, &file.content)
        .map_err(|e| format!("Write {}: {e}", file.path.display()))?;
    file.dirty = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_project_in, delete_file, infer_language, load_project, rename_file,
        sanitize_project_name, validate_entry_name,
    };
    use crate::language::ProjectLanguage;
    use std::path::PathBuf;

    #[test]
    fn samples_load_as_their_language() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let verilog = load_project(root.join("samples/half_adder"), None).expect("verilog");
        assert_eq!(verilog.language, ProjectLanguage::Verilog);
        let vhdl = load_project(root.join("samples/half_adder_vhdl"), None).expect("vhdl");
        assert_eq!(vhdl.language, ProjectLanguage::Vhdl);
        assert_eq!(
            infer_language(&root.join("samples/half_adder")),
            Some(ProjectLanguage::Verilog)
        );
    }

    #[test]
    fn new_project_scaffolds_and_renames() {
        let dir = std::env::temp_dir().join(format!(
            "verilog-ide-proj-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let project = create_project_in(&dir, "Demo Adder", ProjectLanguage::Vhdl).expect("create");
        assert!(project.root.join("Demo_Adder.vhd").is_file());
        assert!(project.root.join("Demo_Adder_tb.vhd").is_file());
        assert_eq!(project.language, ProjectLanguage::Vhdl);
        let from = project.root.join("Demo_Adder.vhd");
        let dest = rename_file(&from, "alu.vhd").expect("rename");
        assert!(dest.ends_with("alu.vhd"));
        assert!(!from.exists());
        delete_file(&dest).expect("delete");
        assert!(!dest.exists());
        assert!(validate_entry_name("../secret").is_err());
        assert!(sanitize_project_name("   ").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
