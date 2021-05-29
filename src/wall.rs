use crate::features::Features;
use crate::line;
use crate::tree;
use crate::types::Snake;
use mcprogedit::block::Block;
use mcprogedit::coordinates::BlockColumnCoord;
use mcprogedit::world_excerpt::WorldExcerpt;

pub fn build_wall(excerpt: &mut WorldExcerpt, town_circumference: &Snake, features: &Features) {
    // Build the walls pt. 1: Segments of wall.
    for wall_segment in town_circumference.windows(2) {
        let (start, end) = (wall_segment[0], wall_segment[1]);
        let start = (start.0 as usize, start.1 as usize);
        let end = (end.0 as usize, end.1 as usize);
        let start_ground = features.terrain_height_map.height_at(start).unwrap() as i64;
        let end_ground = features.terrain_height_map.height_at(end).unwrap() as i64;

        let line = line::line(
            &(start.0 as i64, start_ground + 4, start.1 as i64).into(),
            &(end.0 as i64, end_ground + 4, end.1 as i64).into(),
            3,
        );

        for position in line {
            tree::chop(excerpt, position);
            tree::chop(excerpt, position - (0, 1, 0).into());
            tree::chop(excerpt, position - (0, 2, 0).into());
            tree::chop(excerpt, position - (0, 3, 0).into());
            tree::chop(excerpt, position - (0, 4, 0).into());
            tree::chop(excerpt, position - (0, 5, 0).into());
            excerpt.set_block_at(position, Block::StoneBricks);
            excerpt.set_block_at(position - (0, 1, 0).into(), Block::StoneBricks);
            excerpt.set_block_at(position - (0, 2, 0).into(), Block::StoneBricks);
            excerpt.set_block_at(position - (0, 3, 0).into(), Block::StoneBricks);
            excerpt.set_block_at(position - (0, 4, 0).into(), Block::StoneBricks);
            excerpt.set_block_at(position - (0, 5, 0).into(), Block::StoneBricks);
        }
    }

    // Build the walls pt. 2: Node points.
    for BlockColumnCoord(x, z) in town_circumference {
        // Place pillars
        let ground = features
            .terrain_height_map
            .height_at((*x as usize, *z as usize))
            .unwrap_or(0) as i64;
        for y in ground..ground + 5 {
            let coordinates = (*x, y, *z).into();
            tree::chop(excerpt, coordinates);
            excerpt.set_block_at(coordinates, Block::StoneBricks);
        }
        let coordinates = (*x, ground + 5, *z).into();
        tree::chop(excerpt, coordinates);
        excerpt.set_block_at(coordinates, Block::torch());
    }
}

pub fn build_wall_crowning(
    excerpt: &mut WorldExcerpt,
    town_circumference: &Snake,
    features: &Features,
) {
    for wall_segment in town_circumference.windows(2) {
        let (start, end) = (wall_segment[0], wall_segment[1]);
        let start = (start.0 as usize, start.1 as usize);
        let end = (end.0 as usize, end.1 as usize);
        let start_ground = features.terrain_height_map.height_at(start).unwrap() as i64;
        let end_ground = features.terrain_height_map.height_at(end).unwrap() as i64;

        let line = line::line(
            &(start.0 as i64, start_ground + 4, start.1 as i64).into(),
            &(end.0 as i64, end_ground + 4, end.1 as i64).into(),
            4,
        );

        for position in line {
            tree::chop(excerpt, position);
            excerpt.set_block_at(position, Block::StoneBricks);
        }

        let line = line::double_line(
            &(start.0 as i64, start_ground + 5, start.1 as i64).into(),
            &(end.0 as i64, end_ground + 5, end.1 as i64).into(),
            4,
        );

        for position in line {
            tree::chop(excerpt, position);
            excerpt.set_block_at(position, Block::Cobblestone);
        }
    }
}
