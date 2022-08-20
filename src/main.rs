use clap::Arg;

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
    env_logger::init();

    let matches = clap::Command::new("Another World")
        .version("0.1")
        .arg(
            Arg::new("scene")
                .short('S')
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

    if must_exit {
        return;
    }

    let mut sys: Box<dyn Sys> = sys::sdl2::sdl2_simple::new_from_args(&matches).unwrap();

    let mut vm = Box::new(vm::Vm::new().unwrap());
    vm.init_for_scene(&scenes::SCENES[start_scene]);

    sys.game_loop(&mut vm);
}
