extern crate image;
extern crate mcprogedit;

use image::{GrayImage, RgbImage};
use image::imageops::filter3x3;
use imageproc::contrast::threshold;
use imageproc::distance_transform::Norm;
use mcprogedit::block::*;
use mcprogedit::height_map::HeightMap;
use mcprogedit::world_excerpt::WorldExcerpt;

pub struct Features {
    // Height maps
    pub height_map: HeightMap,
    pub terrain_height_map: HeightMap,

    // Coloured map
    pub coloured_map: RgbImage,

    // Gradients
    pub heights: GrayImage,
    pub terrain: GrayImage,
    pub water_depth: GrayImage,
    pub sobel_relief: GrayImage,
    pub scharr: GrayImage,
    pub scharr_cleaned: GrayImage,

    // Stencils
    pub hilltop: GrayImage,
    pub water: GrayImage,
    pub fertile: GrayImage,
    pub sand: GrayImage,
    pub gravel: GrayImage,
    pub exposed_ore: GrayImage,
    pub forest: GrayImage,
}

impl Features {
    pub fn dimensions(&self) -> (usize, usize) {
        self.height_map.dim()
    }

    pub fn new_from_world_excerpt(excerpt: &WorldExcerpt) -> Self {
        let (x_len, y_len, z_len) = excerpt.dim();

        let height_map = excerpt.height_map();

        // Create a bitmap showing the (raw) height map.
        let mut heights = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len {
            for z in 0..z_len {
                let value = height_map.height_at((x as usize, z as usize)).unwrap_or(0) as u8;
                heights.put_pixel(x as u32, z as u32, image::Luma([value]));
            }
        }
        //heights.save("01 raw height map.png").unwrap();

        // Update the height map not to include foilage.
        let mut terrain_height_map = height_map.clone();
        for x in 0..x_len as usize {
            for z in 0..z_len as usize {
                let y = terrain_height_map.height_at((x, z)).unwrap_or(y_len as u32);

                for y in (0..y).rev() {
                    if let Some(block) = excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                        if !block.is_foilage() {
                            terrain_height_map.set_height((x, z), y as u32 + 1);
                            break;
                        }
                    }
                }
            }
        }

        let mut terrain = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len  as usize {
            for z in 0..z_len as usize {
                let value = terrain_height_map.height_at((x, z)).unwrap_or(0) as u8;
                terrain.put_pixel(x as u32, z as u32, image::Luma([value]));
            }
        }
        //terrain.save("02 height map without foilage.png").unwrap();


