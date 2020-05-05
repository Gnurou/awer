# Another World Engine in Rust

This is a project I have started mainly to teach myself Rust and get a glimpse of how it feels to write a great game engine. I fondly remember the amazement I experienced the first time I played Another World as a kid.

This project is mainly possible thanks to the [work done by Fabien Sanglard
reimplementing the original game engine](http://fabiensanglard.net/anotherWorld_code_review/index.php). Also don't miss his [Polygons of Another World](http://fabiensanglard.net/another_world_polygons/index.html) series.

Another invaluable reference was Eric Chahi's original technical notes in French, which are provided along with the [20th anniversary edition](https://www.gog.com/game/another_world_20th_anniversary_edition).

The code is by no means clean or documented at the moment, but I hope to improve that.

[Piston](https://www.piston.rs/) is used for all input/output.

What is working:

* Virtual machine
* Rendering of polygons and bitmaps
* Input

This makes the game basically playable.

What is not completed yet:

* Sound
* Rendering of text

How to run
----------
First you need a copy of the original DOS game data, which comes in the form of one `memlist.bin` and a bunch of `bank0x` files. Sadly the 20th anniversary edition does not include the data in this format.

Put the game data it in the root directory of this project, then build and run, e.g:

    cargo run -- --scene=1

Options:

`--scene=x`

This will start the game at scene `x`. Mostly useful to skip the password protection screen (use `--scene=1`). Note that some scenes depend on the state left by the previous one, so expect crashes if you use this.

`--render=(raster | poly | line)`

Choose the rendering method. `raster` is a pure software rendering mode at original resolution, which aims at mimicking exactly how the original game looked.

<p align="center"><img src="/screenshots/raster.png?raw=true" width="75%"></p>

`poly` creates quads from the polygons and passes them directly to OpenGL. This makes rendering fast and smooth at higher resolutions. However, since that's clearly not how the game was designed to be rendered, artefacts in the form of gaps and misshaped objects are to be expected. Also, transparency cannot be rendered faithfully in this mode.

<p align="center"><img src="/screenshots/poly.png?raw=true" width="75%"></p>

`line` is also a mode that uses OpenGL, but renders the polygons' outlines only. Useful to study how they are designed, not so much for enjoying the game.

<p align="center"><img src="/screenshots/line.png?raw=true" width="75%"></p>

Keys
----
* Up, Down, Left, Right: Move.
* Space: Action.
* P: Pause/resume.
* F: Fast-forward, useful to make some cinematic scenes go faster.
* B: Rewind 5 seconds back in time. Useful if you die a lot (which you will). Note that the display will probably not look great for a few seconds since the framebuffers and palette are not restored yet.
