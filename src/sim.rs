//! Simulate Verilog + testbench with the vendored [xezim](https://github.com/aionhw/xezim)
//! library (`vendor/xezim`) and collect the resulting VCD waveform.

use crate::project::{collect_hdl_sources, OpenFile};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 1 ms, matching xezim's `--max-time 1ms` (bare unit is nanoseconds).
const MAX_TIME_NS: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct SimJob {
    pub root: PathBuf,
    pub top: String,
    pub sources: Vec<String>,
    pub source_paths: Vec<String>,
    pub include_dirs: Vec<String>,
    pub expected_vcd: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SimResult {
    pub ok: bool,
    pub log: String,
    pub vcd: Option<PathBuf>,
}

impl SimJob {
    pub fn command_preview(&self) -> String {
        format!(
            "xezim --wave -s {} --max-time 1ms ({} file{})",
            self.top,
            self.sources.len(),
            if self.sources.len() == 1 { "" } else { "s" }
        )
    }
}

pub fn prepare_job(
    root: &Path,
    active: Option<&Path>,
    open: &[OpenFile],
) -> Result<SimJob, String> {
    let files = collect_hdl_sources(root);
    if files.is_empty() {
        return Err("No Verilog sources (.v / .sv) in this folder.".into());
    }

    let tb_path = pick_testbench(&files, active, open).ok_or_else(|| {
        "No testbench found. Add a *_tb.v (or *_tb.sv) file, or a module that calls $dumpfile / $finish."
            .to_string()
    })?;

    let tb_src = std::fs::read_to_string(&tb_path)
        .map_err(|e| format!("Read {}: {e}", tb_path.display()))?;
    let top = pick_top_module(&tb_path, &tb_src).ok_or_else(|| {
        format!(
            "Could not find a module in testbench {}",
            tb_path.display()
        )
    })?;

    let vcd_name = parse_dumpfile(&tb_src).unwrap_or_else(|| format!("{top}.vcd"));
    let expected_vcd = resolve_vcd_path(root, &vcd_name);
    let tb_src = ensure_wave_tasks(&tb_src, &top, &expected_vcd);

    let mut include_dirs = Vec::new();
    include_dirs.push(root.to_string_lossy().into_owned());
    for src in &files {
        if let Some(parent) = src.parent() {
            let p = parent.to_string_lossy().into_owned();
            if !include_dirs.iter().any(|d| d == &p) {
                include_dirs.push(p);
            }
        }
    }

    let mut sources = Vec::new();
    let mut source_paths = Vec::new();
    for path in &files {
        source_paths.push(path.to_string_lossy().into_owned());
        if path == &tb_path {
            sources.push(tb_src.clone());
        } else {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("Read {}: {e}", path.display()))?;
            sources.push(text);
        }
    }

    Ok(SimJob {
        root: root.to_path_buf(),
        top,
        sources,
        source_paths,
        include_dirs,
        expected_vcd,
    })
}

pub async fn run_job_async(job: SimJob) -> SimResult {
    tokio::task::spawn_blocking(move || run_job(&job))
        .await
        .unwrap_or_else(|e| SimResult {
            ok: false,
            log: format!("Simulation task failed: {e}\n"),
            vcd: None,
        })
}

fn run_job(job: &SimJob) -> SimResult {
    let mut log = String::new();
    log.push_str(&format!("cwd: {}\n", job.root.display()));
    log.push_str(&format!("{}\n", job.command_preview()));

    xezim::compiler::simulator::set_wave_enabled(true);

    let sim = xezim::simulate_multi(
        &job.sources,
        MAX_TIME_NS,
        Some(&job.top),
        &job.include_dirs,
        &job.source_paths,
        None,
        false,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        0,
        u64::MAX,
        None,
        &[],
        None,
        None,
        None,
        None,
        false,
        None,
    );

    match sim {
        Ok(sim) => {
            for line in &sim.output {
                if !line.message.is_empty() {
                    log.push_str(&line.message);
                    if !line.message.ends_with('\n') {
                        log.push('\n');
                    }
                }
            }
            let vcd = find_vcd(&job.root, &job.expected_vcd);
            match &vcd {
                Some(path) => log.push_str(&format!("VCD written: {}\n", path.display())),
                None => log.push_str(
                    "Simulation finished but no .vcd was found. The testbench needs $dumpfile / $dumpvars.\n",
                ),
            }
            SimResult {
                ok: true,
                log,
                vcd,
            }
        }
        Err(e) => {
            log.push_str(&e);
            if !e.ends_with('\n') {
                log.push('\n');
            }
            SimResult {
                ok: false,
                log,
                vcd: None,
            }
        }
    }
}

