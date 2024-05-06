use clap::Parser;
use image::ImageBuffer;
use log::info;
use memmap::{Mmap, MmapOptions};
use rayon::prelude::*;
use std::{collections::HashMap, hash::Hash, process::Command};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    output_file: String,

    #[arg(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    input_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Color {
    Red,
    Green,
    Blue,
}

fn read_exif(path: &str) -> HashMap<String, String> {
    let exifs = Command::new("exiftool")
        .arg(path)
        .output()
        .expect("failed to execute process");

    fn parse_exiftool_output(output: &str) -> HashMap<String, String> {
        output
            .lines()
            .map(|line| {
                let mut parts = line.splitn(2, ':');
                let key = parts.next().unwrap().trim().to_string();
                let value = parts.next().unwrap().trim().to_string();
                (key, value)
            })
            .collect()
    }

    let exifs = String::from_utf8_lossy(&exifs.stdout);

    parse_exiftool_output(&exifs)
}

fn seq2idx(s: u32) -> u32 {
    match s {
        2 => 0,
        1 => 1,
        4 => 2,
        3 => 3,
        _ => unreachable!(),
    }
}

fn key(t: u32) -> (u32, u32) {
    let sn = t - 1;
    let s = 1 + (sn) % 4;
    let i = seq2idx(s);
    let g = seq2idx(1 + sn / 4);
    (g, i)
}

fn dngcolor(row: u32, col: u32) -> Color {
    let v = 0x94949494u32 >> ((((row) << 1 & 14) + ((col) & 1)) << 1) & 3;

    match v {
        0 => Color::Red,
        1 => Color::Green,
        2 => Color::Blue,
        3 => Color::Green,
        _ => unreachable!(),
    }
}

#[derive(Debug)]
struct RawImage {
    _path: String,
    width: u32,
    height: u32,
    offset: u32,
    _sequence_number: u32,
    group: u32, // which group of 4 images this image belongs to, every group has 4 images
    id: u32,    // which image in the group this image is
    data: Mmap,
}

impl RawImage {
    fn new(path: &str) -> Self {
        let exif_map = read_exif(path);

        let offset = exif_map
            .get("Strip Offsets")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let width = exif_map.get("Image Width").unwrap().parse::<u32>().unwrap();
        let height = exif_map
            .get("Image Height")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let sequence_number = exif_map
            .get("Sequence Number")
            .unwrap()
            .parse::<u32>()
            .unwrap();

        let file = std::fs::File::open(path).unwrap();
        let data = unsafe { MmapOptions::new().map(&file).unwrap() };

        let gi = key(sequence_number);

        Self {
            _path: path.to_string(),
            width,
            height,
            offset,
            _sequence_number: sequence_number,
            group: gi.0,
            id: gi.1,
            data,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> u16 {
        let offset = (y * self.width * 2 + x * 2) as usize + self.offset as usize;
        let px_low = *self.data.get(offset).unwrap_or(&0);
        let px_hig = *self.data.get(offset + 1).unwrap_or(&0);

        u16::from_le_bytes([px_low, px_hig])
    }

    fn pixel_offset(&self, x: u32, y: u32) -> u16 {
        let (r_off, c_off) = self.offsets();
        self.pixel(x - c_off, y - r_off)
    }

    fn color_offset(&self, x: u32, y: u32) -> Color {
        let (r_off, c_off) = self.offsets();
        dngcolor(y - r_off, x - c_off)
    }

    fn offsets(&self) -> (u32, u32) {

        match self.id {
            0 => (1, 1),
            1 => (0, 1),
            2 => (0, 0),
            3 => (1, 0),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MergeMode {
    Mode4,
    Mode16,
}
fn merge_4(files: &[RawImage]) -> ImageBuffer<image::Rgb<u16>, Vec<u16>> {
    info!("merging 4");
    let mut imgbuf = image::ImageBuffer::new(files[0].width, files[0].height);

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let mut px = image::Rgb([0u16, 0, 0]);

        for file in files {
            let val = file.pixel_offset(x, y) as u32;
            let color = file.color_offset(x, y);

            match color {
                Color::Red => px.0[0] += (val) as u16 * 2,
                Color::Green => px.0[1] += (val) as u16,
                Color::Blue => px.0[2] += (val) as u16 * 2,
            }
        }

        *pixel = px;
    }

    imgbuf
}

fn main() {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let now = std::time::Instant::now();

    let args = Args::parse();

    info!("loading files");
    let mut files = args
        .input_files
        .par_iter()
        .map(|path| RawImage::new(path))
        .collect::<Vec<_>>();

    files.sort_by_key(|file| (file.group, file.id));

    for file in &files {
        info!("{:?}", file);
    }

    let mode = match files.len() {
        4 => MergeMode::Mode4,
        16 => MergeMode::Mode16,
        _ => panic!("unsupported number of files"),
    };

    println!("{:?}", mode);

    if mode == MergeMode::Mode4 {
        let imgbuf = merge_4(&files[..4]);

        imgbuf.save(&args.output_file).unwrap();
    }

    if mode == MergeMode::Mode16 {
        let groups = files.chunks(4).collect::<Vec<&[RawImage]>>();

        let groups = groups
            .par_iter()
            .map(|g| merge_4(g))
            .collect::<Vec<ImageBuffer<image::Rgb<u16>, Vec<u16>>>>();

        info!("creating buffer");
        let mut imgbuf = image::ImageBuffer::new(files[0].width * 2, files[0].height * 2);

        info!("merging 16");
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            match (x % 2, y % 2) {
                (0, 0) => {
                    *pixel = *groups[0].get_pixel(x / 2, y / 2);
                }
                (1, 0) => {
                    *pixel = *groups[1].get_pixel(x / 2, y / 2);
                }
                (0, 1) => {
                    *pixel = *groups[2].get_pixel(x / 2, y / 2);
                }
                (1, 1) => {
                    *pixel = *groups[3].get_pixel(x / 2, y / 2);
                }
                _ => unreachable!(),
            };
        }

        info!("saving");

        imgbuf.save(&args.output_file).unwrap();
    }

    info!("done in {:?}", now.elapsed());
}
