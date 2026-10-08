#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use vcd_anon::anonymizer::VCDAnon;

    #[test]
    fn test_flatten_hierarchy() {
        let vcd_content = r#"$date
   Mon Jan 1 00:00:00 2024
$end
$version
   Test VCD
$end
$timescale 1ns $end
$scope module cpu $end
$var parameter 2 # IDLE [1:0] $end
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

        let mut input = NamedTempFile::new().unwrap();
        write!(input, "{}", vcd_content).unwrap();

        let output = NamedTempFile::new().unwrap();

        let mut anonymizer = VCDAnon::new(false, false);
        anonymizer.anonymize_file(input.path(), output.path()).unwrap();

        let (modules, signals, parameters_removed) = anonymizer.get_stats();
        assert!(modules >= 1); // cpu and cpu.alu
        assert_eq!(signals, 2); // clk and data
        assert_eq!(parameters_removed, 1); // IDLE

        // Verify output has flattened structure
        let output_content = std::fs::read_to_string(output.path()).unwrap();
        assert!(output_content.contains("$scope module top $end"));
        assert_eq!(output_content.matches("$scope").count(), 1); // Only one scope
    }
}
