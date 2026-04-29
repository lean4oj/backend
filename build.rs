use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use directories::ProjectDirs;

fn compatible(p1: &Path, p2: &Path) -> bool {
    #[cfg(target_vendor = "apple")]
    use std::os::darwin::fs::MetadataExt;
    #[cfg(not(target_vendor = "apple"))]
    use std::os::linux::fs::MetadataExt;

    let m1 = fs::metadata(p1).unwrap();
    let m2 = fs::metadata(p2).unwrap();
    m1.st_dev() == m2.st_dev()
}

fn main() {
    let Output { stdout, .. } = Command::new("git")
        .args(["show", "-s", "--pretty=%H %ct"])
        .output().unwrap();
    let value = String::from_utf8(stdout).unwrap();
    let (hash, date) = value.split_once(' ').unwrap();

    println!("cargo::rustc-env=SERVER_VERSION_HASH={}", &hash[..10]);
    println!("cargo::rustc-env=SERVER_VERSION_DATE={date}");
    let home = env::home_dir().unwrap();
    let tmp = env::temp_dir();
    let dir = if let Some(d) = env::var_os("LEAN4OJ_RSYNC_TMPDIR") {
        PathBuf::from(d)
    } else if compatible(&home, &tmp) {
        tmp
    } else {
        ProjectDirs::from("com", "kitsune", "lean4oj").unwrap().cache_dir().to_path_buf()
    };
    println!("cargo::rustc-env=LEAN4OJ_RSYNC_TMPDIR={}", dir.display());

    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    // let path = format!("src/libs/util/hash_add.{target}.o");
    println!("cargo::rustc-link-arg=src/libs/util/hash_add.o");
}
