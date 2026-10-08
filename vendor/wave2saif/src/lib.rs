//! Convert a VCD, FST, or GHW waveform to SAIF.
//!
//! Adapted locally to wellen 0.25 (`Item` / `all_vars` / `is_1bit(&Hierarchy)`).
//! Upstream wave2saif targeted wellen 0.17 (`ScopeOrVar`, `iter_vars`).

use std::fmt::Write;
use std::path::{Path, PathBuf};

use nesty::{code, Code};
use num_bigint::BigUint;
use num_traits::One;
use wellen::simple::Waveform;
use wellen::{Item, ItemRef, Scope, Time};

pub struct SaifReport {
    pub path: PathBuf,
    /// Multi-bit signals are omitted; SAIF switching activity here is 1-bit only.
    pub ignored_wide: usize,
}

struct Statistics {
    name: String,
    t0: BigUint,
    t1: BigUint,
    tx: BigUint,
    tz: BigUint,
    tc: BigUint,
}

struct ScopeStats<'a> {
    name: &'a str,
    nets: Vec<Statistics>,
    scopes: Vec<ScopeStats<'a>>,
}

fn format_time_unit(unit: wellen::TimescaleUnit) -> String {
    match unit {
        wellen::TimescaleUnit::ZeptoSeconds => "zs".to_string(),
        wellen::TimescaleUnit::AttoSeconds => "as".to_string(),
        wellen::TimescaleUnit::FemtoSeconds => "fs".to_string(),
        wellen::TimescaleUnit::PicoSeconds => "ps".to_string(),
        wellen::TimescaleUnit::NanoSeconds => "ns".to_string(),
        wellen::TimescaleUnit::MicroSeconds => "us".to_string(),
        wellen::TimescaleUnit::MilliSeconds => "ms".to_string(),
        wellen::TimescaleUnit::Seconds => "s".to_string(),
        wellen::TimescaleUnit::Unknown => {
            log::error!("Unknown times scale unit, defaulting to ns");
            "ns".to_string()
        }
    }
}

fn emit_header(design_name: &Path, time_scale: wellen::Timescale, duration: u64) -> String {
    let mut result = String::new();
    let file_name = design_name
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    writeln!(&mut result, "(SAIFILE").unwrap();
    writeln!(&mut result, "(SAIFVERSION \"2.0\")").unwrap();
    writeln!(&mut result, "(DIRECTION \"backward\")").unwrap();
    writeln!(&mut result, "(DESIGN {file_name:?})").unwrap();
    writeln!(&mut result, "(DATE \"{now:?}\")").unwrap();
    writeln!(&mut result, "(VENDOR \"surfer-project.org\")").unwrap();
    writeln!(&mut result, "(PROGRAM_NAME {:?})", env!("CARGO_PKG_NAME")).unwrap();
    writeln!(&mut result, "(VERSION {:?})", env!("CARGO_PKG_VERSION")).unwrap();
    writeln!(&mut result, "(DIVIDER / )").unwrap();
    writeln!(
        &mut result,
        "(TIMESCALE {} {})",
        time_scale.factor,
        format_time_unit(time_scale.unit)
    )
    .unwrap();
    writeln!(&mut result, "(DURATION {duration})").unwrap();
    result
}

impl<'a> ScopeStats<'a> {
    fn to_saif(&self) -> String {
        let subscopes = self
            .scopes
            .iter()
            .map(|scope| scope.to_saif())
            .collect::<Vec<_>>();

        let nets = self
            .nets
            .iter()
            .map(|net| {
                let Statistics {
                    name,
                    t0,
                    t1,
                    tx,
                    tz,
                    tc,
                } = net;
                format!("({name} (T0 {t0}) (T1 {t1}) (TX {tx}) (TZ {tz}) (TC {tc}) (IG 0))")
            })
            .collect::<Vec<_>>();

        code![
            [0]     format!("(INSTANCE {}", self.name);
            [1]     subscopes;
            [1]     "(NET";
            [2]         nets;
            [1]     ")";
            [0] ")"
        ]
        .to_string()
    }
}

