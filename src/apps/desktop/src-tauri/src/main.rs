#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    apiarray_desktop_lib::run();
}
