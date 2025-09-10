use std::path::PathBuf;

use relm4::prelude::*;
use adw::prelude::*;

use anime_launcher_sdk::anime_game_core::prelude::*;
use anime_launcher_sdk::wincompatlib::prelude::*;

use anime_launcher_sdk::components::*;
use anime_launcher_sdk::components::wine::UnifiedWine;

use anime_launcher_sdk::config::ConfigExt;
use anime_launcher_sdk::wuwa::config::Config;

use super::main::FirstRunAppMsg;

use crate::ui::components::*;
use crate::*;

fn get_installer(uri: &str, temp: Option<PathBuf>) -> anyhow::Result<Installer> {
    Ok(Installer::new(uri)?.with_temp_folder(temp.unwrap_or_else(std::env::temp_dir)))
}

pub struct DownloadComponentsApp {
    progress_bar: AsyncController<ProgressBar>,

    wine_combo: adw::ComboRow,

    wine_versions: Vec<wine::Version>,

    selected_wine: Option<wine::Version>,

    /// `None` - default,
    /// `Some(false)` - processing,
    /// `Some(true)` - done
    downloading_wine: Option<bool>,
    downloading_wine_version: String,

    /// `None` - default,
    /// `Some(false)` - processing,
    /// `Some(true)` - done
    creating_prefix: Option<bool>,
    creating_prefix_path: String,


    downloading: bool
}

#[derive(Debug, Clone)]
pub enum DownloadComponentsAppMsg {
    UpdateVersionsLists,
    DownloadWine,
    CreatePrefix,
    Continue,
    Exit
}

#[relm4::component(async, pub)]
impl SimpleAsyncComponent for DownloadComponentsApp {
    type Init = ();
    type Input = DownloadComponentsAppMsg;
    type Output = FirstRunAppMsg;

    view! {
        adw::PreferencesPage {
            set_hexpand: true,

            add = &adw::PreferencesGroup {
                set_valign: gtk::Align::Center,
                set_vexpand: true,

                gtk::Label {
                    set_label: &tr!("download-components"),
                    add_css_class: "title-1"
                }
            },

            add = &adw::PreferencesGroup {
                set_valign: gtk::Align::Center,
                set_vexpand: true,

                #[watch]
                set_visible: !model.downloading,

                #[local_ref]
                wine_combo -> adw::ComboRow {
                    set_title: &tr!("wine-version"),

                    #[watch]
                    set_model: Some(&gtk::StringList::new(model.wine_versions.iter()
                        .map(|version| version.title.as_ref())
                        .collect::<Vec<&str>>()
                        .as_slice()))
                },

            },

            add = &adw::PreferencesGroup {
                set_valign: gtk::Align::Center,
                set_vexpand: true,

                #[watch]
                set_visible: !model.downloading,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Center,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: &tr!("download"),
                        set_css_classes: &["suggested-action", "pill"],

                        connect_clicked => DownloadComponentsAppMsg::DownloadWine
                    },

                    gtk::Button {
                        set_label: &tr!("exit"),
                        add_css_class: "pill",

                        connect_clicked => DownloadComponentsAppMsg::Exit
                    }
                }
            },

            add = &adw::PreferencesGroup {
                set_valign: gtk::Align::Center,
                set_vexpand: true,

                #[watch]
                set_visible: model.downloading,

                adw::ActionRow {
                    set_title: &tr!("download-wine"),

                    #[watch]
                    set_subtitle: &model.downloading_wine_version,

                    add_prefix = &gtk::Image {
                        #[watch]
                        set_icon_name: match model.downloading_wine {
                            Some(true) => Some("emblem-ok-symbolic"),
                            Some(false) => None, // Some("process-working"),
                            None => None
                        }
                    },

                    add_prefix = &gtk::Spinner {
                        set_spinning: true,

                        #[watch]
                        set_visible: model.downloading_wine == Some(false),
                    }
                },

                adw::ActionRow {
                    set_title: &tr!("create-prefix"),

                    #[watch]
                    set_subtitle: &model.creating_prefix_path,

                    add_prefix = &gtk::Image {
                        #[watch]
                        set_icon_name: match model.creating_prefix {
                            Some(true) => Some("emblem-ok-symbolic"),
                            Some(false) => None, // Some("process-working"),
                            None => None
                        }
                    },

                    add_prefix = &gtk::Spinner {
                        set_spinning: true,

                        #[watch]
                        set_visible: model.creating_prefix == Some(false),
                    }
                },
            },

