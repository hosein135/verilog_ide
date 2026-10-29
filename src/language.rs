//! Verilog or VHDL, chosen when a project is created or opened.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectLanguage {
    Verilog,
    Vhdl,
}

impl ProjectLanguage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Verilog => "Verilog",
            Self::Vhdl => "VHDL",
        }
    }

    pub fn simulator(self) -> &'static str {
        match self {
            Self::Verilog => "xezim",
            Self::Vhdl => "yavs",
        }
    }

    pub fn source_extension(self) -> &'static str {
        match self {
            Self::Verilog => "v",
            Self::Vhdl => "vhd",
        }
    }
}