fn handle_scope<'a>(waveform: &'a Waveform, scope: &'a Scope, times: &[Time]) -> ScopeStats<'a> {
    let hier = waveform.hierarchy();
    let nets = scope
        .vars(hier)
        .filter_map(|var_ref| {
            let Item::Var(var) = ItemRef::Var(var_ref).deref(hier) else {
                return None;
            };

            let signal = waveform.get_signal(var.signal_ref())?;
            let mut changes = signal.iter_changes();
            let Some((_, first_value)) = changes.next() else {
                return None;
            };
            let last_value = signal
                .iter_changes()
                .last()
                .map(|(_, value)| value)
                .unwrap_or(first_value);

            let mut t0 = BigUint::ZERO;
            let mut t1 = BigUint::ZERO;
            let mut tx = BigUint::ZERO;
            let mut tz = BigUint::ZERO;
            let mut tc = BigUint::ZERO;

            let final_sample = ((times.len() - 1) as u32, last_value);

            for ((oidx, ovalue), (nidx, nvalue)) in signal.iter_changes().zip(
                signal
                    .iter_changes()
                    .skip(1)
                    .chain(std::iter::once(final_sample)),
            ) {
                let Some(&t_end) = times.get(nidx as usize) else {
                    continue;
                };
                let Some(&t_start) = times.get(oidx as usize) else {
                    continue;
                };
                let duration = t_end.saturating_sub(t_start);
                let Some(ovalue) = ovalue.to_bit_string() else {
                    continue;
                };
                let Some(nvalue) = nvalue.to_bit_string() else {
                    continue;
                };

                match ovalue.as_str() {
                    "0" => t0 += duration,
                    "1" => t1 += duration,
                    "x" => tx += duration,
                    "z" => tz += duration,
                    _ => log::warn!("Unexpected signal value: {ovalue:?}"),
                }

                match (ovalue.as_str(), nvalue.as_str()) {
                    ("0", "1") => tc += BigUint::one(),
                    ("1", "0") => tc += BigUint::one(),
                    (_, _) => {}
                }
            }

            let index = match var.index() {
                // NOTE: Not caring about MSB because we assume 1 bit signals
                Some(idx) => format!("\\[{}\\]", idx.lsb()),
                None => String::new(),
            };
            let name = format!("{}{}", var.name(hier), index);
            Some(Statistics {
                name,
                t0,
                t1,
                tx,
                tz,
                tc,
            })
        })
        .collect::<Vec<_>>();

    let subscopes = scope
        .scopes(hier)
        .filter_map(|scope_ref| {
            let Item::Scope(scope) = ItemRef::Scope(scope_ref).deref(hier) else {
                return None;
            };
            Some(handle_scope(waveform, scope, times))
        })
        .collect();

    ScopeStats {
        name: scope.name(hier),
        nets,
        scopes: subscopes,
    }
}

/// Read `infile` and write a SAIF file.
///
/// When `outfile` is omitted, the output path is `infile` with a `.saif` extension.
pub fn write_saif(infile: &Path, outfile: Option<&Path>) -> Result<SaifReport, String> {
    let mut waveform = wellen::simple::read(infile)
        .map_err(|e| format!("Failed to read {}: {e}", infile.display()))?;

    let ignored_wide = {
        let hier = waveform.hierarchy();
        hier.all_vars()
            .filter(|var_ref| {
                let var = &hier[*var_ref];
                !var.is_1bit(hier)
            })
            .count()
    };

    let signals = {
        let hier = waveform.hierarchy();
        hier.all_vars()
            .filter_map(|var_ref| {
                let var = &hier[var_ref];
                if var.is_1bit(hier) {
                    Some(var.signal_ref())
                } else {
                    log::warn!(
                        "{} was not a 1 bit signal, ignoring it.",
                        var.full_name(hier)
                    );
                    None
                }
            })
            .collect::<Vec<_>>()
    };

    waveform.load_signals_multi_threaded(&signals);

    let ts = waveform.time_table();
    if ts.is_empty() {
        return Err(format!("{} has no time points", infile.display()));
    }

    let first_scope = waveform
        .hierarchy()
        .first_scope()
        .ok_or_else(|| format!("Did not find a scope in {}", infile.display()))?;

    let data = handle_scope(&waveform, first_scope, ts);

    let time_scale = waveform
        .hierarchy()
        .timescale()
        .ok_or_else(|| format!("{} did not have a time scale", infile.display()))?;
    let duration = ts[ts.len() - 1];

    let result = code![
        [0] emit_header(infile, time_scale, duration);
        [0] data.to_saif();
        [0] ")";
    ]
    .to_string();

    let owned;
    let outfile = match outfile {
        Some(path) => path,
        None => {
            owned = infile.with_extension("saif");
            &owned
        }
    };
    std::fs::write(outfile, result)
        .map_err(|e| format!("Failed to write {}: {e}", outfile.display()))?;

    Ok(SaifReport {
        path: outfile.to_path_buf(),
        ignored_wide,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_VCD: &str = "\
$timescale 1ns $end
$scope module cpu $end
$var wire 1 ! clk $end
$var wire 8 \" data [7:0] $end
$upscope $end
$enddefinitions $end
#0
0!
b00000000 \"
#10
1!
b11111111 \"
";

    #[test]
    fn writes_saif_for_one_bit_signals() {
        let dir = std::env::temp_dir().join(format!("wave2saif-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let vcd = dir.join("tiny.vcd");
        std::fs::write(&vcd, TINY_VCD).unwrap();
        let report = write_saif(&vcd, None).unwrap();
        let text = std::fs::read_to_string(&report.path).unwrap();
        assert!(text.contains("(SAIFILE"));
        assert!(text.contains("(INSTANCE cpu"));
        assert!(text.contains("clk"));
        assert_eq!(report.ignored_wide, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
