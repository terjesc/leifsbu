use crate::pathfinding::RoadPath;
use crate::types::*;

use image::{GrayImage, ImageBuffer, Luma};
use imageproc::contrast::stretch_contrast;
use imageproc::distance_transform::Norm;
use imageproc::drawing::draw_line_segment_mut;
use imageproc::morphology::*;
use imageproc::region_labelling::{connected_components, Connectivity};
use imageproc::stats::histogram;
use imageproc::template_matching::{Extremes, find_extremes};
use num_integer::Roots;
use std::cmp::max;

// Plot partitioning…
// Let's figure out how to do that!
//
// Need some parameters for desired plot sizes, shapes, etc.
// Let's start by only contemplating rectangular or 'rectangular-similar' plots.
// Possible parameters for plots:
//
// * Width (along road): min, preferred, max
// * Depth (perpendicular to road): min, preferred, max
// * Area: min, preferred, max
//
// Rules for the actual structures on the plots are not important here,
// but constraints for such structures may influence plot properties.
//
// The blocks themselves may also have some parameters:
//
// * Width (shortest side): min, preferred, max
// * Length (longest side): min, preferred, max
// * Area: min, preferred, max
//
// Parameters should be such that a city block can contain multiple plots.
//
// There need to be some borders that the city blocks/plots get generated within:
// * Road
// * Street
// * Non-traversable border types (water, cliff, wall, etc.)
//
// Borders follow paths (series of points). Maybe they have some thickness as well?
//
// Borders are also used between individual city blocks (usually as roads, one would
// presume) in order to split the area into multiple city blocks.
//
// Each border type has an "influence area" along it, which corresponds to (half) the
// thickness of the border itself, plus the (min/max/default) plot depth for the type
// of plot that should go along that border. Non-traversable border types do not have
// any plot assigned, and thus the only contribution is from the with (if any) of the
// border object/feature itself.
//
// Algorithm:
// 0) In advance, the area to be filled with city blocks / plots has been determined.
// 
// The end criteria for the algorithm is that all of the given area is "covered" by the
// influence area of the borders.
// 
// 1) Mark the influence area around all roads (and other borders)..
// 2) If everything within the area is marked, we are done.
// 3) Add streets in order to cover some unmarked area, and repeat from 2)
//
// For step 3, different strategies can be used for figuring out where to build the
// streets. Multiple streets can be built in parallel. Roads can be added. Other types
// of borders can also be used.
// 
// Strategies for getting roads, in step 3:
// A) Along non-traversible borders, to the side in need of filling, build a street (or
//    road) in parallel to the non-traversible border, connecting to a traversible
//    border in both ends (possibly continuing along multiple non-traversible borders
//    on the way.) Then use a different strategy for filling in the remaining areas,
//    which may be a method only capable of filling areas that are only surrounded by
//    traversible borders.
// B) (Traversible borders only?) Get the x and z extent of an unfilled area, and build
//    axis-aligned streets along the shortest axis, evenly (or haphazardly) spread apart,
//    such that all the unfilled area is filled. For streets longer than a threshold,
//    build streets across the other direction as well.
// C) Build streets in parallel to roads, then parallel to those streets, etc, until
//    all area is covered. Might need some additional road building perpendicular to
//    the already built roads and streets.
// D) Start streets perpendicular to the roads, let them grow outwards (more or less
//    organically) until they reach another road/street/border.
// 
//
// Overall plan is thus:
//
// 1) Algorithm for splitting an area into city blocks, using existing roads within
//    the area, adding roads/streets/paths to it in such a manner that all of the
//    area is within a minimal distance from roads/streets/paths.
// 2) Algorithm for dividing city blocks into plots.

// Roads (+ other borders?) + Circumference + parameters
// -> Streets (+ other borders?) (+areas?)

