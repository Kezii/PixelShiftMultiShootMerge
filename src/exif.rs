use std::process::Command;

pub struct ExifData {
    pub width: u32,
    pub height: u32,
    pub sequence_number: u32,
    pub offset: u32,
}

pub fn read_exif(path: &str) -> ExifData {
    let exifs = Command::new("exiftool")
        .arg(path)
        .output()
        .expect("failed to execute process");

    let exifs = String::from_utf8_lossy(&exifs.stdout);

    let exifs = exifs.lines().map(|line| {
        let mut parts = line.splitn(2, ':');
        let key = parts.next().unwrap().trim().to_string();
        let value = parts.next().unwrap().trim().to_string();
        (key, value)
    });

    let mut exif_data = ExifData {
        width: 0,
        height: 0,
        sequence_number: 0,
        offset: 0,
    };

    for (key, value) in exifs {
        match key.as_str() {
            "Strip Offsets" => exif_data.offset = value.parse::<u32>().unwrap(),
            "Image Width" => exif_data.width = value.parse::<u32>().unwrap(),
            "Image Height" => exif_data.height = value.parse::<u32>().unwrap(),
            "Sequence Number" => exif_data.sequence_number = value.parse::<u32>().unwrap(),
            _ => (),
        }
    }

    if exif_data.width == 0
        || exif_data.height == 0
        || exif_data.sequence_number == 0
        || exif_data.offset == 0
    {
        panic!("Failed to read exif data");
    }

    exif_data
}
