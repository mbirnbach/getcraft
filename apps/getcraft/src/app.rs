use crate::background::{self, Command, CommandSender, Tray};
use crate::theme::{self, ACCENT, BORDER, CARD, DANGER, FAINT, MUTED, SIDEBAR, SUCCESS, TEXT};
use crate::{icons, instance, notify};
use egui::{Align, Color32, CornerRadius, Frame, Id, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use getcraft_core::engine::{Engine, Event, Job, Snapshot, ToolEntry, now};
use getcraft_core::install::Installer;
use getcraft_core::selfupdate;
use getcraft_core::state::{Paths, UpdatePolicy};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const DISCLAIMER: &str = "GetCraft is an independent, community-made launcher. \
    It is not affiliated with or endorsed by the ArtCraft team.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Apps,
    Updates,
    Settings,
}

/// Something the user asked for this frame, executed after drawing.
enum Action {
    Install(String),
    Cancel(String),
    Launch(String),
    AskUninstall(String),
    Uninstall(String),
    SetPolicy(String, Option<UpdatePolicy>),
    ShowNotes(String),
    DismissError(String),
    Refresh,
    UpdateAll,
    OpenUrl(String),
    RestartForUpdate,
}

struct Toast {
    text: String,
    good: bool,
    until: Instant,
}

pub struct GetCraftApp {
    engine: Engine,
    events: Receiver<Event>,
    page: Page,
    category: Option<String>,
    search: String,
    confirm_uninstall: Option<String>,
    notes_for: Option<String>,
    toasts: Vec<Toast>,
    data_dir: String,
    commands: Receiver<Command>,
    tray: Option<Tray>,
    /// Set when the user really wants to quit (tray menu), so the close isn't turned into a hide.
    quitting: bool,
    /// Whether the window is currently hidden in the menu bar / tray.
    hidden: bool,
    login_item_error: Option<String>,
    ctx: egui::Context,
}

impl GetCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Paths, listener: Option<TcpListener>, hidden: bool) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);

        let data_dir = paths.state_file.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let installer = Installer::new(&paths);
        let (tx, events) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let ctx2 = cc.egui_ctx.clone();
        let engine = Engine::new(
            paths,
            installer,
            selfupdate::locate(),
            move || ctx.request_repaint(),
            move |event| {
                notify::desktop(&event);
                let _ = tx.send(event);
                ctx2.request_repaint();
            },
        );
        engine.start();

        let (cmd_tx, commands) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let sender = CommandSender::new(cmd_tx, move || ctx.request_repaint());
        if let Some(listener) = listener {
            let sender = sender.clone();
            instance::serve(listener, move || sender.send(Command::Show));
        }
        background::on_reopen(sender.clone());
        let tray = Tray::create(sender);
        if hidden {
            background::set_dock_visible(false);
        }
        let login_item_error = background::sync_launch_at_login(engine.snapshot().settings.launch_at_login).err();

        Self {
            engine,
            events,
            page: Page::Apps,
            category: None,
            search: String::new(),
            confirm_uninstall: None,
            notes_for: None,
            toasts: Vec::new(),
            data_dir,
            commands,
            tray,
            quitting: false,
            hidden,
            login_item_error,
            ctx: cc.egui_ctx.clone(),
        }
    }

    /// Installs the downloaded GetCraft update and restarts into it.
    fn restart_for_update(&mut self, ctx: &egui::Context) {
        match self.engine.apply_launcher_update().and_then(|target| selfupdate::relaunch(&target, self.hidden)) {
            Ok(()) => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.toast(format!("Couldn't update GetCraft: {e}"), false),
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        background::set_dock_visible(true);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        background::set_dock_visible(false);
        let settings = self.engine.snapshot().settings;
        if !settings.background_hint_shown {
            let place = if cfg!(target_os = "macos") { "menu bar" } else { "system tray" };
            notify::message(
                "GetCraft is still running",
                &format!("It keeps your apps up to date from the {place}. Quit it from there."),
            );
            self.engine.update_settings(|s| s.background_hint_shown = true);
        }
    }

    fn toast(&mut self, text: String, good: bool) {
        self.toasts.push(Toast { text, good, until: Instant::now() + Duration::from_secs(6) });
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::Install(id) => self.engine.install(&id),
            Action::Cancel(id) => self.engine.cancel(&id),
            Action::Launch(id) => self.engine.launch(&id),
            Action::AskUninstall(id) => self.confirm_uninstall = Some(id),
            Action::Uninstall(id) => self.engine.uninstall(&id),
            Action::SetPolicy(id, p) => self.engine.set_policy(&id, p),
            Action::ShowNotes(id) => self.notes_for = Some(id),
            Action::DismissError(id) => self.engine.dismiss_error(&id),
            Action::Refresh => self.engine.refresh(),
            Action::UpdateAll => self.engine.update_all(),
            Action::RestartForUpdate => self.restart_for_update(&self.ctx.clone()),
            Action::OpenUrl(url) => {
                if let Err(e) = open::that(&url) {
                    log::warn!("could not open {url}: {e}");
                }
            }
        }
    }
}

