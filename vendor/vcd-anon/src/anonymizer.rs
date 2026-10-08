use chrono;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub struct VCDAnon {
    // Maps original signal identifiers to their anonymized names
    signal_map: HashMap<String, String>,
    // Counter for generating unique anonymized names
    signal_counter: usize,
    // Track all variables to write them in flattened form
    variables: Vec<VarInfo>,
    // Current scope path for tracking
    current_scope: Vec<String>,
    // If parameters should be kept in output
    keep_parameters: bool,
    // Keep bit/bit-vector variables as is
    keep_types: bool,
    // Track seen identifiers to avoid duplicates
    seen_identifiers: HashSet<String>,
}

#[derive(Clone)]
struct VarInfo {
    var_type: String,
    size: String,
    identifier: String,
    full_path: String,
    is_parameter: bool,
    anonymized_name: String,
}

impl VarInfo {
    fn as_variable_line(&self) -> String {
        format!(
            "$var {} {} {} {} $end",
            self.var_type, self.size, self.identifier, self.anonymized_name
        )
    }
}

impl VCDAnon {
    pub fn new(keep_parameters: bool, keep_types: bool) -> Self {
        Self {
            signal_map: HashMap::new(),
            signal_counter: 0,
            variables: Vec::new(),
            current_scope: Vec::new(),
            keep_parameters,
            keep_types,
            seen_identifiers: HashSet::new(),
        }
    }

    /// Anonymize a VCD file with flattened hierarchy
    pub fn anonymize_file<P: AsRef<Path>>(
        &mut self,
        input_path: P,
        output_path: P,
    ) -> std::io::Result<()> {
        let input = File::open(input_path)?;
        let reader = BufReader::new(input);
        let mut output = File::create(output_path)?;

        let mut header_lines = Vec::new();
        let mut in_definitions = true;
        let mut past_initial_scope = false;
        let mut allowed_ids: HashSet<String> = HashSet::new();

        // First pass: collect all variables and anonymize the names
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();

            if trimmed.starts_with("$scope") {
                self.handle_scope(&line);
                past_initial_scope = true;
            } else if trimmed.starts_with("$upscope") {
                self.current_scope.pop();
            } else if trimmed.starts_with("$var") {
                self.collect_var(&line);
            } else if trimmed.starts_with("$enddefinitions") {
                in_definitions = false;
                allowed_ids = self.allowed_identifiers();
                // Now write the flattened hierarchy
                self.write_flattened_header(&mut output, &header_lines)?;
                writeln!(output, "{}", line)?;
            } else if in_definitions && !past_initial_scope {
                // Collect header lines (date, version, timescale)
                header_lines.push(line);
            } else if !in_definitions {
                // Only keep value changes for allowed identifiers; always keep timestamps and directives
                if self.should_keep_value_change(trimmed, &allowed_ids) {
                    writeln!(output, "{}", line)?;
                }
            }
        }

