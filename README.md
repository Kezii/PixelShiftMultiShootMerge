# Pixel Shift Multi Shoot

Rust tool to merge sony's Pixel Shift Multi Shoot raw files into a single debayered / rgb image

It supports 4 shots and 16 shots pixel shift raw files.

## speed

Thanks to mmap and parallel processing, a 16 shot 240 megapixel pixel shift can be processed in about 1 second on a 16 core machine.

## usage

```
cargo run --release -- -o /tmp/test.tiff -i ../images/*.ARW
```

## credits

inspired by https://github.com/agriggio/make_arq
