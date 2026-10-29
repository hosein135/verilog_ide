//! Turn a waveform wellen can read (the yavs `.fst`) into a VCD file.
//!
//! [wellen](https://github.com/ekiwi/wellen) only parses FST, VCD, and GHW. It
//! does not write VCD, so this module walks the hierarchy and value changes
//! wellen already loaded and emits a VCD next to the FST.

use std::collections::HashMap;
use std::path::Path;
use wellen::{Hierarchy, Item, ItemRef, ScopeType, SignalRef, SignalValueRef, Var, VarType};

pub fn write_vcd(source: &Path, vcd_path: &Path) -> Result<(), String> {
    let mut waveform = wellen::simple::read(source).map_err(|err| {
        format!(
            "wellen could not read {}: {err}",
            source.display()
        )
    })?;

    let mut ids: HashMap<SignalRef, String> = HashMap::new();
    let mut signals: Vec<SigMeta> = Vec::new();
    let mut next_id = 0usize;
    let nodes = {
        let hierarchy = waveform.hierarchy();
        collect_items(
            hierarchy,
            hierarchy.items(),
            &mut ids,
            &mut signals,
            &mut next_id,
        )
    };

    let signal_ids: Vec<SignalRef> = signals.iter().map(|sig| sig.signal).collect();
    waveform.load_signals(&signal_ids);
    let time_table: Vec<u64> = waveform.time_table().to_vec();

    let mut events: Vec<(u64, String)> = Vec::new();
    for sig in &signals {
        let Some(signal) = waveform.get_signal(sig.signal) else {
            continue;
        };
        for (idx, value) in signal.iter_changes() {
            let time = time_table.get(idx as usize).copied().unwrap_or(0);
            events.push((
                time,
                format_value(sig.kind, sig.width, value, &sig.id),
            ));
        }
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut out = String::new();
    let hierarchy = waveform.hierarchy();
    push_header(&mut out, hierarchy, &nodes);
    push_values(&mut out, &events);

    if let Some(parent) = vcd_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("Could not create {}: {err}", parent.display()))?;
        }
    }
    std::fs::write(vcd_path, out)
        .map_err(|err| format!("Could not write {}: {err}", vcd_path.display()))?;
    Ok(())
}

struct SigMeta {
    id: String,
    signal: SignalRef,
    width: u32,
    kind: ValueKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Bits,
    Real,
    String,
    Event,
}

enum Node {
    Scope {
        keyword: &'static str,
        name: String,
        children: Vec<Node>,
    },
    Var {
        id: String,
        vcd_type: &'static str,
        width: u32,
        name: String,
    },
}

fn collect_items(
    hierarchy: &Hierarchy,
    items: impl Iterator<Item = ItemRef>,
    ids: &mut HashMap<SignalRef, String>,
    signals: &mut Vec<SigMeta>,
    next_id: &mut usize,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    for item in items {
        match item.deref(hierarchy) {
            Item::Scope(scope) => {
                let children = collect_items(
                    hierarchy,
                    scope.items(hierarchy),
                    ids,
                    signals,
                    next_id,
                );
                nodes.push(Node::Scope {
                    keyword: scope_keyword(scope.scope_type()),
                    name: sanitize(scope.name(hierarchy)),
                    children,
                });
            }
            Item::Var(var) => nodes.push(push_var(hierarchy, var, ids, signals, next_id)),
        }
    }
    nodes
}

fn push_var(
    hierarchy: &Hierarchy,
    var: &Var,
    ids: &mut HashMap<SignalRef, String>,
    signals: &mut Vec<SigMeta>,
    next_id: &mut usize,
) -> Node {
    let signal = var.signal_ref();
    let id = if let Some(existing) = ids.get(&signal) {
        existing.clone()
    } else {
        let id = vcd_id(*next_id);
        *next_id += 1;
        ids.insert(signal, id.clone());
        let kind = value_kind(var, hierarchy);
        signals.push(SigMeta {
            id: id.clone(),
            signal,
            width: value_width(var, hierarchy, kind),
            kind,
        });
        id
    };
    let kind = value_kind(var, hierarchy);
    Node::Var {
        id,
        vcd_type: vcd_var_type(var.var_type()),
        width: value_width(var, hierarchy, kind),
        name: vcd_ref_name(var, hierarchy),
    }
}

fn push_header(out: &mut String, hierarchy: &Hierarchy, nodes: &[Node]) {
    let date = hierarchy.date().trim();
    if !date.is_empty() {
        out.push_str("$date\n\t");
        out.push_str(date);
        out.push_str("\n$end\n");
    }
    out.push_str("$version\n\twellen ");
    out.push_str(wellen::VERSION);
    out.push_str(" FST to VCD\n$end\n");
    out.push_str("$timescale\n\t");
    out.push_str(&format_timescale(hierarchy.timescale()));
    out.push_str("\n$end\n");
    for node in nodes {
        push_node(out, node);
    }
    out.push_str("$enddefinitions $end\n");
}

