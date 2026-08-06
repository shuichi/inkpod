use super::*;

fn color(depth: u32, value: u16) -> ApplicationColor {
    ApplicationColor {
        depth,
        red: value,
        green: value / 2,
        blue: value / 3,
        alpha: value,
    }
}

#[test]
fn palette_current_version_round_trip_and_malformed_rejection() {
    let palette = FilePalette {
        colors: vec![color(8, 255), color(16, 65_535)],
    };
    let bytes = encode_palette(&palette).unwrap();
    assert_eq!(&bytes[..8], b"INKPAL1\0");
    assert_eq!(decode_palette(&bytes).unwrap(), palette);

    let mut wrong_version = bytes.clone();
    wrong_version[6] = b'0';
    assert!(decode_palette(&wrong_version).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_palette(&trailing).is_err());
    let invalid = FilePalette {
        colors: vec![color(8, 256)],
    };
    assert!(encode_palette(&invalid).is_err());
}

#[test]
fn color_chart_current_version_round_trip_and_malformed_rejection() {
    let chart = FileColorChart {
        entries: vec![
            FileColorChartEntry {
                color: color(8, 255),
                name: "Smoke Blue".to_owned(),
            },
            FileColorChartEntry {
                color: color(16, 65_535),
                name: "青".to_owned(),
            },
        ],
    };
    let bytes = encode_color_chart(&chart).unwrap();
    assert_eq!(&bytes[..8], b"INKCHT1\0");
    assert_eq!(decode_color_chart(&bytes).unwrap(), chart);

    let mut invalid_utf8 = bytes.clone();
    *invalid_utf8.last_mut().unwrap() = 0xff;
    assert!(decode_color_chart(&invalid_utf8).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_color_chart(&trailing).is_err());
}

#[test]
fn application_data_atomic_save_replaces_existing_files_without_temporary_leaks() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-application-data-test-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let palette_path = directory.join("colors.inkpalette");
    let chart_path = directory.join("colors.inkchart");

    fs::write(&palette_path, b"sentinel palette").unwrap();
    let palette = FilePalette {
        colors: vec![color(8, 255)],
    };
    save_palette_atomic(&palette_path, &palette).unwrap();
    assert_eq!(read_palette(&palette_path).unwrap(), palette);

    fs::write(&chart_path, b"sentinel chart").unwrap();
    let chart = FileColorChart {
        entries: vec![FileColorChartEntry {
            color: color(16, 65_535),
            name: "Replacement".to_owned(),
        }],
    };
    save_color_chart_atomic(&chart_path, &chart).unwrap();
    assert_eq!(read_color_chart(&chart_path).unwrap(), chart);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);

    fs::remove_file(palette_path).unwrap();
    fs::remove_file(chart_path).unwrap();
    fs::remove_dir(directory).unwrap();
}
