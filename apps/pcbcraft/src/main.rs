mod distributor_http;
mod host;
// The transport behind the Library tab's Online source. Its background read and
// cache purge helpers are for the settings page and are not called yet.
#[allow(dead_code)]
mod online_http;
/// Pick the first bare word from `args`, skipping flags and their values.
///
/// # Rules
/// - A word starting with `--` is a flag. If it is in [`TAKES_A_VALUE`] the
///   word after it is its value and is skipped too.
/// - The first word left is the file to open.
///
/// # Do not
/// - Treat a flag's value as a path. `--tab pcb` names a tab, not a file.
fn first_bare_word(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if let Some(key) = arg.strip_prefix("--") {
            skip_next = TAKES_A_VALUE.contains(&key);
            continue;
        }
        return Some(arg.clone());
    }
    None
}

/// Flags that take the next word as their value. A bare path is the first
/// word that is neither a flag nor one of their values.
///
/// # Do not
/// - Add a flag that takes a value without adding it here. The first bare
///   path after it would be read as a file to open.
const TAKES_A_VALUE: [&str; 15] = [
    "project",
    "control",
    "size",
    "tab",
    "capture",
    "select",
    "search",
    "left",
    "menu",
    "show",
    "target",
    "add-tool",
    "view3d",
    "assistant",
    "sheet-name",
];

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    // A bare path opens that design. Double-clicking a file, dragging one
    // onto the window and `pcbcraft board.pcbcraft` all arrive this way, and
    // all three used to start an empty project without saying why. Worse,
    // that empty project then saved itself over the file it was asked to
    // open.
    //
    // A word that follows a flag is that flag's value, never the file.
    // Walking the list in order is what tells the two apart.
    let project = |key: &str| option(key).or_else(|| first_bare_word(&args));
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "PCBCraft — one-window Rust EDA\n\n--sample | --project FILE\n--control 127.0.0.1:7991\n--size WxH\n--tab files|schematic|pcb|analyze|rules|settings|libraries|project-library|symbol-editor|footprint-editor|checks|3d\n--light\n--capture /absolute/path.png\n--select REFERENCE\n--search WORDS\n--place-first-online\n--left library|layers\n--context-menu\n--show file-export|export-dialog|export-flavour|settings-board|settings-rules|settings-classes|settings-electrical|settings-machines|settings-title\n--menu tree-layer|tree-part|tree-net|library-card|finding|page-tab\n--sheet\n--target silk|edge\n--add-tool part|label|power|port|noconnect|pad|via|hole\n--view3d top|front|side|bottom\n--assistant run|run-done|run-open|review|applied|next"
        );
        return Ok(());
    }
    let mut session = if args.iter().any(|a| a == "--sample") {
        match pcbcraft_engine::Session::sample() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sample: {e}");
                std::process::exit(1);
            }
        }
    } else {
        pcbcraft_engine::Session::default()
    };
    session.config = pcbcraft_engine::user_config::UserConfig::from_environment();
    if args.iter().any(|a| a == "--sample") {
        // The demo board shows the default 45 degree corners.
        let cut = serde_json::json!({"points":[[3.0,0.0],[57.0,0.0],[60.0,3.0],[60.0,37.0],[57.0,40.0],[3.0,40.0],[0.0,37.0],[0.0,3.0]]});
        if let Err(e) = session.execute("board.set_outline", cut, None) {
            eprintln!("sample outline: {e}");
        }
    }
    if let Some(path) = project("--project") {
        let result = pcbcraft_io::load_for_open(std::path::Path::new(&path))
            .and_then(|project| session.open_project(project).map_err(|e| e.to_string()));
        if let Err(e) = result {
            eprintln!("project: {e}");
            std::process::exit(1);
        }
    }
    let hub = match pcbcraft_mcp::DocumentHub::new(session, project("--project")) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("documents: {e}");
            std::process::exit(1);
        }
    };
    let shared = match hub.active_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("documents: {e}");
            std::process::exit(1);
        }
    };
    if let Some(address) = option("--control")
        && let Err(e) = pcbcraft_mcp::start_bridge(hub.clone(), &address)
    {
        eprintln!("control bridge: {e}");
        std::process::exit(1);
    }
    let capture = option("--capture");
    let tab = option("--tab");
    let light = args.iter().any(|a| a == "--light");
    let project_path = project("--project");
    let select = option("--select");
    let left = option("--left");
    let context_menu = args.iter().any(|a| a == "--context-menu");
    let menu_tag = option("--menu");
    let show = option("--show");
    let sheet = args.iter().any(|a| a == "--sheet");
    let target = option("--target");
    let add_tool = option("--add-tool");
    let search = option("--search");
    let view3d = option("--view3d");
    let assistant = option("--assistant");
    let place_first = args.iter().any(|a| a == "--place-first-online");
    let offline = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        args.iter().any(|a| a == "--offline"),
    ));
    let distributors_offline = offline.clone();
    let keys_file = pcbcraft_engine::user_config::UserConfig::from_environment()
        .file("part-source-keys.json")
        .ok();
    let options = eframe::NativeOptions {
        viewport: egui_viewport(option("--size").as_deref()),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        "PCBCraft",
        options,
        Box::new(move |cc| {
            let app = pcbcraft_ui_egui::App::new(cc, shared, capture, tab.as_deref(), light)
                .with_project_path(project_path)
                .with_capture_select(select)
                .with_left_tab(left)
                .with_capture_view(context_menu)
                .with_capture_menu(menu_tag)
                .with_capture_sheet(sheet)
                .with_capture_show(show)
                .with_capture_target(target)
                .with_capture_tool(add_tool)
                .with_capture_search(search)
                .with_capture_view3d(view3d)
                .with_capture_place(place_first)
                .with_capture_assistant(assistant)
                .with_online_fetcher(
                    Box::new(online_http::HttpFetcher::new(
                        online_http::Policy::default(),
                        offline.clone(),
                    )),
                    offline,
                );
            // Distributor sources need a place to keep the person's key; with none, the dock shows no card.
            let app = match distributor_http::key_store(keys_file.clone()) {
                Some(store) => app.with_distributors(
                    std::sync::Arc::new(distributor_http::DistributorHttp::new(
                        distributors_offline.clone(),
                    )),
                    store,
                ),
                None => app,
            };
            Ok(Box::new(app.with_host(host::host())))
        }),
    )
}
/// Graphics setup that asks the adapter only for what it can give.
///
/// eframe requests its stock device limits. A weak or old software driver, such as the
/// Mesa 21 Vulkan driver on an Ubuntu 20.04 runner, reports lower maxima (two colour
/// attachments instead of eight) and the stock request then fails before any window opens.
/// The limits are lowered to the adapter's own where it is below the stock value.
///
/// # Rules
/// - Never raises a limit; capable adapters get exactly eframe's stock request.
///
/// # Dependencies
/// - `apps/pcbcraft/src/main.rs` builds the eframe `WgpuConfiguration` (egui-wgpu `setup.rs`).
///
/// # Do not
/// - Hard-code limits here; they must come from the adapter.
fn wgpu_options() -> eframe::WgpuConfiguration {
    let mut config = eframe::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        let stock = setup.device_descriptor.clone();
        setup.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut descriptor = stock(adapter);
            descriptor.required_limits = descriptor
                .required_limits
                .or_worse_values_from(&adapter.limits());
            descriptor
        });
    }
    config
}
fn egui_viewport(size: Option<&str>) -> eframe::egui::ViewportBuilder {
    let size = size
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?]))
        .filter(|[w, h]| (400.0..=4000.0).contains(w) && (300.0..=3000.0).contains(h))
        .unwrap_or([1440.0, 940.0]);
    let builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([1000.0, 680.0])
        .with_app_id("io.github.tangency_labs.pcbcraft");
    match pcbcraft_ui_egui::brand::window_icon() {
        Some(icon) => builder.with_icon(std::sync::Arc::new(icon)),
        None => builder,
    }
}

