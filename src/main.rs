use iced::{
    Element, Length, Task, Theme,
    widget::{
        Container, Scrollable, button, checkbox, column, container, row, scrollable, text,
        text_input,
    },
};
use std::collections::HashSet;
use std::fmt::Display;
use std::process::Command;

pub fn main() -> iced::Result {
    iced::application(AppState::new, AppState::update, AppState::view)
        .theme(Theme::Dark)
        .title("Ubuntu Package Manager")
        .run()
}

struct AppState {
    flatpak_packages: Vec<Package>,
    apt_packages: Vec<Package>,
    snap_packages: Vec<Package>,
    current_page: Page,
    name_search: String,
    source_search: String,
    include_system: bool,
}

#[derive(Debug, Clone)]
struct Package {
    source: Source,
    name: String,
    version: String,
    is_system: bool,
}

#[derive(Debug, Clone)]
enum Source {
    Flatpak,
    Apt,
    Snap,
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Flatpak => write!(f, "Flatpak"),
            Source::Apt => write!(f, "APT"),
            Source::Snap => write!(f, "Snap"),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    AppsLoaded(Result<PackageLists, String>),
    Navigate(Page),
    NameSearchChange(String),
    SourceSearchChange(String),
    IncludeSystemChange(bool),
}

#[derive(Debug, Clone)]
struct PackageLists {
    flatpak_packages: Vec<Package>,
    apt_packages: Vec<Package>,
    snap_packages: Vec<Package>,
}

#[derive(Debug, Clone)]
enum Page {
    Apt,
    Flatpak,
    Snap,
    All,
}

impl AppState {
    fn new() -> (Self, Task<Message>) {
        let state = Self {
            flatpak_packages: Vec::new(),
            apt_packages: Vec::new(),
            snap_packages: Vec::new(),
            current_page: Page::Apt,
            name_search: String::new(),
            source_search: String::new(),
            include_system: false,
        };

        let task = Task::perform(load_app_lists(), Message::AppsLoaded);

        (state, task)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AppsLoaded(result) => match result {
                Ok(lists) => {
                    self.flatpak_packages = lists.flatpak_packages;
                    self.apt_packages = lists.apt_packages;
                    self.snap_packages = lists.snap_packages;
                }
                Err(e) => {
                    eprintln!("Error loading apps: {}", e);
                }
            },
            Message::Navigate(page) => {
                self.current_page = page;
                self.name_search = String::new();
            }
            Message::NameSearchChange(term) => self.name_search = term,
            Message::SourceSearchChange(term) => self.source_search = term,
            Message::IncludeSystemChange(include_system) => self.include_system = include_system,
        }
        Task::none()
    }
}

async fn load_app_lists() -> Result<PackageLists, String> {
    let mut errors = Vec::new();
    let mut flatpak_apps = Vec::new();
    let mut apt_apps = Vec::new();
    let mut snap_apps = Vec::new();

    match load_apt() {
        Ok(apps) => {
            apt_apps = apps;
        }
        Err(error) => {
            errors.push(format!("APT error: {}", error));
        }
    }

    match load_flatpak() {
        Ok(apps) => {
            flatpak_apps = apps;
        }
        Err(error) => {
            errors.push(format!("Flatpak error: {}", error));
        }
    }

    match load_snap() {
        Ok(apps) => {
            snap_apps = apps;
        }
        Err(error) => {
            errors.push(format!("Snap error: {}", error));
        }
    }

    if errors.is_empty() {
        Ok(PackageLists {
            flatpak_packages: flatpak_apps,
            apt_packages: apt_apps,
            snap_packages: snap_apps,
        })
    } else {
        Err(errors.join("\n"))
    }
}