/// Given a city area and existing roads, find a set of streets such that all area
/// within the city area are within reasonable distance from a road or street.
pub fn city_block_divide(circumference: &Snake, town_center: &Point, roads: &Vec<RoadPath>)
-> Vec<Snake> {
    const COVERED: Luma<u8> = Luma([255u8]);

    const ROAD_COVERAGE_RADIUS: u8 = 10;
    const ROAD_HALF_WIDTH: u8 = 2;
    
    const STREET_COVERAGE_RADIUS: u8 = 8;
    const STREET_HALF_WIDTH: u8 = 1;

    const TOWN_BORDER_HALF_WIDTH: u8 = 2;
    const TOWN_BORDER_DISTANCE_TO_CLOSE_STREET: i64 =
            (STREET_HALF_WIDTH + TOWN_BORDER_HALF_WIDTH) as i64;
    const TOWN_BORDER_DISTANCE_TO_FAR_STREET: i64 =
            (STREET_COVERAGE_RADIUS + TOWN_BORDER_HALF_WIDTH) as i64;

    const UNCOVERED_AREA_SIZE_THRESHOLD: u32 = 32;

    // Limit the area of operation to what is strictly necessary
    let (offset, dimensions) = snake_bounding_box(circumference);
    println!("Town circumference bounding box has offset {:?} and size {:?}",
             offset, dimensions);
    let offset_town_center = (
        (town_center.0 - offset.0) as u32,
        (town_center.1 - offset.1) as u32,
    );

    // The full loop of the circumference
    let mut full_circumference = circumference.clone();
    full_circumference.push(circumference[0]);

    // Create an image of the area of operation.
    let mut settlement_stencil = image::ImageBuffer::new(
        dimensions.0 as u32,
        dimensions.1 as u32,
    );

    // Mark the town boundary as covered
    draw_offset_snake(&mut settlement_stencil, &full_circumference, &offset, COVERED);
    settlement_stencil.save("P-01 circumference.png").unwrap();

    // Mark the outside of the town as covered
    let components = connected_components(&settlement_stencil, Connectivity::Four, COVERED);
    let inside_value = components[offset_town_center];
    for (x, z, value) in components.enumerate_pixels() {
        if inside_value != *value {
            settlement_stencil.put_pixel(x, z, COVERED);
        }
    }
    settlement_stencil.save("P-02 area stencil.png").unwrap();

    // Mark roads
    let mut infrastructure = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
    for road in roads {
        draw_offset_road(&mut infrastructure, road, &offset, COVERED);
    }
    infrastructure.save("P-03 existing infrastructure.png").unwrap();


    // Get map of initial areas as divided by initial roads
    let initial_areas = combine_max(&settlement_stencil, &infrastructure);
    initial_areas.save("P-04 initial areas.png").unwrap();

    // Find distinct initial areas
    let initial_areas = image_u32_to_u8(
        &connected_components(&initial_areas, Connectivity::Four, COVERED)
    );
    let Extremes{max_value: full_area_count, ..} = find_extremes(&initial_areas);
    println!("Found {} distinct existing areas.", full_area_count);

    // NB Only for generating nice debug visuals...
    let areas = stretch_contrast(&initial_areas, 0u8, full_area_count);
    areas.save("P-05 full areas.png").unwrap();


    // Mark areas close to roads as covered
    let road_coverage = dilate(&infrastructure, Norm::LInf, ROAD_COVERAGE_RADIUS);
    road_coverage.save("P-06 close to road.png").unwrap();

    // Get map of initial coverage
    let initial_coverage = combine_max(&settlement_stencil, &road_coverage);
    initial_coverage.save("P-07 initial coverage.png").unwrap();

    // Find distinct uncovered areas
    let uncovered_areas = image_u32_to_u8(
        &connected_components(&initial_coverage, Connectivity::Four, COVERED)
    );
    let Extremes{max_value: area_count, ..} = find_extremes(&uncovered_areas);
    println!("Found {} distinct areas that may need coverage.", area_count);

    // NB Only for generating nice debug visuals...
    let areas = stretch_contrast(&uncovered_areas, 0u8, area_count);
    areas.save("P-08 areas.png").unwrap();

    // Find the size of each area
    let stats = histogram(&uncovered_areas);

    // NB Only for debug prints...
    println!("Area statistics:");
    println!("Background size:\t{}", stats.channels[0][0]);
    for area_index in 1..stats.channels[0].len() {
        let size = stats.channels[0][area_index];
        if size > 0 {
            println!("Area {} size:\t{}", area_index, size);
        }
    }


    // TODO refactor all this normal stuff into separate functions
    // Generate Snakes along wall. To be used for filling uncovered area later.
    // First find normals...
    let mut normals: Vec<(i64, i64)> = full_circumference
        .windows(2)
        .map(|points| {
            // Find normals...
            (points[1].1 as i64 - points[0].1 as i64, points[0].0 as i64 - points[1].0 as i64)
        })
        .collect();
    normals.push(normals[0]);
    println!("Normals\n{:?}\n", normals);

    // ...then scale the normals...
    let scaled_normalized_normals: Vec<(i64, i64)> = normals
        .windows(2)
        .map(|normals| {
            let normal_0_scale = max(
                ((normals[0].0 * 10).pow(2) + (normals[0].1 * 10).pow(2)).sqrt(),
                1,
            );
            let normal_1_scale = max(
                ((normals[1].0 * 10).pow(2) + (normals[1].1 * 10).pow(2)).sqrt(),
                1,
            );

            (
                (normals[0].0 * 100) / normal_0_scale + (normals[1].0 * 100) / normal_1_scale,
                (normals[0].1 * 100) / normal_0_scale + (normals[1].1 * 100) / normal_1_scale,
            )
        })
        .collect();
    println!("Scaled normalized normals\n{:?}\n", scaled_normalized_normals);

    let mut street_close_to_border = Vec::new();
    let mut street_far_from_border = Vec::new();

    // ...then add normals to border.
    for (index, normal) in scaled_normalized_normals.iter().enumerate() {
        let close_offset = (
            (normal.0 * -TOWN_BORDER_DISTANCE_TO_CLOSE_STREET) / 20,
            (normal.1 * -TOWN_BORDER_DISTANCE_TO_CLOSE_STREET) / 20,
        );
        let far_offset = (
            (normal.0 * -TOWN_BORDER_DISTANCE_TO_FAR_STREET) / 20,
            (normal.1 * -TOWN_BORDER_DISTANCE_TO_FAR_STREET) / 20,
        );

        street_close_to_border.push((
            (full_circumference[index + 1].0 as i64 + close_offset.0) as usize,
            (full_circumference[index + 1].1 as i64 + close_offset.1) as usize,
        ));
        street_far_from_border.push((
            (full_circumference[index + 1].0 as i64 + far_offset.0) as usize,
            (full_circumference[index + 1].1 as i64 + far_offset.1) as usize,
        ));
    }

    // NB Only for making nice debug visuals...
    let mut wall_roads = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
    draw_offset_snake(&mut wall_roads, &street_close_to_border, &offset, COVERED);
    draw_offset_snake(&mut wall_roads, &street_far_from_border, &offset, COVERED);
    wall_roads.save("P-09 wall roads.png").unwrap();


    let mut streets = Vec::new();

    // Take care of uncovered areas
    for area_index in 1..stats.channels[0].len() {
        let size = stats.channels[0][area_index];

        // If too small, then not worth it...
        if size < UNCOVERED_AREA_SIZE_THRESHOLD {
            continue;
        }

        // Get the uncovered stencil for only this area
        let mut area_stencil = stencil_from_value(&uncovered_areas, Luma([area_index as u8]));

        // Get the full stencil for only this area
        let location = location_from_value(&uncovered_areas, Luma([area_index as u8]))
            .unwrap();
        let value = initial_areas[location];

        // NB If dilating too much, with the test world the far path disappears.
        //    It probably got very short due to winding in and out of the area,
        //    before doing the proper tour... This may be happening on some maps,
        //    regardless of dilation setting, and should be further investigated.
        //    If dilating too little, paths do not properly connect.
        let full_area_stencil = dilate(
            &stencil_from_value(&initial_areas, value),
            Norm::LInf,
            2,
        );

        area_stencil.save(format!("P-10 area {:0>2}.png", area_index)).unwrap();
        full_area_stencil.save(format!("P-10 full area {:0>2}.png", area_index)).unwrap();

        //  Find possible path close by wall
        let close_path = sub_snake(&street_close_to_border, &full_area_stencil, &offset);
        println!("Close path:\n{:?}\n", close_path);

        // Find possible path further from wall
        let far_path = sub_snake(&street_far_from_border, &full_area_stencil, &offset);
        println!("Far path:\n{:?}\n", far_path);

        // NB Only for making nice debug visuals...
        let mut wall_roads = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
        draw_offset_snake(&mut wall_roads, &close_path, &offset, COVERED);
        draw_offset_snake(&mut wall_roads, &far_path, &offset, COVERED);
        wall_roads.save(format!("P-10 wall roads {:0>2}.png", area_index)).unwrap();

        // Find coverage area for found close path
        let mut close_cover = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
        draw_offset_snake(&mut close_cover, &close_path, &offset, COVERED);
        dilate_mut(&mut close_cover, Norm::LInf, STREET_COVERAGE_RADIUS);

        // If it fully covers, add it and go on to next area.
        if fully_covers(&area_stencil, &close_cover) {
            streets.push(close_path);
            continue;
        }

        // Find coverage area for found far path
        let mut far_cover = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
        draw_offset_snake(&mut far_cover, &far_path, &offset, COVERED);
        dilate_mut(&mut far_cover, Norm::LInf, STREET_COVERAGE_RADIUS);

        // If it fully covers, add it and go on to next area.
        if fully_covers(&area_stencil, &far_cover) {
            streets.push(far_path);
            continue;
        }

        // TODO Decide whether the "far" road is the best approach.
        //      Choosing the "best" fit may be an option.
        //      Choosing the "close" road also seem a safer option wrt. weird far path shapes.
        // Put in the "far" road alternative, as it most likely covers the most area
        streets.push(far_path);
        remove_cover(&mut area_stencil, &far_cover);

        // TODO get bounding box for remaining area
        // TODO see if x or z dimension is the shortest
        // TODO calculate covering spread across longest dimension
        // TODO put streets from infrastructure to infrastructure across
        //      the shortes dimension, at interval of the calculated spread
    }

    // Some final visual debug
    for street in &streets {
        draw_offset_snake(&mut infrastructure, &street, &offset, COVERED);
    }
    infrastructure.save("P-11 infrastructure.png").unwrap();

    streets
}