#[cfg(test)]
mod tests {
    use super::first_bare_word;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_bare_path_is_the_file_to_open() {
        assert_eq!(
            first_bare_word(&words("board.pcbcraft")).as_deref(),
            Some("board.pcbcraft"),
            "the only word is the file"
        );
        assert_eq!(
            first_bare_word(&words("--light board.pcbcraft")).as_deref(),
            Some("board.pcbcraft"),
            "a flag with no value does not swallow the file"
        );
        assert_eq!(
            first_bare_word(&words("--tab pcb board.pcbcraft")).as_deref(),
            Some("board.pcbcraft"),
            "pcb is the tab's value, not the file"
        );
        assert_eq!(
            first_bare_word(&words("board.pcbcraft --tab pcb")).as_deref(),
            Some("board.pcbcraft"),
            "the file can come first"
        );
    }

    #[test]
    fn a_flag_value_is_never_read_as_a_file() {
        for line in [
            "--tab pcb",
            "--project a.pcbcraft",
            "--capture /tmp/shot.png",
            "--select R1",
            "--search two words",
            "--sample",
            "",
        ] {
            let got = first_bare_word(&words(line));
            assert!(
                got.is_none() || line.starts_with("--search"),
                "{line:?} names no file to open, got {got:?}"
            );
        }
    }
}
