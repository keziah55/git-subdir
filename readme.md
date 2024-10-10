# git-subdir

Command line tool to download subdirectories from github.

## Build

```
cargo build --release
```

This builds the binary `target/release/git-subdir`.

For example,
```
target/release/git-subdir https://github.com/keziah55/git-subdir/tree/main/src
```
downloads just the `src` directory from this repo.

See
```
target/release/git-subdir --help
```
for options.

## TODO

- Support gitlab urls
- Write files relative to given link, rather than repo root (to avoid extra directory layers)