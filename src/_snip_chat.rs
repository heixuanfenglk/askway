        // 消息区：限制宽度并左右留白，避免文本块撑出窗口
        let area_w = ui.available_width();
        ScrollArea::vertical()
            .id_salt("messages")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(area_w);
                let messages = self
                    .current_conv()
                    .map(|c| c.messages.clone())
                    .unwrap_or_default();

                if messages.is_empty() {
                    ui.add_space(48.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("有什么我能帮你的吗？")
                                .size(24.0)
                                .color(Color32::from_rgb(36, 48, 58)),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("在下方输入框提问，或拖拽文件到窗口")
                                .size(14.0)
                                .color(Color32::from_rgb(120, 132, 142)),
                        );
                    });
                }

                for msg in &messages {
                    self.render_message(ui, msg);
                }
                ui.add_space(16.0);
            });
    }

    /// 底部输入区：宽度由 BottomPanel 约束，避免超出窗口
