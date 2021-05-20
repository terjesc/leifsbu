use crate::geometry::{point_position_relative_to_polygon, InOutSide};
use crate::line;
use crate::pathfinding::{RoadNode, RoadNodeKind, RoadPath};
use crate::tree;
use crate::types::Snake;

use image::GrayImage;
use mcprogedit::block::Block;
use mcprogedit::material::Material;
use mcprogedit::positioning::Axis3;
use mcprogedit::world_excerpt::WorldExcerpt;

/*
// TODO implement a concept of "road", that contains both the path, the width,
//      and possibly more data about a given road (segment)
struct Road {
    width: i64,
    path: RoadPath,
}
*/

/// Splits a set of roads into a set of city roads and a set of country roads,
/// by splitting each road into the parts outside and inside of the given polygon,
/// and putting all inside roads in the first item of the output and all outside
/// roads in the last item of the output.
pub fn roads_split(roads: &Vec<RoadPath>, polygon: &Snake) -> (Vec<RoadPath>, Vec<RoadPath>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();

    for road in roads {
        let (mut inside_parts, mut outside_parts) = road_split(road, polygon);
        inside.append(&mut inside_parts);
        outside.append(&mut outside_parts);
    }

    (inside, outside)
    //(roads.clone(), Vec::new())
}

/// Splits a single road into segments inside and segments outside of the given polygon.
fn road_split(road: &RoadPath, polygon: &Snake) -> (Vec<RoadPath>, Vec<RoadPath>) {
    if road.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut inside = Vec::new();
    let mut outside = Vec::new();

    let first_is_inside = InOutSide::Inside
        == point_position_relative_to_polygon(road[0].coordinates.into(), polygon);

    let (last_segment_is_inside, last_segment) = road.iter().fold(
        (first_is_inside, Vec::new()),
        |accumulator: (bool, RoadPath), node: &RoadNode| {
            let (previous_was_inside, mut acc) = accumulator;

            let is_inside = InOutSide::Inside
                == point_position_relative_to_polygon(node.coordinates.into(), polygon);
            if previous_was_inside == is_inside {
                acc.push(*node);
                (is_inside, acc)
            } else if previous_was_inside {
                // TODO correct both reasoning and logic here;
                // "inside" should NOT contain the neighbouring "outside" node
                // "outside" SHOULD contain the neighbouring "inside" node
                //
                // We have a transition from "inside" to "outside", where `acc` contains
                // everything "inside"

                // The beginning "outside" should contain both the last "inside" node,
                // and `node`, which is the first fully "outside" node.
                let mut new_acc = Vec::new();
                new_acc.push(*acc.last().unwrap());
                new_acc.push(*node);

                inside.push(acc);

                (is_inside, new_acc)
            } else {
                // TODO correct both reasoning and logic here;
                // "inside" should NOT contain the neighbouring "outside" node
                // "outside" SHOULD contain the neighbouring "inside" node
                //
                // We have a transition from "outside" to "inside", where `acc` contains
                // everything "outside" except the first "inside" node which should also
                // be part of "outside".

                // The beginning "inside" should contain the current node only.
                let mut new_acc = Vec::new();
                new_acc.push(*acc.last().unwrap());
                new_acc.push(*node);

                outside.push(acc);

                (is_inside, new_acc)
            }
        },
    );

    if last_segment.len() > 1 {
        if last_segment_is_inside {
            inside.push(last_segment);
        } else {
            outside.push(last_segment);
        }
    }

    (inside, outside)
}

pub fn build_road(
    excerpt: &mut WorldExcerpt,
    path: &RoadPath,
    height_map: &GrayImage,
    road_width: i64,
) {
    // Build the path segments
    for segment in path.windows(2) {
        let line = line::line(
            &(segment[0].coordinates),
            &(segment[1].coordinates),
            road_width,
        );
        for position in &line {
            tree::chop(excerpt, *position - (0, 2, 0).into());
            tree::chop(excerpt, *position - (0, 1, 0).into());
            tree::chop(excerpt, *position);
            tree::chop(excerpt, *position + (0, 1, 0).into());
            tree::chop(excerpt, *position + (0, 2, 0).into());
        }

        match (segment[0].kind, segment[1].kind) {
            (RoadNodeKind::WoodenSupport, RoadNodeKind::WoodenSupport) => {
                for position in &line {
                    excerpt.set_block_at(*position, Block::dark_oak_planks());
                    excerpt.set_block_at(*position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(*position + (0, 2, 0).into(), Block::Air);
                }
            }
            (RoadNodeKind::WoodenSupport, _) | (_, RoadNodeKind::WoodenSupport) => {
                for position in &line {
                    excerpt.set_block_at(*position, Block::bottom_slab(Material::DarkOak));
                    excerpt.set_block_at(*position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(*position + (0, 2, 0).into(), Block::Air);
                }
            }
            (RoadNodeKind::StoneSupport, RoadNodeKind::StoneSupport) => {
                for position in &line {
                    excerpt.set_block_at(*position, Block::Cobblestone);
                    excerpt.set_block_at(*position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(*position + (0, 2, 0).into(), Block::Air);
                }
            }
            (RoadNodeKind::StoneSupport, _) | (_, RoadNodeKind::StoneSupport) => {
                for position in &line {
                    excerpt.set_block_at(*position, Block::bottom_slab(Material::Cobblestone));
                    excerpt.set_block_at(*position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(*position + (0, 2, 0).into(), Block::Air);
                }
            }
            _ => {
                for position in &line {
                    excerpt.set_block_at(*position - (0, 2, 0).into(), Block::Cobblestone);
                    excerpt.set_block_at(*position - (0, 1, 0).into(), Block::Gravel);
                    excerpt.set_block_at(*position, Block::Air);
                    excerpt.set_block_at(*position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(*position + (0, 2, 0).into(), Block::Air);
                }
            }
        }
    }

    // Build the nodes
    for RoadNode {
        coordinates, kind, ..
    } in path
    {
        let (x, y, z) = (coordinates.0, coordinates.1, coordinates.2);

        // Path and support at node
        match kind {
            RoadNodeKind::Ground => {
                tree::chop(excerpt, (x, y - 1, z).into());
                excerpt.set_block_at(
                    (x, y - 1, z).into(),
                    //Block::double_slab(Material::SmoothStone),
                    Block::Andesite,
                    //Block::BlockOfGold,
                );
            }
            RoadNodeKind::WoodenSupport => {
                let image::Luma([ground]) = height_map[(x as u32, z as u32)];
                for y in ground as i64..y {
                    tree::chop(excerpt, (x, y, z).into());
                    excerpt.set_block_at((x, y, z).into(), Block::oak_log(Axis3::Y));
                }
            }
            RoadNodeKind::StoneSupport => {
                let image::Luma([ground]) = height_map[(x as u32, z as u32)];
                for y in ground as i64..y {
                    let coordinates = (x + 1, y, z).into();
                    tree::chop(excerpt, coordinates);
                    excerpt.set_block_at(coordinates, Block::StoneBricks);
                    let coordinates = (x - 1, y, z).into();
                    tree::chop(excerpt, coordinates);
                    excerpt.set_block_at(coordinates, Block::StoneBricks);
                    let coordinates = (x, y, z + 1).into();
                    tree::chop(excerpt, coordinates);
                    excerpt.set_block_at(coordinates, Block::StoneBricks);
                    let coordinates = (x, y, z - 1).into();
                    tree::chop(excerpt, coordinates);
                    excerpt.set_block_at(coordinates, Block::StoneBricks);
                }
            }
            _ => (),
        }
    }
}