fn pick_testbench(
    sources: &[PathBuf],
    active: Option<&Path>,
    open: &[OpenFile],
) -> Option<PathBuf> {
    let scored = |path: &Path, content: &str| -> i32 {
        let mut score = 0;
        if is_testbench_filename(path) {
            score += 8;
        }
        if content.contains("$dumpfile") || content.contains("$dumpvars") {
            score += 6;
        }
        if content.contains("$finish") {
            score += 3;
        }
        if module_names(content)
            .iter()
            .any(|m| is_testbench_ident(m))
        {
            score += 4;
        }
        score
    };

    if let Some(active) = active {
        if sources.iter().any(|s| s == active) {
            let content = open
                .iter()
                .find(|f| f.path == active)
                .map(|f| f.content.clone())
                .or_else(|| std::fs::read_to_string(active).ok())
                .unwrap_or_default();
            if scored(active, &content) > 0 {
                return Some(active.to_path_buf());
            }
        }
    }

    let mut best: Option<(i32, PathBuf)> = None;
    for path in sources {
        let content = open
            .iter()
            .find(|f| f.path == *path)
            .map(|f| f.content.clone())
            .or_else(|| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let score = scored(path, &content);
        if score <= 0 {
            continue;
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, path.clone()));
        }
    }
    best.map(|(_, p)| p)
}

fn pick_top_module(path: &Path, content: &str) -> Option<String> {
    let names = module_names(content);
    if names.is_empty() {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if names.iter().any(|n| n == stem) {
        return Some(stem.to_string());
    }
    names
        .iter()
        .rev()
        .find(|n| is_testbench_ident(n))
        .cloned()
        .or_else(|| names.last().cloned())
}

pub fn is_testbench_filename(path: impl AsRef<Path>) -> bool {
    let name = path
        .as_ref()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
    is_testbench_ident(stem)
}

pub fn is_testbench_ident(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with("_tb")
        || n.starts_with("tb_")
        || n.ends_with("_testbench")
        || n.ends_with("_test")
        || n.contains("testbench")
}

fn module_names(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = src[search_from..].find("module") {
        let idx = search_from + rel;
        let prev = if idx == 0 {
            '\n'
        } else {
            src[..idx].chars().next_back().unwrap_or('\n')
        };
        if prev.is_ascii_alphanumeric() || prev == '_' || prev == '$' {
            search_from = idx + 6;
            continue;
        }
        let after = src[idx + 6..].trim_start();
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
        search_from = idx + 6;
    }
    names
}

fn parse_dumpfile(src: &str) -> Option<String> {
    let key = "$dumpfile";
    let idx = src.find(key)?;
    let rest = src[idx + key.len()..].trim_start();
    let rest = rest.strip_prefix('(')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = rest.get(1..)?;
    let end = body.find(quote)?;
    let name = body[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn resolve_vcd_path(root: &Path, name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.is_absolute() {
        p
    } else {
        root.join(p)
    }
}

fn vcd_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn ensure_wave_tasks(src: &str, top: &str, vcd: &Path) -> String {
    let lit = vcd_literal(vcd);
    let has_file = parse_dumpfile(src).is_some();
    let has_vars = src.contains("$dumpvars");
    if has_file && has_vars {
        replace_dumpfile(src, &lit).unwrap_or_else(|| src.to_string())
    } else {
        inject_dump_block(src, top, &lit)
    }
}

fn replace_dumpfile(src: &str, lit: &str) -> Option<String> {
    let key = "$dumpfile";
    let k = src.find(key)?;
    let bytes = src.as_bytes();
    let mut i = k + key.len();
    while i < src.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= src.len() || bytes[i] != b'(' {
        return None;
    }
    i += 1;
    while i < src.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let q = *bytes.get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let open = i;
    let close = src[open + 1..].find(q as char)? + open + 1;
    Some(format!("{}{}{}", &src[..=open], lit, &src[close..]))
}

fn inject_dump_block(src: &str, top: &str, lit: &str) -> String {
    let block = format!(
        "\n    initial begin\n        $dumpfile(\"{lit}\");\n        $dumpvars(0, {top});\n    end\n"
    );
    if let Some(idx) = src.rfind("endmodule") {
        let mut s = src.to_string();
        s.insert_str(idx, &block);
        s
    } else {
        format!("{src}\n{block}\n")
    }
}

fn find_vcd(root: &Path, expected: &Path) -> Option<PathBuf> {
    if vcd_looks_valid(expected) {
        return Some(expected.to_path_buf());
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return None;
    };
    let mut found = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("vcd") && vcd_looks_valid(&path) {
            found.push(path);
        }
    }
    found.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .unwrap_or(Duration::from_secs(u64::MAX))
    });
    found.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::prepare_job;
    use super::run_job;
    use std::path::PathBuf;

    #[test]
    fn half_adder_sample_writes_vcd() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/half_adder");
        let job = prepare_job(&root, None, &[]).expect("prepare half adder");
        let result = run_job(&job);
        assert!(result.ok, "{}", result.log);
        let vcd = result.vcd.expect("vcd path");
        let text = std::fs::read_to_string(&vcd).unwrap_or_default();
        assert!(
            text.contains("$var") || text.contains("$timescale"),
            "vcd missing header:\n{text}"
        );
        assert!(
            result.log.contains("half_adder: PASS"),
            "simulation log:\n{}",
            result.log
        );
        let _ = std::fs::remove_file(vcd);
    }
}

fn vcd_looks_valid(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 {
        return false;
    }
    let Ok(head) = std::fs::read_to_string(path) else {
        return meta.len() > 16;
    };
    head.contains("$timescale") || head.contains("$var") || head.contains("$date")
}