impl eframe::App for GetCraftApp {
    /// Runs even while the window is hidden, so tray and Dock requests are always handled.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(cmd) = self.commands.try_recv() {
            match cmd {
                Command::Show => self.show_window(ctx),
                Command::CheckNow => self.engine.refresh(),
                Command::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        let snap = self.engine.snapshot();
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting
            && snap.settings.run_in_background
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
        }
        if let Some(tray) = &mut self.tray {
            tray.set_updates(snap.tools.iter().filter(|t| t.update_available()).count());
        }
        // Nobody is looking, so update GetCraft right away instead of asking.
        if self.hidden && !self.quitting && !snap.busy && snap.launcher_update.as_ref().is_some_and(|u| u.ready) {
            self.restart_for_update(ctx);
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Installed { tool, version, updated } => {
                    let verb = if updated { "updated to" } else { "installed," };
                    self.toast(format!("{tool} {verb} version {version}"), true);
                }
                Event::Failed { tool, error } => self.toast(format!("{tool}: {error}"), false),
                Event::UpdateAvailable { .. } | Event::LauncherReady { .. } => {}
            }
        }

        let snap = self.engine.snapshot();
        let mut actions = Vec::new();

        top_bar(ui, &snap, &mut self.search, &mut actions);
        footer(ui, &snap);
        self.sidebar(ui, &snap);
        if self.page != Page::Settings {
            installed_panel(ui, &snap, &mut actions);
        }
        egui::CentralPanel::default().frame(Frame::NONE.inner_margin(Margin::symmetric(28, 20))).show(ui, |ui| {
            if let Some(update) = snap.launcher_update.as_ref().filter(|u| u.ready)
                && launcher_banner(ui, update, snap.busy)
            {
                actions.push(Action::RestartForUpdate);
            }
            match self.page {
                Page::Apps => self.apps_page(ui, &snap, &mut actions),
                Page::Updates => updates_page(ui, &snap, &mut actions),
                Page::Settings => self.settings_page(ui, &snap),
            }
        });

        self.dialogs(ui.ctx(), &snap, &mut actions);
        self.draw_toasts(ui.ctx());

        for action in actions {
            self.run(action);
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Chrome: top bar, sidebar, installed panel, footer

fn top_bar(ui: &mut Ui, snap: &Snapshot, search: &mut String, actions: &mut Vec<Action>) {
    egui::Panel::top("top")
        .exact_size(60.0)
        .frame(Frame::NONE.fill(SIDEBAR).inner_margin(Margin::symmetric(18, 0)).stroke(Stroke::new(1.0, BORDER)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                logo(ui, 28.0);
                ui.label(RichText::new("GetCraft").size(18.0).strong().color(TEXT));

                let search_w = 440.0_f32.min(ui.available_width() - 260.0).max(160.0);
                ui.add_space(((ui.available_width() - search_w) / 2.0 - 90.0).max(16.0));
                ui.add(
                    egui::TextEdit::singleline(search)
                        .hint_text("🔍  Search Crafting Apps")
                        .desired_width(search_w)
                        .margin(vec2(14.0, 8.0)),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if snap.checking {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label(RichText::new("Checking…").color(MUTED));
                    } else if ui
                        .add(egui::Button::new(RichText::new("⟳").size(18.0)).frame(false))
                        .on_hover_text("Check for updates now")
                        .clicked()
                    {
                        actions.push(Action::Refresh);
                    }
                    if let Some(p) = snap.platform {
                        ui.label(RichText::new(p.to_string()).small().color(FAINT));
                    }
                });
            });
        });
}

