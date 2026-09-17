    fn render_message(&self, ui: &mut egui::Ui, msg: &Message) {
        let is_user = msg.role == Role::User;
        let (bg, stroke) = if is_user {
            (
                Color32::from_rgb(232, 240, 255),
                Color32::from_rgb(210, 224, 255),
            )
        } else {
            (
                Color32::from_rgb(255, 255, 255),
                Color32::from_rgb(230, 234, 238),
            )
        };

        ui.add_space(10.0);

        // 与输入框一致：左右留白，气泡不超过可用宽度
        let side_pad = 12.0;
        let avail = ui.available_width().max(80.0);
        let inner_w = (avail - side_pad * 2.0).max(60.0);
        let bubble_w = (inner_w * 0.92).clamp(60.0, inner_w);

        let content = if msg.content.is_empty() && self.streaming && msg.role == Role::Assistant {
            "…"
        } else if msg.content.is_empty() && !msg.attachments.is_empty() {
            "（仅附件）"
        } else {
            msg.content.as_str()
        };

        let draw_bubble = |ui: &mut egui::Ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(bubble_w, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(bubble_w);
                    ui.set_max_width(bubble_w);
                    Frame::new()
                        .fill(bg)
                        .corner_radius(16.0)
                        .inner_margin(Margin::symmetric(12, 10))
                        .stroke(Stroke::new(1.0, stroke))
                        .show(ui, |ui| {
                            let text_w = (bubble_w - 28.0).max(40.0);
                            ui.set_max_width(text_w);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(msg.role.label())
                                        .strong()
                                        .size(12.0)
                                        .color(Color32::from_rgb(90, 110, 130)),
                                );
                                ui.label(
                                    RichText::new(msg.created_at.format("%H:%M").to_string())
                                        .size(11.0)
                                        .color(Color32::from_rgb(150, 160, 170)),
                                );
                            });
                            ui.add_space(4.0);

                            if !msg.attachments.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for att in &msg.attachments {
                                        Frame::new()
                                            .fill(Color32::from_rgb(248, 250, 252))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(220, 226, 232),
                                            ))
                                            .corner_radius(8.0)
                                            .inner_margin(Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(att.summary())
                                                        .size(12.0)
                                                        .color(Color32::from_rgb(50, 80, 95)),
                                                );
                                            });
                                    }
                                });
                                ui.add_space(6.0);
                            }

                            if !content.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(content)
                                            .size(14.5)
                                            .color(Color32::from_rgb(28, 36, 44)),
                                    )
                                    .wrap(),
                                );
                            }
                        });
                },
            );
        };

        ui.horizontal(|ui| {
            ui.set_max_width(avail);
            ui.add_space(side_pad);
            if is_user {
                let spacer = (inner_w - bubble_w).max(0.0);
                ui.add_space(spacer);
                draw_bubble(ui);
            } else {
                draw_bubble(ui);
            }
        });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