fn push_node(out: &mut String, node: &Node) {
    match node {
        Node::Scope {
            keyword,
            name,
            children,
        } => {
            out.push_str("$scope ");
            out.push_str(keyword);
            out.push(' ');
            out.push_str(name);
            out.push_str(" $end\n");
            for child in children {
                push_node(out, child);
            }
            out.push_str("$upscope $end\n");
        }
        Node::Var {
            id,
            vcd_type,
            width,
            name,
        } => {
            out.push_str("$var ");
            out.push_str(vcd_type);
            out.push(' ');
            out.push_str(&width.to_string());
            out.push(' ');
            out.push_str(id);
            out.push(' ');
            out.push_str(name);
            out.push_str(" $end\n");
        }
    }
}

fn push_values(out: &mut String, events: &[(u64, String)]) {
    out.push_str("#0\n$dumpvars\n");
    let mut index = 0;
    while index < events.len() && events[index].0 == 0 {
        out.push_str(&events[index].1);
        out.push('\n');
        index += 1;
    }
    out.push_str("$end\n");
    while index < events.len() {
        let time = events[index].0;
        out.push('#');
        out.push_str(&time.to_string());
        out.push('\n');
        while index < events.len() && events[index].0 == time {
            out.push_str(&events[index].1);
            out.push('\n');
            index += 1;
        }
    }
}

fn format_value(kind: ValueKind, width: u32, value: SignalValueRef<'_>, id: &str) -> String {
    match value {
        SignalValueRef::Real(value) => format!("r{value} {id}"),
        SignalValueRef::String(value) => {
            let value = sanitize_string(value);
            format!("s{value} {id}")
        }
        SignalValueRef::Event => format!("1{id}"),
        SignalValueRef::BitVec(bits) => {
            if kind == ValueKind::Event {
                return format!("1{id}");
            }
            if kind == ValueKind::Real {
                return format!("r0 {id}");
            }
            if kind == ValueKind::String {
                return format!("s{} {id}", sanitize_string(&bits.bit_string()));
            }
            let fitted = fit_bits(&bits.bit_string(), width.max(1));
            if width <= 1 {
                let bit = fitted.chars().next().unwrap_or('x');
                format!("{bit}{id}")
            } else {
                format!("b{fitted} {id}")
            }
        }
    }
}

fn value_kind(var: &Var, hierarchy: &Hierarchy) -> ValueKind {
    if var.is_real(hierarchy)
        || matches!(
            var.var_type(),
            VarType::Real | VarType::RealTime | VarType::ShortReal | VarType::RealParameter
        )
    {
        ValueKind::Real
    } else if var.is_string(hierarchy) || matches!(var.var_type(), VarType::String) {
        ValueKind::String
    } else if matches!(var.var_type(), VarType::Event | VarType::EventParameter) {
        ValueKind::Event
    } else {
        ValueKind::Bits
    }
}

fn value_width(var: &Var, hierarchy: &Hierarchy, kind: ValueKind) -> u32 {
    match kind {
        ValueKind::String => 0,
        ValueKind::Real => var.length(hierarchy).unwrap_or(64),
        ValueKind::Event => 1,
        ValueKind::Bits => var.length(hierarchy).unwrap_or(1).max(1),
    }
}

fn vcd_ref_name(var: &Var, hierarchy: &Hierarchy) -> String {
    let mut name = sanitize(var.name(hierarchy));
    if let Some(index) = var.index() {
        name.push_str(&format!(" [{}:{}]", index.msb(), index.lsb()));
    }
    name
}

fn sanitize(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "unnamed".into();
    }
    trimmed
        .chars()
        .map(|ch| if ch.is_whitespace() { '_' } else { ch })
        .collect()
}

fn sanitize_string(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            ' ' | '\n' | '\r' | '\t' => '_',
            _ => ch,
        })
        .collect()
}

fn fit_bits(bits: &str, width: u32) -> String {
    let width = width.max(1) as usize;
    let mut bits: String = bits
        .chars()
        .map(|ch| match ch {
            '0' | '1' | 'x' | 'z' => ch,
            'X' => 'x',
            'Z' => 'z',
            'H' | 'h' => '1',
            'L' | 'l' => '0',
            _ => 'x',
        })
        .collect();
    if bits.len() > width {
        bits = bits.split_off(bits.len() - width);
    }
    if bits.len() < width {
        let mut padded = "x".repeat(width - bits.len());
        padded.push_str(&bits);
        bits = padded;
    }
    if bits.is_empty() {
        bits = "x".repeat(width);
    }
    bits
}

fn vcd_id(mut n: usize) -> String {
    const ALPH: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut chars = Vec::new();
    loop {
        chars.push(ALPH[n % ALPH.len()]);
        n /= ALPH.len();
        if n == 0 {
            break;
        }
    }
    chars.reverse();
    String::from_utf8(chars).expect("vcd id is ascii")
}