fn footer(ui: &mut Ui, snap: &Snapshot) {
    egui::Panel::bottom("footer")
        .exact_size(30.0)
        .frame(Frame::NONE.fill(SIDEBAR).inner_margin(Margin::symmetric(18, 0)).stroke(Stroke::new(1.0, BORDER)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(DISCLAIMER).small().color(FAINT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text = match (&snap.last_error, snap.last_check) {
                        (Some(e), _) => RichText::new(format!("⚠ {e}")).color(DANGER),
                        (None, Some(t)) => RichText::new(format!("Checked {}", ago(t))).color(FAINT),
                        (None, None) => RichText::new("Not checked yet").color(FAINT),
                    };
                    ui.label(text.small());
                });
            });
        });
}

impl GetCraftApp {
    fn sidebar(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let updates = snap.tools.iter().filter(|t| t.update_available()).count();
        egui::Panel::left("nav")
            .exact_size(92.0)
            .resizable(false)
            .frame(Frame::NONE.fill(SIDEBAR).inner_margin(Margin::symmetric(8, 16)))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for (page, icon, label, badge) in [
                        (Page::Apps, "grid", "Apps", 0),
                        (Page::Updates, "⬇", "Updates", updates),
                        (Page::Settings, "⚙", "Settings", 0),
                    ] {
                        if nav_item(ui, icon, label, badge, self.page == page).clicked() {
                            self.page = page;
                        }
                        ui.add_space(4.0);
                    }
                });
            });
    }
}

fn nav_item(ui: &mut Ui, icon: &str, label: &str, badge: usize, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(76.0, 62.0), Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        theme::INPUT
    } else if resp.hovered() {
        theme::CARD
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(10), bg);
    let color = if selected { TEXT } else { MUTED };
    let icon_center = rect.center() - vec2(0.0, 9.0);
    if icon == "grid" {
        // A 2×2 grid of tiles; the default fonts have no good "apps" glyph.
        for (dx, dy) in [(-4.5, -4.5), (4.5, -4.5), (-4.5, 4.5), (4.5, 4.5)] {
            let tile = egui::Rect::from_center_size(icon_center + vec2(dx, dy), vec2(7.0, 7.0));
            painter.rect_stroke(tile, CornerRadius::same(2), Stroke::new(1.5, color), egui::StrokeKind::Inside);
        }
    } else {
        painter.text(icon_center, egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), color);
    }
    painter.text(
        rect.center() + vec2(0.0, 15.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        color,
    );
    if badge > 0 {
        let c = rect.center() + vec2(14.0, -18.0);
        painter.circle_filled(c, 8.0, ACCENT);
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            badge.to_string(),
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn installed_panel(ui: &mut Ui, snap: &Snapshot, actions: &mut Vec<Action>) {
    egui::Panel::right("installed")
        .exact_size(300.0)
        .resizable(false)
        .frame(Frame::NONE.fill(SIDEBAR).inner_margin(Margin::symmetric(20, 20)))
        .show(ui, |ui| {
            ui.label(RichText::new("Installed").size(18.0).strong());
            let installed: Vec<&ToolEntry> = snap.tools.iter().filter(|t| t.installed.is_some()).collect();
            let updates = installed.iter().filter(|t| t.update_available()).count();
            let status = if installed.is_empty() {
                "Nothing installed yet. Pick an app to get started.".to_owned()
            } else if updates == 0 {
                "All installed apps are up to date.".to_owned()
            } else if updates == 1 {
                "1 update available.".to_owned()
            } else {
                format!("{updates} updates available.")
            };
            ui.label(RichText::new(status).color(MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!snap.checking, theme::light("Check for updates")).clicked() {
                    actions.push(Action::Refresh);
                }
                if updates > 0 && ui.add(theme::primary("Update all")).clicked() {
                    actions.push(Action::UpdateAll);
                }
            });
            ui.add_space(14.0);
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for entry in installed {
                    let id = &entry.tool.id;
                    let resp = ui
                        .horizontal(|ui| {
                            ui.add(icons::image(&entry.tool).fit_to_exact_size(vec2(34.0, 34.0)).corner_radius(7));
                            ui.vertical(|ui| {
                                ui.add_space(1.0);
                                ui.label(RichText::new(&entry.tool.name).strong());
                                let v = &entry.installed.as_ref().unwrap().version;
                                if entry.update_available() {
                                    let new = &entry.latest.as_ref().unwrap().version;
                                    ui.label(RichText::new(format!("{v}  ›  {new}")).small().color(ACCENT));
                                } else {
                                    ui.label(RichText::new(v).small().color(FAINT));
                                }
                            });
                        })
                        .response
                        .interact(Sense::click())
                        .on_hover_text("Open")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.clicked() {
                        actions.push(Action::Launch(id.clone()));
                    }
                    ui.add_space(6.0);
                }
            });
        });
}

