use crate::models::{Attachment, AttachmentKind};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024; // 12 MB
const MAX_TEXT_INLINE: usize = 400_000; // ~400KB 文本内联

pub fn attachments_dir() -> PathBuf {
    crate::storage::data_dir().join("attachments")
}

pub fn ensure_attachments_dir() -> Result<PathBuf, String> {
    let dir = attachments_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建附件目录: {e}"))?;
    Ok(dir)
}

pub fn guess_mime(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" | "tsx" => "text/typescript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "java" => "text/x-java",
        "go" => "text/x-go",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "cxx" | "hpp" => "text/x-c++",
        "toml" => "application/toml",
        "yaml" | "yml" => "text/yaml",
        "pdf" => "application/pdf",
        "doc" | "docx" => "application/msword",
        "xls" | "xlsx" => "application/vnd.ms-excel",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/") && mime != "image/svg+xml"
}

fn is_likely_text(mime: &str, path: &Path) -> bool {
    if mime.starts_with("text/") {
        return true;
    }
    matches!(
        mime,
        "application/json"
            | "application/xml"
            | "application/toml"
            | "application/javascript"
            | "application/typescript"
    ) || {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        matches!(
            ext.as_str(),
            "rs" | "py" | "go" | "java" | "kt" | "swift" | "rb" | "php"
                | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "sh" | "bat"
                | "ps1" | "sql" | "vue" | "svelte" | "jsx" | "tsx" | "ts"
                | "js" | "toml" | "yaml" | "yml" | "ini" | "cfg" | "env"
                | "log" | "gitignore" | "dockerfile" | "makefile" | "md"
                | "txt" | "csv" | "json" | "xml" | "html" | "css"
        )
    }
}

pub fn load_attachment_from_path(path: &Path) -> Result<Attachment, String> {
    if !path.is_file() {
        return Err(format!("不是有效文件: {}", path.display()));
    }

    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "文件过大（最大 {} MB）: {}",
            MAX_FILE_BYTES / (1024 * 1024),
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        ));
    }

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let mime = guess_mime(path);
    let id = Uuid::new_v4();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let stored_name = format!("{id}.{ext}");

    let dir = ensure_attachments_dir()?;
    let dest = dir.join(&stored_name);
    fs::copy(path, &dest).map_err(|e| format!("复制附件失败: {e}"))?;

    let kind = if is_image_mime(&mime) {
        AttachmentKind::Image
    } else if is_likely_text(&mime, path) {
        AttachmentKind::Text
    } else {
        AttachmentKind::Binary
    };

    let text_content = if kind == AttachmentKind::Text {
        match fs::read_to_string(path) {
            Ok(s) if s.len() <= MAX_TEXT_INLINE => Some(s),
            Ok(_) => Some(format!(
                "[文件过大，仅保留路径引用: {}，请分段发送或精简内容]",
                name
            )),
            Err(_) => {
                // 可能不是合法 UTF-8，按二进制处理
                None
            }
        }
    } else {
        None
    };

    let kind = if kind == AttachmentKind::Text && text_content.is_none() {
        AttachmentKind::Binary
    } else {
        kind
    };

    Ok(Attachment {
        id,
        name,
        mime,
        kind,
        stored_name,
        size: meta.len(),
        text_content,
    })
}

pub fn attachment_path(att: &Attachment) -> PathBuf {
    attachments_dir().join(&att.stored_name)
}

pub fn read_bytes(att: &Attachment) -> Result<Vec<u8>, String> {
    fs::read(attachment_path(att)).map_err(|e| format!("读取附件失败 ({}): {e}", att.name))
}

pub fn read_base64(att: &Attachment) -> Result<String, String> {
    use base64::Engine;
    let bytes = read_bytes(att)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
