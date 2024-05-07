use clap::Parser;
use exif::read_exif;
use log::info;
use memmap::{Mmap, MmapOptions};
use rayon::prelude::*;

mod exif;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    output_file: String,

    #[arg(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    input_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Green,
    Blue,
}

fn sequence_to_group_id(t: u32) -> (u32, u32) {
    fn seq2idx(s: u32) -> u32 {
        match s {
            2 => 0,
            1 => 1,
            4 => 2,
            3 => 3,
            _ => unreachable!(),
        }
    }

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
    _sequence_number: u32,
    group: u32, // which group of 4 images this image belongs to, every group has 4 images
    id: u32,    // which image in the group this image is
    data: Mmap,
}

impl RawImage {
    fn new(path: &str) -> Self {
        let exif = read_exif(path);

        let file = std::fs::File::open(path).unwrap();
        let data = unsafe {
            MmapOptions::new()
                .offset(exif.offset as u64)
                .map(&file)
                .unwrap()
        };

        let gi = sequence_to_group_id(exif.sequence_number);

        Self {
            _path: path.to_string(),
            width: exif.width,
            height: exif.height,
            _sequence_number: exif.sequence_number,
            group: gi.0,
            id: gi.1,
            data,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> u16 {
        let offset = (y * self.width * 2 + x * 2) as usize;
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

fn merge_4(files: &[RawImage], x: u32, y: u32) -> image::Rgb<u16> {
    let mut px = image::Rgb([0u16, 0, 0]);

    for file in files {
        let val = file.pixel_offset(x, y) as u32;
        let color = file.color_offset(x, y);

        match color {
            Color::Red => px.0[0] += (val) as u16,
            Color::Green => px.0[1] += (val) as u16,
            Color::Blue => px.0[2] += (val) as u16,
        }
    }

    px
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

    if files
        .iter()
        .enumerate()
        .any(|(i, file)| file.id + file.group * 4 != i as u32)
    {
        panic!("some files are missing");
    }

    let imgbuf = match files.len() {
        4 => {
            info!("creating buffer");
            let mut imgbuf = image::ImageBuffer::new(files[0].width, files[0].height);

            info!("merging 4");
            imgbuf.par_enumerate_pixels_mut().for_each(|(x, y, pixel)| {
                *pixel = merge_4(&files[..4], x, y);
            });

            imgbuf
        }
        16 => {
            let groups = files.chunks(4).collect::<Vec<&[RawImage]>>();

            info!("creating buffer");
            let mut imgbuf = image::ImageBuffer::new(files[0].width * 2, files[0].height * 2);

            info!("merging 16");

            imgbuf.par_enumerate_pixels_mut().for_each(|(x, y, pixel)| {
                // 16 images mode works by doing the 4-way bayer merge 4 times but shifted by half a pixel in a 2x2 grid
                // the 2x2 grid is for each pixel, so the resulting image is quadrupled in size
                // +----+----+
                // | 0  | 1  |
                // +----+----+
                // | 2  | 3  |
                // +----+----+
                match (x % 2, y % 2) {
                    (0, 0) => *pixel = merge_4(groups[0], x / 2, y / 2),
                    (1, 0) => *pixel = merge_4(groups[1], x / 2, y / 2),
                    (0, 1) => *pixel = merge_4(groups[2], x / 2, y / 2),
                    (1, 1) => *pixel = merge_4(groups[3], x / 2, y / 2),
                    _ => unreachable!(),
                }
            });

            imgbuf
        }
        _ => panic!("unsupported number of files"),
    };

    info!("saving");
    imgbuf.save(&args.output_file).unwrap();

    info!("done in {:?}", now.elapsed());
}