        Ok(())
    }

    fn handle_scope(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let scope_name = parts[2];
            self.current_scope.push(scope_name.to_string());
        }
    }

    fn collect_var(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 5 {
            let identifier = parts[3].to_string();

            // Skip if we've already seen this identifier
            if !self.seen_identifiers.insert(identifier.clone()) {
                return;
            }

            let mut var_type = parts[1].to_string();

            let is_parameter = var_type.eq_ignore_ascii_case("parameter")
                || var_type.eq_ignore_ascii_case("real_parameter");

            // Unify bit/bit-vector types to wires if enabled
            if !self.keep_types || !is_parameter {
                if var_type.eq_ignore_ascii_case("real")
                    || var_type.eq_ignore_ascii_case("real_time")
                    || var_type.eq_ignore_ascii_case("shortreal")
                {
                    var_type = "real".to_string();
                } else if var_type.eq_ignore_ascii_case("integer")
                    || var_type.eq_ignore_ascii_case("time")
                    || var_type.eq_ignore_ascii_case("shortint")
                    || var_type.eq_ignore_ascii_case("enum")
                {
                    var_type = "integer".to_string();
                } else if !(var_type.eq_ignore_ascii_case("event")
                    || var_type.eq_ignore_ascii_case("longint")
                    || var_type.eq_ignore_ascii_case("string"))
                {
                    var_type = "wire".to_string();
                }
            }

            let size = parts[2].to_string();

            // Extract original signal name
            let name_start = line.find(&identifier).unwrap() + identifier.len();
            let name_end = line.rfind("$end").unwrap();
            let original_name = line[name_start..name_end].trim().to_string();

            // Create full hierarchical path
            let full_path = if self.current_scope.is_empty() {
                original_name.clone()
            } else {
                format!("{}.{}", self.current_scope.join("."), original_name)
            };

            // Generate anonymized name
            let anonymized_name = self.get_or_create_variable_name(&full_path);

            self.variables.push(VarInfo {
                var_type,
                size,
                identifier,
                full_path,
                is_parameter,
                anonymized_name,
            });
        }
    }

    fn write_flattened_header(
        &self,
        output: &mut File,
        header_lines: &[String],
    ) -> std::io::Result<()> {
        // Write modified header with anonymized version and current date
        let mut i = 0;
        while i < header_lines.len() {
            let trimmed = header_lines[i].trim();

            if trimmed.starts_with("$version") {
                // Skip all lines until we find $end
                while i < header_lines.len() && !header_lines[i].trim().contains("$end") {
                    i += 1;
                }
                // Write replacement
                writeln!(
                    output,
                    "$version\n  VCD Anon {}\n$end",
                    env!("CARGO_PKG_VERSION")
                )?;
                i += 1;
            } else if trimmed.starts_with("$date") {
                // Skip all lines until we find $end
                while i < header_lines.len() && !header_lines[i].trim().contains("$end") {
                    i += 1;
                }
                // Write replacement with current date
                let now = chrono::Local::now();
                writeln!(
                    output,
                    "$date\n  {}\n$end",
                    now.format("%a %b %e %H:%M:%S %Y")
                )?;
                i += 1;
            } else {
                writeln!(output, "{}", header_lines[i])?;
                i += 1;
            }
        }

        // Write single flattened scope
        writeln!(output, "$scope module top $end")?;

        // Write all variables in flattened form
        for var in self
            .variables
            .iter()
            .filter(|v| self.keep_parameters || !v.is_parameter)
        {
            writeln!(output, "{}", var.as_variable_line())?;
        }

        writeln!(output, "$upscope $end")?;

        Ok(())
    }

    fn get_or_create_variable_name(&mut self, full_path: &str) -> String {
        if let Some(anonymized) = self.signal_map.get(full_path) {
            anonymized.clone()
        } else {
            let anonymized = format!("var_{}", self.signal_counter);
            self.signal_counter += 1;
            self.signal_map
                .insert(full_path.to_string(), anonymized.clone());
            anonymized
        }
    }

    /// Generate a mapping file for reference
    pub fn save_mapping<P: AsRef<Path>>(&self, output_path: P) -> std::io::Result<()> {
        let mut output = File::create(output_path)?;

        writeln!(output, "=== Flattened Signal Mapping ===")?;
        writeln!(output, "All signals moved to: top")?;
        writeln!(output)?;

        let allowed: HashSet<&str> = self
            .variables
            .iter()
            .filter(|v| self.keep_parameters || !v.is_parameter)
            .map(|v| v.anonymized_name.as_str())
            .collect();

        let mut signals: Vec<_> = self
            .signal_map
            .iter()
            .filter(|(_, anonymized)| allowed.contains(anonymized.as_str()))
            .collect();
        signals.sort_by_key(|(_, v)| v.as_str());

        for (original, anonymized) in signals {
            writeln!(output, "{} -> top.{}", original, anonymized)?;
        }

        Ok(())
    }

    pub fn get_stats(&self) -> (usize, usize, usize) {
        let filtered: Vec<&VarInfo> = self
            .variables
            .iter()
            .filter(|v| self.keep_parameters || !v.is_parameter)
            .collect();

        let parameters_removed: Vec<&VarInfo> = self
            .variables
            .iter()
            .filter(|v| !self.keep_parameters && v.is_parameter)
            .collect();

        // Count unique module levels in original paths, only for kept vars
        let mut unique_modules = HashSet::new();
        for var in &filtered {
            let parts: Vec<&str> = var.full_path.split('.').collect();
            for i in 0..parts.len().saturating_sub(1) {
                unique_modules.insert(parts[..=i].join("."));
            }
        }

        (
            unique_modules.len(),
            filtered.len(),
            parameters_removed.len(),
        )
    }

    fn allowed_identifiers(&self) -> HashSet<String> {
        self.variables
            .iter()
            .filter(|v| self.keep_parameters || !v.is_parameter)
            .map(|v| v.identifier.clone())
            .collect()
    }

    fn should_keep_value_change(&self, trimmed_line: &str, allowed_ids: &HashSet<String>) -> bool {
        if trimmed_line.is_empty() {
            return false;
        }

        // Always keep timestamps and directives
        if trimmed_line.starts_with('#') || trimmed_line.starts_with('$') {
            return true;
        }

        if let Some(id) = Self::extract_identifier(trimmed_line) {
            allowed_ids.contains(id)
        } else {
            // Unknown line format: keep to avoid accidental data loss
            true
        }
    }

    fn extract_identifier(line: &str) -> Option<&str> {
        if line.is_empty() {
            return None;
        }

        let first = line.as_bytes()[0] as char;

        match first {
            'b' | 'B' | 'r' | 'R' => {
                // Vector/real change: identifier is the last whitespace-separated token
                line.split_whitespace().last()
            }
            _ => {
                // Scalar change: first char is value, rest is identifier
                if line.len() > 1 {
                    Some(&line[1..])
                } else {
                    None
                }
            }
        }
    }
}
