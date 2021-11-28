use clap::{App, Arg};

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
    env_logger::init();

    let matches = App::new("Another World")
        .version("0.1")
        .arg(
            Arg::with_name("scene")
                .short("S")
                .long("scene")
                .help("The scene to start from (0..9)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("render")
                .short("r")
                .long("render")
                .help("How to render the game (raster, gl_raster, gl_poly, gl_line)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("dump_resources")
                .short("d")
                .long("dump_resources")
                .help("Dump all resources to disk"),
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

    if matches.is_present("dump_resources") {
        println!("Dumping all resources...");
        let mut resman = res::ResourceManager::new().unwrap();
        resman.dump_resources().unwrap();
        return;
    }

    let mut sys: Box<dyn Sys> = sys::sdl2::new(&matches).unwrap();

    let mut vm = Box::new(vm::Vm::new().unwrap());
    vm.init(&scenes::SCENES[start_scene]);

    sys.game_loop(&mut vm);
}