            add = &adw::PreferencesGroup {
                set_valign: gtk::Align::Start,
                set_vexpand: true,

                #[watch]
                set_visible: model.downloading,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Center,
                    set_spacing: 20,
                    set_margin_top: 64,

                    append = model.progress_bar.widget(),
                }
            }
        }
    }

    async fn init(_init: Self::Init, root: Self::Root, sender: AsyncComponentSender<Self>) -> AsyncComponentParts<Self> {
        let model = Self {
            progress_bar: ProgressBar::builder()
                .launch(ProgressBarInit {
                    caption: None,
                    display_progress: true,
                    display_fraction: true,
                    visible: true
                })
                .detach(),

            wine_combo: adw::ComboRow::new(),

            wine_versions: vec![],

            selected_wine: None,

            downloading_wine: None,
            downloading_wine_version: String::new(),

            creating_prefix: None,
            creating_prefix_path: String::new(),


            downloading: false
        };

        model.progress_bar.widget().set_width_request(360);

        let wine_combo = &model.wine_combo;

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncComponentSender<Self>) {
        match msg {
            DownloadComponentsAppMsg::UpdateVersionsLists => {
                let config = Config::get().unwrap_or_else(|_| CONFIG.clone());

                // 4 latest versions of 4 first available wine group
                self.wine_versions = wine::get_groups(&config.components.path).unwrap()
                    .into_iter()
                    .take(4)
                    .flat_map(|group| group.versions.into_iter().take(4))
                    .collect();

            }

            #[allow(unused_must_use)]
            DownloadComponentsAppMsg::DownloadWine => {
                let config = Config::get().unwrap_or_else(|_| CONFIG.clone());

                self.selected_wine = Some(self.wine_versions[self.wine_combo.selected() as usize].clone());

                self.downloading_wine_version = self.selected_wine.clone().unwrap().title;
                self.creating_prefix_path     = config.game.wine.prefix.to_string_lossy().to_string();

                self.downloading = true;
                self.downloading_wine = Some(false);

                let wine = self.selected_wine.clone().unwrap();
                let progress_bar_input = self.progress_bar.sender().clone();

                // Skip wine downloading if it was already done
                if wine.is_downloaded_in(&config.game.wine.builds) {
                    tracing::info!("Wine already installed: {}", wine.name);

                    let mut config = Config::get().unwrap_or_else(|_| CONFIG.clone());

                    config.game.wine.selected = Some(wine.name);

                    if let Err(err) = Config::update_raw(config) {
                        tracing::error!("Failed to update config: {err}");

                        sender.output(Self::Output::Toast {
                            title: tr!("config-update-error"),
                            description: Some(err.to_string())
                        });
                    }

                    sender.input(DownloadComponentsAppMsg::CreatePrefix);
                }

                // Otherwise download wine
                else {
                    std::thread::spawn(move || {
                        tracing::info!("Installing wine: {}", wine.name);

                        // Install wine
                        match get_installer(&wine.uri, config.launcher.temp.clone()) {
                            Ok(mut installer) => {
                                // Create wine builds folder
                                if config.game.wine.builds.exists() {
                                    std::fs::create_dir_all(&config.game.wine.builds)
                                        .expect("Failed to create wine builds directory");
                                }

                                installer.install(&config.game.wine.builds, move |update| {
                                    match &update {
                                        InstallerUpdate::DownloadingError(err) => {
                                            tracing::error!("Failed to download wine: {err}");

                                            sender.output(Self::Output::Toast {
                                                title: tr!("wine-download-error"),
                                                description: Some(err.to_string())
                                            });
                                        }

                                        InstallerUpdate::UnpackingError(err) => {
                                            tracing::error!("Failed to unpack wine: {err}");

                                            sender.output(Self::Output::Toast {
                                                title: tr!("wine-unpack-errror"),
                                                description: Some(err.clone())
                                            });
                                        }

                                        // Create prefix
                                        InstallerUpdate::UnpackingFinished => {
                                            let mut config = Config::get().unwrap_or_else(|_| CONFIG.clone());

                                            config.game.wine.selected = Some(wine.name.clone());

                                            if let Err(err) = Config::update_raw(config) {
                                                tracing::error!("Failed to update config: {err}");

                                                sender.output(Self::Output::Toast {
                                                    title: tr!("config-update-error"),
                                                    description: Some(err.to_string())
                                                });
                                            }

                                            sender.input(DownloadComponentsAppMsg::CreatePrefix);
                                        },

                                        _ => ()
                                    }

                                    progress_bar_input.send(ProgressBarMsg::UpdateFromState(update));
                                });
                            }

                            Err(err) => {
                                tracing::error!("Failed to initialize wine installer: {err}");

                                sender.output(Self::Output::Toast {
                                    title: tr!("wine-install-failed"),
                                    description: Some(err.to_string())
                                });
                            }
                        }
                    });
                }
            }

            // TODO: perhaps I could re-use main/create_prefix.rs here?

            #[allow(unused_must_use)]
            DownloadComponentsAppMsg::CreatePrefix => {
                self.downloading_wine = Some(true);
                self.creating_prefix = Some(false);

                let config = Config::get().unwrap_or_else(|_| CONFIG.clone());

                tracing::info!("Creating wine prefix");

                let wine = self.selected_wine.as_ref().unwrap();

                let wine = wine
                    .to_wine(config.components.path, Some(config.game.wine.builds.join(&wine.name)))
                    .with_prefix(&config.game.wine.prefix)
                    .with_loader(WineLoader::Current)
                    .with_arch(WineArch::Win64);

                std::thread::spawn(move || {
                    match wine.init_prefix(None::<&str>) {
                        // Aight we good
                        Ok(_) => sender.input(DownloadComponentsAppMsg::Continue),

                        Err(err) => {
                            tracing::error!("Failed to create prefix: {err}");

                            sender.output(Self::Output::Toast {
                                title: tr!("wine-prefix-update-failed"),
                                description: Some(err.to_string())
                            });
                        }
                    }
                });
            }

            #[allow(unused_must_use)]
            DownloadComponentsAppMsg::Continue => {
                std::fs::remove_file(FIRST_RUN_FILE.as_path());

                sender.output(Self::Output::ScrollToFinish);
            }

            DownloadComponentsAppMsg::Exit => relm4::main_application().quit()
        }
    }
}
