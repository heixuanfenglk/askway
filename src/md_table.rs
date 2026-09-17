//! 将 Markdown 中的 GFM 表格拆出，按预测量列宽、自上而下手动画表。
//!
//! 注意：不可放在 `horizontal` 布局里画多行，否则每一行会被排到同一行右侧。

use eframe::egui::{
    self, text::LayoutJob, Align, Color32, FontId, Frame, Galley, Layout, Margin, ScrollArea,
    Sense, TextFormat, Vec2,
};
use std::sync::Arc;

#[derive(Debug)]
pub enum MdSegment {
    Markdown(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// 按 GFM 表格规则拆分文档（表头行 + `|---|` 分隔行 + 数据行）。
pub fn split_markdown_tables(text: &str) -> Vec<MdSegment> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut md_buf: Vec<&str> = Vec::new();

    let flush_md = |buf: &mut Vec<&str>, out: &mut Vec<MdSegment>| {
        if buf.is_empty() {
            return;
        }
        let s = buf.join("\n");
        buf.clear();
        if !s.trim().is_empty() {
            out.push(MdSegment::Markdown(s));
        }
    };

    while i < lines.len() {
        if i + 1 < lines.len()
            && looks_like_table_row(lines[i])
            && is_table_separator_row(lines[i + 1])
        {
            flush_md(&mut md_buf, &mut out);
            let headers = split_row(lines[i]);
            let cols = headers.len().max(1);
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && looks_like_table_row(lines[i]) && !is_table_separator_row(lines[i])
            {
                let mut cells = split_row(lines[i]);
                // 列数对齐，避免缺列导致错位
                while cells.len() < cols {
                    cells.push(String::new());
                }
                if cells.len() > cols {
                    cells.truncate(cols);
                }
                rows.push(cells);
                i += 1;
            }
            out.push(MdSegment::Table { headers, rows });
            continue;
        }
        md_buf.push(lines[i]);
        i += 1;
    }
    flush_md(&mut md_buf, &mut out);

    if out.is_empty() && !text.trim().is_empty() {
        out.push(MdSegment::Markdown(text.to_string()));
    }
    out
}

fn normalize_pipes(line: &str) -> String {
    // 中文全角竖线 → ASCII，便于模型输出兼容
    line.replace('｜', "|")
}

fn looks_like_table_row(line: &str) -> bool {
    let t = normalize_pipes(line).trim().to_string();
    if t.is_empty() {
        return false;
    }
    // 标准 GFM：以 | 开头；也兼容 `a | b | c` 无首尾 |
    let pipes = t.matches('|').count();
    if pipes >= 2 && t.starts_with('|') {
        return true;
    }
    if pipes >= 1 && !t.starts_with('|') {
        // 至少两列：`col1 | col2`
        return t.split('|').map(|c| c.trim()).filter(|c| !c.is_empty()).count() >= 2
            && !is_table_separator_row(&t);
    }
    false
}

fn is_table_separator_row(line: &str) -> bool {
    let t = normalize_pipes(line);
    let t = t.trim().trim_matches('|').trim();
    if t.is_empty() {
        return false;
    }
    t.split('|').all(|cell| {
        let c = cell.trim();
        if c.is_empty() {
            return false;
        }
        // 允许 ---、:---、---:、:---:，以及常见 unicode 横线
        let mut has_dash = false;
        for ch in c.chars() {
            match ch {
                '-' | '─' | '—' | '–' | ':' | ' ' => {
                    if ch == '-' || ch == '─' || ch == '—' || ch == '–' {
                        has_dash = true;
                    }
                }
                _ => return false,
            }
        }
        has_dash
    })
}

fn split_row(line: &str) -> Vec<String> {
    let t = normalize_pipes(line);
    let t = t.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// 解析单元格内 `**粗体**`、`` `代码` ``，其余为普通文本。
fn append_inline_md(job: &mut LayoutJob, text: &str, size: f32, color: Color32, header: bool) {
    let body_font = FontId::proportional(size);
    let emph_size = if header { size + 0.5 } else { size };
    let emph_font = FontId::proportional(emph_size);
    let mut i = 0;

    while i < text.len() {
        if text[i..].starts_with("**") {
            if let Some(rel) = text[i + 2..].find("**") {
                let inner = &text[i + 2..i + 2 + rel];
                job.append(
                    inner,
                    0.0,
                    TextFormat {
                        font_id: emph_font.clone(),
                        color,
                        extra_letter_spacing: 0.25,
                        ..Default::default()
                    },
                );
                i += 4 + rel;
                continue;
            }
        }
        if text.as_bytes()[i] == b'`' {
            if let Some(rel) = text[i + 1..].find('`') {
                let inner = &text[i + 1..i + 1 + rel];
                job.append(
                    inner,
                    0.0,
                    TextFormat {
                        font_id: FontId::monospace(size * 0.95),
                        color: Color32::from_rgb(60, 80, 100),
                        background: Color32::from_rgb(232, 236, 240),
                        ..Default::default()
                    },
                );
                i += 2 + rel;
                continue;
            }
        }

        let rest = &text[i..];
        let next = rest
            .find("**")
            .into_iter()
            .chain(rest.find('`').into_iter())
            .min()
            .unwrap_or(rest.len());
        let chunk = &rest[..next];
        job.append(
            chunk,
            0.0,
            TextFormat {
                font_id: if header {
                    emph_font.clone()
                } else {
                    body_font.clone()
                },
                color,
                extra_letter_spacing: if header { 0.2 } else { 0.0 },
                ..Default::default()
            },
        );
        i += next;
    }

    if job.sections.is_empty() {
        job.append(
            "",
            0.0,
            TextFormat {
                font_id: body_font,
                color,
                ..Default::default()
            },
        );
    }
}

fn make_job(text: &str, size: f32, color: Color32, header: bool, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    append_inline_md(&mut job, text, size, color, header);
    job
}

fn measure_unwrapped(ui: &egui::Ui, text: &str, size: f32, color: Color32, header: bool) -> f32 {
    let job = make_job(text, size, color, header, f32::INFINITY);
    ui.fonts(|f| f.layout_job(job).size().x)
}

fn build_row_galleys(
    ui: &egui::Ui,
    cells: &[String],
    col_w: &[f32],
    font_size: f32,
    color: Color32,
    header: bool,
) -> Vec<Arc<Galley>> {
    let mut out = Vec::with_capacity(col_w.len());
    for c in 0..col_w.len() {
        let text = cells.get(c).map(String::as_str).unwrap_or("");
        let job = make_job(text, font_size, color, header, col_w[c]);
        out.push(ui.fonts(|f| f.layout_job(job)));
    }
    out
}

pub fn render_markdown_table(ui: &mut egui::Ui, headers: &[String], rows: &[Vec<String>]) {
    let cols = headers.len().max(1);
    let avail = ui.available_width().max(120.0);
    let font_size = 13.5;
    let header_color = Color32::from_rgb(40, 60, 80);
    let cell_color = Color32::from_rgb(28, 36, 44);
    let col_gap = 20.0;
    let pad_x = 4.0;
    let max_col = (avail * 0.42).clamp(120.0, 280.0);

    let mut col_w = vec![48.0_f32; cols];
    for (c, h) in headers.iter().enumerate() {
        col_w[c] = col_w[c].max(measure_unwrapped(ui, h, font_size, header_color, true) + pad_x);
    }
    for row in rows {
        for c in 0..cols {
            let cell = row.get(c).map(String::as_str).unwrap_or("");
            col_w[c] =
                col_w[c].max(measure_unwrapped(ui, cell, font_size, cell_color, false) + pad_x);
        }
    }
    for w in &mut col_w {
        *w = (*w).clamp(48.0, max_col);
    }

    let table_w: f32 = col_w.iter().sum::<f32>() + col_gap * (cols.saturating_sub(1) as f32);
    let content_w = table_w.min(avail);

    // 必须强制 top_down：父级若是 horizontal / wrapping，行会全部排到同一行
    ui.allocate_ui_with_layout(
        Vec2::new(content_w + 24.0, 0.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_max_width(avail);
            Frame::new()
                .fill(Color32::from_rgb(248, 250, 252))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(220, 226, 232)))
                .corner_radius(8.0)
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ScrollArea::horizontal()
                        .id_salt(ui.id().with("md_table_scroll"))
                        .max_width(avail)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(table_w, 0.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    paint_row(
                                        ui,
                                        headers,
                                        &col_w,
                                        col_gap,
                                        font_size,
                                        header_color,
                                        true,
                                    );
                                    ui.add_space(4.0);
                                    let y = ui.cursor().top();
                                    let x0 = ui.cursor().left();
                                    ui.painter().hline(
                                        egui::Rangef::new(x0, x0 + table_w),
                                        y,
                                        egui::Stroke::new(
                                            1.0,
                                            Color32::from_rgb(220, 226, 232),
                                        ),
                                    );
                                    ui.add_space(6.0);

                                    for (ri, row) in rows.iter().enumerate() {
                                        let cells: Vec<String> = (0..cols)
                                            .map(|c| row.get(c).cloned().unwrap_or_default())
                                            .collect();
                                        let galleys = build_row_galleys(
                                            ui, &cells, &col_w, font_size, cell_color, false,
                                        );
                                        let row_h =
                                            galleys.iter().map(|g| g.size().y).fold(0.0, f32::max);
                                        let (rect, _) = ui.allocate_exact_size(
                                            Vec2::new(table_w, row_h + 4.0),
                                            Sense::hover(),
                                        );
                                        if ri % 2 == 1 {
                                            ui.painter().rect_filled(
                                                rect.expand2(Vec2::new(2.0, 1.0)),
                                                4.0,
                                                Color32::from_rgb(240, 244, 248),
                                            );
                                        }
                                        let mut x = rect.left();
                                        for c in 0..cols {
                                            ui.painter().galley(
                                                egui::pos2(x, rect.top() + 2.0),
                                                galleys[c].clone(),
                                                cell_color,
                                            );
                                            x += col_w[c] + col_gap;
                                        }
                                        ui.add_space(4.0);
                                    }
                                },
                            );
                        });
                });
        },
    );

    ui.add_space(8.0);
}