// ------------------------------------------------------------------------------------------------
// Pages

impl GetCraftApp {
    fn apps_page(&mut self, ui: &mut Ui, snap: &Snapshot, actions: &mut Vec<Action>) {
        // Category chips, only for categories that have tools.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if chip(ui, "All apps", self.category.is_none()).clicked() {
                self.category = None;
            }
            for cat in &snap.categories {
                if snap.tools.iter().any(|t| t.tool.category == cat.id)
                    && chip(ui, &cat.name, self.category.as_deref() == Some(cat.id.as_str())).clicked()
                {
                    self.category = Some(cat.id.clone());
                }
            }
            if snap.tools.iter().any(|t| t.tool.discovered)
                && chip(ui, "New", self.category.as_deref() == Some("other")).clicked()
            {
                self.category = Some("other".into());
            }
        });
        ui.add_space(6.0);
        ui.separator();

        let query = self.search.trim().to_lowercase();
        let visible: Vec<&ToolEntry> = snap
            .tools
            .iter()
            .filter(|t| self.category.as_ref().is_none_or(|c| &t.tool.category == c))
            .filter(|t| {
                query.is_empty()
                    || t.tool.name.to_lowercase().contains(&query)
                    || t.tool.kind.to_lowercase().contains(&query)
                    || t.tool.description.to_lowercase().contains(&query)
            })
            .collect();

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(10.0);
            let curated: Vec<&ToolEntry> = visible.iter().copied().filter(|t| !t.tool.discovered).collect();
            let discovered: Vec<&ToolEntry> = visible.iter().copied().filter(|t| t.tool.discovered).collect();
            if !curated.is_empty() {
                ui.label(RichText::new("Crafting Apps").heading().strong());
                ui.label(
                    RichText::new("Free, open-source creative apps from the ArtCraft team, built in Rust.")
                        .color(MUTED),
                );
                ui.add_space(8.0);
                card_grid(ui, &curated, snap, actions);
            }
            if !discovered.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new("New on GitHub").heading().strong());
                ui.label(RichText::new("Recently published tools that GetCraft found automatically.").color(MUTED));
                ui.add_space(8.0);
                card_grid(ui, &discovered, snap, actions);
            }
            if visible.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    let text = if snap.checking { "Looking for apps…" } else { "No apps match your search." };
                    ui.label(RichText::new(text).color(MUTED));
                });
            }
            ui.add_space(20.0);
        });
    }

    fn settings_page(&mut self, ui: &mut Ui, snap: &Snapshot) {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(640.0);
            ui.label(RichText::new("Settings").heading().strong());
            ui.add_space(12.0);

            section(ui, "When an update is released", |ui| {
                let mut policy = snap.settings.default_policy;
                for p in UpdatePolicy::ALL {
                    ui.radio_value(&mut policy, p, p.label());
                }
                if policy != snap.settings.default_policy {
                    self.engine.update_settings(|s| s.default_policy = policy);
                }
                ui.label(
                    RichText::new("Applies to every app unless you choose differently in its ⋯ menu. Apps are never updated while they're open.")
                        .small()
                        .color(FAINT),
                );
            });

            section(ui, "Check for updates", |ui| {
                let mut hours = snap.settings.check_interval_hours;
                egui::ComboBox::from_id_salt("interval")
                    .selected_text(interval_label(hours))
                    .show_ui(ui, |ui| {
                        for h in [1, 3, 6, 12, 24, 72] {
                            ui.selectable_value(&mut hours, h, interval_label(h));
                        }
                    });
                if hours != snap.settings.check_interval_hours {
                    self.engine.update_settings(|s| s.check_interval_hours = hours);
                }
            });

            section(ui, "Background", |ui| {
                let place = if cfg!(target_os = "macos") { "menu bar" } else { "system tray" };
                let mut run_in_background = snap.settings.run_in_background;
                if ui
                    .checkbox(&mut run_in_background, format!("Keep running in the {place} when the window is closed"))
                    .changed()
                {
                    self.engine.update_settings(|s| s.run_in_background = run_in_background);
                }
                let mut launch_at_login = snap.settings.launch_at_login;
                let installed = background::is_installed_build();
                if ui
                    .add_enabled(installed, egui::Checkbox::new(&mut launch_at_login, "Start GetCraft when I log in"))
                    .changed()
                {
                    self.engine.update_settings(|s| {
                        s.launch_at_login = launch_at_login;
                        s.login_prompt_answered = true;
                    });
                    self.login_item_error = background::sync_launch_at_login(launch_at_login).err();
                }
                if !installed {
                    ui.label(RichText::new("Available once GetCraft is installed (not in development builds).").small().color(FAINT));
                }
                if let Some(e) = &self.login_item_error {
                    ui.label(RichText::new(format!("Couldn't change the login item: {e}")).small().color(DANGER));
                }
                ui.label(
                    RichText::new("While GetCraft runs it checks for updates and applies automatic ones, even with its window closed.")
                        .small()
                        .color(FAINT),
                );
            });

            section(ui, "Locations", |ui| {
                kv(ui, "Apps are installed to", &snap.apps_dir.display().to_string());
                kv(ui, "GetCraft settings", &self.data_dir);
            });

            section(ui, "About", |ui| {
                ui.label(format!("GetCraft {}", env!("CARGO_PKG_VERSION")));
                ui.label(RichText::new(DISCLAIMER).color(MUTED));
                ui.label(
                    RichText::new(
                        "Apps are downloaded straight from the official GitHub releases of each project and checked \
                         against their published checksums.",
                    )
                    .color(MUTED),
                );
                ui.horizontal(|ui| {
                    ui.hyperlink_to("The Crafting Apps", "https://getartcraft.com/apps");
                    ui.hyperlink_to("Source code", "https://github.com/mbirnbach/getcraft");
                    ui.hyperlink_to("Licenses", "https://github.com/mbirnbach/getcraft/blob/main/NOTICE");
                    ui.hyperlink_to("Privacy", "https://github.com/mbirnbach/getcraft/blob/main/PRIVACY.md");
                });
            });

            section(ui, "Credits", |ui| {
                ui.label(
                    RichText::new(
                        "GetCraft is free software under the MIT or Apache-2.0 license. Its app icon is \
                         original artwork for GetCraft and not covered by that license.",
                    )
                    .color(MUTED),
                );
                ui.label(
                    RichText::new(
                        "The app icons shown here are copyright (c) 2026 ArtCraft Team and the contributors of \
                         each app, used under the MIT License. ArtCraft is a trademark of the ArtCraft Team.",
                    )
                    .color(MUTED),
                );
                ui.hyperlink_to("Full attribution", "https://github.com/mbirnbach/getcraft/blob/main/ATTRIBUTION.md");
            });
        });
    }
}

