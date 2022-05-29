use crate::geometry;
use crate::pathfinding;
use crate::pathfinding::{road_path_from_snake, snake_from_road_path, RoadPath};
use crate::types::*;

use image::{GrayImage, ImageBuffer, Luma};
use imageproc::distance_transform::Norm;
use imageproc::drawing::draw_line_segment_mut;
use imageproc::morphology::*;
use imageproc::region_labelling::{connected_components, Connectivity};
use imageproc::stats::histogram;
use imageproc::template_matching::{find_extremes, Extremes};
use log::warn;
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use num_integer::Roots;
use std::cmp::{max, min};

#[cfg(feature = "debug_images")]
use imageproc::contrast::stretch_contrast;

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

/// Given a town area and existing roads, find a set of streets such that all area
/// within the town area are within reasonable distance from a road or street.
pub fn divide_town_into_blocks(
    circumference: &Snake,
    town_center: &BlockColumnCoord,
    roads: &[RoadPath],
    height_map: &GrayImage,
) -> Vec<RoadPath> {
    const COVERED: Luma<u8> = Luma([255u8]);

    const ROAD_COVERAGE_RADIUS: u8 = 10;
    const _ROAD_HALF_WIDTH: u8 = 3;

    const STREET_COVERAGE_RADIUS: u8 = 9; // 8
    const STREET_COVERAGE_FULL_WIDTH: u8 = 2 * (STREET_COVERAGE_RADIUS + STREET_HALF_WIDTH);
    const STREET_HALF_WIDTH: u8 = 2;

    const TOWN_BORDER_HALF_WIDTH: u8 = 2;
    const TOWN_BORDER_DISTANCE_TO_CLOSE_STREET: i64 =
        (STREET_HALF_WIDTH + TOWN_BORDER_HALF_WIDTH) as i64;
    const TOWN_BORDER_DISTANCE_TO_FAR_STREET: i64 =
        (STREET_COVERAGE_RADIUS + TOWN_BORDER_HALF_WIDTH - 1) as i64;

    const UNCOVERED_AREA_SIZE_THRESHOLD: u32 = 32;

    // Limit the area of operation to what is strictly necessary
    let (offset, dimensions) = snake_bounding_box(circumference);
    println!(
        "Town circumference bounding box has offset {:?} and size {:?}",
        offset, dimensions
    );
    let offset_town_center = (
        (town_center.0 - offset.0) as u32,
        (town_center.1 - offset.1) as u32,
    );

    // The full loop of the circumference
    let mut full_circumference = circumference.clone();
    full_circumference.push(circumference[0]);

    // Create an image of the area of operation.
    let mut settlement_stencil = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);

    // Mark the town boundary as covered
    draw_offset_snake(
        &mut settlement_stencil,
        &full_circumference,
        &offset,
        COVERED,
    );

    #[cfg(feature = "debug_images")]
    settlement_stencil.save("P-01 circumference.png").unwrap();

    // Mark the outside of the town as covered
    let components = connected_components(&settlement_stencil, Connectivity::Four, COVERED);
    let inside_value = components[offset_town_center];
    for (x, z, value) in components.enumerate_pixels() {
        if inside_value != *value {
            settlement_stencil.put_pixel(x, z, COVERED);
        }
    }

    #[cfg(feature = "debug_images")]
    settlement_stencil.save("P-02 area stencil.png").unwrap();

    // Mark roads
    let mut infrastructure = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
    for road in roads {
        draw_offset_road(&mut infrastructure, road, &offset, COVERED);
    }

    #[cfg(feature = "debug_images")]
    infrastructure
        .save("P-03 existing infrastructure.png")
        .unwrap();

    // Get map of initial areas as divided by initial roads
    let initial_areas = combine_max(&settlement_stencil, &infrastructure);

    #[cfg(feature = "debug_images")]
    initial_areas.save("P-04 initial areas.png").unwrap();

    // Find distinct initial areas
    let initial_areas = image_u32_to_u8(&connected_components(
        &initial_areas,
        Connectivity::Four,
        COVERED,
    ));
    let Extremes {
        max_value: full_area_count,
        ..
    } = find_extremes(&initial_areas);
    println!("Found {} distinct existing areas.", full_area_count);

    #[cfg(feature = "debug_images")]
    if full_area_count > 0 {
        let areas = stretch_contrast(&initial_areas, 0u8, full_area_count);
        areas.save("P-05 full areas.png").unwrap();
    }

    // Mark areas close to roads as covered
    let road_coverage = dilate(&infrastructure, Norm::LInf, ROAD_COVERAGE_RADIUS);

    #[cfg(feature = "debug_images")]
    road_coverage.save("P-06 close to road.png").unwrap();

    // Get map of initial coverage
    let initial_coverage = combine_max(&settlement_stencil, &road_coverage);

    #[cfg(feature = "debug_images")]
    initial_coverage.save("P-07 initial coverage.png").unwrap();

    // Find distinct uncovered areas
    let uncovered_areas = image_u32_to_u8(&connected_components(
        &initial_coverage,
        Connectivity::Four,
        COVERED,
    ));
    let Extremes {
        max_value: area_count,
        ..
    } = find_extremes(&uncovered_areas);
    println!(
        "Found {} distinct areas that may need coverage.",
        area_count
    );

    #[cfg(feature = "debug_images")]
    if area_count > 0 {
        let areas = stretch_contrast(&uncovered_areas, 0u8, area_count);
        areas.save("P-08 areas.png").unwrap();
    }

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
            (
                points[1].1 as i64 - points[0].1 as i64,
                points[0].0 as i64 - points[1].0 as i64,
            )
        })
        .collect();
    normals.push(normals[0]);

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

        street_close_to_border.push(
            (
                (full_circumference[index + 1].0 as i64 + close_offset.0),
                (full_circumference[index + 1].1 as i64 + close_offset.1),
            )
                .into(),
        );
        street_far_from_border.push(
            (
                (full_circumference[index + 1].0 as i64 + far_offset.0),
                (full_circumference[index + 1].1 as i64 + far_offset.1),
            )
                .into(),
        );
    }

    // Modify the street options, in order to get reasonable segment lengths
    let street_close_to_border = resnake(&street_close_to_border, 2f32, 4f32);
    let street_far_from_border = resnake(&street_far_from_border, 2f32, 4f32);

    // NB Only for making nice debug visuals...
    let mut wall_roads = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
    draw_offset_snake(&mut wall_roads, &street_close_to_border, &offset, COVERED);
    draw_offset_snake(&mut wall_roads, &street_far_from_border, &offset, COVERED);

    #[cfg(feature = "debug_images")]
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

        #[cfg(feature = "debug_images")]
        area_stencil
            .save(format!("P-10 area {:0>2}.png", area_index))
            .unwrap();

        // Get the full stencil for only this area
        let location = location_from_value(&uncovered_areas, Luma([area_index as u8])).unwrap();
        let value = initial_areas[location];
        let full_area_stencil = stencil_from_value(&initial_areas, value);

        #[cfg(feature = "debug_images")]
        full_area_stencil
            .save(format!("P-10 full area {:0>2}.png", area_index))
            .unwrap();

        //  Find possible path close by wall
        let close_path = sub_snake(&street_close_to_border, &full_area_stencil, &offset);
        let close_path = attach_to_road_system(&close_path, roads, 6f32);

        // Find possible path further from wall
        let far_path = sub_snake(&street_far_from_border, &full_area_stencil, &offset);
        let far_path = attach_to_road_system(&far_path, roads, 6f32);

        #[cfg(feature = "debug_images")]
        {
            let mut wall_roads = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
            draw_offset_snake(&mut wall_roads, &close_path, &offset, COVERED);
            draw_offset_snake(&mut wall_roads, &far_path, &offset, COVERED);

            wall_roads
                .save(format!("P-10 wall roads {:0>2}.png", area_index))
                .unwrap();
        }

        // Find coverage area for found close path
        let mut close_cover = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
        draw_offset_snake(&mut close_cover, &close_path, &offset, COVERED);
        dilate_mut(&mut close_cover, Norm::LInf, STREET_COVERAGE_RADIUS);

        // If it fully covers, add it and go on to next area.
        if fully_covers(&area_stencil, &close_cover) {
            let close_path = road_path_from_snake(&close_path, height_map);
            streets.push(close_path);
            continue;
        }

        // Find coverage area for found far path
        let mut far_cover = image::ImageBuffer::new(dimensions.0 as u32, dimensions.1 as u32);
        draw_offset_snake(&mut far_cover, &far_path, &offset, COVERED);
        dilate_mut(&mut far_cover, Norm::LInf, STREET_COVERAGE_RADIUS);

        // If it fully covers, add it and go on to next area.
        if fully_covers(&area_stencil, &far_cover) {
            let far_path = road_path_from_snake(&far_path, height_map);
            streets.push(far_path);
            continue;
        }

        // Put in the "far" road alternative, as it most likely covers the most area
        {
            let far_path = road_path_from_snake(&far_path, height_map);
            streets.push(far_path.clone());
        }
        remove_cover(&mut area_stencil, &far_cover);


        #[cfg(feature = "debug_images")]
        area_stencil
            .save(format!("P-10 area {:0>2} after wall path.png", area_index))
            .unwrap();

        // Add border street to infrastructure
        let mut new_infrastructure = infrastructure.clone();
        draw_offset_snake(&mut new_infrastructure, &far_path, &offset, COVERED);

        // Get continuous regions from infrastructure
        let continuous_regions = image_u32_to_u8(&connected_components(
            &new_infrastructure,
            Connectivity::Four,
            COVERED,
        ));

        // Find uncovered pixel location from area_stencil
        let arbitrary_uncovered_location = location_from_value(&area_stencil, Luma([255u8]));
        // Find colour at that location from the newly made continuous regions
        let area_colour = continuous_regions[arbitrary_uncovered_location.unwrap()];
        // Extract that colour, make stencil out of it
        let new_area_stencil = stencil_from_value(&continuous_regions, area_colour);


        #[cfg(feature = "debug_images")]
        new_area_stencil
            .save(format!("P-10 new area {:0>2}.png", area_index))
            .unwrap();

        // Get bounding box for remaining area
        let (uncovered_offset, uncovered_size) = stencil_bounding_box(&area_stencil);

        fn calculate_offsets(uncovered_length: u32) -> Vec<u32> {
            fn ceiling_div(dividend: u32, divisor: u32) -> u32 {
                (dividend + divisor - 1) / divisor
            }

            let full_distance = STREET_COVERAGE_FULL_WIDTH as u32 + uncovered_length;
            let interval_count = ceiling_div(full_distance, STREET_COVERAGE_FULL_WIDTH as u32);
            let interval_length = full_distance / interval_count;
            let edge_offset = (full_distance - (interval_count * interval_length)) / 2;

            println!(
                "Found {} intervals of size {}, edges offset by {}, to cover the {} long gap.",
                interval_count, interval_length, edge_offset, uncovered_length,
            );

            let mut offsets = Vec::with_capacity((interval_count - 1) as usize);
            for i in 1..interval_count {
                let offset = edge_offset + i * interval_length;
                offsets.push(offset - STREET_COVERAGE_FULL_WIDTH as u32 / 2);
            }
            offsets
        }

        if uncovered_size.0 < uncovered_size.1 {
            // shortest along x axis
            println!("Decided to spread along Z axis.");
            let z_offsets = calculate_offsets(uncovered_size.1);
            println!("Z offsets: {:?}", z_offsets);

            // Fill with horizontal paths
            for z in z_offsets {
                let z = z + uncovered_offset.1;
                let x0 = first_on_row(&new_area_stencil, z);
                let x1 = last_on_row(&new_area_stencil, z);
                if let (Some(x0), Some(x1)) = (x0, x1) {
                    let Luma([y0]) = height_map[(x0 + offset.0 as u32, z + offset.1 as u32)];
                    let Luma([y1]) = height_map[(x1 + offset.0 as u32, z + offset.1 as u32)];
                    let mut start_point = (
                        x0 as i64 + offset.0 as i64,
                        y0 as i64,
                        z as i64 + offset.1 as i64,
                    )
                        .into();
                    let mut goal_point = (
                        x1 as i64 + offset.0 as i64,
                        y1 as i64,
                        z as i64 + offset.1 as i64,
                    )
                        .into();

                    // Adjust the end points to the nearby road or street
                    if let Some(new_point) = closest_road_node(roads, &start_point, 4f32) {
                        start_point = new_point;
                    } else if let Some(new_point) = closest_road_node(&streets, &start_point, 4f32)
                    {
                        start_point = new_point;
                    }

                    if let Some(new_point) = closest_road_node(roads, &goal_point, 4f32) {
                        goal_point = new_point;
                    } else if let Some(new_point) = closest_road_node(&streets, &goal_point, 4f32) {
                        goal_point = new_point;
                    }

                    // Get the path
                    if let Some(horizontal_path) =
                        pathfinding::road_path(start_point, goal_point, height_map, None)
                    {
                        streets.push(horizontal_path);
                    }
                }
            }
        } else {
            // shortest along z axis
            println!("Decided to spread along X axis.");
            let x_offsets = calculate_offsets(uncovered_size.0);
            println!("X offsets: {:?}", x_offsets);

            // Fill with vertical paths
            for x in x_offsets {
                let x = x + uncovered_offset.0;
                let z0 = first_on_column(&new_area_stencil, x);
                let z1 = last_on_column(&new_area_stencil, x);
                if let (Some(z0), Some(z1)) = (z0, z1) {
                    let Luma([y0]) = height_map[(x + offset.0 as u32, z0 + offset.1 as u32)];
                    let Luma([y1]) = height_map[(x + offset.0 as u32, z1 + offset.1 as u32)];
                    let mut start_point = (
                        x as i64 + offset.0 as i64,
                        y0 as i64,
                        z0 as i64 + offset.1 as i64,
                    )
                        .into();
                    let mut goal_point = (
                        x as i64 + offset.0 as i64,
                        y1 as i64,
                        z1 as i64 + offset.1 as i64,
                    )
                        .into();

                    // Adjust the end points to the nearby road or street
                    if let Some(new_point) = closest_road_node(roads, &start_point, 4f32) {
                        start_point = new_point;
                    } else if let Some(new_point) = closest_road_node(&streets, &start_point, 4f32)
                    {
                        start_point = new_point;
                    }

                    if let Some(new_point) = closest_road_node(roads, &goal_point, 4f32) {
                        goal_point = new_point;
                    } else if let Some(new_point) = closest_road_node(&streets, &goal_point, 4f32) {
                        goal_point = new_point;
                    }

                    // Get the path
                    if let Some(vertical_path) =
                        pathfinding::road_path(start_point, goal_point, height_map, None)
                    {
                        streets.push(vertical_path);
                    }
                }
            }
        }
    }

    // Some final visual debug
    for street in &streets {
        let street = snake_from_road_path(street);
        draw_offset_snake(&mut infrastructure, &street, &offset, COVERED);
    }

    // TODO Save only if debug images is enabled
    //infrastructure.save("P-11 infrastructure.png").unwrap();

    streets
}

