use clap::{App, Arg};

mod gfx;
mod input;
mod res;
mod scenes;
mod sys;
mod vm;

use gfx::piston::gl::PolyRender;
use scenes::SCENES;

fn main() {
    env_logger::init();

    let matches = App::new("Another World")
        .version("0.1")
        .arg(
            Arg::with_name("scene")
                .short("s")
                .long("scene")
                .help("The scene to start from (0..9)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("render")
                .short("r")
                .long("render")
                .help("How to render the game (raster, line, poly)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("dump_resources")
                .short("d")
                .long("dump_resources")
                .help("Dump all resources to disk"),
        )
        .get_matches();

    let start_scene = usize::from_str_radix(matches.value_of("scene").unwrap_or("0"), 10)
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

    let mut sys = sys::piston::new();

    let mut gfx = match matches.value_of("render").unwrap_or("raster") {
        rdr @ "line" | rdr @ "poly" => {
            let poly_render = match rdr {
                "line" => PolyRender::Line,
                "poly" => PolyRender::Poly,
                _ => panic!(),
            };
            Box::new(gfx::piston::gl::new().set_poly_render(poly_render))
                as Box<dyn gfx::piston::PistonBackend>
        }
        "raster" => Box::new(gfx::piston::raster::new()) as Box<dyn gfx::piston::PistonBackend>,
        _ => panic!("unexpected poly_render option"),
    };

    let mut vm = Box::new(vm::VM::new().unwrap());
    vm.init(&scenes::SCENES[start_scene]);

    sys.game_loop(&mut vm, &mut *gfx);
}