fn updates_page(ui: &mut Ui, snap: &Snapshot, actions: &mut Vec<Action>) {
    let updates: Vec<&ToolEntry> = snap
        .tools
        .iter()
        .filter(|t| {
            t.update_available()
                || matches!(t.job, Some(Job::Downloading { .. } | Job::Installing)) && t.installed.is_some()
        })
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new("Updates").heading().strong());
        ui.add_space(8.0);
        if updates.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("✔").size(36.0).color(SUCCESS));
                ui.label(RichText::new("Everything is up to date").size(16.0).strong());
                let when = snap.last_check.map_or("never".into(), ago);
                ui.label(RichText::new(format!("Last checked {when}")).color(MUTED));
            });
        } else {
            card_grid(ui, &updates, snap, actions);
        }
    });
}

// ------------------------------------------------------------------------------------------------
// Cards

const CARD_MIN_W: f32 = 300.0;
const CARD_H: f32 = 190.0;
const GAP: f32 = 16.0;

fn card_grid(ui: &mut Ui, tools: &[&ToolEntry], snap: &Snapshot, actions: &mut Vec<Action>) {
    let avail = ui.available_width();
    let cols = (((avail + GAP) / (CARD_MIN_W + GAP)).floor() as usize).max(1);
    let card_w = ((avail - GAP * (cols - 1) as f32) / cols as f32).floor();
    for row in tools.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for entry in row {
                tool_card(ui, entry, card_w, snap, actions);
            }
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y);
    }
}

