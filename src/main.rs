#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Lunula Terminal — Ultra-lightweight GPU-accelerated terminal emulator.
//!
//! Entry point initializing logging, Lua configuration, and the Winit event loop.

mod app;
mod config;
mod pane;
mod platform;
mod pty;
mod render;
mod terminal;
pub mod theme;
mod wobble;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use app::LunulaApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    env_logger::init();
    let config = config::LunulaConfig::load();
    let el = EventLoop::new().expect("event loop");
    let proxy = el.create_proxy();
    el.set_control_flow(ControlFlow::Wait);
    let mut app = LunulaApp {
        config,
        proxy,
        state: None,
    };
    el.run_app(&mut app).expect("run");
}
