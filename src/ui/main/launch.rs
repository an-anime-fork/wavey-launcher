use relm4::prelude::*;
use gtk::prelude::*;

use glob;

use anime_launcher_sdk::wincompatlib::prelude::*;

use anime_launcher_sdk::config::ConfigExt;
use anime_launcher_sdk::wuwa::config::Config;
use anime_launcher_sdk::wuwa::config::schema::prelude::LauncherBehavior;

use crate::*;

use super::{App, AppMsg};

// TODO: decide if i watch the dir for creation of the file, the file for update, or both
//       sometimes the debug.log is actually wiped off and re-created because there's no
//       trace of previous pull histories.
fn get_watch_target() -> Option<PathBuf> {
    //let target_watch = "debug.log";
    let target_watch = "KRSDKWebView";
    if steam::launched_from_steam() && steam::is_install_managed_by_steam() {
        match std::env::var("STEAM_COMPAT_APP_ID") {
            Ok(app_id) => {
                if app_id != "0" {
                    // All right, we KNOW we're the Steam-handled one.
                    // Fucking run it.
                    match std::env::var("STEAM_COMPAT_INSTALL_PATH") {
                        Ok(game) => {
                            for one in glob::glob(&format!("{}/**/debug.log", game))
                                .expect("Failed to read glob pattern")
                            {
                                match one {
                                    Ok(path) => return Some(path),
                                    Err(_) => continue
                                }
                            }
                        },
                        Err(_) => {}
                    }
                }
            }
            Err(_) => { /*noop*/ }
        }
        // STEAM_COMPAT_APP_ID=0
    }
    None
}

pub fn launch(sender: ComponentSender<App>) {
    let config = Config::get().unwrap();

    match config.launcher.behavior {
        // Disable launch button and show kill game button if behavior set to "Nothing" to prevent sussy actions
        LauncherBehavior::Nothing => {
            sender.input(AppMsg::DisableButtons(true));
            sender.input(AppMsg::SetKillGameButton(true));
        }

        // Hide launcher window if behavior set to "Hide" or "Close"
        LauncherBehavior::Hide | LauncherBehavior::Close => sender.input(AppMsg::HideWindow)
    }

    std::thread::spawn(move || {
        let debug_file = get_debug_log_file();

        // find debug.log file, which is the url log source for now
        // todo: implement fs traversal from here, as mod
        // then use notify to watch every debug.log file being deleted
        // todo: notify as file watcher
        // todo: handle file deletion, don't nuke the launcher
        // I honestly don't care anymore
        let wine = config.get_selected_wine().unwrap().unwrap();

        if let Err(err) = anime_launcher_sdk::wuwa::game::run() {
            tracing::error!("Failed to launch game: {err}");

            sender.input(AppMsg::Toast {
                title: tr!("game-launching-failed"),
                description: Some(err.to_string())
            });
        }

        match config.launcher.behavior {
            // Enable launch button and hide kill game button if behavior set to "Nothing" after the game has closed
            LauncherBehavior::Nothing => {
                sender.input(AppMsg::DisableButtons(false));
                sender.input(AppMsg::SetKillGameButton(false));
            }

            // Show back launcher window if behavior set to "Hide" and the game has closed
            LauncherBehavior::Hide => sender.input(AppMsg::ShowWindow),
            
            // Show Window again but automatically start a 10-second (customisable) timer to
            // auto-close the launcher after the game closes
            //LauncherBehavior::CloseUnlessAction => {
            //    sender.input(AppMsg::ShowWindowWithAutoClose),
            //}

            // Otherwise close the launcher if behavior set to "Close" and the game has closed
            // We're calling quit method from the main context here because otherwise app won't be closed properly
            LauncherBehavior::Close => gtk::glib::MainContext::default().invoke(|| {
                relm4::main_application().quit();
            })
        }
    });
}
