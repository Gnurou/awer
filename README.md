# Another World Engine in Rust

<p align="center"><img src="/screenshots/intro.gif?raw=true" width="75%"></p>

This is a project I have started mainly to teach myself Rust and get a glimpse of how it could possibly feel to write a great game engine.

I fondly remember the first time I played Another World (or *Out of this World* as it is known in North America) as a kid, and the disbelief that ensued. A game by [Eric Chahi](http://www.anotherworld.fr/anotherworld_uk/another_world.htm) released in 1991, it redefined the way video games could be designed to tell a story. No UI, no score, no dialogues, no ammo, no health indicator. No sprites that moved over a background layer made of fixed-size tiles - the game was rendered almost entirely using polygons, and this made it feel very different. It was alive, unpredictable, not constrained by limited game mechanics and hand-drawn sprites. Anything could happen, and very often anything did happen. Although heavily scripted, the game felt like it was having an existence of its own.

Look at the screen above and try to picture all the things that are happening. Of course, you notice that the beast has noticed you, and you know you will meet again. There is what looks like two birds passing by in the background, and a dandelion-like plant moves by the wind. The water makes small waves. But there is also dust being randomly blown your way - you need to pay more attention to this one.

The whole game is built with this level of detail, and in 1991 it was mind-blowing. This was made possible thanks to the use of a cooperatively multi-threaded virtual machine allowing for very complex in-game scripts, and which incidentally also made porting the game much easier. In 1991, this was both an artistic *and* a technical tour-de-force.

The technical tale of Another World has been told much better than I could by Fabien Sanglard with his [C++ reimplementation of the original game engine](http://fabiensanglard.net/anotherWorld_code_review/index.php) and [Polygons of Another World](http://fabiensanglard.net/another_world_polygons/index.html) series. This work has been the main source of guidance for the present project.

Another invaluable reference was Eric Chahi's original technical notes (in French, mind you), which are provided along with the [20th anniversary edition](https://www.gog.com/game/another_world_20th_anniversary_edition).

Contrary to Fabien's work, which apparently aimed at staying as close as possible to the original program (going as far as emulating the layout of DOS conventional memory to load the resources), this project takes liberties where it felt more optimal to do so. After all, you cannot program in Rust like it's 1991. Hopefully these choices will also make the code easier to understand.

Not that the code is by any means clean or documented at the moment, it's complete Rust n00b rubbish. I do hope to improve that however.

[SDL2](https://www.libsdl.org/) is used for all input and output.

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

    cargo run --release -- --scene=1

Options:

`--scene=x`

This will start the game at scene `x`. Mostly useful to skip the password protection screen (use `--scene=1`). Note that some scenes depend on the state left by the previous one, so expect crashes if you use this.

`--render=(raster | gl_raster | gl_poly | gl_line)`

Choose the rendering method.

`raster` is a pure software rendering mode at original 320x200 resolution and aims at showing the game the way ~~God~~ Eric Chahi intended. The final 320x200 image is scaled using SDL2, which will use whatever hardware acceleration is supported by this step only.

<p align="center"><img src="/screenshots/raster.png?raw=true" width="75%"></p>

`gl_raster` is similar to `raster`, but uses a GL shader to convert and scale the game screen to our modern displays. It is more efficient than `raster`, but introduces a dependency to GL.

`gl_poly` creates triangles from the polygons and renders them using OpenGL. This makes rendering fast and smooth at higher resolutions. However, since that's clearly not how the game was designed to be rendered, artefacts in the form of gaps and misshaped objects are to be expected. Also, transparency cannot be rendered faithfully in this mode.

<p align="center"><img src="/screenshots/poly.png?raw=true" width="75%"></p>

`gl_line` is also a mode that uses OpenGL, but renders the polygons' outlines only. Useful to study how they are designed, not so much for enjoying the game.

<p align="center"><img src="/screenshots/line.png?raw=true" width="75%"></p>

Keys
----
* `Up`, `Down`, `Left`, `Right`: Move.
* `Space`: Action.
* `P`: Pause/resume. While in pause:
    * `N`: Take a snapshot of the game's state and continue up to the next frame.
    * `B`: Restore the last snapshot (moving back to the previous frame if you pressed `N`).
* `F`: Fast-forward, useful to make some cinematic scenes go faster.
* `B`: Rewind to the last snapshot. Snapshots are taken roughly every 5 seconds. Useful if you die a lot (which you will).
