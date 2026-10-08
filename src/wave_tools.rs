//! VCD tools vendored from the Surfer project and compiled with the IDE.
//!
//! * [vcd-anon](https://gitlab.com/surfer-project/vcd-anon) flattens a VCD and
//!   renames every variable.
//! * [wave2saif](https://gitlab.com/surfer-project/wave2saif) writes a SAIF
//!   switching-activity file from a VCD, FST, or GHW dump.

use crate::project::{is_vcd_path, is_wave_path};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveToolKind {
    Anonymize,
    ToSaif,
}

#[derive(Debug, Clone)]
pub struct WaveToolOutput {
    pub log: String,
    pub output: PathBuf,
    pub open_as_wave: bool,
}

pub async fn run_async(kind: WaveToolKind, input: PathBuf) -> Result<WaveToolOutput, String> {
    tokio::task::spawn_blocking(move || run(kind, &input))
        .await
        .unwrap_or_else(|e| Err(format!("VCD tool task failed: {e}")))
}

pub fn run(kind: WaveToolKind, input: &Path) -> Result<WaveToolOutput, String> {
    match kind {
        WaveToolKind::Anonymize => anonymize(input),
        WaveToolKind::ToSaif => to_saif(input),
    }
}

fn anonymize(input: &Path) -> Result<WaveToolOutput, String> {
    if !is_vcd_path(input) {
        return Err(format!(
            "Anon VCD reads .vcd files. {} is not a VCD.",
            input.display()
        ));
    }
    if !input.is_file() {
        return Err(format!("VCD not found: {}", input.display()));
    }

    let stem = file_stem(input);
    let output = sibling(input, &format!("{stem}.anon.vcd"));
    let mapping = sibling(input, &format!("{stem}.anon.mapping.txt"));

    let mut anonymizer = vcd_anon::anonymizer::VCDAnon::new(false, false);
    anonymizer
        .anonymize_file(input, &output)
        .map_err(|e| format!("Anon VCD failed for {}: {e}", input.display()))?;
    anonymizer
        .save_mapping(&mapping)
        .map_err(|e| format!("Could not write mapping {}: {e}", mapping.display()))?;

    let (levels, signals, parameters_removed) = anonymizer.get_stats();
    let log = format!(
        "Anon VCD (vcd-anon)\n  input:  {}\n  output: {}\n  mapping: {}\n  hierarchy levels: {levels}\n  variables anonymized: {signals}\n  parameters removed: {parameters_removed}\n",
        input.display(),
        output.display(),
        mapping.display(),
    );
    Ok(WaveToolOutput {
        log,
        output,
        open_as_wave: true,
    })
}

fn to_saif(input: &Path) -> Result<WaveToolOutput, String> {
    if !is_wave_path(input) {
        return Err(format!(
            "To SAIF reads .vcd, .fst, or .ghw files. {} is not a waveform.",
            input.display()
        ));
    }
    if !input.is_file() {
        return Err(format!("Waveform not found: {}", input.display()));
    }

    let report = wave2saif::write_saif(input, None)?;
    let wide = if report.ignored_wide == 0 {
        String::new()
    } else {
        format!("  multi-bit signals ignored: {}\n", report.ignored_wide)
    };
    let log = format!(
        "To SAIF (wave2saif)\n  input:  {}\n  output: {}\n{wide}",
        input.display(),
        report.path.display(),
    );
    Ok(WaveToolOutput {
        log,
        output: report.path,
        open_as_wave: false,
    })
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("wave")
        .to_string()
}

fn sibling(path: &Path, file_name: &str) -> PathBuf {
    let mut out = path.to_path_buf();
    out.set_file_name(file_name);
    out
}

/// Pick the waveform a toolbar button should use.
///
/// The active editor wins when it matches. Otherwise an open tab, then the
/// newest matching file in the project folder.
pub fn pick_input(
    active: Option<&Path>,
    open: &[PathBuf],
    root: Option<&Path>,
    kind: WaveToolKind,
) -> Result<PathBuf, String> {
    let pred = |path: &Path| match kind {
        WaveToolKind::Anonymize => is_vcd_path(path),
        WaveToolKind::ToSaif => is_wave_path(path),
    };

    if let Some(path) = active.filter(|path| pred(path)) {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = open.iter().find(|path| pred(path)) {
        return Ok(path.clone());
    }
    if let Some(root) = root {
        if let Some(path) = newest_match(root, &pred) {
            return Ok(path);
        }
    }

    Err(match kind {
        WaveToolKind::Anonymize => {
            "Open a .vcd file, or run a simulation that writes one, then click Anon VCD.".into()
        }
        WaveToolKind::ToSaif => "Open a .vcd, .fst, or .ghw file, then click To SAIF.".into(),
    })
}

fn newest_match(root: &Path, pred: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || matches!(name.as_str(), "target" | "node_modules" | "vendor" | ".git")
            {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !pred(&path) {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            let replace = best.as_ref().is_none_or(|(time, _)| modified >= *time);
            if replace {
                best = Some((modified, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_VCD: &str = r#"$date
   Mon Jan 1 00:00:00 2024
$end
$version
   Test VCD
$end
$timescale 1ns $end
$scope module cpu $end
$var wire 1 ! clk $end
$scope module alu $end
$var wire 8 " data [7:0] $end
$upscope $end
$upscope $end
$enddefinitions $end
#0
0!
b00000000 "
#10
1!
b11111111 "
"#;

    #[test]
    fn anonymize_and_saif_on_a_vcd() {
        let dir = std::env::temp_dir().join(format!("rust-hdl-ide-vcd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let vcd = dir.join("tiny.vcd");
        std::fs::write(&vcd, TINY_VCD).unwrap();

        let anon = anonymize(&vcd).unwrap();
        let anon_text = std::fs::read_to_string(&anon.output).unwrap();
        assert!(anon_text.contains("$scope module top $end"));
        assert!(anon_text.contains("var_"));
        assert!(dir.join("tiny.anon.mapping.txt").is_file());

        let saif = to_saif(&vcd).unwrap();
        let saif_text = std::fs::read_to_string(&saif.output).unwrap();
        assert!(saif_text.contains("(SAIFILE"));
        assert!(saif_text.contains("(INSTANCE"));
        assert!(saif_text.contains("clk"));
        assert!(!saif.open_as_wave);
        assert!(anon.open_as_wave);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
