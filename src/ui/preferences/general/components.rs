use relm4::prelude::*;
use gtk::prelude::*;
use adw::prelude::*;
use gtk::builders::PopoverBuilder;
use anime_launcher_sdk::wincompatlib::prelude::*;

use anime_launcher_sdk::components::*;
use anime_launcher_sdk::components::wine::UnifiedWine;

use super::GeneralAppMsg;

use crate::ui::components::*;
use crate::*;

pub struct ComponentsPage {
    wine_components: AsyncController<ComponentsList<ComponentsPageMsg>>,

    downloaded_wine_versions: Vec<(wine::Version, wine::Features)>,
    available_steamrt_versions: Vec<(wine::Version, wine::Features)>,

    selected_wine_version: u32,

    selecting_wine_version: bool
}

#[derive(Debug, Clone)]
pub enum ComponentsPageMsg {
    WineRecommendedOnly(bool),

    UpdateDownloadedWine,

    SelectWine(usize),

    ResetWineSelection(usize)
}

#[relm4::component(async, pub)]
impl SimpleAsyncComponent for ComponentsPage {
    type Init = ();
    type Input = ComponentsPageMsg;
    type Output = GeneralAppMsg;

    view! {
        adw::NavigationPage {
            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                adw::HeaderBar {
                    #[wrap(Some)]
                    set_title_widget = &adw::WindowTitle {
                        set_title: &tr!("components")
                    }
                },

                adw::PreferencesPage {
                    add = &adw::PreferencesGroup {
                        set_title: &tr!("wine-version"),

                        adw::ComboRow {
                            set_title: &tr!("selected-version"),

                            #[watch]
                            #[block_signal(wine_selected_notify)]
                            set_model: Some(&gtk::StringList::new(&model.downloaded_wine_versions.iter().map(|(version, _)| version.title.as_str()).collect::<Vec<&str>>())),

                            #[watch]
                            #[block_signal(wine_selected_notify)]
                            set_selected: model.selected_wine_version,

                            #[watch]
                            set_activatable: !model.selecting_wine_version,

                            connect_selected_notify[sender] => move |row| {
                                if is_ready() {
                                    sender.input(ComponentsPageMsg::SelectWine(row.selected() as usize));
                                }
                            } @wine_selected_notify,

                            add_suffix = &gtk::Spinner {
                                set_spinning: true,

                                #[watch]
                                set_visible: model.selecting_wine_version
                            },
                            add_suffix = &gtk::Popover {
                                set_position: gtk::PositionType::Right,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 5,

                                    gtk::Label {
                                        set_text: "Updating Wine Prefix",
                                        set_margin_start: 5,
                                        set_margin_end: 5,
                                    },
                                },

                                #[watch]
                                set_visible: model.selecting_wine_version
                            },
                        },

                        adw::ActionRow {
                            set_title: &tr!("recommended-only"),
                            set_subtitle: &tr!("wine-recommended-description"),

                            add_suffix = &gtk::Switch {
                                set_valign: gtk::Align::Center,

                                #[block_signal(wine_recommended_notify)]
                                set_active: true,

                                connect_state_notify[sender] => move |switch| {
                                    if is_ready() {
                                        sender.input(ComponentsPageMsg::WineRecommendedOnly(switch.is_active()));
                                    }
                                } @wine_recommended_notify
                            }
                        }
                    },
                    add = &adw::PreferencesGroup {
                        set_title: &tr!("steamrt-version"),

                        adw::ComboRow {
                            set_title: &tr!("selected-steamrt-version"),

                            #[watch]
                            #[block_signal(wine_selected_notify)]
                            set_model: Some(&gtk::StringList::new(&model.downloaded_wine_versions.iter().map(|(version, _)| version.title.as_str()).collect::<Vec<&str>>())),

                            #[watch]
                            #[block_signal(wine_selected_notify)]
                            set_selected: model.selected_wine_version,

                            #[watch]
                            set_activatable: !model.selecting_wine_version,

                            connect_selected_notify[sender] => move |row| {
                                if is_ready() {
                                    sender.input(ComponentsPageMsg::SelectWine(row.selected() as usize));
                                }
                            } @wine_selected_notify,

                            add_suffix = &gtk::Spinner {
                                set_spinning: true,

                                #[watch]
                                set_visible: model.selecting_wine_version
                            },
                            add_suffix = &gtk::Popover {
                                set_position: gtk::PositionType::Right,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 5,

                                    gtk::Label {
                                        set_text: "Updating Wine Prefix",
                                        set_margin_start: 5,
                                        set_margin_end: 5,
                                    },
                                },

                                #[watch]
                                set_visible: model.selecting_wine_version
                            },
                        },
                    },
                }
            }
        }
    }

    async fn init(_init: Self::Init, root: Self::Root, sender: AsyncComponentSender<Self>) -> AsyncComponentParts<Self> {
        tracing::info!("Initializing general settings -> components page");

        let model = Self {
            wine_components: ComponentsList::builder()
                .launch(ComponentsListInit {
                    pattern: ComponentsListPattern {
                        download_folder: CONFIG.game.wine.builds.clone(),
                        groups: wine::get_groups(&CONFIG.components.path).unwrap_or_default()
                            .into_iter()
                            .map(|mut group| {
                                group.versions = group.versions.into_iter().take(12).collect();

                                let mut group: ComponentsListGroup = group.into();
                                let mut recommended = 6;

                                for i in 0..group.versions.len() {
                                    if recommended > 0 && group.versions[i].recommended {
                                        recommended -= 1;
                                    }

                                    else {
                                        group.versions[i].recommended = false;
                                    }
                                }

                                group
                            })
                            .collect()
                    },
                    on_downloaded: Some(ComponentsPageMsg::UpdateDownloadedWine),
                    on_deleted: Some(ComponentsPageMsg::UpdateDownloadedWine)
                })
                .forward(sender.input_sender(), std::convert::identity),

            downloaded_wine_versions: vec![],

            selected_wine_version: 0,

            selecting_wine_version: false
        };

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncComponentSender<Self>) {
        tracing::debug!("Called general settings event: {:?}", msg);

        match msg {
            ComponentsPageMsg::WineRecommendedOnly(state) => {
                // todo
                self.wine_components.sender().send(ComponentsListMsg::ShowRecommendedOnly(state)).unwrap();
            }

            ComponentsPageMsg::UpdateDownloadedWine => {
                self.downloaded_wine_versions = wine::get_downloaded(&CONFIG.components.path, &CONFIG.game.wine.builds)
                    .unwrap_or_default()
                    .into_iter()
                    .flat_map(|group| group.versions.clone().into_iter()
                        .map(move |version| {
                            let features = version.features_in(&group).unwrap_or_default();

                            (version, features)
                        })
                    ).collect();

                self.selected_wine_version = if let Some(selected) = &CONFIG.game.wine.selected {
                    let mut index = 0;

                    for (i, (version, _)) in self.downloaded_wine_versions.iter().enumerate() {
                        if &version.name == selected {
                            index = i;

                            break;
                        }
                    }

                    index as u32
                }

                else {
                    0
                };
            }

            ComponentsPageMsg::SelectWine(index) => {
                if let Ok(mut config) = Config::get() {
                    if let Some((version, features)) = self.downloaded_wine_versions.get(index) {
                        if config.game.wine.selected.as_ref() != Some(&version.title) {
                            self.selecting_wine_version = true;

                            let wine = version
                                .to_wine(&config.components.path, Some(&config.game.wine.builds.join(&version.name)))
                                .with_prefix(&config.game.wine.prefix)
                                .with_loader(WineLoader::Current)
                                .with_arch(WineArch::Win64);

                            let wine_name = version.name.to_string();

                            std::thread::spawn(move || {
                                match wine.update_prefix(None::<&str>) {
                                    Ok(_) => {
                                        config.game.wine.selected = Some(wine_name); 

                                        Config::update(config);
                                        // Launch signal back to mainwindow to refresh launch button
                                    }

                                    Err(err) => {
                                        sender.output(GeneralAppMsg::Toast {
                                            title: tr!("wine-prefix-update-failed"),
                                            description: Some(err.to_string())
                                        }).unwrap();
                                    }
                                }

                                sender.input(ComponentsPageMsg::ResetWineSelection(index));
                            });
                        }
                    }
                }
            }

            ComponentsPageMsg::ResetWineSelection(index) => {
                self.selecting_wine_version = false;
                self.selected_wine_version = index as u32;
            }

        }
    }
}