fn run_cmd(bin: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run `{bin}`: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!(
            "`{}` exited with {}{}",
            bin,
            out.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn load_manual_set() -> Result<HashSet<String>, String> {
    let out = run_cmd("apt-mark", &["showmanual"])?;

    Ok(out
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

pub fn load_apt() -> Result<Vec<Package>, String> {
    let manual = load_manual_set()?;

    let stdout = run_cmd("dpkg-query", &["-W", "-f=${Package}\t${Version}\n"])?;

    let mut pkgs = Vec::new();

    for line in stdout.lines() {
        let mut parts = line.split('\t');

        let name = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();

        if name.is_empty() {
            continue;
        }

        let is_manual = manual.contains(name);
        let is_lib = name.starts_with("lib");
        let is_meta = name.starts_with("linux-")
            || name.starts_with("language-pack-")
            || name.ends_with("-data")
            || name.ends_with("-common");

        pkgs.push(Package {
            source: Source::Apt,
            name: name.to_string(),
            version: version.to_string(),
            is_system: !(is_manual && !is_lib && !is_meta),
        });
    }

    Ok(pkgs)
}

pub fn load_flatpak() -> Result<Vec<Package>, String> {
    let stdout = run_cmd(
        "flatpak",
        &[
            "list",
            "--app",
            "--columns=application,version,branch,origin",
        ],
    )?;

    let mut pkgs = Vec::new();

    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.is_empty() {
            continue;
        }

        let name = cols.get(0).unwrap_or(&"").trim();
        let version = cols.get(1).unwrap_or(&"").trim();

        if name.is_empty() {
            continue;
        }

        pkgs.push(Package {
            source: Source::Flatpak,
            name: name.to_string(),
            version: version.to_string(),
            is_system: false,
        });
    }

    Ok(pkgs)
}

pub fn load_snap() -> Result<Vec<Package>, String> {
    let stdout = run_cmd("snap", &["list"])?;

    let mut pkgs = Vec::new();

    for (i, line) in stdout.lines().enumerate() {
        if i == 0 {
            continue;
        }

        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }

        let name = cols[0];
        let version = cols[1];
        let notes = cols.last().unwrap_or(&"");

        pkgs.push(Package {
            source: Source::Snap,
            name: name.to_string(),
            version: version.to_string(),
            is_system: is_snap_runtime(name, notes),
        });
    }

    Ok(pkgs)
}

fn is_snap_runtime(name: &str, notes: &str) -> bool {
    if notes.contains("base") || notes.contains("kernel") || notes.contains("gadget") {
        return true;
    }

    name.starts_with("core")
        || name.starts_with("gnome-")
        || name.starts_with("gtk-")
        || name.starts_with("mesa-")
}

impl AppState {
    fn view(&self) -> Element<'_, Message> {
        container(row![get_menu(), column![get_page(&self)]]).into()
    }
}

fn get_menu() -> Container<'static, Message> {
    let apt_btn = button("Apt Packages").on_press(Message::Navigate(Page::Apt));
    let flatpack_btn = button("Flatpack Packages").on_press(Message::Navigate(Page::Flatpak));
    let snap_btn = button("Snap Packages").on_press(Message::Navigate(Page::Snap));
    let all_btn = button("All Packages").on_press(Message::Navigate(Page::All));

    container(column![apt_btn, flatpack_btn, snap_btn, all_btn].spacing(10)).into()
}

fn get_page(app_state: &AppState) -> Element<'_, Message> {
    let filtered: Vec<&Package> = match &app_state.current_page {
        Page::Apt => app_state
            .apt_packages
            .iter()
            .filter(|pkg| {
                filter_package(
                    pkg,
                    &app_state.name_search,
                    &app_state.source_search,
                    app_state.include_system,
                )
            })
            .collect(),
        Page::Flatpak => app_state
            .flatpak_packages
            .iter()
            .filter(|pkg| {
                filter_package(
                    pkg,
                    &app_state.name_search,
                    &app_state.source_search,
                    app_state.include_system,
                )
            })
            .collect(),
        Page::Snap => app_state
            .snap_packages
            .iter()
            .filter(|pkg| {
                filter_package(
                    pkg,
                    &app_state.name_search,
                    &app_state.source_search,
                    app_state.include_system,
                )
            })
            .collect(),
        Page::All => app_state
            .apt_packages
            .iter()
            .chain(app_state.flatpak_packages.iter())
            .chain(app_state.snap_packages.iter())
            .filter(|pkg| {
                filter_package(
                    pkg,
                    &app_state.name_search,
                    &app_state.source_search,
                    app_state.include_system,
                )
            })
            .collect(),
    };

    get_package_scrollable(app_state, filtered)
}

fn filter_package(pkg: &Package, name: &str, source: &str, include_system: bool) -> bool {
    let mut show = true;
    if name.is_empty() {
        show = show && true;
    } else {
        show = show && pkg.name.to_lowercase().contains(&name.to_lowercase());
    }

    if source.is_empty() {
        show = show && true;
    } else {
        show = show
            && pkg
                .source
                .to_string()
                .to_lowercase()
                .contains(&source.to_lowercase());
    }

    show && (pkg.is_system == false || (pkg.is_system == true && include_system))
}

fn get_package_scrollable<'a>(
    app_state: &'a AppState,
    package_list: Vec<&'a Package>,
) -> Element<'a, Message> {
    let header_row = row![
        text("Source").width(Length::FillPortion(2)),
        text("Name").width(Length::FillPortion(4)),
        text("Version").width(Length::FillPortion(4)),
        text("Include System").width(Length::FillPortion(2))
    ];
    let filter_row = row![
        text_input("Source", &app_state.source_search)
            .on_input(Message::SourceSearchChange)
            .width(Length::FillPortion(2)),
        text_input("Name", &app_state.name_search)
            .on_input(Message::NameSearchChange)
            .width(Length::FillPortion(4)),
        text("Version").width(Length::FillPortion(4)),
        checkbox(app_state.include_system)
            .on_toggle(Message::IncludeSystemChange)
            .width(Length::FillPortion(2))
    ];
    container(
        scrollable(package_list.iter().enumerate().fold(
            column![header_row, filter_row].spacing(2),
            |col, (_, package)| {
                col.push(
                    row![
                        text(format!("{:?}", package.source)).width(Length::FillPortion(1)),
                        text(&package.name).width(Length::FillPortion(2)),
                        text(&package.version).width(Length::FillPortion(2)),
                        checkbox(package.is_system).width(Length::FillPortion(2)),
                    ]
                    .spacing(10)
                    .padding(5),
                )
            },
        ))
        .height(Length::Fill),
    )
    .into()
}
