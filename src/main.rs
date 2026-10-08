//! Verilog IDE — desktop application for Verilog HDL and testbenches.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod app;
mod editor;
mod icons;
mod project;
mod sim;
mod templates;
mod verilog_highlighter;
mod wave_tools;
mod waveform;

fn main() -> iced::Result {
    app::run()
}
