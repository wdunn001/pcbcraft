mod host;
// The transport behind the Library tab's Online source. Its background read and
// cache purge helpers are for the settings page and are not called yet.
#[allow(dead_code)]
mod online_http;
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "PCBCraft — one-window Rust EDA\n\n--sample | --project FILE\n--control 127.0.0.1:7991\n--size WxH\n--tab files|schematic|pcb|rules|settings|libraries|project-library|symbol-editor|footprint-editor|checks|3d\n--light\n--capture /absolute/path.png\n--select REFERENCE\n--search WORDS\n--place-first-online\n--left library|layers\n--context-menu\n--sheet\n--target silk|edge"
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
    if let Some(path) = option("--project") {
        let result = pcbcraft_io::load(std::path::Path::new(&path))
            .and_then(|project| session.replace(project).map_err(|e| e.to_string()));
        if let Err(e) = result {
            eprintln!("project: {e}");
            std::process::exit(1);
        }
    }
    let hub = match pcbcraft_mcp::DocumentHub::new(session, option("--project")) {
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
    let project_path = option("--project");
    let select = option("--select");
    let left = option("--left");
    let context_menu = args.iter().any(|a| a == "--context-menu");
    let sheet = args.iter().any(|a| a == "--sheet");
    let target = option("--target");
    let search = option("--search");
    let place_first = args.iter().any(|a| a == "--place-first-online");
    let offline = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        args.iter().any(|a| a == "--offline"),
    ));
    let options = eframe::NativeOptions {
        viewport: egui_viewport(option("--size").as_deref()),
        ..Default::default()
    };
    eframe::run_native(
        "PCBCraft",
        options,
        Box::new(move |cc| {
            Ok(Box::new(
                pcbcraft_ui_egui::App::new(cc, shared, capture, tab.as_deref(), light)
                    .with_project_path(project_path)
                    .with_capture_select(select)
                    .with_left_tab(left)
                    .with_capture_view(context_menu)
                    .with_capture_sheet(sheet)
                    .with_capture_target(target)
                    .with_capture_search(search)
                    .with_capture_place(place_first)
                    .with_online_fetcher(
                        Box::new(online_http::HttpFetcher::new(
                            online_http::Policy::default(),
                            offline.clone(),
                        )),
                        offline,
                    )
                    .with_host(host::host()),
            ))
        }),
    )
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
        .with_app_id("io.github.tangency-labs.pcbcraft");
    match pcbcraft_ui_egui::brand::window_icon() {
        Some(icon) => builder.with_icon(std::sync::Arc::new(icon)),
        None => builder,
    }
}
