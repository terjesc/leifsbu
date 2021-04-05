use std::cmp::{min, max};
use std::f32::consts::{PI, TAU};

use image::{GrayImage, RgbImage};
use image::imageops::colorops::invert;
use imageproc::contrast::*;
use imageproc::distance_transform::*;
use imageproc::map::map_colors;
use imageproc::morphology::*;
use imageproc::suppress::suppress_non_maximum;

use crate::Features;
use crate::Areas;

/// Find the most suitable closed loop perimeter for a town wall.
pub fn walled_town_contour(features: &Features, areas: &Areas) -> Snake {
    let mut not_town = areas.town.clone();
    invert(&mut not_town);
    not_town.save("T-01 not town.png").unwrap();

    let (x_len, z_len) = not_town.dimensions();

    // Mask for town circumference start circle
    // Energy map for finding town circumference
    // * Top of hill (+)
    // * (Larger) water (-)
    // * Steep terrain (-)
    // * Forest (-)

    // TODO Find better cost image.
    // Water depth -> penalty (squared?)
    let water_depth_energy = features.water_depth.clone();
    let water_depth_energy = map_colors(
        &water_depth_energy,
        |p| image::Luma([p[0].saturating_mul(p[0])]),
    );
    water_depth_energy.save("T-04 water depth energy.png").unwrap();

    // Distance from shore -> penalty
    // TODO Maybe start the penalty a few blocks ashore?
    let mut offshore_distance_energy = features.water.clone();
    invert(&mut offshore_distance_energy);
    distance_transform_mut(&mut offshore_distance_energy, Norm::L1);
    let offshore_distance_energy = map_colors(
        &offshore_distance_energy,
        |p| image::Luma([p[0].saturating_mul(4)]),
    );
    offshore_distance_energy.save("T-05 offshore distance energy.png").unwrap();

    // Steep terrain -> penalty
    let mut slope_energy = features.scharr.clone();
    threshold_mut(&mut slope_energy, 16u8);
    close_mut(&mut slope_energy, Norm::LInf, 3);
    slope_energy.save("T-07 slope energy.png").unwrap();

    let mut energy = image::ImageBuffer::new(x_len as u32, z_len as u32);
    for x in 0..x_len {
        for z in 0..z_len {
            let image::Luma([water_depth]) = water_depth_energy[(x, z)];
            let image::Luma([offshore_distance]) = offshore_distance_energy[(x, z)];
            let image::Luma([slope]) = slope_energy[(x, z)];
            let image::Luma([not_town]) = not_town[(x, z)];
            energy[(x, z)] = image::Luma([water_depth
                .saturating_add(offshore_distance)
                .saturating_add(slope)
                .saturating_add(not_town)
            ]);
        }
    }
    const NEUTRAL_ENERGY: u8 = u8::MAX / 2;
    let energy = imageproc::map::map_colors2(
        &energy,
        &features.hilltop,
        |p, q| {
            image::Luma([
                p[0]
                .saturating_add(NEUTRAL_ENERGY)
                .saturating_sub(q[0])
            ])
        },
    );
    energy.save("T-10 energy.png").unwrap();

    // map of distance from (potential) town edge
    let town_density = distance_transform(&threshold(&energy, NEUTRAL_ENERGY), Norm::LInf);
    town_density.save("T-02 town density.png").unwrap();

    // points the farthest away from (potential) town edge are potential town centers.
    let mut town_centers = suppress_non_maximum(&town_density, 8);

    // List and sort town center points according to potential town size.
    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct TownCenterPoint {
        radius: u8,
        point: Point,
    }

    let mut town_center_list = Vec::new();
    for x in 1..x_len as usize - 1 {
        for z in 1..z_len as usize - 1 {
            let image::Luma([radius]) = town_centers[(x as u32, z as u32)];
            if radius != 0 {
                town_center_list.push(TownCenterPoint { radius, point: (x, z) });
            }
        }
    }
    town_center_list.sort_by(|a, b| b.partial_cmp(a).unwrap());

    threshold_mut(&mut town_centers, 0u8);
    town_centers.save("T-03 town centers.png").unwrap();

    // TODO Maybe calculate and rate the N most promising locations?
    //      For now: Use the one the farthest away from "non-suitable" features/areas.
    const TOWN_INDEX: usize = 0; // Nth largest town center: TODO reset to 0
    walled_town_contour_internal(
        &energy,
        &features.coloured_map,
        town_center_list[TOWN_INDEX].radius,
        town_center_list[TOWN_INDEX].point,
    )
}

// types for active contour model
pub type Point = (usize, usize);
pub type Snake = Vec<Point>;

fn circle_snake(
    num_points: usize,
    start_radius: usize,
    center: Point
) -> Snake {
    let mut snake: Snake = Vec::new();
    for i in 0..num_points {
        let angle = i as f32 * TAU / num_points as f32;
        let x = (center.0 as f32 + start_radius as f32 * angle.cos()) as usize;
        let y = (center.1 as f32 + start_radius as f32 * angle.sin()) as usize;
        snake.push((x, y));
    }
    snake
}

// Try to find a good walled town circumference
fn walled_town_contour_internal(
    costs: &GrayImage,
    map_img: &RgbImage,
    radius: u8,
    center: Point,
) -> Snake {
    // Parameters for the starting circle snake
    const _NUM_POINTS: usize = 50;

    // Parameters for the active contour model
    const ALPHA: f32 = 0.60; // weight for averaging snake line lengths
    const BETA: f32 = 0.40; // weight for snake curvature
    const GAMMA: f32 = 0.10; // weight for costs from image
    const INFLATE: f32 = 5.0; // weight for inflating the balloon

    let (center_x, center_y) = center;
    let num_points = radius as usize * 2;
    let snake = circle_snake(num_points, radius as usize, (center_x, center_y));
    save_snake_image(&snake, &map_img, "acm_000.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_001.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_010.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_020.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_030.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_040.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_050.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_060.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_070.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_080.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_090.png".to_string());

    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    let (snake, _energy) = active_contour_model(snake, &costs, ALPHA, BETA, GAMMA, INFLATE);
    save_snake_image(&snake, &map_img, "acm_100.png".to_string());

    snake
}