        // Coloured land heightmap with water
        let mut colour_img = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as usize {
            for z in 0..z_len as usize {
                let y = terrain_height_map.height_at((x, z)).unwrap_or(0) as i64;
                let pixel = match excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                    Some(Block::WaterSource) => image::Rgb([0u8, 0u8, 255u8]),
                    _ => image::Rgb([0u8, (y as u8).saturating_sub(60) * 3, 0u8]),
                };
                colour_img.put_pixel(x as u32, z as u32, pixel);
            }
        }
        //colour_img.save("03 coloured map.png").unwrap();


        // Various kernels for slope and edge detection
        let horizontal_sobel = [1.0f32, 0.0, -1.0,
                                2.0, 0.0, -2.0,
                                1.0, 0.0, -1.0];
        let vertical_sobel = [1.0f32, 2.0, 1.0,
                              0.0, 0.0, 0.0,
                              -1.0, -2.0, -1.0];
        let horizontal_reverse_sobel = [-1.0f32, 0.0, 1.0,
                                        -2.0, 0.0, 2.0,
                                        -1.0, 0.0, 1.0];
        let vertical_reverse_sobel = [-1.0f32, -2.0, -1.0,
                                      0.0, 0.0, 0.0,
                                      1.0, 2.0, 1.0];

        const F: f32 = 0.5;
        let horizontal_scharr = [3.0f32*F, 0.0*F, -3.0*F,
                                 10.0*F, 0.0*F, -10.0*F,
                                 3.0*F, 0.0*F, -3.0*F];
        let vertical_scharr = [3.0f32*F, 10.0*F, 3.0*F,
                               0.0*F, 0.0*F, 0.0*F,
                               -3.0*F, -10.0*F, -3.0*F];
        let horizontal_reverse_scharr = [-3.0f32*F, 0.0*F, 3.0*F,
                                 -10.0*F, 0.0*F, 10.0*F,
                                 -3.0*F, 0.0*F, 3.0*F];
        let vertical_reverse_scharr = [-3.0f32*F, -10.0*F, -3.0*F,
                               0.0*F, 0.0*F, 0.0*F,
                               3.0*F, 10.0*F, 3.0*F];

        let gauss = [1.0f32/8.0, 1.0/4.0, 1.0/8.0,
                     1.0/4.0, 1.0/2.0, 1.0/4.0,
                     1.0/8.0, 1.0/4.0, 1.0/8.0];
        let edge = [-8.0f32, -8.0, -8.0,
                    -8.0, 64.0, -8.0,
                    -8.0, -8.0, -8.0];

        let horizontal_img = image::imageops::filter3x3(&terrain, &horizontal_sobel);
        let vertical_img = image::imageops::filter3x3(&terrain, &vertical_sobel);
        let horizontal_reverse_img = image::imageops::filter3x3(&terrain, &horizontal_reverse_sobel);
        let vertical_reverse_img = image::imageops::filter3x3(&terrain, &vertical_reverse_sobel);
        
        let edge_img = image::imageops::filter3x3(&terrain, &gauss);
        let edge_img = image::imageops::filter3x3(&edge_img, &gauss);
        let edge_img = image::imageops::filter3x3(&edge_img, &gauss);
        let edge_img = image::imageops::filter3x3(&edge_img, &edge);


        // TODO Save only if debug images is enabled
        horizontal_img.save("04a horizontal sobel.png").unwrap();
        vertical_img.save("04b vertical sobel.png").unwrap();
        edge_img.save("04c edge.png").unwrap();

        // Full Sobel
        let mut sobel_relief = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let image::Luma([horizontal_value]) = horizontal_img[(x, z)];
                let image::Luma([vertical_value]) = vertical_img[(x, z)];
                let image::Luma([horizontal_reverse_value]) = horizontal_reverse_img[(x, z)];
                let image::Luma([vertical_reverse_value]) = vertical_reverse_img[(x, z)];
                let value = std::u8::MAX / 2
                    - horizontal_value / 3
                    - vertical_value / 3
                    + horizontal_reverse_value / 3
                    + vertical_reverse_value / 3;
                //let value = value * 192;
                sobel_relief.put_pixel(x, z, image::Luma([value]));
            }
        }

        // TODO Save only if debug images is enabled
        sobel_relief.save("04d sobel.png").unwrap();

        // Full Scharr
        let horizontal_img = image::imageops::filter3x3(&terrain, &horizontal_scharr);
        let vertical_img = image::imageops::filter3x3(&terrain, &vertical_scharr);
        let horizontal_reverse_img = image::imageops::filter3x3(&terrain, &horizontal_reverse_scharr);
        let vertical_reverse_img = image::imageops::filter3x3(&terrain, &vertical_reverse_scharr);

        let mut scharr = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let image::Luma([horizontal_value]) = horizontal_img[(x, z)];
                let image::Luma([vertical_value]) = vertical_img[(x, z)];
                let image::Luma([horizontal_reverse_value]) = horizontal_reverse_img[(x, z)];
                let image::Luma([vertical_reverse_value]) = vertical_reverse_img[(x, z)];
                let value = horizontal_value
                    .saturating_add(vertical_value)
                    .saturating_add(horizontal_reverse_value)
                    .saturating_add(vertical_reverse_value);
                scharr.put_pixel(x, z, image::Luma([value as u8]));
            }
        }

        // TODO Save only if debug images is enabled
        scharr.save("04e scharr.png").unwrap();

        // Hilltops (double scharr)
        const THRESHOLD: u8 = 9;
        let horizontal_img = threshold(&horizontal_img, THRESHOLD);
        let horizontal_img = filter3x3(&horizontal_img, &horizontal_reverse_scharr);
        let vertical_img = threshold(&vertical_img, THRESHOLD);
        let vertical_img = filter3x3(&vertical_img, &vertical_reverse_scharr);
        let horizontal_reverse_img = threshold(&horizontal_reverse_img, THRESHOLD);
        let horizontal_reverse_img = filter3x3(&horizontal_reverse_img, &horizontal_scharr);
        let vertical_reverse_img = threshold(&vertical_reverse_img, THRESHOLD);
        let vertical_reverse_img = filter3x3(&vertical_reverse_img, &vertical_scharr);

        let mut hilltop = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let image::Luma([horizontal_value]) = horizontal_img[(x, z)];
                let image::Luma([vertical_value]) = vertical_img[(x, z)];
                let image::Luma([horizontal_reverse_value]) = horizontal_reverse_img[(x, z)];
                let image::Luma([vertical_reverse_value]) = vertical_reverse_img[(x, z)];
                let value = horizontal_value
                    .saturating_add(vertical_value)
                    .saturating_add(horizontal_reverse_value)
                    .saturating_add(vertical_reverse_value);
                hilltop.put_pixel(x, z, image::Luma([value as u8]));
            }
        }
        let scharr_mask = threshold(&scharr, THRESHOLD);
        let hilltop = imageproc::morphology::dilate(&hilltop, Norm::LInf, 1);
        let hilltop = imageproc::map::map_colors2(
            &hilltop,
            &scharr_mask,
            |p, q| {
                image::Luma([p[0].saturating_sub(q[0])])
            },
        );

        // TODO Save only if debug images is enabled
        hilltop.save("04f hilltop.png").unwrap();

        // scharr with low values removed
        let mut scharr_cleaned = scharr.clone();
        const TRESHOLD: u8 = 32;
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let image::Luma([value]) = scharr[(x, z)];
                if value < TRESHOLD {
                    scharr_cleaned.put_pixel(x, z, image::Luma([0u8]));
                }
            }
        }

        // TODO Save only if debug images is enabled
        scharr_cleaned.save("04f scharr cleaned.png").unwrap();

        // Various features
        let mut water = image::ImageBuffer::new(x_len as u32, z_len as u32);
        let mut fertile = image::ImageBuffer::new(x_len as u32, z_len as u32);
        let mut sand = image::ImageBuffer::new(x_len as u32, z_len as u32);
        let mut gravel = image::ImageBuffer::new(x_len as u32, z_len as u32);
        let mut exposed_ore = image::ImageBuffer::new(x_len as u32, z_len as u32);

        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let y = terrain_height_map.height_at((x as usize, z as usize)).unwrap_or(1);
                if let Some(block) = excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                    match block {
                        Block::WaterSource
                        | Block::Water { .. } => water.put_pixel(x, z, image::Luma([255u8])),
                        _ => if let Some(block) = excerpt.block_at((x as i64, y as i64 - 1, z as i64).into()) {
                            match block {
                                Block::CoarseDirt
                                | Block::Dirt
                                | Block::Farmland { .. }
                                | Block::GrassBlock
                                | Block::Podzol => fertile.put_pixel(x, z, image::Luma([255u8])),
                                Block::RedSand
                                | Block::Sand => sand.put_pixel(x, z, image::Luma([255u8])),
                                Block::Gravel => gravel.put_pixel(x, z, image::Luma([255u8])),
                                Block::CoalOre
                                | Block::DiamondOre
                                | Block::EmeraldOre
                                | Block::GoldOre
                                | Block::IronOre
                                | Block::LapisLazuliOre
                                | Block::RedstoneOre => exposed_ore.put_pixel(x, z, image::Luma([255u8])),
                                _ => (),
                            }
                        },
                    }
                }
            }
        }

        // TODO Save only if debug images is enabled
        water.save("05a water.png").unwrap();
        fertile.save("05b fertile land.png").unwrap();
        sand.save("05c sand.png").unwrap();
        gravel.save("05d gravel.png").unwrap();
        exposed_ore.save("05e exposed ore.png").unwrap();

        // Forests
        let mut forest = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                let y = height_map.height_at((x as usize, z as usize)).unwrap_or(1) - 1;
                if let Some(block) = excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                    match block {
                        Block::Leaves { .. }
                        | Block::Log(_) => forest.put_pixel(x, z, image::Luma([255u8])),
                        _ => (),
                    }
                }
            }
        }

        // TODO Save only if debug images is enabled
        forest.save("05f forest.png").unwrap();

        // Water depth
        let mut water_depth = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len {
            for z in 0..z_len {
                let start = terrain_height_map.height_at((x, z)).unwrap();
                let end = height_map.height_at((x, z)).unwrap();
                for y in start..= end {
                    if let Some(Block::WaterSource) = excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                    } else {
                        let depth = y as u8 - start as u8;
                        water_depth.put_pixel(x as u32, z as u32, image::Luma([depth]));
                        break;
                    }
                }
            }
        }

        // TODO Save only if debug images is enabled
        water_depth.save("06 water depth.png").unwrap();

        Self {
            // Height maps
            height_map,
            terrain_height_map,

            // Coloured map
            coloured_map: colour_img,

            // Gradients
            heights,
            terrain,
            water_depth,
            sobel_relief,
            scharr,
            scharr_cleaned,

            // Stencils
            hilltop,
            water,
            fertile,
            sand,
            gravel,
            exposed_ore,
            forest,
        }
    }
}
