use eframe::egui;
use std::path::{Path, PathBuf};
use std::thread;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::fs;

// 引用 TransJLC 本地库的核心逻辑
use TransJLC::{Config, Converter};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
        .with_inner_size([680.0, 560.0])     // 默认大小
        .with_min_inner_size([600.0, 500.0]) // 最小大小限制
        .with_resizable(true)                // 允许调整大小
        .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "TransJLC Pro",
        options,
        Box::new(|cc| {
            setup_custom_fonts(&cc.egui_ctx);
            configure_custom_style(&cc.egui_ctx);
            Ok(Box::new(MyApp::default()))
        }),
    )
}

// --- 🎨 样式配置 ---
fn configure_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 12.0);
    style.spacing.window_margin = egui::Margin::same(20.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    ctx.set_style(style);
}

// --- 🔤 字体配置 ---
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "cjk_font".to_owned(),
                           egui::FontData::from_owned(load_system_font()).tweak(
                               egui::FontTweak { scale: 1.25, ..Default::default() }
                           ),
    );

    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "cjk_font".to_owned());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "cjk_font".to_owned());

    ctx.set_fonts(fonts);
}

fn load_system_font() -> Vec<u8> {
    let font_paths = [
        "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
        "/usr/share/fonts/wenquanyi/wqy-zenhei/wqy-zenhei.ttc",
        "/usr/share/fonts/adobe-source-han-sans/SourceHanSansCN-Regular.otf",
    ];

    for path_str in font_paths {
        let path = Path::new(path_str);
        if path.exists() {
            if let Ok(data) = fs::read(path) {
                return data;
            }
        }
    }
    Vec::new()
}

fn load_icon() -> eframe::egui::IconData {
    eframe::egui::IconData::default()
}

// --- 🖥️ 程序逻辑 ---

struct MyApp {
    input_path: PathBuf,
    output_path: PathBuf,
    eda_type: String,
    zip_enabled: bool,
    zip_name: String,
    status_message: String,
    status_type: StatusType,
    is_processing: bool,
    rx: Receiver<String>,
    tx: Sender<String>,
}

#[derive(PartialEq)]
enum StatusType {
    Info,
    Success,
    Error,
}

