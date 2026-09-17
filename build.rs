fn main() {
    // 仅 Windows 把 .ico 嵌入可执行文件（资源管理器 / 任务栏 / 快捷方式图标）
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // 缺 RC 工具时不阻断开发构建，但给出提示
            println!("cargo:warning=嵌入 Windows 图标失败: {e}");
            println!("cargo:warning=请确认已安装 Windows SDK / MSVC 构建工具");
        }
    }

    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon_256.png");
}