fn tool_card(ui: &mut Ui, entry: &ToolEntry, width: f32, snap: &Snapshot, actions: &mut Vec<Action>) {
    let id = entry.tool.id.clone();
    let margin = 18.0;
    let inner = vec2(width - 2.0 * margin - 2.0, CARD_H - 2.0 * margin - 2.0);
    Frame::NONE.fill(CARD).stroke(Stroke::new(1.0, BORDER)).corner_radius(theme::RADIUS).inner_margin(margin).show(
        ui,
        |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                card_actions(ui, entry, snap, actions);
                if let Some(err) = &entry.error {
                    let r = ui.add(
                        egui::Label::new(RichText::new(format!("⚠ {err}")).small().color(DANGER))
                            .truncate()
                            .sense(Sense::click()),
                    );
                    if r.on_hover_text(format!("{err}\n\nClick to dismiss")).clicked() {
                        actions.push(Action::DismissError(id.clone()));
                    }
                }
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(icons::image(&entry.tool).fit_to_exact_size(vec2(42.0, 42.0)).corner_radius(9));
                        ui.vertical(|ui| {
                            ui.add_space(2.0);
                            ui.label(RichText::new(&entry.tool.name).size(17.0).strong().color(TEXT));
                            ui.label(RichText::new(&entry.tool.kind).small().color(MUTED));
                        });
                        if let Some(latest) = &entry.latest {
                            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                ui.label(RichText::new(format!("v{}", latest.version)).small().color(FAINT));
                            });
                        }
                    });
                    ui.add_space(6.0);
                    let mut job = egui::text::LayoutJob::simple(
                        entry.tool.description.clone(),
                        egui::FontId::proportional(14.0),
                        MUTED,
                        inner.x,
                    );
                    job.wrap.max_rows = 2;
                    ui.label(job);
                });
            });
        },
    );
}

fn card_actions(ui: &mut Ui, entry: &ToolEntry, snap: &Snapshot, actions: &mut Vec<Action>) {
    let id = entry.tool.id.clone();
    ui.horizontal(|ui| {
        ui.set_height(30.0);
        more_menu(ui, entry, actions);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &entry.job {
            Some(Job::Downloading { done, total }) => {
                if ui.add(egui::Button::new(RichText::new("Cancel").small().color(MUTED)).frame(false)).clicked() {
                    actions.push(Action::Cancel(id.clone()));
                }
                let frac = total.map_or(0.0, |t| *done as f32 / t.max(1) as f32);
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(ui.available_width().min(220.0))
                        .text(RichText::new(format!("{} of {}", mb(*done), total.map_or("?".into(), mb))).small()),
                );
            }
            Some(Job::Installing) => {
                ui.label(RichText::new("Installing…").color(MUTED));
                ui.add(egui::Spinner::new());
            }
            Some(Job::Removing) => {
                ui.label(RichText::new("Removing…").color(MUTED));
                ui.add(egui::Spinner::new());
            }
            None => match (&entry.installed, &entry.latest) {
                (Some(_), _) if entry.update_available() => {
                    if ui.add(theme::primary("Update")).clicked() {
                        actions.push(Action::Install(id.clone()));
                    }
                    if ui.add(theme::pill("Open")).clicked() {
                        actions.push(Action::Launch(id.clone()));
                    }
                }
                (Some(_), _) => {
                    if ui.add(theme::pill("Open")).clicked() {
                        actions.push(Action::Launch(id.clone()));
                    }
                }
                (None, Some(_)) if entry.available() => {
                    let size = entry.latest.as_ref().and_then(|l| l.package.as_ref()).map(|(a, _)| mb(a.size));
                    let resp = ui.add(theme::pill("Install"));
                    if resp.clicked() {
                        actions.push(Action::Install(id.clone()));
                    }
                    if let Some(size) = size {
                        ui.label(RichText::new(size).small().color(FAINT));
                    }
                }
                (None, Some(_)) => {
                    let os = snap.platform.map_or("this system".into(), |p| p.os.to_string());
                    ui.label(RichText::new(format!("Not available for {os} yet")).small().color(FAINT));
                }
                (None, None) if snap.checking => {
                    ui.add(egui::Spinner::new());
                }
                (None, None) => {
                    ui.label(RichText::new("No release yet").small().color(FAINT));
                }
            },
        });
    });
}