impl Default for MyApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            input_path: std::env::current_dir().unwrap_or(PathBuf::from(".")),
            output_path: std::env::current_dir().unwrap_or(PathBuf::from(".")).join("output"),
            eda_type: "auto".to_string(),
            zip_enabled: true,
            zip_name: "Gerber".to_string(),
            status_message: "等待任务...".to_string(),
            status_type: StatusType::Info,
            is_processing: false,
            rx,
            tx,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(msg) = self.rx.try_recv() {
            self.status_message = msg.clone();
            if msg.contains("成功") {
                self.status_type = StatusType::Success;
                self.is_processing = false;
            } else if msg.contains("失败") || msg.contains("Error") {
                self.status_type = StatusType::Error;
                self.is_processing = false;
            } else {
                self.status_type = StatusType::Info;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // 标题栏 (已居中)
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.heading(
                    egui::RichText::new("TransJLC Gerber 转换器")
                    .size(28.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 200, 255))
                );
                ui.label(egui::RichText::new("适配 Arch Linux KDE 环境").italics().weak());
            });
            ui.add_space(20.0);

            // 区域 1: 路径设置
            egui::Frame::group(ui.style())
            .inner_margin(15.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
            .show(ui, |ui| {
                // ✅ 关键修改：使用 vertical_centered 包裹内部元素
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("📂 路径设置").size(16.0).strong());
                    ui.add_space(5.0);

                    egui::Grid::new("path_grid")
                    .num_columns(3)
                    .spacing([10.0, 15.0])
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        // 输入
                        ui.label("输入源:");
                        let btn_in = ui.button("🔍 选择文件夹");
                        if btn_in.clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.input_path = path;
                            }
                        }
                        let available_width = ui.available_width();
                        ui.label(
                            egui::RichText::new(smart_truncate_path(&self.input_path, available_width))
                            .monospace()
                        );
                        ui.end_row();

                        // 输出
                        ui.label("保存到:");
                        let btn_out = ui.button("📂 选择文件夹");
                        if btn_out.clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.output_path = path;
                            }
                        }
                        let available_width = ui.available_width();
                        ui.label(
                            egui::RichText::new(smart_truncate_path(&self.output_path, available_width))
                            .monospace()
                        );
                        ui.end_row();
                    });
                });
            });

            ui.add_space(15.0);

            // 区域 2: 选项
            egui::Frame::group(ui.style())
            .inner_margin(15.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
            .show(ui, |ui| {
                // ✅ 关键修改：使用 vertical_centered 包裹内部元素
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("⚙️ 转换选项").size(16.0).strong());
                    ui.add_space(5.0);

                    egui::Grid::new("options_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                        ui.label("EDA 格式:");
                        egui::ComboBox::from_id_salt("eda_select")
                        .selected_text(&self.eda_type)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.eda_type, "auto".to_string(), "✨ 自动检测 (推荐)");
                            ui.separator();
                            ui.selectable_value(&mut self.eda_type, "kicad".to_string(), "KiCad");
                            ui.selectable_value(&mut self.eda_type, "protel".to_string(), "Protel / Altium");
                            ui.selectable_value(&mut self.eda_type, "jlc".to_string(), "JLC 标准格式");
                        });
                        ui.end_row();

                        ui.label("压缩输出:");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.zip_enabled, "生成 ZIP");
                            if self.zip_enabled {
                                ui.add(egui::TextEdit::singleline(&mut self.zip_name).hint_text("文件名").desired_width(150.0));
                                ui.label(".zip");
                            }
                        });
                        ui.end_row();
                    });
                });
            });

            ui.add_space(30.0);

            // 区域 3: 按钮 (已居中)
            ui.vertical_centered(|ui| {
                if self.is_processing {
                    ui.add(egui::Spinner::new().size(32.0));
                    ui.label(egui::RichText::new("正在转换...").size(14.0));
                } else {
                    let btn = egui::Button::new(
                        egui::RichText::new("🚀 开始转换")
                        .size(20.0)
                        .strong()
                        .color(egui::Color32::WHITE)
                    )
                    .min_size(egui::vec2(200.0, 50.0))
                    .fill(egui::Color32::from_rgb(0, 120, 215));

                    if ui.add(btn).on_hover_text("点击开始处理").clicked() {
                        self.start_conversion();
                    }
                }
            });

            // 底部状态栏
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(10.0);
                egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(50))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("状态:");
                        let (color, icon) = match self.status_type {
                            StatusType::Info => (egui::Color32::LIGHT_GRAY, "ℹ"),
                                  StatusType::Success => (egui::Color32::GREEN, "✅"),
                                  StatusType::Error => (egui::Color32::from_rgb(255, 100, 100), "❌"),
                        };
                        ui.label(egui::RichText::new(format!("{} {}", icon, self.status_message)).color(color).strong());
                    });
                });
            });
        });
    }
}

impl MyApp {
    fn start_conversion(&mut self) {
        self.is_processing = true;
        self.status_message = "初始化...".to_string();
        self.status_type = StatusType::Info;

        let config = Config {
            eda: self.eda_type.clone(),
            path: self.input_path.clone(),
            output_path: self.output_path.clone(),
            zip: self.zip_enabled,
            zip_name: self.zip_name.clone(),
            verbose: true,
            no_progress: true,
            top_color_image: None,
            bottom_color_image: None,
        };

        let tx = self.tx.clone();

        thread::spawn(move || {
            let mut converter = Converter::new(config);
            match converter.run() {
                Ok(_) => { let _ = tx.send("转换成功！文件已保存。".to_string()); }
                Err(e) => { let _ = tx.send(format!("转换失败: {:#}", e)); }
            }
        });
    }
}

fn smart_truncate_path(path: &Path, available_width: f32) -> String {
    let text = path.to_string_lossy().to_string();
    let max_chars = (available_width / 9.0) as usize;

    if text.len() > max_chars && max_chars > 3 {
        format!("...{}", &text[text.len() - (max_chars - 3)..])
    } else {
        text
    }
}