// Print a snake superimposed on an image
fn save_snake_image(snake: &Snake, image: &RgbImage, path: String) {
    const MARKER_RADIUS: usize = 0;
    let (x_len, z_len) = image.dimensions();

    let mut image = image.clone();

    for (x, z) in snake {
        for x in max(0, x-MARKER_RADIUS)..=min(x+MARKER_RADIUS, x_len as usize - 1) {
            for z in max(0, z-MARKER_RADIUS)..=min(z+MARKER_RADIUS, z_len as usize - 1) {
                image.put_pixel(x as u32, z as u32, image::Rgb([255u8, 255u8, 255u8]));
            }
        }
    }

    image.save(path).unwrap();
}

/// Perform one iteration of active contour model, for a circular (closed) snake.
/// Returns the new snake, and an estimate of its energy.
fn active_contour_model(
    snake: Snake,
    image_costs: &GrayImage,
    alpha: f32,
    beta: f32,
    gamma: f32,
    inflate: f32,
) -> (Snake, f32) {
    // TODO consider a larger neighbourhood, some points dotted a bit further away:
    //     !?  !?
    //
    // !?  <><><>  !?
    //     <><><>
    // !?  <><><>  !?
    //
    //     !?  !?
    fn neighbourhood((x, y): &Point, (x_len, y_len): (u32, u32)) -> Snake {
        const RADIUS: usize = 3;
        let mut neighbourhood = Vec::with_capacity(9);
        for x in x.saturating_sub(RADIUS)..=min(x + RADIUS, x_len as usize - 1) {
            for y in y.saturating_sub(RADIUS)..=min(y + RADIUS, y_len as usize - 1) {
                neighbourhood.push((x, y));
            }
        }
        neighbourhood
    }

    fn internal_energy(
        (alpha, beta, inflate): (f32, f32, f32),
        snake: &Snake,
        index: usize,
        (x, y): Point,
    ) -> f32 {
        let i_prev = (index + snake.len() - 1) % snake.len();
        let i_next = (index + 1) % snake.len();

        // Distance energy (difference from average segment distance)
        // TODO Consider some «target distance» metric as well
        let mut snake_circumference = 0.0f32;
        for i in 0..snake.len() {
            let i_next = (i + 1) % snake.len();
            let x_length = snake[i].0 as f32 - snake[i_next].0 as f32;
            let y_length = snake[i].1 as f32 - snake[i_next].1 as f32;
            let length = (x_length * x_length + y_length * y_length).sqrt();
            snake_circumference += length;
        }
        let snake_segment_average_length = snake_circumference / snake.len() as f32;

        let x_length = snake[i_prev].0 as f32 - x as f32;
        let y_length = snake[i_prev].1 as f32 - y as f32;
        let length_prev = (x_length * x_length + y_length * y_length).sqrt();

        let x_length = snake[i_next].0 as f32 - x as f32;
        let y_length = snake[i_next].1 as f32 - y as f32;
        let length_next = (x_length * x_length + y_length * y_length).sqrt();

        let distance_energy = ((length_prev - snake_segment_average_length).abs()
            + (length_next - snake_segment_average_length).abs())
            / 2.0f32;

        // Curvature energy
        let curvature_energy =
            (snake[i_prev].0 as f32 - 2.0 * x as f32 + snake[i_next].0 as f32).powi(2)
            + (snake[i_prev].1 as f32 - 2.0 * y as f32 + snake[i_next].1 as f32).powi(2);

        // Inflation energy
        let (x_current, y_current, x1, y1, x2, y2) = (
            snake[index].0 as f32, snake[index].1 as f32,
            snake[i_prev].0 as f32, snake[i_prev].1 as f32,
            snake[i_next].0 as f32, snake[i_next].1 as f32,
        );
        let p1p2_len = ((x2-x1).powi(2) + (y2-y1).powi(2)).sqrt();
        let cross_current = (x2 - x1) * (y1 - y_current) - (x1 - x_current) * (y2 - y1);
        let cross_new = (x2 - x1) * (y1 - y as f32) - (x1 - x as f32) * (y2 - y1);
        let inflation_energy = ((cross_new - cross_current) / p1p2_len - 1.0).abs();

        alpha * distance_energy + beta * curvature_energy + inflate * inflation_energy
    }

    fn external_energy(
        gamma: f32,
        image_costs: &GrayImage,
        at: Point,
    ) -> f32 {
        let image::Luma([cost]) = image_costs[(at.0 as u32, at.1 as u32)];
        gamma * cost as f32
    }

    // one iteration
    let mut new_snake = snake.clone();
    let mut total_energy_estimate = 0.0f32;

    for (index, snake_point) in snake.iter().enumerate() {
        let mut best_energy = f32::MAX;
        for point in neighbourhood(snake_point, image_costs.dimensions()) {
            let energy = internal_energy((alpha, beta, inflate), &snake, index, point)
                + external_energy(gamma, &image_costs, point);
            if energy < best_energy {
                best_energy = energy;
                new_snake[index] = point;
            }
        }
        total_energy_estimate += best_energy;
    }

    (new_snake, total_energy_estimate)
}