fn paint_row(
    ui: &mut egui::Ui,
    cells: &[String],
    col_w: &[f32],
    col_gap: f32,
    font_size: f32,
    color: Color32,
    header: bool,
) {
    let galleys = build_row_galleys(ui, cells, col_w, font_size, color, header);
    let row_h = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max);
    let row_w = col_w.iter().sum::<f32>() + col_gap * (col_w.len().saturating_sub(1) as f32);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::hover());
    let mut x = rect.left();
    for c in 0..col_w.len() {
        ui.painter()
            .galley(egui::pos2(x, rect.top()), galleys[c].clone(), color);
        x += col_w[c] + col_gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_simple_table() {
        let md = "前言\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n后记\n";
        let segs = split_markdown_tables(md);
        assert_eq!(segs.len(), 3);
        assert!(matches!(segs[0], MdSegment::Markdown(_)));
        assert!(matches!(segs[1], MdSegment::Table { .. }));
        assert!(matches!(segs[2], MdSegment::Markdown(_)));
        if let MdSegment::Table { headers, rows } = &segs[1] {
            assert_eq!(headers, &["a".to_string(), "b".to_string()]);
            assert_eq!(rows[0], vec!["1".to_string(), "2".to_string()]);
        }
    }

    #[test]
    fn splits_bioinformatics_style_tables() {
        let md = r#"### 基础处理

| 工具 | 用途 | 安装 |
| --- | --- | --- |
| fastqc | 测序数据质控报告 | conda install -c bioconda fastqc |

### 比对与变异

| 工具 | 用途 |
|---|---|
| bwa | 序列比对 |
| samtools | BAM 处理 |
"#;
        let segs = split_markdown_tables(md);
        let tables: Vec<_> = segs
            .iter()
            .filter(|s| matches!(s, MdSegment::Table { .. }))
            .collect();
        assert_eq!(tables.len(), 2);
        if let MdSegment::Table { headers, rows } = tables[0] {
            assert_eq!(headers.len(), 3);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0][0], "fastqc");
        }
        if let MdSegment::Table { rows, .. } = tables[1] {
            assert_eq!(rows.len(), 2);
        }
    }

    #[test]
    fn splits_without_leading_pipe() {
        let md = "工具 | 用途\n--- | ---\nfastqc | 质控\n";
        let segs = split_markdown_tables(md);
        assert!(matches!(segs[0], MdSegment::Table { .. }));
    }

    #[test]
    fn splits_fullwidth_pipes() {
        let md = "｜工具｜用途｜\n｜---｜---｜\n｜a｜b｜\n";
        let segs = split_markdown_tables(md);
        assert!(matches!(segs[0], MdSegment::Table { .. }));
    }
}