fn more_menu(ui: &mut Ui, entry: &ToolEntry, actions: &mut Vec<Action>) {
    let id = entry.tool.id.clone();
    ui.menu_button(RichText::new("•••").color(MUTED), |ui| {
        ui.set_min_width(220.0);
        if entry.latest.is_some() && ui.button("What's new").clicked() {
            actions.push(Action::ShowNotes(id.clone()));
        }
        if ui.button("View on GitHub").clicked() {
            actions.push(Action::OpenUrl(entry.tool.repo_url()));
        }
        if let Some(installed) = &entry.installed {
            let folder = getcraft_core::install::install_root(&installed.path)
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let label = if cfg!(target_os = "macos") { "Show in Finder" } else { "Open install folder" };
            if ui.button(label).clicked() {
                actions.push(Action::OpenUrl(folder));
            }
            ui.separator();
            ui.label(RichText::new("Updates").small().color(FAINT));
            let custom = entry.policy;
            for p in UpdatePolicy::ALL {
                if ui.radio(custom == p, p.label()).clicked() {
                    actions.push(Action::SetPolicy(id.clone(), Some(p)));
                }
            }
            ui.separator();
            if ui.button(RichText::new("Uninstall…").color(DANGER)).clicked() {
                actions.push(Action::AskUninstall(id.clone()));
            }
        }
    });
}

// ------------------------------------------------------------------------------------------------
// Dialogs and toasts

impl GetCraftApp {
    fn dialogs(&mut self, ctx: &egui::Context, snap: &Snapshot, actions: &mut Vec<Action>) {
        // Starting at login changes the system's configuration, so ask once instead of just
        // doing it. Development builds never register a login item, so they never ask.
        if !snap.settings.login_prompt_answered && !self.hidden && background::is_installed_build() {
            let mut answer = None;
            egui::Modal::new(Id::new("login-prompt")).show(ctx, |ui| {
                ui.set_width(400.0);
                ui.horizontal(|ui| {
                    logo(ui, 36.0);
                    ui.label(RichText::new("Keep your apps up to date?").size(17.0).strong());
                });
                ui.add_space(4.0);
                let place = if cfg!(target_os = "macos") { "menu bar" } else { "system tray" };
                ui.label(
                    RichText::new(format!(
                        "GetCraft can start when you log in and check for updates quietly from the {place}. \
                         You can change this any time in Settings."
                    ))
                    .color(MUTED),
                );
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary("Start at login")).clicked() {
                        answer = Some(true);
                    }
                    if ui.add(theme::pill("Not now")).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if let Some(enabled) = answer {
                self.engine.update_settings(|s| {
                    s.launch_at_login = enabled;
                    s.login_prompt_answered = true;
                });
                self.login_item_error = background::sync_launch_at_login(enabled).err();
            }
        }

        if let Some(id) = self.confirm_uninstall.clone() {
            let name = snap.tools.iter().find(|t| t.tool.id == id).map_or(id.clone(), |t| t.tool.name.clone());
            let modal = egui::Modal::new(Id::new("confirm-uninstall")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(format!("Uninstall {name}?")).size(17.0).strong());
                let where_to = if cfg!(target_os = "macos") { "moved to the Trash" } else { "removed" };
                ui.label(
                    RichText::new(format!("The app will be {where_to}. Your documents are not affected.")).color(MUTED),
                );
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary("Uninstall").fill(DANGER)).clicked() {
                        actions.push(Action::Uninstall(id.clone()));
                        self.confirm_uninstall = None;
                    }
                    if ui.add(theme::pill("Cancel")).clicked() {
                        self.confirm_uninstall = None;
                    }
                });
            });
            if modal.should_close() {
                self.confirm_uninstall = None;
            }
        }

        if let Some(id) = self.notes_for.clone() {
            let Some(entry) = snap.tools.iter().find(|t| t.tool.id == id) else {
                self.notes_for = None;
                return;
            };
            let Some(latest) = &entry.latest else { return };
            let modal = egui::Modal::new(Id::new("notes")).show(ctx, |ui| {
                ui.set_width(560.0);
                ui.horizontal(|ui| {
                    ui.add(icons::image(&entry.tool).fit_to_exact_size(vec2(36.0, 36.0)).corner_radius(8));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{} {}", entry.tool.name, latest.version)).size(17.0).strong());
                        if let Some(date) = &latest.published_at {
                            ui.label(
                                RichText::new(format!("Released {}", &date[..date.len().min(10)])).small().color(FAINT),
                            );
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    let notes =
                        if latest.notes.trim().is_empty() { "No release notes." } else { latest.notes.as_str() };
                    ui.label(RichText::new(notes).color(MUTED));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.link("Open on GitHub").clicked() {
                        actions.push(Action::OpenUrl(latest.html_url.clone()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(theme::pill("Close")).clicked() {
                            self.notes_for = None;
                        }
                    });
                });
            });
            if modal.should_close() {
                self.notes_for = None;
            }
        }
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-20.0, -44.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    Frame::NONE
                        .fill(theme::INPUT)
                        .stroke(Stroke::new(1.0, if toast.good { SUCCESS } else { DANGER }))
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_max_width(340.0);
                            ui.label(RichText::new(&toast.text).color(TEXT));
                        });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