fn snake_bounding_box(snake: &Snake) -> (Point, Point) {
    let offset = snake
        .iter()
        .copied()
        .reduce(|a, b| {
            (
                if a.0 < b.0 { a.0 } else { b.0 },
                if a.1 < b.1 { a.1 } else { b.1 },
            )
        })
        .unwrap();
    let dimensions_plus_offset = snake
        .iter()
        .copied()
        .reduce(|a, b| {
            (
                if a.0 > b.0 { a.0 } else { b.0 },
                if a.1 > b.1 { a.1 } else { b.1 },
            )
        })
        .unwrap();
    let dimensions = (
        dimensions_plus_offset.0 - offset.0,
        dimensions_plus_offset.1 - offset.1,
    );

    (offset, dimensions)
}

fn draw_offset_snake(
    image: &mut GrayImage,
    snake: &Snake,
    offset: &Point,
    colour: Luma<u8>,
) {
    if snake.len() <= 1 {
        return;
    }

    snake[1..].iter().fold(snake[0], |a, b| {
        draw_line_segment_mut(
            image,
            ((a.0 - offset.0) as f32, (a.1 - offset.1) as f32),
            ((b.0 - offset.0) as f32, (b.1 - offset.1) as f32),
            colour,
        );
        *b
    });
}

fn draw_offset_road(
    image: &mut GrayImage,
    road: &RoadPath,
    offset: &Point,
    colour: Luma<u8>,
) {
    road[1..].iter().fold(road[0], |a, b| {
        draw_line_segment_mut(
            image,
            (
                (a.coordinates.0 - offset.0 as i64) as f32,
                (a.coordinates.2 - offset.1 as i64) as f32,
            ),
            (
                (b.coordinates.0 - offset.0 as i64) as f32,
                (b.coordinates.2 - offset.1 as i64) as f32,
            ),
            colour,
        );
        *b
    });
}

