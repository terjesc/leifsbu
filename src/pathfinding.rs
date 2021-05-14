use image::{GrayImage, RgbImage};
use imageproc::drawing::draw_line_segment_mut;
use mcprogedit::coordinates::BlockCoord;
//use mcprogedit::positioning::Direction16;
use num_integer::Roots;
use pathfinding::prelude::astar;
use std::cmp::{min, max};

use crate::types::*;

// For distance calculations, how many units to divide one block length into.
const SUB_UNITS: i64 = 100;
const CUT_DEPTH_MAX: i64 = 4;
const WOODEN_SUPPORT_HEIGHT_MAX: i64 = 8;
const STONE_SUPPORT_HEIGHT_MAX: i64 = 24;
const WOODEN_SUPPORT_COST: i64 = 200;
const STONE_SUPPORT_COST: i64 = 300;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct RoadNode {
    pub coordinates: BlockCoord,
    pub kind: RoadNodeKind,
    //azimuth: Direction16,
    //elevation: i8,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum RoadNodeKind {
    Start,
    Ground,
    WoodenSupport,
    StoneSupport,
    //Cutting,
    //Tunnel,
}

pub type RoadPath = Vec<RoadNode>;

pub fn draw_road_path(image: &mut RgbImage, path: &RoadPath) {
    const MARKER_RADIUS: i64 = 1;
    let (x_len, z_len) = image.dimensions();

    // Lines
    for line in path.windows(2) {
        let line_colour = match (line[0].kind, line[1].kind) {
            (RoadNodeKind::WoodenSupport, _)
                | (RoadNodeKind::StoneSupport, _)
                | (_, RoadNodeKind::WoodenSupport)
                | (_, RoadNodeKind::StoneSupport)
                => image::Rgb([191u8, 32u8, 32u8]),
            _ => image::Rgb([127u8, 0u8, 0u8]),
        };

        let start = (line[0].coordinates.0 as f32, line[0].coordinates.2 as f32);
        let end = (line[1].coordinates.0 as f32, line[1].coordinates.2 as f32);

        draw_line_segment_mut(image, start, end, line_colour);
    }

    // Node markers
    for RoadNode { coordinates, kind, .. } in path {
        let (x, z) = (coordinates.0, coordinates.2);
        
        let node_colour = match kind {
            RoadNodeKind::WoodenSupport => image::Rgb([64u8, 0u8, 0u8]),
            RoadNodeKind::StoneSupport => image::Rgb([32u8, 32u8, 32u8]),
            _ => continue,
        };

        for x in max(0, x-MARKER_RADIUS)..=min(x+MARKER_RADIUS, x_len as i64 - 1) {
            for z in max(0, z-MARKER_RADIUS)..=min(z+MARKER_RADIUS, z_len as i64 - 1) {
                image.put_pixel(x as u32, z as u32, node_colour);
            }
        }
    }
}

