//! Simulate VHDL with the vendored [yavs](https://github.com/ru551n/yavs) binary.
//!
//! yavs is a separate workspace under `vendor/yavs`. `build.rs` compiles it the
//! same way this crate compiles xezim: from the vendored sources, without cloning
//! it again. The simulator stays a process of its own because a run installs a
//! Ctrl-C handler and can exit the process.

use crate::project::{collect_vhdl_sources, OpenFile};
use crate::sim::{self, SimResult};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct VhdlJob {
    pub root: PathBuf,
    pub top: String,
    pub sources: Vec<PathBuf>,
    pub work: PathBuf,
    pub wave: PathBuf,
    pub yavs: PathBuf,
}

impl VhdlJob {
    pub fn command_preview(&self) -> String {
        format!(
            "yavs analyze + run --tier=interp --wave {} --stop-time=1ms ({} file{})",
            self.wave.display(),
            self.sources.len(),
            if self.sources.len() == 1 { "" } else { "s" }
        )
    }
}

pub fn prepare_job(
    root: &Path,
    active: Option<&Path>,
    open: &[OpenFile],
) -> Result<VhdlJob, String> {
    let files = collect_vhdl_sources(root);
    if files.is_empty() {
        return Err("No VHDL sources (.vhd / .vhdl) in this project.".into());
    }
    let ordered = analyze_order(&files);
    let tb_path = pick_testbench(&ordered, active, open).ok_or_else(|| {
        "No VHDL testbench found. Add a *_tb.vhd file, or an entity whose name ends in _tb."
            .to_string()
    })?;
    let tb_src = read_open_or_disk(&tb_path, open)?;
    let top = pick_top_entity(&tb_path, &tb_src).ok_or_else(|| {
        format!("Could not find an entity in {}", tb_path.display())
    })?;
    let wave_stem = top.strip_suffix("_tb").unwrap_or(&top);
    let wave = root.join(format!("{wave_stem}.fst"));
    let work = root.join(".yavs").join("work");
    Ok(VhdlJob {
        root: root.to_path_buf(),
        top,
        sources: ordered,
        work,
        wave,
        yavs: yavs_binary(),
    })
}

pub fn yavs_binary() -> PathBuf {
    PathBuf::from(env!("YAVS_BIN"))
}

pub async fn run_job_async(job: VhdlJob) -> SimResult {
    tokio::task::spawn_blocking(move || run_job(&job))
        .await
        .unwrap_or_else(|e| SimResult {
            ok: false,
            log: format!("Simulation task failed: {e}\n"),
            wave: None,
        })
}

pub fn run_job(job: &VhdlJob) -> SimResult {
    let mut log = String::new();
    log.push_str(&format!("cwd: {}\n", job.root.display()));
    log.push_str(&format!("{}\n", job.command_preview()));
    if !job.yavs.is_file() {
        log.push_str(&format!(
            "yavs binary is missing at {}.\nBuild the IDE again so vendor/yavs is compiled.\n",
            job.yavs.display()
        ));
        return SimResult {
            ok: false,
            log,
            wave: None,
        };
    }
    if let Err(err) = std::fs::create_dir_all(&job.work) {
        log.push_str(&format!("Could not create {}: {err}\n", job.work.display()));
        return SimResult {
            ok: false,
            log,
            wave: None,
        };
    }

    let work_spec = "work:.yavs/work";
    let mut analyze = vec![
        "analyze".into(),
        "--color=never".into(),
        "--message-format=short".into(),
        "--std=2008".into(),
        format!("--work={work_spec}"),
    ];
    for path in &job.sources {
        analyze.push(arg_path(path));
    }
    log.push_str(&format!("$ {}\n", command_line(&job.yavs, &analyze)));
    let analyzed = run_yavs(&job.yavs, &analyze, &job.root);
    log.push_str(&analyzed.output);
    if !analyzed.ok {
        log.push_str("yavs analyze failed.\n");
        return SimResult {
            ok: false,
            log,
            wave: None,
        };
    }

    let run_args = vec![
        "run".into(),
        "--color=never".into(),
        "--message-format=short".into(),
        "--std=2008".into(),
        "--tier=interp".into(),
        "--stop-time=1ms".into(),
        format!("--work={work_spec}"),
        format!("--wave={}", arg_path(&job.wave)),
        job.top.clone(),
    ];
    log.push_str(&format!("$ {}\n", command_line(&job.yavs, &run_args)));
    let ran = run_yavs(&job.yavs, &run_args, &job.root);
    log.push_str(&ran.output);
    let fst = wave_file(&job.wave);
    let mut wave = fst.clone();
    match &fst {
        Some(path) => {
            log.push_str(&format!("FST written: {}\n", path.display()));
            let vcd_path = path.with_extension("vcd");
            match crate::fst_to_vcd::write_vcd(path, &vcd_path) {
                Ok(()) => {
                    log.push_str(&format!("VCD written: {}\n", vcd_path.display()));
                    wave = Some(vcd_path);
                }
                Err(err) => {
                    log.push_str(&format!("Could not convert FST to VCD: {err}\n"));
                }
            }
        }
        None => log.push_str("Simulation finished but no .fst was found.\n"),
    }
    if !ran.ok {
        log.push_str("yavs run failed.\n");
    }
    SimResult {
        ok: ran.ok,
        log,
        wave,
    }
}

struct ToolOutput {
    ok: bool,
    output: String,
}

fn run_yavs(bin: &Path, args: &[String], cwd: &Path) -> ToolOutput {
    match Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.is_empty() {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&err);
            }
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            ToolOutput {
                ok: output.status.success(),
                output: text,
            }
        }
        Err(err) => ToolOutput {
            ok: false,
            output: format!("Could not start {}: {err}\n", bin.display()),
        },
    }
}