// ------------------------------------------------------------------------------------------------
// Small helpers

/// The GetCraft app icon, drawn `size` points tall.
fn logo(ui: &mut Ui, size: f32) {
    ui.add(
        egui::Image::from_bytes("bytes://getcraft-logo.png", crate::icons::GETCRAFT)
            .fit_to_exact_size(vec2(size, size))
            .corner_radius(size * 0.2),
    );
}

/// "A new GetCraft is ready" bar. Returns true when the user clicks restart.
fn launcher_banner(ui: &mut Ui, update: &getcraft_core::engine::LauncherUpdate, busy: bool) -> bool {
    let mut restart = false;
    Frame::NONE
        .fill(theme::ACCENT_SOFT)
        .stroke(Stroke::new(1.0, ACCENT))
        .corner_radius(theme::RADIUS)
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                logo(ui, 18.0);
                ui.label(RichText::new(format!("GetCraft {} is ready to install.", update.version)).strong());
                if ui.link("What's new").clicked() {
                    let _ = open::that(&update.html_url);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let button = ui.add_enabled(!busy, theme::primary("Restart now"));
                    restart = button.on_disabled_hover_text("Wait for app installs to finish").clicked();
                });
            });
        });
    ui.add_space(12.0);
    restart
}

fn chip(ui: &mut Ui, text: &str, selected: bool) -> egui::Response {
    let (fill, color, stroke) = if selected {
        (TEXT, theme::BG, Stroke::NONE)
    } else {
        (Color32::TRANSPARENT, MUTED, Stroke::new(1.0, BORDER))
    };
    ui.add(
        egui::Button::new(RichText::new(text).color(color).strong())
            .fill(fill)
            .stroke(stroke)
            .corner_radius(theme::PILL)
            .min_size(vec2(0.0, 30.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    Frame::NONE.fill(CARD).stroke(Stroke::new(1.0, BORDER)).corner_radius(theme::RADIUS).inner_margin(18).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(4.0);
            add(ui);
        },
    );
    ui.add_space(12.0);
}

fn kv(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).color(MUTED));
        ui.label(RichText::new(value).monospace().small());
    });
}

fn interval_label(hours: u32) -> String {
    match hours {
        1 => "Every hour".into(),
        24 => "Once a day".into(),
        h if h % 24 == 0 => format!("Every {} days", h / 24),
        h => format!("Every {h} hours"),
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_000_000.0)
}

fn ago(unix: u64) -> String {
    let secs = now().saturating_sub(unix);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}
