use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader},
};

pub type GameStrings = BTreeMap<usize, String>;

pub fn load_strings() -> io::Result<GameStrings> {
    let file = File::open("strings.txt")?;
    let lines = BufReader::new(file).lines();

    let mut strings = GameStrings::new();

    for line in lines {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                log::error!("Error parsing string: {}", e);
                continue;
            }
        };
        let parts = line.split_at(5);

        let index = usize::from_str_radix(&parts.0[2..], 16).unwrap();
        let string = parts.1[2..].replace("\\n", "\n");

        strings.insert(index, string);
    }

    Ok(strings)
}