fn command_line(bin: &Path, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(bin.display().to_string());
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn arg_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn analyze_order(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = files.to_vec();
    files.sort_by(|a, b| {
        let key = |path: &Path| {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let stripped = strip_comments(&text).to_ascii_lowercase();
            let package = stripped.contains("package ") && !stripped.contains("entity ");
            let tb = sim::is_testbench_filename(path);
            (tb, !package, path.to_path_buf())
        };
        key(a).cmp(&key(b))
    });
    files
}

fn pick_testbench(
    sources: &[PathBuf],
    active: Option<&Path>,
    open: &[OpenFile],
) -> Option<PathBuf> {
    let score_of = |path: &Path| -> i32 {
        let content = read_open_or_disk(path, open).unwrap_or_default();
        let mut score = 0;
        if sim::is_testbench_filename(path) {
            score += 8;
        }
        if entity_names(&content)
            .iter()
            .any(|name| sim::is_testbench_ident(name))
        {
            score += 4;
        }
        if content.to_ascii_lowercase().contains("finish") {
            score += 2;
        }
        score
    };
    if let Some(active) = active {
        if sources.iter().any(|path| path == active) && score_of(active) > 0 {
            return Some(active.to_path_buf());
        }
    }
    let mut best: Option<(i32, PathBuf)> = None;
    for path in sources {
        let score = score_of(path);
        if score <= 0 {
            continue;
        }
        if best.as_ref().is_none_or(|(have, _)| score > *have) {
            best = Some((score, path.clone()));
        }
    }
    best.map(|(_, path)| path)
}

fn pick_top_entity(path: &Path, content: &str) -> Option<String> {
    let names = entity_names(content);
    if names.is_empty() {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    if names.iter().any(|name| name == &stem) {
        return Some(stem);
    }
    names
        .iter()
        .rev()
        .find(|name| sim::is_testbench_ident(name))
        .cloned()
        .or_else(|| names.last().cloned())
}

fn entity_names(src: &str) -> Vec<String> {
    let stripped = strip_comments(src);
    let lower = stripped.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let key = b"entity";
    let mut names = Vec::new();
    let mut i = 0;
    while i + key.len() <= bytes.len() {
        if bytes[i..].starts_with(key) {
            let prev_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let next = i + key.len();
            let next_ok = next >= bytes.len() || !is_ident_byte(bytes[next]);
            if prev_ok && next_ok {
                let rest = lower[next..].trim_start();
                let name: String = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                    .collect();
                let after = rest[name.len()..].trim_start();
                let declares = after.starts_with("is")
                    && after
                        .as_bytes()
                        .get(2)
                        .is_none_or(|b| !is_ident_byte(*b));
                if !name.is_empty() && declares {
                    names.push(name);
                }
            }
        }
        i += 1;
    }
    names
}

fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        let (body, nl) = line
            .strip_suffix('\n')
            .map(|body| (body, "\n"))
            .unwrap_or((line, ""));
        if let Some(idx) = body.find("--") {
            let before = &body[..idx];
            if before.matches('"').count() % 2 == 0 {
                out.push_str(before);
                out.push_str(nl);
                continue;
            }
        }
        out.push_str(body);
        out.push_str(nl);
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn read_open_or_disk(path: &Path, open: &[OpenFile]) -> Result<String, String> {
    if let Some(file) = open.iter().find(|file| file.path == path) {
        return Ok(file.content.clone());
    }
    std::fs::read_to_string(path).map_err(|err| format!("Read {}: {err}", path.display()))
}

fn wave_file(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_file() && meta.len() > 0 {
        Some(path.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{entity_names, prepare_job, run_job};
    use std::path::PathBuf;

    #[test]
    fn half_adder_entity_is_the_testbench() {
        let src = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("samples/half_adder_vhdl/half_adder_tb.vhd"),
        )
        .unwrap();
        assert_eq!(entity_names(&src), vec!["half_adder_tb".to_string()]);
    }

    #[test]
    fn half_adder_prepare_selects_tb_and_fst() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/half_adder_vhdl");
        let job = prepare_job(&root, None, &[]).expect("prepare");
        assert_eq!(job.top, "half_adder_tb");
        assert_eq!(job.wave.file_name().unwrap(), "half_adder.fst");
        assert_eq!(job.sources.len(), 2);
        assert!(job.sources[0].ends_with("half_adder.vhd"));
        assert!(job.sources[1].ends_with("half_adder_tb.vhd"));
    }

    #[test]
    fn half_adder_sample_writes_fst() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/half_adder_vhdl");
        let job = prepare_job(&root, None, &[]).expect("prepare half adder vhdl");
        let result = run_job(&job);
        assert!(result.ok, "{}", result.log);
        let vcd = result.wave.expect("vcd path");
        assert_eq!(vcd.extension().and_then(|ext| ext.to_str()), Some("vcd"));
        assert!(
            std::fs::metadata(&vcd).map(|m| m.len() > 16).unwrap_or(false),
            "vcd missing:\n{}",
            result.log
        );
        let vcd_text = std::fs::read_to_string(&vcd).unwrap_or_default();
        assert!(
            vcd_text.contains("$enddefinitions"),
            "vcd text:\n{vcd_text}\nlog:\n{}",
            result.log
        );
        let fst = vcd.with_extension("fst");
        assert!(
            fst.is_file(),
            "fst missing next to {}:\n{}",
            vcd.display(),
            result.log
        );
        assert!(
            result.log.contains("half_adder: PASS"),
            "simulation log:\n{}",
            result.log
        );
        let _ = std::fs::remove_file(&vcd);
        let _ = std::fs::remove_file(fst);
        let _ = std::fs::remove_dir_all(root.join(".yavs"));
    }
}
