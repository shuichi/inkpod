use inkpod_image::{
    Channel, Filter, PixelFormat, PixelValue, TILE_SIZE, TileCoord, TileData, TileRaster,
    apply_filter,
};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let quick = std::env::args().any(|argument| argument == "--quick");
    let sparse_samples = if quick { 512_u32 } else { 2_048_u32 };
    let dense_side = if quick { 1_024_u32 } else { 2_048_u32 };

    let sparse_started = Instant::now();
    let mut sparse = TileRaster::new(
        inkpod_image::MAX_RASTER_DIMENSION,
        inkpod_image::MAX_RASTER_DIMENSION,
        PixelFormat::StraightRgba8,
    )
    .expect("maximum-dimension sparse raster must be valid");
    assert_eq!(sparse.allocated_tile_count(), 0);
    for index in 0..sparse_samples {
        let x = index * TILE_SIZE;
        let y = (index * 8_191) % inkpod_image::MAX_RASTER_DIMENSION;
        sparse
            .set_pixel(x, y, PixelValue::Rgba([1, 2, 3, 255]), 1)
            .expect("bounded sparse pixel write must succeed");
    }
    let sparse_tiles = sparse.allocated_tile_count();
    assert!(sparse_tiles <= sparse_samples as usize);
    let mut sparse_clone = sparse.clone();
    assert_eq!(sparse_clone.allocated_tile_count(), sparse_tiles);
    sparse_clone
        .set_pixel(0, 0, PixelValue::Rgba([9, 8, 7, 255]), 2)
        .expect("copy-on-write edit must succeed");
    assert_eq!(
        sparse
            .pixel(0, 0)
            .expect("source pixel must remain readable"),
        PixelValue::Rgba([1, 2, 3, 255])
    );
    black_box(sparse_clone.checksum());
    let sparse_elapsed = sparse_started.elapsed();

    let dense_started = Instant::now();
    let mut dense = TileRaster::new(dense_side, dense_side, PixelFormat::StraightRgba8)
        .expect("dense benchmark raster must be valid");
    let tiles_per_axis = dense_side / TILE_SIZE;
    for tile_y in 0..tiles_per_axis {
        for tile_x in 0..tiles_per_axis {
            let mut bytes = vec![0_u8; (TILE_SIZE * TILE_SIZE * 4) as usize];
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[32, 64, 96, 255]);
            }
            dense
                .insert_tile(TileData {
                    coord: TileCoord {
                        x: tile_x,
                        y: tile_y,
                    },
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                    bytes,
                    revision: 1,
                })
                .expect("dense benchmark tile must be valid");
        }
    }
    let filtered = apply_filter(
        &dense,
        None,
        &Filter::Invert {
            channel: Channel::Rgb,
        },
        2,
    )
    .expect("bounded dense filter must succeed");
    assert_eq!(
        filtered.allocated_tile_count(),
        dense.allocated_tile_count()
    );
    black_box(filtered.checksum());
    let dense_elapsed = dense_started.elapsed();

    println!(
        "inkpod-large-document sparse={}x{} samples={} tiles={} elapsed_ms={} dense={}x{} bytes={} elapsed_ms={}",
        sparse.width(),
        sparse.height(),
        sparse_samples,
        sparse_tiles,
        sparse_elapsed.as_millis(),
        dense_side,
        dense_side,
        u64::from(dense_side) * u64::from(dense_side) * 4,
        dense_elapsed.as_millis()
    );
}
