use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::{self};

pub type GameStrings = BTreeMap<usize, String>;

pub fn load_strings() -> io::Result<GameStrings> {
    let file = File::open("strings.txt")?;
    let lines = BufReader::new(file).lines();

    let mut strings = GameStrings::new();

    for line in lines {
        let line = line?;
        let parts = line.split_at(5);

        let index = usize::from_str_radix(&parts.0[2..], 16).map_err(std::io::Error::other)?;
        let string = parts.1[2..].replace("\\n", "\n");

        strings.insert(index, string);
    }

    Ok(strings)
}
