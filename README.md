# Pixel Shift Multi Shoot

Rust tool to merge sony's Pixel Shift Multi Shoot raw files into a single debayered / rgb image

It supports 4 shots and 16 shots pixel shift raw files.

## usage

```
cargo run --release -- -o /tmp/test.tiff -i ../images/*.ARW
```

## credits

based on https://github.com/agriggio/make_arq
