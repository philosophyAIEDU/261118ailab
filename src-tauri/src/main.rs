// Windows 배포 빌드에서 콘솔 창이 뜨지 않도록 한다
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    philosophy_research_hub_lib::run()
}
