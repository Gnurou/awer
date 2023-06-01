use clap::Arg;
use tracing_subscriber::prelude::*;

mod audio;
mod font;
mod gfx;
mod input;
mod res;
mod scenes;
mod strings;
mod sys;
mod vm;

use scenes::SCENES;
use sys::Sys;

fn main() {
    let matches = clap::Command::new("Another World")
        .version("0.1")
        .arg(
            Arg::new("scene")
                .short('s')
                .long("scene")
                .help("The scene to start from (0..9)")
                .takes_value(true),
        )
        .arg(
            Arg::new("render")
                .short('r')
                .long("render")
                .help("How to render the game (raster, gl_raster, gl_poly, gl_line)")
                .takes_value(true),
        )
        .arg(
            Arg::new("list-resources")
                .short('l')
                .long("list-resources")
                .help("List all the available resources with their properties and exit"),
        )
        .arg(
            Arg::new("dump-resources")
                .short('d')
                .long("dump-resources")
                .help("Dump all resources into the \"resources\" folder and exit"),
        )
        .arg(
            Arg::new("trace_file")
                .short('t')
                .long("chrome-trace")
                .help("Record a trace in the Chrome format into trace_file instead of printing events on the standard output")
                .takes_value(true),
        )
        .get_matches();

    let start_scene = matches
        .value_of("scene")
        .unwrap_or("0")
        .parse::<usize>()
        .expect("expected integer for scene option.");

    let start_scene = match start_scene {
        scene if scene <= SCENES.len() => scene,
        _ => panic!("Invalid scene number!"),
    };

    let mut must_exit = false;

    if matches.is_present("list-resources") {
        let resman = res::ResourceManager::new().unwrap();
        resman.list_resources();
        must_exit = true;
    }

    if matches.is_present("dump-resources") {
        println!("Dumping all resources...");
        let mut resman = res::ResourceManager::new().unwrap();
        resman.dump_resources().unwrap();
        must_exit = true;
    }

    let _trace_flush_guard = if let Some(trace_file) = matches.value_of("trace_file") {
        let (chrome_layer, flush_guard) = tracing_chrome::ChromeLayerBuilder::new()
            .include_args(true)
            .include_locations(false)
            .file(trace_file)
            .build();
        tracing_subscriber::registry().with(chrome_layer).init();
        Some(flush_guard)
    } else {
        tracing_subscriber::fmt::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
        None
    };

    if must_exit {
        return;
    }

    let mut sys: Box<dyn Sys> = sys::sdl2::sdl2_simple::new_from_args(&matches).unwrap();

    let mut vm = Box::new(vm::Vm::new().unwrap());
    vm.init_for_scene(&scenes::SCENES[start_scene]);

    sys.game_loop(&mut vm);
}