fn scope_keyword(scope: ScopeType) -> &'static str {
    match scope {
        ScopeType::Module
        | ScopeType::VhdlArchitecture
        | ScopeType::VhdlBlock
        | ScopeType::Package
        | ScopeType::VhdlPackage
        | ScopeType::Program
        | ScopeType::Interface
        | ScopeType::Class
        | ScopeType::GhwGeneric
        | ScopeType::Unknown
        | ScopeType::Clocking => "module",
        ScopeType::Task | ScopeType::VhdlProcedure => "task",
        ScopeType::Function | ScopeType::VhdlFunction => "function",
        ScopeType::Begin | ScopeType::VhdlProcess => "begin",
        ScopeType::Fork => "fork",
        ScopeType::Generate
        | ScopeType::VhdlForGenerate
        | ScopeType::VhdlIfGenerate
        | ScopeType::VhdlGenerate => "generate",
        ScopeType::Struct | ScopeType::VhdlRecord => "struct",
        ScopeType::Union => "union",
        ScopeType::VhdlArray | ScopeType::SvArray => "begin",
        _ => "module",
    }
}

fn vcd_var_type(ty: VarType) -> &'static str {
    match ty {
        VarType::Event | VarType::EventParameter => "event",
        VarType::Integer | VarType::Int | VarType::LongInt | VarType::ShortInt | VarType::Byte => {
            "integer"
        }
        VarType::Parameter => "parameter",
        VarType::Real | VarType::ShortReal | VarType::RealParameter => "real",
        VarType::RealTime => "realtime",
        VarType::Reg => "reg",
        VarType::Supply0 => "supply0",
        VarType::Supply1 => "supply1",
        VarType::Time => "time",
        VarType::Tri => "tri",
        VarType::TriAnd => "triand",
        VarType::TriOr => "trior",
        VarType::TriReg => "trireg",
        VarType::Tri0 => "tri0",
        VarType::Tri1 => "tri1",
        VarType::WAnd => "wand",
        VarType::WOr => "wor",
        VarType::String => "string",
        VarType::Wire
        | VarType::Boolean
        | VarType::Bit
        | VarType::Logic
        | VarType::Enum
        | VarType::StdLogic
        | VarType::StdULogic
        | VarType::BitVector
        | VarType::StdLogicVector
        | VarType::StdULogicVector
        | VarType::Port
        | VarType::SparseArray => "wire",
    }
}

fn format_timescale(timescale: Option<wellen::Timescale>) -> String {
    match timescale {
        Some(timescale) => format!("{}{}", timescale.factor, unit_suffix(timescale.unit)),
        None => "1ns".into(),
    }
}

fn unit_suffix(unit: wellen::TimescaleUnit) -> &'static str {
    use wellen::TimescaleUnit::*;
    match unit {
        ZeptoSeconds => "zs",
        AttoSeconds => "as",
        FemtoSeconds => "fs",
        PicoSeconds => "ps",
        NanoSeconds => "ns",
        MicroSeconds => "us",
        MilliSeconds => "ms",
        Seconds => "s",
        Unknown => "ns",
    }
}

#[cfg(test)]
mod tests {
use super::write_vcd;

    #[test]
    fn vcd_roundtrip_keeps_scope_names_and_a_change() {
        let dir = std::env::temp_dir().join(format!(
            "verilog-ide-fst-vcd-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.vcd");
        let dst = dir.join("out.vcd");
        std::fs::write(
            &src,
            "\
$date
	today
$end
$version
	test
$end
$timescale
	1ns
$end
$scope module tb $end
$var wire 1 ! clk $end
$var wire 2 \" sum $end
$upscope $end
$enddefinitions $end
#0
$dumpvars
0!
b00 \"
$end
#5
1!
#10
b11 \"
",
        )
        .unwrap();

        write_vcd(&src, &dst).expect("convert");
        let text = std::fs::read_to_string(&dst).unwrap();
        assert!(text.contains("$scope module tb $end"), "{text}");
        assert!(text.contains(" clk $end"), "{text}");
        assert!(text.contains(" sum $end"), "{text}");
        assert!(text.contains("$timescale\n\t1ns\n$end"), "{text}");
        assert!(text.contains("#5\n"), "{text}");
        assert!(text.contains("#10\n"), "{text}");

        let mut wave = wellen::simple::read(&dst).expect("reread vcd");
        let refs: Vec<_> = {
            let hierarchy = wave.hierarchy();
            hierarchy
                .all_vars()
                .map(|var| {
                    let var = &hierarchy[var];
                    (var.name(hierarchy).to_string(), var.signal_ref())
                })
                .collect()
        };
        assert!(refs.iter().any(|(name, _)| name == "clk"), "{refs:?}");
        assert!(refs.iter().any(|(name, _)| name == "sum"), "{refs:?}");
        let clk = refs.iter().find(|(name, _)| name == "clk").unwrap().1;
        wave.load_signals(&[clk]);
        let signal = wave.get_signal(clk).expect("clk signal");
        let times = wave.time_table();
        let values: Vec<_> = signal
            .iter_changes()
            .map(|(idx, value)| {
                let label = match value {
                    wellen::SignalValueRef::BitVec(bits) => bits.bit_string(),
                    other => format!("{other:?}"),
                };
                (times[idx as usize], label)
            })
            .collect();
        assert!(
            values.iter().any(|(time, bits)| *time == 5 && bits.starts_with('1')),
            "{values:?}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
