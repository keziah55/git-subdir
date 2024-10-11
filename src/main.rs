use clap::Parser;
use reqwest;
use scraper::{Html, Selector};
use serde_json::Value;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(version = "0.1")]
#[command(about="download a subdirectory from github repo", long_about=None)]
pub struct Cli {
    /// Github url
    url: String,

    /// Output directory. Is created if it doesn't exist.
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Ignore subdirectories
    #[arg(short = 'i', long)]
    ignore_subdirs: bool,

    /// Write paths relative to given url rather than repo root.
    #[arg(short = 'r', long)]
    relative: bool,
}

struct GitHubUrl {
    username: String,
    repo_name: String,
    branch: String,
    path: PathBuf,
}

impl GitHubUrl {
    /// Create new GitHubUrl instance
    pub fn new(url: &String) -> GitHubUrl {
        let prefix = "https://github.com/";
        if !url.starts_with(prefix) {
            panic!("'{}' is not a github url", url);
        }

        let url_parts: Vec<&str> = url.strip_prefix(prefix).unwrap().split("/").collect();
        if url_parts.len() < 4 {
            panic!("'{}' is not a url to a directory within a github repo", url);
        }

        if url_parts[2] != "tree" {
            panic!("Cannot parse url '{}'", url);
        }

        let username = String::from(url_parts[0]);
        let repo_name = String::from(url_parts[1]);
        let branch = String::from(url_parts[3]);
        let path = PathBuf::from(url_parts[4..].join("/"));

        GitHubUrl {
            username,
            repo_name,
            branch,
            path,
        }
    }

    /// Return url to directory
    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/{}/tree/{}/{}",
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
    }

    /// Return the name of the requested dir
    pub fn basename(&self) -> String {
        String::from(
            self.path
                .components()
                .last()
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap(),
        )
    }

    /// Return new GitHubUrl with `part` appended 
    pub fn join(&self, part: &str) -> GitHubUrl {
        let new_url = format!("{}/{}", self.url(), part);
        GitHubUrl::new(&new_url)
    }

    /// Return url to get raw file
    pub fn as_raw_url(&self) -> String {
        format!("{}?raw=true", self.url())
    }

    /// Return relative path from self to other
    pub fn relative_to(&self, other: &GitHubUrl) -> Result<PathBuf, String>{        
        let prefix = format!("{}/", other.path.to_str().unwrap());
        Ok(PathBuf::from(self.path.clone().strip_prefix(prefix).unwrap()))

    }
}

impl fmt::Display for GitHubUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "GitHubUrl {{\n  username: {},\n  repo: {},\n  branch: {},\n  path: {}\n}}",
            self.username,
            self.repo_name,
            self.branch,
            self.path.to_str().unwrap()
        )
    }
}

fn make_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|_| panic!("Could not create dir '{}'", path.to_str().unwrap()));
}

fn get_git_dir(
    url: &GitHubUrl,
    output_path: &PathBuf,
    ignore_subdirs: bool,
    preserve_path_structure: bool,
) {
    // note: using blocking instead of async because this function is called recursively
    let text = reqwest::blocking::get(url.url()).unwrap().text().unwrap();

    let document = Html::parse_document(&text);
    let selector =
        Selector::parse(r#"script[type="application/json"][data-target="react-app.embeddedData"]"#)
            .unwrap();
    for title in document.select(&selector) {
        let v: Value = serde_json::from_str(&title.inner_html()).unwrap();

        let items = v["payload"]["tree"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_object().unwrap());

        for item in items {
            download(
                url,
                item,
                output_path,
                ignore_subdirs,
                preserve_path_structure,
            );
        }
    }
}

fn download(
    base_url: &GitHubUrl,
    item_info: &serde_json::Map<String, serde_json::Value>,
    output_path: &PathBuf,
    ignore_subdirs: bool,
    preserve_path_structure: bool,
) {
    let item_type = item_info["contentType"].as_str().unwrap();
    let item_name = item_info["name"].as_str().unwrap();
    let item_path = item_info["path"].as_str().unwrap();

    let url = base_url.join(item_name);

    // println!();
    // println!("base_url: {}", base_url);
    // println!("base_url path len: {}", base_url.path.iter().count());
    // println!("url: {}", url);

    // let rel_path = url.relative_to(&base_url).unwrap();

    // println!("rel_path: {}", rel_path.to_str().unwrap());

    // println!();

    let mut filename = PathBuf::from(output_path);

    filename.push(item_path); //rel_path);

    // if !preserve_path_structure && base_url.path.iter().count() > 1 {

    // } else {
    //     filename.push(rel_path);
    // }

    // if !filename.parent().unwrap().exists() {
    //     make_dir(filename.clone().parent().unwrap());
    // }

    match item_type {
        "file" => download_file(base_url, &filename),
        "directory" => {
            if !ignore_subdirs {
                get_git_dir(&url, output_path, ignore_subdirs, preserve_path_structure)
            }
        }
        "symlink_file" => {
            println!("Skipping symlink file '{}'", url.path.to_str().unwrap());
        }
        _ => panic!("Cannot handle item type '{}'", item_type),
    }
}

fn download_file(url: &GitHubUrl, filename: &PathBuf) {// item_path: &str, output_path: &PathBuf) {
    let raw_url = url.as_raw_url();

    let text = reqwest::blocking::get(raw_url).unwrap().text().unwrap();

    if !filename.parent().unwrap().exists() {
        make_dir(filename.clone().parent().unwrap());
    }

    fs::write(filename, text).unwrap();

    println!("Downloaded '{}'", filename.to_str().unwrap());
}

/// Download directory from github
pub fn get_git_subdir(
    url: &String,
    output: Option<String>,
    ignore_subdirs: bool,
    preserve_path_structure: bool,
) {
    let url = GitHubUrl::new(url);

    // if not given output path, use basename from url
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(url.basename()),
    };

    if !output_path.exists() {
        make_dir(&output_path);
    }

    get_git_dir(&url, &output_path, ignore_subdirs, preserve_path_structure);
}