/// Given an area surrounded by roads, streets, or other borders,
/// divide that area into plots.
pub fn _divide_area_into_plots(
    _circumference: &Snake,
    _town_center: &BlockColumnCoord,
    _roads: &[RoadPath],
    _height_map: &GrayImage,
) -> Vec<RoadPath> {
    unimplemented!();
}

fn attach_to_road_system(path: &Snake, attach_to: &[RoadPath], epsilon: f32) -> Snake {
    let mut path = path.clone();

    if let Some(last_point) = path.last() {
        if let Some(new_point) = closest_road_point(attach_to, last_point, epsilon) {
            if *last_point != new_point {
                path.push(new_point);
            }
        } else {
            warn!("Could not attach last point.");
        }
    }

    if let Some(first_point) = path.first_mut() {
        if let Some(new_point) = closest_road_point(attach_to, first_point, epsilon) {
            *first_point = new_point;
        } else {
            warn!("Could not attach first point.");
        }
    }

    path
}

fn closest_road_point(
    roads: &[RoadPath],
    closest_to: &BlockColumnCoord,
    epsilon: f32,
) -> Option<BlockColumnCoord> {
    let mut closest_point = *closest_to;
    let mut closest_manhattan = usize::MAX / 2;
    let mut closest_euclidean = f32::MAX;

    for road in roads {
        for node in road {
            let node_point = node.coordinates.into();
            let manhattan = geometry::manhattan_distance(node_point, *closest_to);
            if manhattan < (2 * closest_manhattan) {
                let euclidean = geometry::euclidean_distance(node_point, *closest_to);
                if euclidean < closest_euclidean {
                    closest_point = node_point;
                    closest_manhattan = manhattan;
                    closest_euclidean = euclidean;
                }
            }
        }
    }

    if closest_euclidean <= epsilon {
        Some(closest_point)
    } else {
        None
    }
}

