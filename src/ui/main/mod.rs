use std::io::Read;
use relm4::{
    prelude::*,
    actions::*,
    MessageBroker
};

use gtk::prelude::*;
use adw::prelude::*;

use gtk::glib::clone;

mod repair_game;
// mod update_patch;
mod download_wine;
mod create_prefix;
mod download_diff;
mod disable_telemetry;
mod launch;

use anime_launcher_sdk::config::ConfigExt;
use anime_launcher_sdk::wuwa::config::Config;

use anime_launcher_sdk::wuwa::config::schema::launcher::LauncherStyle;

use anime_launcher_sdk::wuwa::states::*;
use anime_launcher_sdk::wuwa::consts::*;

use crate::*;
use crate::ui::components::*;

use super::preferences::main::*;
use super::about::*;

use std::sync::atomic::{Ordering};

use ksni;
use ksni::blocking::TrayMethods;

use image::{GenericImageView};
use std::sync::LazyLock;
use gtk::gio;

struct LauncherSystray {
    sender: ComponentSender<App>,
}

impl ksni::Tray for LauncherSystray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        tr!("application-name").into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.sender.input(AppMsg::ToggleWindow)
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // Directly taken from the ksni example code
        static ICON: LazyLock<ksni::Icon> = LazyLock::new(|| {
            // Load data from the GTK toolkit resource bits
            let icon_ref = gio::functions::resources_open_stream(
                &format!("{APP_RESOURCE_PATH}/icons/hicolor/scalable/apps/{APP_ID}.png").as_str(),
                gio::ResourceLookupFlags::NONE
            ).expect("yes");
            let mut datastore: Vec<u8> = Vec::new();
            let _ = icon_ref.into_read().read_to_end(&mut datastore);

            // Process data frmo PNG
            let img = image::load_from_memory_with_format(
                datastore.as_mut_slice(),
                image::ImageFormat::Png,
            ).expect("valid image");
            let (width, height) = img.dimensions();
            let mut data = img.into_rgba8().into_vec();
            assert_eq!(data.len() % 4, 0);
            for pixel in data.chunks_exact_mut(4) {
                pixel.rotate_right(1) // rgba to argb
            }

            // Generate the icon, finally
            ksni::Icon {
                width: width as i32,
                height: height as i32,
                data,
            }
        });

        // A clone is a waste for static icon, but the API have to accommodate dynamically generated
        // icons, and keep simplicity
        vec![ICON.clone()]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Show/Hide".into(),
                activate: Box::new(|this: &mut Self| {
                    this.sender.input(AppMsg::ToggleWindow)
                }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: tr!("exit").into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| {
                    std::process::exit(0)
                }),
                ..Default::default()
            }.into(),
        ]
    }
}


relm4::new_action_group!(WindowActionGroup, "win");

relm4::new_stateless_action!(LauncherFolder, WindowActionGroup, "launcher_folder");
relm4::new_stateless_action!(GameFolder, WindowActionGroup, "game_folder");
relm4::new_stateless_action!(ConfigFile, WindowActionGroup, "config_file");
relm4::new_stateless_action!(DebugFile, WindowActionGroup, "debug_file");

relm4::new_stateless_action!(About, WindowActionGroup, "about");

pub static mut MAIN_WINDOW: Option<adw::ApplicationWindow> = None;
pub static mut PREFERENCES_WINDOW: Option<AsyncController<PreferencesApp>> = None;
pub static mut ABOUT_DIALOG: Option<Controller<AboutDialog>> = None;

pub struct App {
    progress_bar: AsyncController<ProgressBar>,

    toast_overlay: adw::ToastOverlay,

    loading: Option<Option<String>>,
    style: LauncherStyle,
    state: Option<LauncherState>,

    downloading: bool,
    disabled_buttons: bool,
    kill_game_button: bool,
    disabled_kill_game_button: bool,

    game_is_running: bool
}

#[derive(Debug)]
pub enum AppMsg {
    UpdateLauncherState {
        /// Perform action when game or voice downloading is required
        /// Needed for chained executions (e.g. update one voice after another)
        perform_on_download_needed: bool,

        /// Show status gathering progress page
        show_status_page: bool
    },

    /// Supposed to be called automatically on app's run when the latest game version
    /// was retrieved from the API
    //SetGameDiff(Option<VersionDiff>),

    /// Supposed to be called automatically on app's run when the latest main patch version
    /// was retrieved from remote repos
    // SetMainPatch(Option<(Version, JadeitePatchStatusVariant)>),

    /// Supposed to be called automatically on app's run when the launcher state was chosen
    SetLauncherState(Option<LauncherState>),

