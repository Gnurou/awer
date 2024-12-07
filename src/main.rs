mod audio;
mod font;
mod gfx;
mod input;
mod res;
mod scenes;
mod strings;
mod sys;
mod vm;

use clap::Parser;
use scenes::SCENES;
use tracing_subscriber::prelude::*;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The scene to start from (0..9)
    #[arg(short, long, value_name = "SCENE")]
    scene: Option<u8>,
    /// How to render the game (raster, gl_raster, gl_poly, gl_line)
    #[arg(short, long, value_name = "RENDERER")]
    renderer: Option<String>,
    /// List all the available resources with their properties and exit
    #[arg(short, long)]
    list_resources: bool,
    /// Dump all resources into the \"resources\" folder and exit
    #[arg(short, long)]
    dump_resources: bool,
    /// Record a trace in the Chrome format into trace_file instead of printing events on the
    /// standard output
    #[arg(short, long, value_name = "TRACE_FILE")]
    trace_file: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let start_scene = match cli.scene.unwrap_or(0) as usize {
        scene if scene <= SCENES.len() => scene,
        _ => panic!("invalid scene number"),
    };

    let mut must_exit = false;

    if cli.list_resources {
        let resman = res::ResourceManager::new().unwrap();
        resman.list_resources();
        must_exit = true;
    }

    if cli.dump_resources {
        println!("Dumping all resources...");
        let mut resman = res::ResourceManager::new().unwrap();
        resman.dump_resources().unwrap();
        must_exit = true;
    }

    let _trace_flush_guard = if let Some(trace_file) = cli.trace_file {
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

    let Some(mut sys) = sys::sdl2::sdl2_simple::new_with_renderer(&cli.renderer) else {
        panic!("failed to create system component");
    };

    let mut vm = Box::new(vm::Vm::new().unwrap());
    vm.request_scene(start_scene);

    sys.game_loop(&mut vm);
}