fn combine_max_mut(a: &mut GrayImage, b: &GrayImage) {
    for (x, z, value) in b.enumerate_pixels() {
        let Luma([a_value]) = a[(x, z)];
        let Luma([b_value]) = value;
        if a_value < *b_value {
            a.put_pixel(x, z, *value);
        }
    }
}

fn combine_max(a: &GrayImage, b: &GrayImage) -> GrayImage {
    let mut image = a.clone();
    combine_max_mut(&mut image, b);
    image
}

fn image_u32_to_u8(image_u32: &ImageBuffer<Luma<u32>, Vec<u32>>) -> GrayImage {
    let (x_len, z_len) = image_u32.dimensions();
    let mut image_u8 = GrayImage::new(x_len, z_len);

    for (x, z, value) in image_u32.enumerate_pixels() {
        let Luma([value]) = *value;
        image_u8.put_pixel(x, z, Luma([value as u8]));
    }
    image_u8
}

fn stencil_from_value(image: &GrayImage, foreground: Luma<u8>) -> GrayImage {
    let (x_len, z_len) = image.dimensions();
    let mut output_image = GrayImage::new(x_len, z_len);

    for (x, z, value) in image.enumerate_pixels() {
        if *value == foreground {
            output_image.put_pixel(x, z, Luma([255u8]));
        }
    }
    output_image
}

