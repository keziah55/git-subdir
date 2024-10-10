# git-subdir

Command line tool to download subdirectories from github.

## Build

```
cargo build --release
```

This downloads dependencies and builds the executable `target/release/git-subdir`, which 
you may want to move or symlink to a location in your `PATH`

## Usage

For example (using the `git-subdir` executable),
```
git-subdir https://github.com/keziah55/git-subdir/tree/main/src
```
downloads just the `src` directory from this repo.

See
```
git-subdir --help
```
for options.

## TODO

- Support gitlab urls
- Write files relative to given link, rather than repo root (to avoid extra directory layers)