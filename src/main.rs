//! Verilog IDE — desktop application for Verilog, VHDL, and testbenches.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod app;
mod editor;
mod icons;
mod language;
mod project;
mod sim;
mod templates;
mod verilog_highlighter;
mod vhdl_sim;
mod wave_tools;
mod waveform;

fn main() -> iced::Result {
    app::run()
}