fn location_from_value(image: &GrayImage, foreground: Luma<u8>) -> Option<(u32, u32)> {
    for (x, z, value) in image.enumerate_pixels() {
        if *value == foreground {
            return Some((x, z));
        }
    }
    None
}

/// Make a snake out of the points in `snake` that are within the area marked by `stencil`.
fn sub_snake(snake: &Snake, stencil: &GrayImage, offset: &Point) -> Snake {
    let mut new_snake = Vec::new();
    let mut snake_started: bool = false;
    let mut snake_ended: bool = false;

    for i in 0..(snake.len() * 2)-1 {
        let coordinates = snake[i % snake.len()];
        let inside = if Luma([255u8]) == stencil[(
            (coordinates.0 as i64 - offset.0 as i64) as u32,
            (coordinates.1 as i64 - offset.1 as i64) as u32,
        )] {
            true
        } else {
            false
        };

        if !snake_ended {
            if !inside {
                snake_ended = true;
            }
        } else if inside {
            snake_started = true;
            new_snake.push(coordinates);
        } else if snake_started {
            break;
        }
    }

    new_snake
}

fn fully_covers(under: &GrayImage, covering: &GrayImage) -> bool {
    for (x, z, value) in under.enumerate_pixels() {
        if *value == Luma([255u8]) {
            if covering[(x, z)] != Luma([255u8]) {
                return false;
            }
        }
    }
    true
}

fn remove_cover(under: &mut GrayImage, covering: &GrayImage) {
    for (x, z, value) in covering.enumerate_pixels() {
        if *value == Luma([255u8]) {
            under.put_pixel(x, z, Luma([0u8]));
        }
    }
}