/// Given a point and a set of roads, returns the road node closest to the point
fn closest_road_node(
    roads: &[RoadPath],
    closest_to: &BlockCoord,
    epsilon: f32,
) -> Option<BlockCoord> {
    let mut closest_point = *closest_to;
    let mut closest_manhattan = usize::MAX / 2;
    let mut closest_euclidean = f32::MAX;

    for road in roads {
        for node in road {
            let node_point = node.coordinates;
            let manhattan = geometry::manhattan_distance_3d(node_point, *closest_to);
            if manhattan < (2 * closest_manhattan) {
                let euclidean = geometry::euclidean_distance_3d(node_point, *closest_to);
                if euclidean < closest_euclidean {
                    closest_point = node_point;
                    closest_manhattan = manhattan;
                    closest_euclidean = euclidean;
                }
            }
        }
    }

    if closest_euclidean <= epsilon {
        Some(closest_point)
    } else {
        None
    }
}

pub fn snake_bounding_box(snake: &Snake) -> (BlockColumnCoord, BlockColumnCoord) {
    let offset = snake
        .iter()
        .copied()
        .reduce(|a, b| {
            (
                if a.0 < b.0 { a.0 } else { b.0 },
                if a.1 < b.1 { a.1 } else { b.1 },
            )
                .into()
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
                .into()
        })
        .unwrap();
    let dimensions = (
        dimensions_plus_offset.0 - offset.0,
        dimensions_plus_offset.1 - offset.1,
    )
        .into();

    (offset, dimensions)
}

