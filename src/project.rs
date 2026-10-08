//! Project file tree and open-buffer management.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdeProject {
    pub name: String,
    pub root: PathBuf,
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
    pub fn new(root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".into());
        Self { name, root }
    }

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
                || p.join("half_adder.v").is_file())
    })
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
pub fn initial_editor_files(root: &Path) -> Vec<PathBuf> {
    let files = collect_hdl_sources(root);
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

fn collect_verilog_files(dir: &Path, out: &mut Vec<PathBuf>) {
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
            collect_verilog_files(&path, out);
        } else if is_verilog_file(&path) {
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
        "target" | ".git" | "node_modules" | ".verilog-ide-data" | ".idea" | ".vscode" | "vendor"
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
