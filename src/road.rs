use crate::line;
use crate::pathfinding::{RoadNode, RoadNodeKind, RoadPath};
use crate::tree;

use image::GrayImage;
use mcprogedit::block::Block;
use mcprogedit::material::Material;
use mcprogedit::positioning::Axis3;
use mcprogedit::world_excerpt::WorldExcerpt;

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
                    //Block::Andesite,
                    Block::BlockOfGold,
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