    SetLauncherStyle(LauncherStyle),
    SetLoadingStatus(Option<Option<String>>),

    SetGameIsRunningState(bool),
    SetDownloading(bool),
    DisableButtons(bool),
    SetKillGameButton(bool),
    DisableKillGameButton(bool),

    OpenPreferences,
    RepairGame,

    PerformAction,

    HideWindow,
    ShowWindow,
    //ShowWindowWithAutoClose,
    ToggleWindow,

    Toast {
        title: String,
        description: Option<String>
    }
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();

    menu! {
        main_menu: {
            section! {
                &tr!("launcher-folder") => LauncherFolder,
                &tr!("game-folder") => GameFolder,
                &tr!("config-file") => ConfigFile,
                &tr!("debug-file") => DebugFile,
            },

            section! {
                &tr!("about") => About
            }
        }
    }

    view! {
        main_window = adw::ApplicationWindow {
            set_icon_name: Some(APP_ID),

            #[watch]
            set_default_size: (
                match model.style {
                    LauncherStyle::Modern => 900,
                    LauncherStyle::Classic => 1094, // (w = 1280 / 730 * h, where 1280x730 is default background picture resolution)
                    LauncherStyle::SteamDeck => 1280
                },
                match model.style {
                    LauncherStyle::Modern => 600,
                    LauncherStyle::Classic => 624,
                    LauncherStyle::SteamDeck => 800
                }
            ),

            #[watch]
            set_css_classes: &{
                let mut classes = vec!["background", "csd"];

                if APP_DEBUG {
                    classes.push("devel");
                }

                match model.style {
                    LauncherStyle::Modern => (),
                    LauncherStyle::SteamDeck => (),
                    LauncherStyle::Classic => {
                        if model.loading.is_none() {
                            classes.push("classic-style");
                        }
                    }
                }

                classes
            },

            #[local_ref]
            toast_overlay -> adw::ToastOverlay {
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    adw::HeaderBar {
                        #[watch]
                        set_css_classes: match model.style {
                            LauncherStyle::Modern => &[""],
                            LauncherStyle::SteamDeck => &[""],
                            LauncherStyle::Classic => &["flat"],
                        },

                        #[wrap(Some)]
                        set_title_widget = &adw::WindowTitle {
                            #[watch]
                            set_title: match model.style {
                                LauncherStyle::Modern => "", // NOOO
                                LauncherStyle::Classic => "",
                                LauncherStyle::SteamDeck => ""
                            }
                        },

                        pack_end = &gtk::MenuButton {
                            set_icon_name: "open-menu-symbolic",
                            set_menu_model: Some(&main_menu)
                        }
                    },

                    adw::StatusPage {
                        set_title: &tr!("loading-data"),
                        set_icon_name: Some(APP_ID),
                        set_vexpand: true,

                        #[watch]
                        set_description: match &model.loading {
                            Some(Some(desc)) => Some(desc),
                            Some(None) | None => None
                        },

                        #[watch]
                        set_visible: model.loading.is_some()
                    },

                    adw::PreferencesPage {
                        #[watch]
                        set_visible: model.loading.is_none(),

                        add = &adw::PreferencesGroup {
                            set_margin_top: 48,

                            #[watch]
                            set_visible: model.style == LauncherStyle::Modern,

                            gtk::Picture {
                                set_resource: Some(&format!("{APP_RESOURCE_PATH}/icons/hicolor/scalable/apps/{APP_ID}.png")),
                                set_vexpand: true,
                                set_content_fit: gtk::ContentFit::ScaleDown
                            },

                            gtk::Label {
                                set_label: &tr!("application-name"),
                                set_margin_top: 32,
                                add_css_class: "title-1"
                            }
                        },

                        add = &adw::PreferencesGroup {
                            #[watch]
                            set_valign: match model.style {
                                LauncherStyle::SteamDeck => gtk::Align::Center,
                                LauncherStyle::Modern => gtk::Align::Center,
                                LauncherStyle::Classic => gtk::Align::End
                            },

                            #[watch]
                            set_width_request: match model.style {
                                LauncherStyle::Modern => -1,
                                LauncherStyle::SteamDeck => 800,
                                LauncherStyle::Classic => 800
                            },

                            #[watch]
                            set_visible: model.downloading,

                            set_vexpand: true,
                            set_margin_top: 48,
                            set_margin_bottom: 48,

                            add = model.progress_bar.widget(),
                        },

                        add = &adw::PreferencesGroup {
                            #[watch]
                            set_valign: match model.style {
                                LauncherStyle::SteamDeck => gtk::Align::Center,
                                LauncherStyle::Modern => gtk::Align::Center,
                                LauncherStyle::Classic => gtk::Align::End
                            },

                            #[watch]
                            set_width_request: match model.style {
                                LauncherStyle::Modern => -1,
                                LauncherStyle::SteamDeck => 800,
                                LauncherStyle::Classic => 800
                            },

                            #[watch]
                            set_visible: !model.downloading,

                            #[watch]
                            set_margin_bottom: match model.style {
                                LauncherStyle::Modern => 48,
                                LauncherStyle::SteamDeck => 48,
                                LauncherStyle::Classic => 0
                            },

                            set_vexpand: true,

                            gtk::Box {
                                #[watch]
                                set_halign: match model.style {
                                    LauncherStyle::Modern => gtk::Align::Center,
                                    LauncherStyle::SteamDeck => gtk::Align::Center,
                                    LauncherStyle::Classic => gtk::Align::End
                                },

                                #[watch]
                                set_height_request: match model.style {
                                    LauncherStyle::Modern => -1,
                                    LauncherStyle::SteamDeck => -1,
                                    LauncherStyle::Classic => 40
                                },

                                set_margin_top: 64,
                                set_spacing: 8,

                                adw::Bin {
                                    set_css_classes: &["background", "round-bin"],

                                    #[watch]
                                    set_visible: !model.kill_game_button,

                                    gtk::Button {
                                        adw::ButtonContent {
                                            #[watch]
                                            set_icon_name: match &model.state {
                                                Some(LauncherState::Launch) // |
                                                    => "media-playback-start-symbolic",

                                                Some(LauncherState::WineNotInstalled) |
                                                Some(LauncherState::PrefixNotExists) => "document-save-symbolic",

                                                None => "window-close-symbolic"
                                            },

                                            #[watch]
                                            set_label: &match &model.state {
                                                Some(LauncherState::Launch) => tr!("launch"),


                                                Some(LauncherState::WineNotInstalled) => tr!("download-wine"),
                                                Some(LauncherState::PrefixNotExists)  => tr!("create-prefix"),

                                                None => String::from("...")
                                            }
                                        },

                                        #[watch]
                                        set_sensitive: !model.disabled_buttons && match &model.state {
                                            Some(_) => true,
                                            None => false
                                        },

                                        #[watch]
                                        set_css_classes: match &model.state {
                                            Some(_) => &["suggested-action", "pill"],
                                            None => &["pill"]
                                        },

                                        #[watch]
                                        set_tooltip_text: Some(&match &model.state {
                                            _ => String::new()
                                        }),

                                        set_hexpand: false,
                                        set_width_request: 200,

                                        connect_clicked => AppMsg::PerformAction
                                    }
                                },

                                adw::Bin {
                                    set_css_classes: &["background", "round-bin"],

                                    #[watch]
                                    set_visible: model.kill_game_button,

                                    gtk::Button {
                                        adw::ButtonContent {
                                            set_icon_name: "violence-symbolic", // window-close-symbolic
                                            set_label: &tr!("kill-game-process")
                                        },

                                        #[watch]
                                        set_sensitive: !model.disabled_kill_game_button,

                                        set_css_classes: &["error", "pill"],

                                        set_hexpand: false,
                                        set_width_request: 200,

                                        connect_clicked[sender] => move |_| {
                                            sender.input(AppMsg::DisableKillGameButton(true));

                                            std::thread::spawn(clone!(
                                                #[strong] sender,
                                                move || {
                                                    std::thread::sleep(std::time::Duration::from_secs(3));

                                                    sender.input(AppMsg::DisableKillGameButton(false));
                                                }
                                            ));

                                            let result = std::process::Command::new("pkill")
                                                .arg("-f") // full text search
                                                .arg("-i") // case-insensitive
                                                .arg("Client-Win64-Sh")
                                                .spawn();

                                            if let Err(err) = result {
                                                sender.input(AppMsg::Toast {
                                                    title: tr!("kill-game-process-failed"),
                                                    description: Some(err.to_string())
                                                });
                                            }
                                        }
                                    }
                                },

                                adw::Bin {
                                    set_css_classes: &["background", "round-bin"],

                                    gtk::Button {
                                        #[watch]
                                        set_sensitive: !model.disabled_buttons,

                                        set_width_request: 44,

                                        add_css_class: "circular",
                                        set_icon_name: "emblem-system-symbolic",

                                        connect_clicked => AppMsg::OpenPreferences
                                    }
                                }
                            }
                        }
                    }
                }
            },

            connect_close_request[sender] => move |_| {
                if let Err(err) = Config::flush() {
                    sender.input(AppMsg::Toast {
                        title: tr!("config-update-error"),
                        description: Some(err.to_string())
                    });
                }

                gtk::glib::Propagation::Proceed
            }
        }
    }

    fn init(_init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        tracing::info!("Initializing main window");

        let model = App {
            progress_bar: ProgressBar::builder()
                .launch(ProgressBarInit {
                    caption: None,
                    display_progress: true,
                    display_fraction: true,
                    visible: true
                })
                .detach(),

            toast_overlay: adw::ToastOverlay::new(),

            loading: Some(None),
            style: CONFIG.launcher.style,
            state: None,

            downloading: false,
            disabled_buttons: false,
            kill_game_button: false,
            disabled_kill_game_button: false,
            game_is_running: false
        };

        model.progress_bar.widget().set_halign(gtk::Align::Center);
        model.progress_bar.widget().set_width_request(360);

        let toast_overlay = &model.toast_overlay;

        let widgets = view_output!();

        let about_dialog_broker: MessageBroker<AboutDialogMsg> = MessageBroker::new();
        let main_dialog_broker:  MessageBroker<AppMsg> = MessageBroker::new();

        unsafe {
            MAIN_WINDOW = Some(widgets.main_window.clone());

            PREFERENCES_WINDOW = Some(PreferencesApp::builder()
                .launch(widgets.main_window.clone().into())
                .forward(sender.input_sender(), std::convert::identity));

            ABOUT_DIALOG = Some(AboutDialog::builder()
                .transient_for(widgets.main_window.clone())
                .launch_with_broker((), &about_dialog_broker)
                .detach());
        }

        // KDE StatusNotifierItem systray icon
        let systray = std::thread::spawn(clone!(
            #[strong] sender,
            move || {
                let tray = LauncherSystray { sender: sender };
                let spawned = tray.spawn().unwrap();
                loop { std::thread::park() }
            })
        );

        let mut group = RelmActionGroup::<WindowActionGroup>::new();

        // TODO: reduce code somehow

        group.add_action::<LauncherFolder>(RelmAction::new_stateless(clone!(
            #[strong] sender,
            move |_| {
            if let Err(err) = open::that(LAUNCHER_FOLDER.as_path()) {
                sender.input(AppMsg::Toast {
                    title: tr!("launcher-folder-opening-error"),
                    description: Some(err.to_string())
                });

                tracing::error!("Failed to open launcher folder: {err}");
            }
        })));

        group.add_action::<GameFolder>(RelmAction::new_stateless(clone!(
            #[strong] sender,
            move  |_| {
            let path = match Config::get() {
                Ok(config) => config.game.path.for_edition(config.launcher.edition).to_path_buf(),
                Err(_) => CONFIG.game.path.for_edition(CONFIG.launcher.edition).to_path_buf()
            };

            if let Err(err) = open::that(path) {
                sender.input(AppMsg::Toast {
                    title: tr!("game-folder-opening-error"),
                    description: Some(err.to_string())
                });

                tracing::error!("Failed to open game folder: {err}");
            }
        })));

        group.add_action::<ConfigFile>(RelmAction::new_stateless(clone!(
            #[strong] sender,
            move |_| {
            if let Ok(file) = config_file() {
                if let Err(err) = open::that(file) {
                    sender.input(AppMsg::Toast {
                        title: tr!("config-file-opening-error"),
                        description: Some(err.to_string())
                    });

                    tracing::error!("Failed to open config file: {err}");
                }
            }
        })));

        group.add_action::<DebugFile>(RelmAction::new_stateless(clone!(
            #[strong] sender,
            move |_| {
                if let Err(err) = open::that(crate::DEBUG_FILE.as_os_str()) {
                    sender.input(AppMsg::Toast {
                        title: tr!("debug-file-opening-error"),
                        description: Some(err.to_string())
                    });

                    tracing::error!("Failed to open debug file: {err}");
                }
            }
        )));

        group.add_action::<About>(RelmAction::new_stateless(move |_| {
            about_dialog_broker.send(AboutDialogMsg::Show);
        }));

        widgets.main_window.insert_action_group("win", Some(&group.into_action_group()));

        tracing::info!("Main window initialized");

        // Initialize some heavy tasks
        std::thread::spawn(move || {
            tracing::info!("Initializing heavy tasks");
            // Update launcher state
            // this launchers the reaction updating the pages
            sender.input(AppMsg::UpdateLauncherState {
                perform_on_download_needed: false,
                show_status_page: true
            });

            // Mark app as loaded
            crate::READY.store(true, Ordering::Relaxed);

            tracing::info!("App is ready");
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        tracing::debug!("Called main window event: {:?}", msg);

        match msg {
            // TODO: make function from this message like with toast
            AppMsg::UpdateLauncherState { perform_on_download_needed, show_status_page } => {
                if show_status_page {
                    sender.input(AppMsg::SetLoadingStatus(Some(Some(tr!("loading-launcher-state")))));
                } else {
                    self.disabled_buttons = true;
                }

                let updater = clone!(
                    #[strong] sender,
                    move |state| {
                        if show_status_page {
                            match state {
                                StateUpdating::Components => {
                                    // tr!("loading-launcher-state--components")
                                    sender.input(AppMsg::SetLoadingStatus(Some(Some(String::from("Components")))));
                                }

                                StateUpdating::Game => {
                                    sender.input(AppMsg::SetLoadingStatus(Some(Some(tr!("loading-launcher-state--game")))));
                                }
                            }
                        }
                    }
                );

                let state = match LauncherState::get_from_config(updater) {
                    Ok(state) => Some(state),
                    Err(err) => {
                        tracing::error!("Failed to update launcher state: {err}");

                        self.toast(tr!("launcher-state-updating-error"), Some(err.to_string()));
    
                        None
                    }
                };

                sender.input(AppMsg::SetLauncherState(state.clone()));

                if show_status_page {
                    sender.input(AppMsg::SetLoadingStatus(None));
                } else {
                    self.disabled_buttons = false;
                }

                if(steam::is_steam_deck()) {
                    sender.input(AppMsg::SetLauncherStyle(LauncherStyle::SteamDeck));
                }

                if let Some(state) = state {
                    match state {

                        _ => ()
                    }
                }
            }

            AppMsg::SetLauncherState(state) => {
                self.state = state;
            }

            AppMsg::SetLoadingStatus(status) => {
                self.loading = status;
            }

            AppMsg::SetLauncherStyle(style) => {
                self.style = style;
            }

            AppMsg::SetDownloading(state) => {
                self.downloading = state;
            }

            AppMsg::SetGameIsRunningState(state) => {
                self.game_is_running = state;
            }

            AppMsg::DisableButtons(state) => {
                self.disabled_buttons = state;
            }

            AppMsg::SetKillGameButton(state) => {
                self.kill_game_button = state;
            }

            AppMsg::DisableKillGameButton(state) => {
                self.disabled_kill_game_button = state;
            }

            AppMsg::OpenPreferences => unsafe {
                PREFERENCES_WINDOW.as_ref().unwrap_unchecked().widget().present();
            }

            AppMsg::RepairGame => repair_game::repair_game(sender, self.progress_bar.sender().to_owned()),

            AppMsg::PerformAction => unsafe {
                match self.state.as_ref().unwrap_unchecked() {
                    LauncherState::Launch => {
                        launch::launch(sender);
                    },

                    LauncherState::WineNotInstalled => download_wine::download_wine(sender, self.progress_bar.sender().to_owned()),
                    LauncherState::PrefixNotExists => create_prefix::create_prefix(sender),
                }
            }

            AppMsg::HideWindow => unsafe {
                MAIN_WINDOW.as_ref().unwrap_unchecked().set_visible(false);
            }

            AppMsg::ShowWindow => unsafe {
                MAIN_WINDOW.as_ref().unwrap_unchecked().present();
            }

            AppMsg::ToggleWindow => unsafe {
                match MAIN_WINDOW.as_ref().unwrap_unchecked().is_visible() {
                    true => MAIN_WINDOW.as_ref().unwrap_unchecked().set_visible(false),
                    false => MAIN_WINDOW.as_ref().unwrap_unchecked().present()
                }
            }

            AppMsg::Toast { title, description } => self.toast(title, description)
        }
    }
}

impl App {
    pub fn toast<T: AsRef<str>>(&mut self, title: T, description: Option<T>) {
        let toast = adw::Toast::new(title.as_ref());

        toast.set_timeout(4);

        if let Some(description) = description {
            toast.set_button_label(Some(&tr!("details")));

            let dialog = adw::MessageDialog::new(
                Some(unsafe { MAIN_WINDOW.as_ref().unwrap_unchecked() }),
                Some(title.as_ref()),
                Some(description.as_ref())
            );

            dialog.add_response("close", &tr!("close", { "form" = "noun" }));
            dialog.add_response("save", &tr!("save"));

            dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

            dialog.connect_response(Some("save"), |_, _| {
                if let Err(err) = open::that(crate::DEBUG_FILE.as_os_str()) {
                    tracing::error!("Failed to open debug file: {err}");
                }
            });

            toast.connect_button_clicked(move |_| {
                dialog.present();
            });
        }

        self.toast_overlay.add_toast(toast);
    }
}