pub fn road_path(
    start: BlockCoord,
    goal: BlockCoord,
    height_map: &GrayImage,
    ground_block_map: &GrayImage,
) -> Option<RoadPath> {
    let (x_len, z_len) = height_map.dimensions();

    // Euclidean distance stretched in the vertical direction
    fn stretched_euclidean_distance(a: &BlockCoord, b: &BlockCoord) -> u64 {
        const STRETCH: i64 = 5;

        (((a.0 - b.0) * SUB_UNITS).pow(2)
            + ((a.1 - b.1) * SUB_UNITS * STRETCH).pow(2)
            + ((a.2 - b.2) * SUB_UNITS).pow(2)).sqrt() as u64
    }

    let get_terrain_height = |x: i64, z: i64| -> Option<i64> {
        if x >= 0 && x < x_len as i64 && z >= 0 && z < z_len as i64 {
            let image::Luma([terrain_height]) = height_map[(x as u32, z as u32)];
            Some(terrain_height as i64)
        } else {
            None
        }
    };

    let support_cost = |node: &RoadNode| -> u64 {
        let cost = match node.kind {
            RoadNodeKind::WoodenSupport => (node.coordinates.1
                - get_terrain_height(node.coordinates.0, node.coordinates.2).unwrap()
                + 1)
                * WOODEN_SUPPORT_COST,
            RoadNodeKind::StoneSupport => (node.coordinates.1
                - get_terrain_height(node.coordinates.0, node.coordinates.2).unwrap()
                + 1)
                * STONE_SUPPORT_COST,
            _ => 0,
        } as u64;
        cost
    };

    // Calculate the cost between two given road nodes.
    let cost = |a: &RoadNode, b: &RoadNode| -> u64 {
        stretched_euclidean_distance(&a.coordinates, &b.coordinates)
        + support_cost(&a)
        + support_cost(&b)
    };

    let is_ground_blocked = |x: i64, z: i64| -> bool {
        image::Luma([0u8]) != ground_block_map[(x as u32, z as u32)]
    };

    // Find all potential neighbours for a given road node
    let neighbours = |node: &RoadNode| -> Vec<RoadNode> {
        let mut neighbours = Vec::new();
        let (x, y, z) = (node.coordinates.0, node.coordinates.1, node.coordinates.2);

        match node.kind {
            RoadNodeKind::Start => {
                // NB when adding directionality, start must create nodes in all directions
                if let Some(terrain_height) = get_terrain_height(x, z) {
                    if terrain_height == y {
                        // On ground
                        neighbours.push(RoadNode {
                            coordinates: (x, y, z).into(),
                            kind: RoadNodeKind::Ground,
                        });
                    } else if terrain_height < y {
                        // Bridge
                        let support_height = y - terrain_height;
                        if support_height <= WOODEN_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (x, y, z).into(),
                                kind: RoadNodeKind::WoodenSupport,
                            });
                        }
                        if support_height <= STONE_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (x, y, z).into(),
                                kind: RoadNodeKind::StoneSupport,
                            });
                        }
                    } else if terrain_height > (y + CUT_DEPTH_MAX) {
                        // Tunnel
                    } else { // Terrain barely higher than node
                        // Cut
                    }
                }
            }
            RoadNodeKind::Ground => {
                for (new_x, new_z) in &ground_neighbour_locations(x, z) {
                    if let Some(terrain_height) = get_terrain_height(*new_x, *new_z) { 
                        // Add edges to Ground
                        if !is_ground_blocked(*new_x, *new_z) {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, terrain_height, *new_z).into(),
                                kind: RoadNodeKind::Ground,
                            });
                        }
                        // Add edges to WoodenSupport
                        // NB Currently only flat bridge. Add slopes as well?
                        if y > terrain_height
                        && y <= terrain_height + WOODEN_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::WoodenSupport,
                            });
                        }
                        // Add edges to StoneSupport
                        // NB Currently only flat bridge. Add slopes as well?
                        if y > terrain_height
                        && y <= terrain_height + STONE_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::StoneSupport,
                            });
                        }
                    }
                }
            }
            RoadNodeKind::WoodenSupport => {
                for (new_x, new_z) in &ground_neighbour_locations(x, z) {
                    if let Some(terrain_height) = get_terrain_height(*new_x, *new_z) {
                        // Add ground node if on ground level
                        if y == terrain_height {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::Ground,
                            });
                        }
                    }
                }
                for (new_x, new_z) in &wood_neighbour_locations(x, z) {
                    if let Some(terrain_height) = get_terrain_height(*new_x, *new_z) {
                        // Add support node if above ground and below support limit
                        if y > terrain_height
                        && y <= terrain_height + WOODEN_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::WoodenSupport,
                            });
                        }
                    }
                }
            }
            RoadNodeKind::StoneSupport => {
                for (new_x, new_z) in &ground_neighbour_locations(x, z) {
                    if let Some(terrain_height) = get_terrain_height(*new_x, *new_z) {
                        // Add ground node if on ground level
                        if y == terrain_height {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::Ground,
                            });
                        }
                    }
                }
                for (new_x, new_z) in &stone_neighbour_locations(x, z) {
                    if let Some(terrain_height) = get_terrain_height(*new_x, *new_z) {
                        // Add support node if above ground and below support limit
                        if y > terrain_height
                        && y <= terrain_height + STONE_SUPPORT_HEIGHT_MAX {
                            neighbours.push(RoadNode {
                                coordinates: (*new_x, y, *new_z).into(),
                                kind: RoadNodeKind::StoneSupport,
                            });
                        }
                    }
                }
            }
        }

        neighbours
    };

    // Calculates neighbours and cost of traveling to neighbours, for A* algorithm
    let successors = |node: &RoadNode| -> Vec<(RoadNode, u64)> {
        neighbours(&node).into_iter().map(|n| (n, cost(node, &n))).collect()
    };

    // Heuristic, for A* algorithm
    let heuristic = |node: &RoadNode| {
        // TODO consider using a cheaper and/or more accurate calculation here...
        stretched_euclidean_distance(&node.coordinates, &goal)
    };

    // Goal node calculations, for success criteria
    let image::Luma([goal_terrain_height]) = height_map[(goal.0 as u32, goal.2 as u32)];
    let goal_y = goal.1;
    let goal_relative_height = goal_y - goal_terrain_height as i64;

    // Success criteria (goal reached?), for A* algorithm
    let success = |node: &RoadNode| -> bool {
        if goal_relative_height == 0 {
            node.coordinates == goal && node.kind == RoadNodeKind::Ground
        } else {
            node.coordinates == goal
        }
    };

    // Start node, for A* algorithm
    let start_node = RoadNode {
        coordinates: start,
        kind: RoadNodeKind::Start,
    };

    // Run A* algorithm
    if let Some((path, _)) = astar(&start_node, successors, heuristic, success) {
        Some(path)
    } else {
        None
    }
}