fn main() {
    let cli = Cli::parse();

    // let url = String::from("https://github.com/gjf2a/midi_fundsp/tree/master/examples");
    get_git_subdir(&cli.url, cli.output, cli.ignore_subdirs, !cli.relative);
}

// TESTS
#[test]
fn test_default_args() {
    let url = String::from("https://github.com/keziah55/ABBAd_day/tree/master/ABBAd_day");

    get_git_subdir(&url, None, false, true);

    let expected_path = PathBuf::from("ABBAd_day");
    assert!(expected_path.exists());
    let expected_files = vec!["ABBAd_day.ino", "fileArray.ino"];
    for filename in expected_files {
        assert!(expected_path.join(filename).exists());
    }

    fs::remove_dir_all(expected_path).unwrap();
}

#[test]
fn test_custom_output_path() {
    let url = String::from("https://github.com/keziah55/git-subdir/tree/main/src");

    let output_path = PathBuf::from("tmp_test");
    let expected_path = output_path.clone();
    let output_path_arg = Some(output_path.into_os_string().into_string().unwrap());

    get_git_subdir(&url, output_path_arg, false, true);

    assert!(expected_path.exists());
    let filepath = expected_path.join("main.rs");
    assert!(filepath.exists());

    fs::remove_dir_all(expected_path).unwrap();
}

#[test]
fn test_ignore_subdirs() {
    let url = String::from("https://github.com/keziah55/pick/tree/main/mediabrowser");

    get_git_subdir(&url, None, true, true);

    let output_path = PathBuf::from("mediabrowser");
    let expected_path = output_path.clone();
    assert!(expected_path.exists());
    for item in fs::read_dir(expected_path).unwrap() {
        let p = item.unwrap().path();
        assert!(!p.is_dir());
        assert_eq!(p.extension().unwrap(), "py");
    }

    fs::remove_dir_all(output_path).unwrap();
}

// #[test]
// fn test_relative_path() {
//     let url = String::from("https://github.com/keziah55/git-subdir/tree/main/src");

//     let output_path = PathBuf::from("tmp_test");
//     let expected_path = output_path.clone();
//     let output_path_arg = Some(output_path.into_os_string().into_string().unwrap());

//     get_git_subdir(&url, output_path_arg, false, false);

//     assert!(expected_path.exists());
//     let filepath = expected_path.join("src/main.rs");
//     assert!(filepath.exists());

//     fs::remove_dir_all(expected_path).unwrap();
// }



// #[test]
// fn test_relative_path() {
//     let url = String::from(
//         "https://github.com/keziah55/pick/tree/main/mediabrowser/templates/mediabrowser",
//     );

//     let output_path = PathBuf::from("tmp_test");
//     let expected_path = output_path.clone();
//     let output_path_arg = Some(output_path.clone().into_os_string().into_string().unwrap());

//     get_git_subdir(&url, output_path_arg, false, false);

//     for item in fs::read_dir(expected_path).unwrap() {
//         let p = item.unwrap().path();
//         assert!(!p.is_dir());
//         assert_eq!(p.extension().unwrap(), "html");
//     }

//     // fs::remove_dir_all(output_path).unwrap();
// }
