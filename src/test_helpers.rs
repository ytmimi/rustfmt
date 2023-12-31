use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn is_subpath<P>(path: &Path, subpath: &P) -> bool
where
    P: AsRef<Path>,
{
    (0..path.components().count())
        .map(|i| {
            path.components()
                .skip(i)
                .take(subpath.as_ref().components().count())
        })
        .any(|c| c.zip(subpath.as_ref().components()).all(|(a, b)| a == b))
}

fn is_file_skip(path: &Path, skip_list: &[&str]) -> bool {
    skip_list
        .iter()
        .any(|file_path| is_subpath(path, file_path))
}

// Reads significant comments of the form: `// rustfmt-key: value` into a hash map.
pub fn read_significant_comments(file_name: &Path) -> HashMap<String, String> {
    let file = fs::File::open(file_name)
        .unwrap_or_else(|_| panic!("couldn't read file {}", file_name.display()));
    let reader = BufReader::new(file);
    let pattern = r"^\s*//\s*rustfmt-([^:]+):\s*(\S+)";
    let regex = regex::Regex::new(pattern).expect("failed creating pattern 1");

    // Matches lines containing significant comments or whitespace.
    let line_regex = regex::Regex::new(r"(^\s*$)|(^\s*//\s*rustfmt-[^:]+:\s*\S+)")
        .expect("failed creating pattern 2");

    reader
        .lines()
        .map(|line| line.expect("failed getting line"))
        .filter(|line| line_regex.is_match(line))
        .filter_map(|line| {
            regex.captures_iter(&line).next().map(|capture| {
                (
                    capture
                        .get(1)
                        .expect("couldn't unwrap capture")
                        .as_str()
                        .to_owned(),
                    capture
                        .get(2)
                        .expect("couldn't unwrap capture")
                        .as_str()
                        .to_owned(),
                )
            })
        })
        .collect()
}

// Returns a `Vec` containing `PathBuf`s of files with an  `rs` extension in the
// given path. The `recursive` argument controls if files from subdirectories
// are also returned.
pub fn get_test_files(path: &Path, recursive: bool, skip_list: &[&str]) -> Vec<PathBuf> {
    let mut files = vec![];
    if path.is_dir() {
        for entry in fs::read_dir(path).expect(&format!(
            "couldn't read directory {}",
            path.to_str().unwrap()
        )) {
            let entry = entry.expect("couldn't get `DirEntry`");
            let path = entry.path();
            if path.is_dir() && recursive {
                files.append(&mut get_test_files(&path, recursive, skip_list));
            } else if path.extension().map_or(false, |f| f == "rs")
                && !is_file_skip(&path, skip_list)
            {
                files.push(path);
            }
        }
    }
    files
}