pub fn draw_offset_snake(
    image: &mut GrayImage,
    snake: &Snake,
    offset: &BlockColumnCoord,
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
    offset: &BlockColumnCoord,
    colour: Luma<u8>,
) {
    road[1..].iter().fold(road[0], |a, b| {
        draw_line_segment_mut(
            image,
            (
                (a.coordinates.0 - offset.0) as f32,
                (a.coordinates.2 - offset.1) as f32,
            ),
            (
                (b.coordinates.0 - offset.0) as f32,
                (b.coordinates.2 - offset.1) as f32,
            ),
            colour,
        );
        *b
    });
}

fn first_on_row(image: &GrayImage, row: u32) -> Option<u32> {
    for x in 0..image.dimensions().0 {
        if image[(x, row)] == Luma([255u8]) {
            return Some(x);
        }
    }
    None
}

fn last_on_row(image: &GrayImage, row: u32) -> Option<u32> {
    for x in (0..image.dimensions().0).rev() {
        if image[(x, row)] == Luma([255u8]) {
            return Some(x);
        }
    }
    None
}

fn first_on_column(image: &GrayImage, column: u32) -> Option<u32> {
    for z in 0..image.dimensions().1 {
        if image[(column, z)] == Luma([255u8]) {
            return Some(z);
        }
    }
    None
}