// TODO handle water, steepness, etc. as well...
pub fn road_path_from_snake(path: &Snake, height_map: &GrayImage) -> RoadPath {
    let mut road_path = Vec::with_capacity(path.len());

    for (x, z) in path {
        let image::Luma([y]) = height_map[(*x as u32, *z as u32)];
        let coordinates: BlockCoord = (*x as i64, y as i64, *z as i64).into();
        road_path.push(RoadNode { coordinates, kind: RoadNodeKind::Ground });
    }

    road_path
}

// NB deprecated
pub fn path(start: Point, goal: Point, height_map: &GrayImage) -> Option<Snake> {
    fn euclidean_distance(current: &Point, next: &Point) -> usize {
        ((current.0 as i64 - next.0 as i64).pow(2) * SUB_UNITS +
         (current.1 as i64 - next.1 as i64).pow(2) * SUB_UNITS).sqrt() as usize
    }

    let inclination = |current: &Point, next: &Point, distance: &usize| -> usize {
        let image::Luma([current_height]) = height_map[(current.0 as u32, current.1 as u32)];
        let image::Luma([next_height]) = height_map[(next.0 as u32, next.1 as u32)];
        let height = (SUB_UNITS * (current_height as i64 - next_height as i64).abs()) as usize;
        height / distance
    };

    // TODO consider direction, turning, etc. as part of the equation

    let cost = |current: &Point, next: &Point| -> usize {
        let distance_cost = euclidean_distance(&current, &next);
        distance_cost + inclination(&current, &next, &distance_cost)
    };

    let successors = |point: &Point| -> Vec<(Point, usize)> {
        // TODO better (larger) neighbourhood (?)
        let (x, z) = *point;
        let (x_len, z_len) = height_map.dimensions();

        const RADIUS: usize = 5;
        let mut neighbours = Vec::with_capacity((2 * RADIUS + 1).pow(2) - 1);

        for nx in x.saturating_sub(RADIUS)..=min(x+RADIUS, x_len as usize - 1) {
            for nz in z.saturating_sub(RADIUS)..=min(z+RADIUS, z_len as usize - 1) {
                if x != nx || z != nz {
                    neighbours.push((nx, nz));
                }
            }
        }

        neighbours.into_iter().map(|p| (p, cost(point, &p))).collect()
    };

    let heuristic = |point: &Point| {
        // TODO consider using a cheaper calculation here...
        euclidean_distance(&point, &goal)
    };

    let success = |point: &Point| -> bool {
        *point == goal
    };

    if let Some((path, _)) = astar(&start, successors, heuristic, success) {
        Some(path)
    } else {
        None
    }
}

fn ground_neighbour_locations(x: i64, z: i64) -> [(i64, i64); 20] {
    [
                (x-2, z-1), (x-2, z), (x-2, z+1),
    (x-1, z-2), (x-1, z-1), (x-1, z), (x-1, z+1), (x-1, z+2),
    (x,   z-2), (x,   z-1), /*node*/  (x,   z+1), (x,   z+2),
    (x+1, z-2), (x+1, z-1), (x+1, z), (x+1, z+1), (x+1, z+2),
                (x+2, z-1), (x+2, z), (x+2, z+1),
    ]
}

fn wood_neighbour_locations(x: i64, z: i64) -> [(i64, i64); 16] {
    [
                            (x-5, z),
                    (x-4, z-2),     (x-4, z+2),
                (x-3, z-3),             (x-3, z+3),
            (x-2, z-4),                     (x-2, z+4),

        (x, z-5),           /*node*/            (x, z+5),

            (x+2, z-4),                     (x+2, z-4),
                (x+3, z-3),             (x+3, z+3),
                    (x+4, z-2),     (x+4, z+2),
                            (x+5, z),
    ]
}

fn stone_neighbour_locations(x: i64, z: i64) -> [(i64, i64); 16] {
    [
                            (x-7, z),
                    (x-6, z-3),     (x-6, z+3),
                (x-5, z-5),             (x-5, z+5),
            (x-3, z-6),                     (x-3, z+6),

        (x, z-7),           /*node*/            (x, z+7),

            (x+3, z-6),                     (x+3, z-6),
                (x+5, z-5),             (x+5, z+5),
                    (x+6, z-3),     (x+6, z+3),
                            (x+7, z),
    ]
}