fn last_on_column(image: &GrayImage, column: u32) -> Option<u32> {
    for z in (0..image.dimensions().1).rev() {
        if image[(column, z)] == Luma([255u8]) {
            return Some(z);
        }
    }
    None
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

/// Makes a stencil corresponding to one colour in the input image..
/// All pixels of the foreground colour gets fully white in the output.
/// All pixels of other colours gets fully black in the output.
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

/// Returns one location in the picture that has the given colour, if such a location exist.
fn location_from_value(image: &GrayImage, foreground: Luma<u8>) -> Option<(u32, u32)> {
    for (x, z, value) in image.enumerate_pixels() {
        if *value == foreground {
            return Some((x, z));
        }
    }
    None
}

/// Make a snake out of the points in `snake` that are within the area marked by `stencil`.
fn sub_snake(snake: &Snake, stencil: &GrayImage, offset: &BlockColumnCoord) -> Snake {
    let mut new_snake = Vec::new();
    let mut snake_started: bool = false;
    let mut snake_ended: bool = false;

    for i in 0..(snake.len() * 2) - 1 {
        let coordinates = snake[i % snake.len()];
        let inside = Luma([255u8])
            == stencil[(
                (coordinates.0 as i64 - offset.0 as i64) as u32,
                (coordinates.1 as i64 - offset.1 as i64) as u32,
            )];

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
        if *value == Luma([255u8]) && covering[(x, z)] != Luma([255u8]) {
            return false;
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

fn stencil_bounding_box(image: &GrayImage) -> ((u32, u32), (u32, u32)) {
    let mut max_point = (0, 0);
    let mut min_point = image.dimensions();

    for (x, z, value) in image.enumerate_pixels() {
        if *value == Luma([255]) {
            min_point.0 = min(x, min_point.0);
            min_point.1 = min(z, min_point.1);
            max_point.0 = max(x, max_point.0);
            max_point.1 = max(z, max_point.1);
        }
    }

    let size = (
        max_point.0.saturating_sub(min_point.0),
        max_point.1.saturating_sub(min_point.1),
    );

    (min_point, size)
}

fn resnake(snake: &Snake, min_length: f32, max_length: f32) -> Snake {
    assert!(min_length < max_length);

    if snake.is_empty() {
        return Vec::new();
    }

    let mut output = vec![snake[0]];

    let distances: Vec<f32> = snake
        .windows(2)
        .map(|points| {
            let (a, b) = (points[0], points[1]);
            ((b.0 as f32 - a.0 as f32).powi(2) + (b.1 as f32 - a.1 as f32).powi(2)).sqrt()
        })
        .collect();

    let mut accumulated_distance = 0f32;

    for (index, point) in snake.iter().enumerate() {
        if index == 0 {
            continue;
        }

        if index > distances.len() {
            unreachable!();
        }

        let distance_from_previous_point = distances[index - 1];
        accumulated_distance += distance_from_previous_point;

        if accumulated_distance < min_length {
            // Too close to the previously added point; do not add
        } else if accumulated_distance <= max_length {
            // We are within the length boundaries; add point
            output.push(*point);
            accumulated_distance = 0f32;
        } else {
            // We have gone too far; add multiple points
            let num_points = (accumulated_distance / max_length).ceil() as usize;
            let previous_point = snake[index - 1];
            let x_diff = point.0 as f32 - previous_point.0 as f32;
            let z_diff = point.1 as f32 - previous_point.1 as f32;
            let scaling = accumulated_distance / distance_from_previous_point;
            let x_unit = x_diff * scaling / num_points as f32;
            let z_unit = z_diff * scaling / num_points as f32;

            for i in (0..num_points).rev() {
                let x = point.0 as f32 - (i as f32 * x_unit);
                let z = point.1 as f32 - (i as f32 * z_unit);
                output.push((x as i64, z as i64).into());
            }

            accumulated_distance = 0f32;
        }
    }

    println!(
        "Generated new snake of length {}, from old snake of length {}.",
        output.len(),
        snake.len(),
    );
    output
}
